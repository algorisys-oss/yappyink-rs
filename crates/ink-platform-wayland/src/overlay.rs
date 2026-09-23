//! The production overlay surface on Wayland (T011).
//!
//! This is the adapter: it owns the native surface and the event loop, turns
//! Wayland events into [`PlatformEvent`]s for the controller, and carries out
//! the [`Effect`]s the controller returns. It decides nothing about modes,
//! gestures, or documents; that is `ink-app` and `ink-core`.
//!
//! # What this backend can and cannot do on GNOME
//!
//! Measured, not assumed. See `docs/evidence/E002` and `E003`:
//!
//! - The surface is **floating**. Fullscreen would cover the output but Mutter
//!   unredirects it and the transparency is lost, so fullscreen is not an
//!   option for an overlay.
//! - It cannot choose its **output**. xdg-shell has no positioning, and going
//!   fullscreen on a target first does not survive un-fullscreening.
//! - It cannot raise itself. Staying above other windows needs the user to
//!   apply **Always on Top** from the window menu, once per launch.
//!
//! None of that is a bug to be worked around. It is what this protocol offers,
//! and the honest response is to report it rather than to fake it. A
//! layer-shell backend (T006) or a GNOME companion (ADR-002) removes these
//! limits; nothing in this file can.

use ink_app::{Action, Controller, Effect, Mode, PlatformEvent, Preview, Tool, TransitionId};
use ink_core::{IdSource, LogicalPoint, LogicalSize, Object, OutputId, Session, Shape, Style};
use ink_platform::PlatformError;
use ink_render::{Canvas, Scale};
use smithay_client_toolkit::activation::{ActivationHandler, ActivationState, RequestData};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::channel::{Event as ChannelEvent, channel};
use smithay_client_toolkit::reexports::calloop::{EventLoop, channel as calloop_channel};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers,
};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::xdg::window::{
    Window, WindowConfigure, WindowDecorations, WindowHandler,
};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_registry, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

/// The left mouse button, as Wayland reports it.
const BTN_LEFT: u32 = 0x110;

/// Something asked of the overlay from outside its event loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayRequest {
    /// A controller action: a mode change or a withdrawal.
    Act(Action),
    /// Stop running and withdraw.
    Quit,
}

/// Sends requests into a running overlay from another thread.
#[derive(Clone)]
pub struct ControlSender(calloop_channel::Sender<OverlayRequest>);

/// The overlay is no longer running, so the request went nowhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayStopped;

impl std::fmt::Display for OverlayStopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the overlay is no longer running")
    }
}

impl std::error::Error for OverlayStopped {}

impl ControlSender {
    /// Returns [`OverlayStopped`] once the overlay has exited.
    pub fn send(&self, request: OverlayRequest) -> Result<(), OverlayStopped> {
        self.0.send(request).map_err(|_| OverlayStopped)
    }
}

/// The receiving half, handed to [`OverlayConfig`].
pub struct ControlChannel(calloop_channel::Channel<OverlayRequest>);

/// Creates a control channel.
///
/// A calloop channel rather than `std::sync::mpsc` so it can be a source in
/// the event loop. That is what lets the loop block until something actually
/// happens instead of waking up to ask (NFR-002).
pub fn control_channel() -> (ControlSender, ControlChannel) {
    let (sender, receiver) = channel();
    (ControlSender(sender), ControlChannel(receiver))
}

/// How the overlay is set up for a run.
pub struct OverlayConfig {
    /// Requested surface size in logical units. The compositor decides the
    /// real size, and the configure it sends is what is used.
    pub size: LogicalSize,
    /// Requests from outside the Wayland event loop, normally the control
    /// socket (T012, FR-005).
    ///
    /// Not a convenience. Once PassThrough works, the application underneath
    /// owns the keyboard and nothing inside our surface can hear the user, so
    /// an outside route is the only way back. Hidden makes it starker still:
    /// there is no surface at all.
    pub control: Option<ControlChannel>,
}

/// Runs the overlay until it is asked to quit.
///
/// Returns when the user quits or the compositor disconnects. The surface is
/// withdrawn on every exit path, including the error paths: FR-019 does not
/// allow a failure to leave an invisible surface intercepting input.
pub fn run(config: OverlayConfig) -> Result<Session, PlatformError> {
    let conn = Connection::connect_to_env().map_err(|e| {
        PlatformError::disconnected(format!("could not connect to the compositor: {e}"))
    })?;
    let (globals, queue) = registry_queue_init::<Overlay>(&conn)
        .map_err(|e| PlatformError::disconnected(format!("the registry could not be read: {e}")))?;
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)
        .map_err(|e| PlatformError::unsupported("wl_compositor", e.to_string()))?;
    let xdg_shell = XdgShell::bind(&globals, &qh)
        .map_err(|e| PlatformError::unsupported("xdg_wm_base", e.to_string()))?;
    let shm = Shm::bind(&globals, &qh)
        .map_err(|e| PlatformError::unsupported("wl_shm", e.to_string()))?;
    // Optional. Without it the surface simply cannot raise itself, which is
    // reported rather than treated as a failure to start.
    let activation = ActivationState::bind::<Overlay>(&globals, &qh).ok();

    let width = config.size.width().round().max(1.0) as u32;
    let height = config.size.height().round().max(1.0) as u32;
    let pool = SlotPool::new(width as usize * height as usize * 4, &shm)
        .map_err(|e| PlatformError::unsupported("wl_shm pool", e.to_string()))?;

    // The output is unknown until the surface is mapped and the compositor
    // says which one it entered. Until then the document is bound to a
    // placeholder, and `surface_enter` rebinds it.
    let output = OutputId::new("pending");

    let mut overlay = Overlay {
        qh: qh.clone(),
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        seat_state: SeatState::new(&globals, &qh),
        shm,
        pool,
        compositor,
        xdg_shell,
        activation,
        window: None,
        keyboard: None,
        pointer: None,
        width,
        height,
        scale: Scale::ONE,
        configured: false,
        needs_redraw: false,
        controller: Controller::new(),
        session: Session::new(output, config.size),
        ids: IdSource::starting_at(1),
        painter: ink_render::Painter::new(),
        pending_confirmations: Vec::new(),
        quit: false,
    };

    // FR-004: startup is Hidden. Launching with the intent to draw is itself
    // the activation, so the first thing the run does is ask for Draw. Until
    // T012 there is no global shortcut that could ask for it later.
    let start = overlay.controller_act(Action::EnterDraw);
    overlay.apply(start);

    print_controls();

    // An event loop rather than a poll. NFR-002 asks for no continuous wakeups
    // when nothing is changing, and the Wayland queue and the control channel
    // are both sources it can block on, so an idle overlay costs nothing.
    let mut event_loop: EventLoop<Overlay> = EventLoop::try_new()
        .map_err(|e| PlatformError::unsupported("event loop", e.to_string()))?;
    let handle = event_loop.handle();

    WaylandSource::new(conn.clone(), queue)
        .insert(handle.clone())
        .map_err(|e| PlatformError::unsupported("wayland event source", e.to_string()))?;

    if let Some(ControlChannel(receiver)) = config.control {
        handle
            .insert_source(receiver, |event, _, overlay: &mut Overlay| match event {
                ChannelEvent::Msg(OverlayRequest::Act(action)) => {
                    let effects = overlay.controller.act(action);
                    overlay.apply(effects);
                }
                ChannelEvent::Msg(OverlayRequest::Quit) => overlay.quit = true,
                // The control thread has gone. The overlay keeps running on
                // its own keys, so losing the socket never strands the user
                // with ink they cannot dismiss.
                ChannelEvent::Closed => {}
            })
            .map_err(|e| PlatformError::unsupported("control event source", e.to_string()))?;
    }

    while !overlay.quit {
        // `None` means block until something happens. An overlay nobody is
        // touching does not wake at all.
        event_loop
            .dispatch(None, &mut overlay)
            .map_err(|e| PlatformError::disconnected(format!("the event loop failed: {e}")))?;

        // A mode whose native work was issued during this dispatch has now had
        // its commit sent, so the transition is genuinely complete.
        let confirmations = std::mem::take(&mut overlay.pending_confirmations);
        if !confirmations.is_empty() {
            conn.flush().ok();
            for transition in confirmations {
                let effects = overlay
                    .controller
                    .handle(PlatformEvent::ModeApplied { transition });
                overlay.apply(effects);
            }
        }

        if overlay.needs_redraw {
            overlay.draw();
            conn.flush().ok();
        }
    }

    // Withdrawal on the way out, not as a side effect of the process dying.
    overlay.withdraw();
    conn.roundtrip().ok();
    Ok(overlay.session)
}

fn print_controls() {
    eprintln!(
        "yappyink overlay\n\n\
         This surface is floating and cannot raise itself: press Alt+Space and\n\
         choose \"Always on Top\", or the ink will be covered when you click\n\
         another window. Mutter offers no way for an application to do this\n\
         itself. See docs/evidence/E002.\n\n\
         Keys, while the overlay has focus:\n\
         \x20 d      draw\n\
         \x20 p      pass through: ink stays, input goes to what is underneath\n\
         \x20 h      hide the ink, keeping it in memory\n\
         \x20 1 / 2  pen / highlighter\n\
         \x20 3 - 6  line / arrow / rectangle / ellipse\n\
         \x20 7 or e eraser: removes whole objects its sweep touches\n\
         \x20 u / r  undo / redo\n\
         \x20 x      clear everything, undoably\n\
         \x20 c      next colour\n\
         \x20 [ / ]  thinner / thicker\n\
         \x20 - / =  less / more opaque\n\
         \x20 Esc    cancel a stroke, or leave draw mode\n\
         \x20 q      quit\n\n\
         While hidden the overlay has no surface and no keyboard, so nothing you\n\
         press can reach it. Run `yappyink toggle-draw` from anywhere, or bind\n\
         that command to a chord in your desktop's keyboard settings.\n"
    );
}

struct Overlay {
    /// Kept so the surface can be recreated after Hidden withdrew it.
    qh: QueueHandle<Overlay>,
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    shm: Shm,
    pool: SlotPool,
    compositor: CompositorState,
    xdg_shell: XdgShell,
    activation: Option<ActivationState>,
    window: Option<Window>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    width: u32,
    height: u32,
    scale: Scale,
    configured: bool,
    needs_redraw: bool,
    controller: Controller,
    session: Session,
    ids: IdSource,
    painter: ink_render::Painter,
    /// Transitions whose native work has been issued and will be confirmed on
    /// the next loop iteration, once the commit has reached the compositor.
    pending_confirmations: Vec<TransitionId>,
    quit: bool,
}

impl Overlay {
    fn controller_act(&mut self, action: Action) -> Vec<Effect> {
        self.controller.act(action)
    }

    /// Carries out the controller's effects, in the order given.
    fn apply(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::ApplyMode { mode, transition } => self.apply_mode(mode, transition),
                Effect::WithdrawImmediately => self.withdraw(),
                Effect::CommitObject { shape, style } => self.commit(shape, style),
                Effect::EraseAlong { path, radius } => {
                    match self.session.erase_along(&path, radius) {
                        Ok(0) => eprintln!("[erase] the sweep touched nothing"),
                        Ok(count) => {
                            eprintln!("[erase] removed {count} object(s) as one undoable action");
                            self.needs_redraw = true;
                        }
                        Err(error) => eprintln!("[rejected] {error}"),
                    }
                }
                Effect::Undo => match self.session.undo() {
                    Ok(true) => self.needs_redraw = true,
                    Ok(false) => eprintln!("[undo] nothing left to undo"),
                    Err(error) => eprintln!("[rejected] {error}"),
                },
                Effect::Redo => match self.session.redo() {
                    Ok(true) => self.needs_redraw = true,
                    Ok(false) => eprintln!("[redo] nothing left to redo"),
                    Err(error) => eprintln!("[rejected] {error}"),
                },
                Effect::Clear => match self.session.clear() {
                    Ok(0) => eprintln!("[clear] there was nothing to clear"),
                    Ok(count) => {
                        eprintln!("[clear] removed {count} object(s); undo brings them back");
                        self.needs_redraw = true;
                    }
                    Err(error) => eprintln!("[rejected] {error}"),
                },
                Effect::GestureDiscarded { reason } => {
                    // The user finished this gesture and nothing appeared, so
                    // they are told why rather than left guessing (FR-008).
                    eprintln!("[discarded] {reason}");
                    self.needs_redraw = true;
                }
                Effect::GestureCancelled { points } => {
                    eprintln!("[cancelled] a gesture of {points} sample(s) was discarded");
                    self.needs_redraw = true;
                }
                Effect::Faulted { error } => {
                    eprintln!("[fault {}] {error}", error.class());
                }
            }
        }
    }

    fn apply_mode(&mut self, mode: Mode, transition: TransitionId) {
        match mode {
            Mode::Hidden => {
                self.withdraw();
                eprintln!(
                    "[mode] hidden. {} object(s) kept in memory. `yappyink toggle-draw` shows them.",
                    self.session.document().len()
                );
            }
            Mode::Draw | Mode::PassThrough => {
                self.ensure_window();
                let Some(window) = &self.window else { return };
                match mode {
                    // `None` restores the default input region: the whole
                    // surface, transparent pixels included. E003 finding 1
                    // confirmed transparency does not affect this.
                    Mode::Draw => window.wl_surface().set_input_region(None),
                    _ => {
                        let Ok(region) = Region::new(&self.compositor) else {
                            return;
                        };
                        // A region with no rectangles is empty, so the
                        // compositor delivers pointer input to whatever is
                        // underneath. Nothing is forwarded or synthesised.
                        window
                            .wl_surface()
                            .set_input_region(Some(region.wl_region()));
                    }
                }
                window.commit();
                self.needs_redraw = true;
                eprintln!(
                    "[mode] {}",
                    if mode == Mode::Draw {
                        "draw"
                    } else {
                        "pass-through"
                    }
                );

                if mode == Mode::Draw {
                    self.request_raise();
                }
            }
        }

        // Confirmed on the next iteration rather than here. Wayland does not
        // acknowledge an input region, so the honest moment to call the
        // transition complete is after the commit has been flushed, which the
        // next roundtrip does.
        self.pending_confirmations.push(transition);
    }

    fn commit(&mut self, shape: Shape, style: Style) {
        let object = Object::new(
            self.ids.next_id(),
            self.session.document().output().clone(),
            style,
            shape,
        );
        match self.session.add(object) {
            Ok(_) => self.needs_redraw = true,
            // A full document or an output mismatch is reported, never
            // swallowed: the user's gesture did not become ink and they need
            // to know why (NFR-003).
            Err(error) => eprintln!("[rejected] {error}"),
        }
    }

    /// Creates the surface if it does not exist.
    fn ensure_window(&mut self) {
        if self.window.is_some() {
            return;
        }
        let surface = self.compositor.create_surface(&self.queue_handle());
        let window = self.xdg_shell.create_window(
            surface,
            WindowDecorations::RequestServer,
            &self.queue_handle(),
        );
        window.set_title("yappyink");
        window.set_app_id("dev.yappyink.Overlay");
        window.commit();
        self.configured = false;
        self.window = Some(window);
    }

    /// Destroys the surface.
    ///
    /// Dropping the [`Window`] destroys the xdg role and the `wl_surface`, so
    /// nothing is left that could intercept input. The document is untouched:
    /// FR-004 keeps committed annotations in memory while hidden.
    fn withdraw(&mut self) {
        self.window = None;
        self.configured = false;
    }

    fn draw(&mut self) {
        let Some(window) = &self.window else {
            self.needs_redraw = false;
            return;
        };
        if !self.configured || self.width == 0 || self.height == 0 {
            return;
        }
        self.needs_redraw = false;

        let stride = self.width as i32 * 4;
        let Ok((buffer, bytes)) = self.pool.create_buffer(
            self.width as i32,
            self.height as i32,
            stride,
            wl_shm::Format::Argb8888,
        ) else {
            eprintln!("[fault surface_lost] a frame buffer could not be allocated");
            return;
        };

        let Some(mut canvas) = Canvas::new(bytes, self.width, self.height) else {
            eprintln!("[fault invalid_data] the frame buffer is the wrong size");
            return;
        };
        canvas.clear();
        self.painter
            .paint(self.session.document(), &mut canvas, self.scale);

        // The gesture in flight is drawn but not in the document, which is the
        // whole point of keeping the preview separate from committed state.
        if let Some(preview) = self.controller.preview() {
            let built = match preview {
                Preview::Stroke {
                    points,
                    kind,
                    style,
                } => Shape::stroke(kind, points.to_vec())
                    .ok()
                    .map(|shape| (shape, style)),
                Preview::Shape { shape, style } => Some((shape, style)),
                // The eraser's sweep is drawn as a faint grey trail so the
                // user can see what it is about to take. It is chrome: never
                // in the document, and it commits nothing.
                Preview::Erase { path, radius } => {
                    Shape::stroke(ink_core::StrokeKind::Pen, path.to_vec())
                        .ok()
                        .and_then(|shape| {
                            let width = ink_core::Width::new(radius * 2.0)?;
                            let faint = ink_core::Opacity::new(0.35)?;
                            let grey = ink_core::Rgb::new(200, 200, 200);
                            Some((shape, Style::new(grey, width, faint)))
                        })
                }
            };
            if let Some((shape, style)) = built {
                let object = Object::new(
                    ink_core::ObjectId::from_raw(u64::MAX),
                    self.session.document().output().clone(),
                    style,
                    shape,
                );
                self.painter.paint_object(&object, &mut canvas, self.scale);
            }
        }

        paint_chrome(
            &mut canvas,
            self.controller.mode(),
            self.controller.style(),
            self.controller.tool(),
        );

        let surface = window.wl_surface();
        surface.damage_buffer(0, 0, self.width as i32, self.height as i32);
        if buffer.attach_to(surface).is_err() {
            eprintln!("[fault surface_lost] the frame buffer could not be attached");
            return;
        }
        window.commit();
    }

    /// Asks the compositor to raise and focus the surface.
    ///
    /// Only on an explicit request to Draw. That distinction is the whole
    /// argument for doing this at all: `ux-state-machine.md` forbids the ink
    /// surface *reactivating itself during PassThrough*, because that would
    /// steal focus back from the application the user just clicked. Raising
    /// because the user asked to draw is the opposite, and it is what
    /// `xdg_activation_v1` exists for.
    ///
    /// Compositors are entitled to refuse, and Mutter is strict about
    /// self-activation without a recent input serial. A refusal is not an
    /// error: it means the user must still apply "Always on Top" themselves
    /// (E002), which is what they have to do today anyway.
    fn request_raise(&mut self) {
        let (Some(activation), Some(window)) = (&self.activation, &self.window) else {
            return;
        };
        activation.request_token::<Overlay, ()>(
            &self.qh,
            RequestData {
                seat_and_serial: None,
                surface: Some(window.wl_surface().clone()),
                app_id: Some("dev.yappyink.Overlay".to_owned()),
                udata: (),
            },
        );
    }

    fn queue_handle(&self) -> QueueHandle<Self> {
        self.qh.clone()
    }
}

/// Draws the frame and mode badge.
///
/// Not decoration for its own sake. The surface is transparent and
/// undecorated, so without an outline the user cannot tell where it is, which
/// makes it impossible to aim at. The badge answers "which mode am I in?"
/// without looking away at a terminal.
///
/// This is chrome, not document content: it is painted after the document,
/// never stored in it, and an ink-only export must exclude it (FR-024). A real
/// toolbar is T013.
fn paint_chrome(canvas: &mut Canvas, mode: Mode, style: Style, tool: Tool) {
    // Premultiplied, memory order B, G, R, A.
    let frame = match mode {
        // Cyan: this surface is taking your pointer.
        Mode::Draw => [0x30, 0x30, 0x00, 0x30],
        // Amber: your pointer belongs to whatever is underneath.
        Mode::PassThrough => [0x00, 0x20, 0x30, 0x30],
        // Unreachable while a frame is being painted: Hidden has no surface.
        Mode::Hidden => return,
    };

    let (width, height) = (i64::from(canvas.width()), i64::from(canvas.height()));
    let thickness = 2;
    canvas.fill_rect(0, 0, width, thickness, frame);
    canvas.fill_rect(0, height - thickness, width, thickness, frame);
    canvas.fill_rect(0, 0, thickness, height, frame);
    canvas.fill_rect(width - thickness, 0, thickness, height, frame);

    // The mode swatch.
    let mode_colour = match mode {
        Mode::Draw => [0xC0, 0xC0, 0x00, 0xC0],
        Mode::PassThrough => [0x00, 0x80, 0xC0, 0xC0],
        Mode::Hidden => return,
    };
    canvas.fill_rect(10, 10, 18, 18, mode_colour);

    // The tool swatch: the colour and width about to be drawn, so the user can
    // see the setting rather than remember it. A real toolbar is T013.
    let alpha = style.opacity.get();
    let channel = |value: u8| (f64::from(value) * alpha).round().clamp(0.0, 255.0) as u8;
    let ink = [
        channel(style.color.b),
        channel(style.color.g),
        channel(style.color.r),
        (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ];
    let bar = style.width.get().round().max(1.0) as i64;
    canvas.fill_rect(34, 10 + (18 - bar).max(0) / 2, 40, bar.min(18), ink);

    // A pip count marks which tool is selected, so the tools are not told
    // apart by colour alone. A toolbar showing them properly is T013.
    let pips = match tool {
        Tool::Pen => 0,
        Tool::Highlighter => 1,
        Tool::Line => 2,
        Tool::Arrow => 3,
        Tool::Rectangle => 4,
        Tool::Ellipse => 5,
        Tool::Eraser => 6,
    };
    for pip in 0..pips {
        canvas.fill_rect(80 + i64::from(pip) * 12, 14, 8, 8, ink);
    }
}

impl ActivationHandler for Overlay {
    type RequestUdata = ();

    fn new_token(&mut self, token: String, _data: &RequestData<()>) {
        let (Some(activation), Some(window)) = (&self.activation, &self.window) else {
            return;
        };
        activation.activate::<Overlay>(window.wl_surface(), token);
    }
}

impl PointerHandler for Overlay {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        // Positions arrive surface-local, which is already the document's
        // coordinate space once the scale is divided out (FR-012).
        let mut produced = Vec::new();
        for event in events {
            let logical = LogicalPoint::new(
                event.position.0 / self.scale.get(),
                event.position.1 / self.scale.get(),
            );
            let Some(at) = logical else {
                // A non-finite position from the compositor is dropped rather
                // than allowed into the document.
                continue;
            };
            match event.kind {
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                    produced.push(PlatformEvent::PointerDown { at });
                }
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                    produced.push(PlatformEvent::PointerUp { at });
                }
                PointerEventKind::Motion { .. } => {
                    produced.push(PlatformEvent::PointerMoved { at });
                }
                PointerEventKind::Leave { .. } => {
                    produced.push(PlatformEvent::PointerCancelled);
                }
                _ => {}
            }
        }
        for event in produced {
            let effects = self.controller.handle(event);
            // Any pointer event can change the preview, so a redraw is asked
            // for whenever a gesture is in flight, whatever kind it is. Asking
            // only about freehand samples was a bug: a shape being dragged out
            // has no samples, so it only appeared once it was committed.
            if self.controller.is_gesturing() {
                self.needs_redraw = true;
            }
            self.apply(effects);
        }
    }
}

impl KeyboardHandler for Overlay {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        // Expected in PassThrough: clicking an application underneath gives it
        // focus, which is the behaviour FR-003 asks for, not a failure.
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        let action = match event.keysym {
            Keysym::d | Keysym::D => Some(Action::EnterDraw),
            Keysym::p | Keysym::P => Some(Action::ToggleDraw),
            Keysym::h | Keysym::H => Some(Action::ToggleVisibility),
            Keysym::Escape => Some(Action::Escape),
            Keysym::_1 => Some(Action::SelectTool(Tool::Pen)),
            Keysym::_2 => Some(Action::SelectTool(Tool::Highlighter)),
            Keysym::_3 => Some(Action::SelectTool(Tool::Line)),
            Keysym::_4 => Some(Action::SelectTool(Tool::Arrow)),
            Keysym::_5 => Some(Action::SelectTool(Tool::Rectangle)),
            Keysym::_6 => Some(Action::SelectTool(Tool::Ellipse)),
            Keysym::_7 | Keysym::e | Keysym::E => Some(Action::SelectTool(Tool::Eraser)),
            // Single keys rather than Ctrl chords, because modifier tracking
            // is not wired up yet. Local editing shortcuts with the platform's
            // proper modifier belong with the toolbar (T013).
            Keysym::u | Keysym::U => Some(Action::Undo),
            Keysym::r | Keysym::R => Some(Action::Redo),
            Keysym::x | Keysym::X => Some(Action::Clear),
            Keysym::c | Keysym::C => Some(Action::CycleColor),
            Keysym::bracketleft => Some(Action::AdjustWidth(-1)),
            Keysym::bracketright => Some(Action::AdjustWidth(1)),
            Keysym::minus => Some(Action::AdjustOpacity(-1)),
            Keysym::equal | Keysym::plus => Some(Action::AdjustOpacity(1)),
            Keysym::q | Keysym::Q => {
                self.quit = true;
                None
            }
            _ => None,
        };
        if let Some(action) = action {
            let effects = self.controller.act(action);
            self.apply(effects);
            // A tool change produces no effects but does change the chrome.
            self.needs_redraw = true;
        }
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }

    /// Ignored: a held mode key must not toggle the mode repeatedly.
    fn repeat_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _modifiers: Modifiers,
        _raw: RawModifiers,
        _layout: u32,
    ) {
    }
}

impl SeatHandler for Overlay {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl CompositorHandler for Overlay {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        if let Some(scale) = Scale::new(f64::from(new_factor)) {
            self.scale = scale;
            self.needs_redraw = true;
        }
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        // Which output the compositor chose is only knowable once the surface
        // is on one. E003 finding 4: we cannot ask for a particular output, so
        // the document binds to whichever we were given.
        let Some(name) = self.output_state.info(output).and_then(|info| info.name) else {
            return;
        };
        if self.session.document().output().as_str() == name {
            return;
        }
        if self.session.rebind(OutputId::new(&name)) {
            eprintln!("[output] bound to {name}");
        } else {
            // Rebinding a document that already holds ink would silently move
            // annotations between outputs. T027 owns doing this properly.
            eprintln!(
                "[output] now on {name}, but {} object(s) are bound to {}. They stay where they \
                 are; moving a document between outputs is T027.",
                self.session.document().len(),
                self.session.document().output()
            );
        }
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl WindowHandler for Overlay {
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &Window) {
        self.quit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        if let (Some(width), Some(height)) = configure.new_size {
            self.width = width.get();
            self.height = height.get();
        }
        self.configured = true;
        self.needs_redraw = true;
    }
}

impl OutputHandler for Overlay {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
        // FR-019: cancel the gesture, keep the committed scene, withdraw the
        // surface. The controller decides all of that; this only reports it.
        let effects = self.controller.handle(PlatformEvent::OutputLost);
        self.apply(effects);
    }
}

impl ShmHandler for Overlay {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Overlay {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_registry!(Overlay);
smithay_client_toolkit::delegate_dispatch2!(Overlay);
