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

use ink_app::{Action, Controller, Effect, Icon, Mode, PlatformEvent, Tool, Toolbar, TransitionId};
use ink_core::{IdSource, LogicalPoint, LogicalSize, Object, OutputId, Session, Shape, Style};
use ink_platform::PlatformError;
use ink_render::{Canvas, Scale};
use smithay_client_toolkit::activation::{ActivationHandler, ActivationState, RequestData};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::channel::{Event as ChannelEvent, channel};
use smithay_client_toolkit::reexports::calloop::{EventLoop, channel as calloop_channel};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers,
};
use smithay_client_toolkit::seat::pointer::{
    CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
};
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
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3;
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ZwpTextInputV3;

/// The left mouse button, as Wayland reports it.
const BTN_LEFT: u32 = 0x110;

/// A snapshot of everything the chrome is drawn from.
///
/// A named struct rather than a tuple so that adding a field is a visible,
/// reviewable change rather than an easily missed one.
#[derive(Clone, PartialEq)]
struct VisualState {
    tool: Tool,
    style: Style,
    mode: Mode,
    gesturing: bool,
    hovered: Option<Icon>,
    picker_open: bool,
    /// The caret hint follows the pointer, so its position is part of what is
    /// drawn.
    caret_hint: Option<ink_core::LogicalPoint>,
    selection: Vec<ink_core::ObjectId>,
    selection_drag: Option<ink_app::SelectionDrag>,
}

/// Picks a cursor theme that actually has cursors in it.
///
/// `XCURSOR_THEME` first, as the user's explicit choice. Otherwise the
/// candidates below, each checked for a real `cursors` directory rather than
/// trusted by name. That check is the point: on Ubuntu the theme called
/// "default" is an `index.theme` containing nothing but `Inherits=`, with no
/// cursors of its own, so asking for it by name yields nothing and the cursor
/// silently never changes.
fn cursor_theme() -> (String, u32) {
    let size = std::env::var("XCURSOR_SIZE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(24);

    let roots: Vec<std::path::PathBuf> = [
        std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from),
        std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".icons")),
        Some(std::path::PathBuf::from("/usr/share/icons")),
        Some(std::path::PathBuf::from("/usr/local/share/icons")),
    ]
    .into_iter()
    .flatten()
    .collect();

    let has_cursors = |name: &str| {
        roots
            .iter()
            .any(|root| root.join(name).join("cursors").is_dir())
    };

    if let Ok(chosen) = std::env::var("XCURSOR_THEME")
        && has_cursors(&chosen)
    {
        return (chosen, size);
    }
    for candidate in ["Yaru", "Adwaita", "breeze_cursors", "DMZ-White", "default"] {
        if has_cursors(candidate) {
            return (candidate.to_owned(), size);
        }
    }
    // Nothing found. Named anyway, so the failure is reported by set_cursor
    // rather than guessed at here.
    ("default".to_owned(), size)
}

/// Which interactive operation the compositor is being asked to run.
#[derive(Clone, Copy, Debug)]
enum InteractiveGrab {
    Move,
    Resize,
}

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

    // Optional too. Without it the text tool still works for anything with one
    // key per character, which is most Latin typing; what is lost is composing
    // in scripts that need an input method, and the application says so rather
    // than silently dropping their keystrokes.
    let text_input_manager = globals
        .bind::<ZwpTextInputManagerV3, _, _>(&qh, 1..=1, crate::ime::TextInputManagerData)
        .ok();
    if text_input_manager.is_none() {
        eprintln!(
            "[ime] zwp_text_input_v3 is not available, so input methods will not work in the \
             text tool"
        );
    }

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
        restore_size: None,
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
        seat: None,
        themed_pointer: None,
        cursor: None,
        cursor_failures: std::collections::HashSet::new(),
        ime_pending: crate::ime::Pending::default(),
        text_input_manager,
        text_input: None,
        ime_enabled: false,
        ime_focused: false,
        last_pointer: None,
        last_press_serial: None,
        width,
        height,
        scale: Scale::ONE,
        configured: false,
        needs_redraw: false,
        controller: Controller::new(),
        session: Session::new(output, config.size),
        ids: IdSource::starting_at(1),
        ink: ink_ui::InkLayer::new(),
        painter: match ink_render::text::TextFont::discover() {
            Ok(font) => {
                eprintln!("[font] {}", font.source().display());
                for fallback in font.fallbacks() {
                    eprintln!(
                        "[font] fallback {}, loaded when first needed",
                        fallback.display()
                    );
                }
                ink_render::Painter::new().with_font(font)
            }
            Err(reason) => {
                // Not fatal: everything except the text tool works without a
                // font. Saying so beats drawing nothing and leaving the user
                // to guess (NFR-005).
                eprintln!("[font] none found, so the text tool will draw nothing: {reason}");
                ink_render::Painter::new()
            }
        },
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

        // The engine is told about the editor here rather than at each of the
        // several places one can open or close, for the same reason the scene
        // summary is recomputed here: one place that always runs beats many
        // places that must each remember.
        overlay.sync_input_method();

        // The cursor depends on the tool and the mode, both of which change
        // without the pointer moving. Re-evaluated every iteration from the
        // last known position; `apply_cursor` does nothing when it is already
        // right, so this costs a comparison.
        if let Some(at) = overlay.last_pointer {
            let wanted = overlay.controller.cursor_at(at);
            overlay.apply_cursor(wanted, &conn);
        }

        if overlay.needs_redraw {
            // The scene summary is cheap and the document may have changed in
            // any of a dozen ways this iteration. Recomputing it here once is
            // less fragile than remembering to do it at each of them.
            overlay.publish_scene();
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
         This surface is floating and cannot raise itself. Press t, or the\n\
         pinned-window button on the toolbar, and choose \"Always on Top\", or\n\
         the ink will be covered when you click another window.\n\
         That opens the compositor's own menu: Mutter gives an application no\n\
         way to set the property itself, so this saves remembering Alt+Space\n\
         rather than removing the step. The GNOME extension in\n\
         integrations/gnome removes it. See docs/evidence/E002.\n\n\
         Keys, while the overlay has focus:\n\
         \x20 d      draw\n\
         \x20 p      pass through: ink stays, input goes to what is underneath\n\
         \x20 h      hide the ink, keeping it in memory\n\
         \x20 g      shrink to just the toolbar, and back\n\
         \x20 1 / 2  pen / highlighter\n\
         \x20 3 - 6  line / arrow / rectangle / ellipse\n\
         \x20 7 or e eraser: removes whole objects its sweep touches\n\
         \x20 8 or s select: click an object, drag to move, corners to resize\n\
         \x20 9      text: click to place a caret, then type; Esc discards\n\
         \x20 Del    delete what is selected\n\
         \x20 u / r  undo / redo\n\
         \x20 w / o  write / open the session file\n\
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

pub struct Overlay {
    /// Kept so the surface can be recreated after Hidden withdrew it.
    qh: QueueHandle<Overlay>,
    /// The size to go back to when leaving Parked.
    restore_size: Option<(u32, u32)>,
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
    /// The seat, kept because an interactive move or resize has to name one.
    seat: Option<wl_seat::WlSeat>,
    /// A themed pointer, so the cursor can be set. A client that never sets
    /// one shows whatever the previously entered surface left behind.
    themed_pointer: Option<ThemedPointer>,
    /// The cursor currently applied, so it is only set when it changes.
    cursor: Option<CursorIcon>,
    /// Cursors that could not be set, so each is complained about once.
    cursor_failures: std::collections::HashSet<CursorIcon>,
    /// What the input method has said since its last `done`.
    pub(crate) ime_pending: crate::ime::Pending,
    /// The manager, kept so a text-input object can be made once a seat
    /// exists. Seats can appear after startup.
    text_input_manager: Option<ZwpTextInputManagerV3>,
    /// The text-input object, when the compositor offers the protocol.
    pub(crate) text_input: Option<ZwpTextInputV3>,
    /// Whether the engine has been told an editor is open, so enable and
    /// disable are each sent once rather than on every frame.
    pub(crate) ime_enabled: bool,
    /// Whether text-input focus is on our surface.
    ///
    /// Enabling before the compositor says so does nothing at all, which is
    /// how the engine came to be silently absent: every key then arrives as an
    /// ordinary press and composes into Latin.
    pub(crate) ime_focused: bool,
    /// Where the pointer was last seen, in logical units.
    ///
    /// Kept because the right cursor depends on the tool as well as the
    /// position, and the tool changes without the pointer moving: a key press
    /// or a toolbar click leaves it exactly where it was. Without this the
    /// cursor stays stale until the next motion, so selecting the text tool
    /// and clicking straight away showed the old pointer for that first click.
    last_pointer: Option<LogicalPoint>,
    /// The serial of the most recent pointer press. The compositor requires it
    /// to start a move or resize, and refuses a stale or invented one.
    last_press_serial: Option<u32>,
    width: u32,
    height: u32,
    scale: Scale,
    configured: bool,
    pub(crate) needs_redraw: bool,
    pub(crate) controller: Controller,
    session: Session,
    ids: IdSource,
    painter: ink_render::Painter,
    /// The committed ink, rasterised once and reused until it changes, so a
    /// pointer move does not re-rasterise every stroke on the page (E015).
    ink: ink_ui::InkLayer,
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
    pub(crate) fn apply(&mut self, effects: Vec<Effect>) {
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
                Effect::MoveSelection { ids, dx, dy } => {
                    match self.session.move_objects(&ids, dx, dy) {
                        Ok(0) => {}
                        Ok(_) => self.needs_redraw = true,
                        Err(error) => eprintln!("[rejected] {error}"),
                    }
                }
                Effect::ScaleSelection {
                    ids,
                    anchor,
                    sx,
                    sy,
                } => match self.session.scale_objects(&ids, anchor, sx, sy) {
                    Ok(0) => {}
                    Ok(_) => self.needs_redraw = true,
                    Err(error) => eprintln!("[rejected] {error}"),
                },
                Effect::DeleteSelection { ids } => match self.session.delete(&ids) {
                    Ok(0) => {}
                    Ok(count) => {
                        eprintln!("[delete] removed {count} object(s); undo brings them back");
                        self.needs_redraw = true;
                    }
                    Err(error) => eprintln!("[rejected] {error}"),
                },
                Effect::Quit => self.quit = true,
                Effect::Save => self.save_session(),
                Effect::Load => self.load_session(),
                Effect::ShowWindowMenu { at } => self.show_window_menu(at),
                Effect::BeginWindowDrag => self.begin_interactive(InteractiveGrab::Move),
                Effect::BeginWindowResize => self.begin_interactive(InteractiveGrab::Resize),
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
            Mode::Draw | Mode::PassThrough | Mode::Parked => {
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
                        // The toolbar's rectangle, and nothing else. Everywhere
                        // outside it the compositor delivers pointer input to
                        // whatever is underneath; nothing is forwarded or
                        // synthesised. This is what makes the pinned toolbar
                        // honest rather than a picture of buttons: the pixels
                        // that look clickable are the pixels we asked for.
                        let bounds = self.controller.interactive_bounds();
                        let scale = self.scale.get();
                        let to_px = |v: f64| (v * scale).round() as i32;
                        region.add(
                            to_px(bounds.min.x),
                            to_px(bounds.min.y),
                            to_px(bounds.max.x) - to_px(bounds.min.x),
                            to_px(bounds.max.y) - to_px(bounds.min.y),
                        );
                        window
                            .wl_surface()
                            .set_input_region(Some(region.wl_region()));
                    }
                }
                // Parked pins the surface to the toolbar's size. Min and max
                // together are the protocol's way of saying "exactly this",
                // and they are cleared on the way out or the window would stay
                // small for ever.
                if mode == Mode::Parked {
                    let bounds = self.controller.toolbar().bounds();
                    let scale = self.scale.get();
                    let size = (
                        ((bounds.max.x - bounds.min.x + bounds.min.x * 2.0) * scale).round() as u32,
                        ((bounds.max.y - bounds.min.y + bounds.min.y * 2.0) * scale).round() as u32,
                    );
                    if self.restore_size.is_none() {
                        self.restore_size = Some((self.width, self.height));
                    }
                    window.set_min_size(Some(size));
                    window.set_max_size(Some(size));
                    self.width = size.0;
                    self.height = size.1;
                } else if let Some((width, height)) = self.restore_size.take() {
                    window.set_min_size(None);
                    window.set_max_size(None);
                    self.width = width;
                    self.height = height;
                }

                window.commit();
                self.needs_redraw = true;
                eprintln!(
                    "[mode] {}",
                    match mode {
                        Mode::Draw => "draw",
                        Mode::PassThrough => "pass-through",
                        _ => "parked, showing only the toolbar",
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

    /// Sets the cursor, if it is not already what we want.
    ///
    /// Only on a change: setting it on every motion event would ask the
    /// compositor to reload a cursor image hundreds of times a second.
    fn apply_cursor(&mut self, cursor: ink_app::Cursor, conn: &Connection) {
        let icon = match cursor {
            ink_app::Cursor::Default => CursorIcon::Default,
            ink_app::Cursor::Crosshair => CursorIcon::Crosshair,
            ink_app::Cursor::Text => CursorIcon::Text,
            ink_app::Cursor::Move => CursorIcon::Move,
            ink_app::Cursor::ResizeBottomRight => CursorIcon::SeResize,
        };
        if self.cursor == Some(icon) {
            return;
        }
        let Some(themed) = &self.themed_pointer else {
            return;
        };
        match themed.set_cursor(conn, icon) {
            Ok(()) => {
                // Logged on every change. They are rare, only happening when
                // the pointer crosses between chrome and canvas or the tool
                // changes, and without them a cursor that never updates and
                // one that updates to the wrong thing look identical.
                eprintln!("[cursor] {icon:?}");
                self.cursor = Some(icon);
            }
            Err(error) => {
                // Reported once per icon rather than on every motion event.
                // Swallowing it entirely is what let a missing cursor theme
                // look like a bug in this application for two rounds.
                if self.cursor_failures.insert(icon) {
                    eprintln!("[cursor] {icon:?} could not be set: {error}");
                }
            }
        }
    }

    /// Keeps the input method in step with the editor.
    ///
    /// Enables on open and disables on close, each once, and moves the
    /// candidate window as the caret moves. FR-023 asks for text focus to be
    /// released on leaving the editor, and with an engine involved that means
    /// telling the engine: one that still believes a field is focused will go
    /// on composing into nothing.
    pub(crate) fn sync_input_method(&mut self) {
        let Some(input) = &self.text_input else {
            return;
        };
        // Only while the compositor says the focus is ours.
        let editing = self.controller.is_editing_text() && self.ime_focused;

        if editing && !self.ime_enabled {
            eprintln!("[ime] enabled for the open editor");
            crate::ime::enable(input, self.caret_rectangle_px());
            self.ime_enabled = true;
            return;
        }
        if !editing && self.ime_enabled {
            eprintln!("[ime] disabled");
            crate::ime::disable(input);
            self.ime_enabled = false;
            return;
        }
        if editing {
            // Cheap, and the candidate window has to follow the caret or it
            // covers the text being composed.
            crate::ime::move_caret(input, self.caret_rectangle_px());
        }
    }

    /// The caret in surface pixels, which is what the protocol wants.
    fn caret_rectangle_px(&self) -> (i32, i32, i32, i32) {
        let Some((at, width, height)) = self.controller.caret_rectangle() else {
            return (0, 0, 1, 1);
        };
        let scale = self.scale.get();
        (
            (at.x * scale).round() as i32,
            (at.y * scale).round() as i32,
            (width * scale).round().max(1.0) as i32,
            (height * scale).round().max(1.0) as i32,
        )
    }

    /// Tells the controller what is in the document now.
    ///
    /// Called after every change, including undo, so the selection and its
    /// handles never refer to something that has gone.
    fn publish_scene(&mut self) {
        let scene = self
            .session
            .document()
            .objects()
            .map(|object| (object.id(), object.bounds()))
            .collect();
        self.controller.set_scene(scene);
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

        // SCTK rounds every slot up to a multiple of 64 bytes, so the slice can
        // be longer than the frame. 1280x720 happens to be a multiple, which is
        // how this hid until a maximized 1366x697 window and the first resize
        // failed every frame. Canvas is strict about length on purpose; the
        // extra bytes are simply not part of the image.
        let needed = (stride as usize * self.height as usize).min(bytes.len());
        let bytes = &mut bytes[..needed];
        let Some(mut canvas) = Canvas::new(bytes, self.width, self.height) else {
            eprintln!("[fault invalid_data] the frame buffer is the wrong size");
            return;
        };
        canvas.clear();
        self.ink.paint(
            &mut canvas,
            &self.controller,
            self.session.document(),
            &mut self.painter,
            self.scale,
        );

        // The gesture in flight, drawn but never in the document. Shared with
        // the other backends, which is how they got text preview at all.
        ink_ui::paint_preview(
            &mut canvas,
            &self.controller,
            self.session.document().output(),
            &mut self.painter,
            self.scale,
        );

        ink_ui::paint_chrome(
            &mut canvas,
            self.controller.mode(),
            self.controller.style(),
            self.controller.tool(),
        );

        ink_ui::paint_caret_hint(&mut canvas, &self.controller, self.last_pointer, self.scale);

        ink_ui::paint_selection(
            &mut canvas,
            &self.controller,
            self.session.document(),
            &mut self.painter,
            self.scale,
        );

        if self.controller.toolbar_visible() {
            ink_ui::paint_toolbar(
                &mut canvas,
                self.controller.toolbar(),
                self.controller.tool(),
                self.controller.style().color,
                self.scale,
            );
            if let Some(corner) = self.controller.resize_corner() {
                ink_ui::paint_resize_corner(&mut canvas, corner, self.scale);
            }
            ink_ui::paint_swatches(&mut canvas, &self.controller, self.scale);
            if let Some(button) = self.controller.hovered_button() {
                ink_ui::paint_tooltip(
                    &mut canvas,
                    button,
                    self.controller.toolbar().bounds(),
                    self.scale,
                );
            }
        }

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

    /// Writes the document to the session file.
    fn save_session(&mut self) {
        let path = match ink_storage::default_session_path() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("[save] {error}");
                return;
            }
        };
        match ink_storage::save(self.session.document(), &path) {
            Ok(()) => eprintln!(
                "[save] {} object(s) written to {}",
                self.session.document().len(),
                path.display()
            ),
            // The previous file is untouched on any failure, which is the
            // whole point of the atomic write (FR-017).
            Err(error) => eprintln!("[save {}] {error}", error.class()),
        }
    }

    /// Replaces the document with the session file's contents.
    ///
    /// A failed load changes nothing: the file is fully validated into domain
    /// objects before anything is adopted, so there is no half-loaded state to
    /// recover from.
    fn load_session(&mut self) {
        let path = match ink_storage::default_session_path() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("[load] {error}");
                return;
            }
        };
        let loaded = match ink_storage::load(&path) {
            Ok(loaded) => loaded,
            Err(error) => {
                eprintln!("[load {}] {error}", error.class());
                eprintln!("[load] what is on screen is unchanged");
                return;
            }
        };

        // FR-015: output bindings are hints. A document saved on another
        // monitor is reported rather than stretched or moved to fit this one.
        let current = self.session.document().output().clone();
        if loaded.saved_output != current && current.as_str() != "pending" {
            eprintln!(
                "[load] this was saved on {} and the overlay is on {}. The annotations keep \
                 their saved coordinates; remapping between outputs is T021's remap UI.",
                loaded.saved_output, current
            );
        }

        let count = loaded.document.len();
        self.session.adopt(loaded.document);
        // Past every id in the file, or a new object would collide with a
        // loaded one and the eraser would take the wrong thing.
        self.ids = IdSource::starting_at(loaded.next_object_id);
        self.needs_redraw = true;
        eprintln!("[load] {count} object(s) from {}", path.display());
    }

    /// Opens the compositor's own window menu.
    ///
    /// This is how "Always on Top" is reachable from inside the application.
    /// Mutter gives a client no way to set that property, and E002 measured
    /// that without it the ink is covered by the first click elsewhere. What a
    /// client may do is ask for the menu where the user can set it, which is
    /// one click instead of remembering Alt+Space.
    ///
    /// It is the compositor's menu. Nothing here draws or imitates it, and if
    /// the compositor declines there is no menu rather than a fake one.
    fn show_window_menu(&mut self, at: ink_core::LogicalPoint) {
        let (Some(window), Some(seat), Some(serial)) =
            (&self.window, &self.seat, self.last_press_serial)
        else {
            eprintln!("[window] no recent press to open the window menu from");
            return;
        };
        window.show_window_menu(
            seat,
            serial,
            (
                (at.x * self.scale.get()).round() as i32,
                (at.y * self.scale.get()).round() as i32,
            ),
        );
    }

    /// Hands the pointer to the compositor to run a move or a resize.
    ///
    /// A Wayland client cannot place or size its own window, so this is the
    /// only route (E003 finding 4). The compositor takes the pointer for the
    /// duration, which is also why the controller leaves no gesture armed.
    fn begin_interactive(&mut self, grab: InteractiveGrab) {
        let (Some(window), Some(seat), Some(serial)) =
            (&self.window, &self.seat, self.last_press_serial)
        else {
            eprintln!("[window] no recent press to start a drag from");
            return;
        };
        match grab {
            InteractiveGrab::Move => window.move_(seat, serial),
            InteractiveGrab::Resize => {
                window.resize(seat, serial, xdg_toplevel::ResizeEdge::BottomRight)
            }
        }
    }

    /// Everything the chrome is drawn from.
    ///
    /// Compared before and after handling input to decide whether to repaint.
    /// **Anything drawn on screen must appear here.** Every omission has cost
    /// a bug that looks like the feature not working: shapes invisible until
    /// release, the toolbar's selection not moving until the next stroke, and
    /// a tooltip that never appeared. The compiler cannot catch a missing
    /// field in a tuple, so this is the one place to check when something is
    /// correct in the model and absent on screen.
    fn visual_state(&self) -> VisualState {
        VisualState {
            tool: self.controller.tool(),
            style: self.controller.style(),
            mode: self.controller.mode(),
            gesturing: self.controller.is_gesturing(),
            hovered: self.controller.hovered_button().map(|button| button.icon),
            picker_open: self.controller.picker_open(),
            caret_hint: (self.controller.tool() == Tool::Text)
                .then_some(self.last_pointer)
                .flatten(),
            selection: self.controller.selection().to_vec(),
            selection_drag: self.controller.selection_drag(),
        }
    }

    fn queue_handle(&self) -> QueueHandle<Self> {
        self.qh.clone()
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
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        // Positions arrive surface-local, which is already the document's
        // coordinate space once the scale is divided out (FR-012).
        let mut produced = Vec::new();
        let mut wanted_cursor = None;
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
            // What the pointer should look like here. Recomputed per event
            // rather than per frame so that entering the surface, moving over
            // the toolbar, and moving back onto the canvas all update it.
            // Compared before the position is overwritten: the caret hint
            // follows the pointer, and the redraw check later in this function
            // runs after `last_pointer` has already moved, so it would see no
            // change. This is the same trap as the other repaint bugs, one
            // level further in.
            let hint_moved = self.controller.tool() == Tool::Text
                && Toolbar::canvas_is_interactive(self.controller.mode())
                && self.last_pointer != Some(at);
            self.last_pointer = Some(at);
            if hint_moved {
                self.needs_redraw = true;
            }
            wanted_cursor = Some(self.controller.cursor_at(at));

            match event.kind {
                PointerEventKind::Press { button, serial, .. } if button == BTN_LEFT => {
                    // Kept for a move or resize, which the compositor will only
                    // start from a recent, genuine input serial.
                    self.last_press_serial = Some(serial);
                    produced.push(PlatformEvent::PointerDown { at });
                }
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                    produced.push(PlatformEvent::PointerUp { at });
                }
                PointerEventKind::Motion { .. } => {
                    produced.push(PlatformEvent::PointerMoved { at });
                }
                PointerEventKind::Leave { .. } => {
                    self.last_pointer = None;
                    produced.push(PlatformEvent::PointerCancelled);
                }
                _ => {}
            }
        }
        if let Some(cursor) = wanted_cursor {
            self.apply_cursor(cursor, conn);
        }

        for event in produced {
            let before = self.visual_state();
            let effects = self.controller.handle(event);
            // A gesture in flight means the preview moved; a change in the
            // visible state means the chrome did. Either way, repaint.
            if self.controller.is_gesturing() || self.visual_state() != before {
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
        // FR-023: while the editor is open, keyboard focus belongs to it. A
        // key is a character, not a shortcut, or typing "q" would quit and
        // typing "x" would erase the drawing.
        if self.controller.is_editing_text() {
            // Logged only while the editor is open. When text comes out wrong,
            // what arrived here is the first thing worth knowing: it separates
            // "the keyboard is still producing Latin" from "the characters are
            // right and we failed to draw them", and those have nothing in
            // common. Guessing between them cost a day. See learning.md.
            eprintln!(
                "[key] keysym {} utf8 {:?}",
                event.keysym.name().unwrap_or("unnamed"),
                event.utf8
            );
            let action = match event.keysym {
                Keysym::Escape => Some(Action::Escape),
                Keysym::BackSpace => Some(Action::BackspaceText),
                Keysym::Return | Keysym::KP_Enter => Some(Action::NewlineText),
                _ => event
                    .utf8
                    .as_ref()
                    .and_then(|text| text.chars().next())
                    .filter(|character| !character.is_control())
                    .map(Action::TypeText),
            };
            if let Some(action) = action {
                let effects = self.controller.act(action);
                self.apply(effects);
                self.needs_redraw = true;
            }
            return;
        }

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
            Keysym::g | Keysym::G => Some(Action::TogglePark),
            Keysym::w | Keysym::W => Some(Action::Save),
            Keysym::o | Keysym::O => Some(Action::Load),
            Keysym::t | Keysym::T => Some(Action::ShowWindowMenu),
            Keysym::_8 | Keysym::s | Keysym::S => Some(Action::SelectTool(Tool::Select)),
            Keysym::_9 => Some(Action::SelectTool(Tool::Text)),
            Keysym::Delete | Keysym::BackSpace => Some(Action::DeleteSelection),
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
        self.seat = Some(seat.clone());
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();

            // Text input belongs to a seat, so it is created with the keyboard
            // rather than at startup.
            if let Some(manager) = &self.text_input_manager
                && self.text_input.is_none()
            {
                self.text_input =
                    Some(manager.get_text_input(&seat, qh, crate::ime::TextInputData));
                eprintln!("[ime] input methods available through zwp_text_input_v3");
            }
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            // A themed pointer rather than a plain one, because a plain one
            // cannot set a cursor and the overlay would keep showing whatever
            // the last window chose.
            let surface = self.compositor.create_surface(qh);
            let (theme, size) = cursor_theme();
            eprintln!("[cursor] theme {theme:?} at size {size}");
            match self.seat_state.get_pointer_with_theme::<_, ()>(
                qh,
                &seat,
                self.shm.wl_shm(),
                surface,
                ThemeSpec::Named { name: &theme, size },
            ) {
                Ok(themed) => {
                    self.pointer = Some(themed.pointer().clone());
                    self.themed_pointer = Some(themed);
                }
                Err(error) => {
                    // Not fatal: without a theme the cursor is whatever the
                    // compositor last showed, which is odd rather than broken.
                    // Saying so beats a silently wrong pointer.
                    eprintln!("[cursor] no cursor theme, the pointer will look wrong: {error}");
                    self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
                }
            }
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
        let previous = (self.width, self.height);
        if let (Some(width), Some(height)) = configure.new_size {
            self.width = width.get();
            self.height = height.get();
            // The controller offers the resize corner only once it knows the
            // surface's size. Nothing told it until 0.8.1, so on GNOME the
            // corner never appeared and the 1280x720 window could not be
            // enlarged from the app, which is the only way to annotate more of
            // the screen here (E002: fullscreen loses transparency).
            if let Some(size) = LogicalSize::new(f64::from(width.get()), f64::from(height.get())) {
                self.controller.set_surface_size(size);
            }
        }

        // Logged on the first configure and whenever the size changes. This is
        // how a run says what the compositor actually granted, which is the
        // only way to tell whether something else, such as the Shell
        // extension, has resized the surface.
        if !self.configured || previous != (self.width, self.height) {
            eprintln!(
                "[note  ] configure: {}x{} logical, state {:?}, decorations {:?}",
                self.width, self.height, configure.state, configure.decoration_mode
            );
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
