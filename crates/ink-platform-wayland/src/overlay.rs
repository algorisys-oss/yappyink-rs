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

use std::sync::mpsc::{Receiver, TryRecvError};

use ink_app::{Action, Controller, Effect, Mode, PlatformEvent, TransitionId};
use ink_core::{Document, IdSource, LogicalPoint, LogicalSize, Object, OutputId, Shape, Style};
use ink_platform::PlatformError;
use ink_render::{Canvas, Scale};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
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

/// How long the loop waits between polls.
///
/// A poll rather than a blocking dispatch, because the recovery channel is not
/// a Wayland file descriptor and a blocking dispatch would not wake for it.
/// NFR-002 wants no idle wakeups at all; making this event-driven, with the
/// recovery channel folded into the event loop, belongs with T012's control
/// channel and T014's demand-driven redraw.
const POLL: std::time::Duration = std::time::Duration::from_millis(8);

/// How the overlay is set up for a run.
pub struct OverlayConfig {
    /// Style for new strokes. The tool palette is T015/T016.
    pub style: Style,
    /// Requested surface size in logical units. The compositor decides the
    /// real size, and the configure it sends is what is used.
    pub size: LogicalSize,
    /// Receives an action from outside the Wayland event loop.
    ///
    /// A stand-in for T012's control channel and global shortcut. It exists
    /// because Hidden withdraws the surface, which takes the keyboard with it:
    /// without some outside route there would be no way back, and FR-004's
    /// "hide without clearing" could not be demonstrated at all.
    pub recovery: Option<Receiver<Action>>,
}

/// Runs the overlay until it is asked to quit.
///
/// Returns when the user quits or the compositor disconnects. The surface is
/// withdrawn on every exit path, including the error paths: FR-019 does not
/// allow a failure to leave an invisible surface intercepting input.
pub fn run(config: OverlayConfig) -> Result<Document, PlatformError> {
    let conn = Connection::connect_to_env().map_err(|e| {
        PlatformError::disconnected(format!("could not connect to the compositor: {e}"))
    })?;
    let (globals, mut queue) = registry_queue_init::<Overlay>(&conn)
        .map_err(|e| PlatformError::disconnected(format!("the registry could not be read: {e}")))?;
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)
        .map_err(|e| PlatformError::unsupported("wl_compositor", e.to_string()))?;
    let xdg_shell = XdgShell::bind(&globals, &qh)
        .map_err(|e| PlatformError::unsupported("xdg_wm_base", e.to_string()))?;
    let shm = Shm::bind(&globals, &qh)
        .map_err(|e| PlatformError::unsupported("wl_shm", e.to_string()))?;

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
        window: None,
        keyboard: None,
        pointer: None,
        width,
        height,
        scale: Scale::ONE,
        configured: false,
        needs_redraw: false,
        controller: Controller::new(),
        document: Document::new(output, config.size),
        ids: IdSource::starting_at(1),
        style: config.style,
        pending_confirmations: Vec::new(),
        quit: false,
    };

    // FR-004: startup is Hidden. Launching with the intent to draw is itself
    // the activation, so the first thing the run does is ask for Draw. Until
    // T012 there is no global shortcut that could ask for it later.
    let start = overlay.controller_act(Action::EnterDraw);
    overlay.apply(start);

    print_controls();

    while !overlay.quit {
        std::thread::sleep(POLL);
        queue.roundtrip(&mut overlay).map_err(|e| {
            PlatformError::disconnected(format!("the compositor stopped responding: {e}"))
        })?;

        // A mode requested last iteration has now been committed and flushed,
        // so the transition is genuinely complete and can be confirmed.
        let confirmations = std::mem::take(&mut overlay.pending_confirmations);
        for transition in confirmations {
            let effects = overlay
                .controller
                .handle(PlatformEvent::ModeApplied { transition });
            overlay.apply(effects);
        }

        if let Some(recovery) = &config.recovery {
            match recovery.try_recv() {
                Ok(action) => {
                    let effects = overlay.controller.act(action);
                    overlay.apply(effects);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {}
            }
        }

        if overlay.needs_redraw {
            overlay.draw();
        }
    }

    // Withdrawal on the way out, not as a side effect of the process dying.
    overlay.withdraw();
    conn.roundtrip().ok();
    Ok(overlay.document)
}

fn print_controls() {
    eprintln!(
        "yappyink overlay\n\n\
         This surface is floating and cannot raise itself: press Alt+Space and\n\
         choose \"Always on Top\", or the ink will be covered when you click\n\
         another window. Mutter offers no way for an application to do this\n\
         itself. See docs/evidence/E002.\n\n\
         Keys, while the overlay has focus:\n\
         \x20 d    draw\n\
         \x20 p    pass through: ink stays, input goes to what is underneath\n\
         \x20 h    hide the ink, keeping it in memory\n\
         \x20 Esc  cancel a stroke, or leave draw mode\n\
         \x20 q    quit\n\n\
         While hidden the overlay has no keyboard. Press Enter in this terminal\n\
         to bring it back; that is a stand-in until T012 adds a real control\n\
         channel and global shortcut.\n"
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
    window: Option<Window>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    width: u32,
    height: u32,
    scale: Scale,
    configured: bool,
    needs_redraw: bool,
    controller: Controller,
    document: Document,
    ids: IdSource,
    style: Style,
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
                Effect::CommitStroke { points } => self.commit_stroke(points),
                Effect::GestureCancelled { points } => {
                    eprintln!("[cancelled] a stroke of {points} sample(s) was discarded");
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
                    "[mode] hidden. {} object(s) kept in memory. Press Enter here to show them.",
                    self.document.len()
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
            }
        }

        // Confirmed on the next iteration rather than here. Wayland does not
        // acknowledge an input region, so the honest moment to call the
        // transition complete is after the commit has been flushed, which the
        // next roundtrip does.
        self.pending_confirmations.push(transition);
    }

    fn commit_stroke(&mut self, points: Vec<LogicalPoint>) {
        let shape = match Shape::stroke(ink_core::StrokeKind::Pen, points) {
            Ok(shape) => shape,
            Err(error) => {
                eprintln!("[rejected] {error}");
                return;
            }
        };
        let object = Object::new(
            self.ids.next_id(),
            self.document.output().clone(),
            self.style,
            shape,
        );
        match self.document.add(object) {
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
        ink_render::paint(&self.document, &mut canvas, self.scale);

        // The gesture in flight is drawn but not in the document, which is the
        // whole point of keeping the preview separate from committed state.
        if let Some(points) = self.controller.gesture_points()
            && let Ok(shape) = Shape::stroke(ink_core::StrokeKind::Pen, points.to_vec())
        {
            let preview = Object::new(
                ink_core::ObjectId::from_raw(u64::MAX),
                self.document.output().clone(),
                self.style,
                shape,
            );
            ink_render::paint_object(&preview, &mut canvas, self.scale);
        }

        paint_chrome(&mut canvas, self.controller.mode());

        let surface = window.wl_surface();
        surface.damage_buffer(0, 0, self.width as i32, self.height as i32);
        if buffer.attach_to(surface).is_err() {
            eprintln!("[fault surface_lost] the frame buffer could not be attached");
            return;
        }
        window.commit();
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
fn paint_chrome(canvas: &mut Canvas, mode: Mode) {
    // Premultiplied, memory order B, G, R, A.
    let (badge, frame) = match mode {
        // Cyan: this surface is taking your pointer.
        Mode::Draw => ([0xC0, 0xC0, 0x00, 0xC0], [0x30, 0x30, 0x00, 0x30]),
        // Amber: your pointer belongs to whatever is underneath.
        Mode::PassThrough => ([0x00, 0x80, 0xC0, 0xC0], [0x00, 0x20, 0x30, 0x30]),
        // Unreachable while a frame is being painted: Hidden has no surface.
        Mode::Hidden => return,
    };

    let (width, height) = (i64::from(canvas.width()), i64::from(canvas.height()));
    let thickness = 2;
    canvas.fill_rect(0, 0, width, thickness, frame);
    canvas.fill_rect(0, height - thickness, width, thickness, frame);
    canvas.fill_rect(0, 0, thickness, height, frame);
    canvas.fill_rect(width - thickness, 0, thickness, height, frame);

    canvas.fill_rect(10, 10, 18, 18, badge);
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
            // Any pointer event can change what the preview looks like, so the
            // press that starts a stroke paints its dot at once rather than
            // waiting for the first movement.
            if self.controller.gesture_points().is_some() {
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
            Keysym::q | Keysym::Q => {
                self.quit = true;
                None
            }
            _ => None,
        };
        if let Some(action) = action {
            let effects = self.controller.act(action);
            self.apply(effects);
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
        if self.document.output().as_str() == name {
            return;
        }
        if self.document.is_empty() {
            self.document = Document::new(OutputId::new(&name), self.document.output_size());
            eprintln!("[output] bound to {name}");
        } else {
            // Rebinding a document that already holds ink would silently move
            // annotations between outputs. T027 owns doing this properly.
            eprintln!(
                "[output] now on {name}, but {} object(s) are bound to {}. They stay where they \
                 are; moving a document between outputs is T027.",
                self.document.len(),
                self.document.output()
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
