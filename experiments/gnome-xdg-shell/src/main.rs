//! T007 step 2: the interaction probe.
//!
//! **Throwaway experiment code, not an adapter.** Its product is an observation
//! recorded in `docs/evidence/`. It shares nothing with `ink-core` on purpose:
//! it is testing the compositor, not our architecture.
//!
//! Step 1 (recorded in E002) established that on Mutter a *floating* transparent
//! surface with the user applying "Always on Top" keeps ink visible while input
//! reaches the application underneath. Fullscreen destroys transparency and a
//! maximized surface cannot be raised. This step answers the four questions
//! still standing before any real adapter is written:
//!
//! 1. Does the surface receive pointer events over **transparent** pixels?
//!    Every press lands on a transparent pixel unless it lands on existing ink,
//!    and each one is logged with its position.
//! 2. Can Draw and PassThrough be switched **repeatedly**, both ways? The probe
//!    cycles automatically, so the answer does not depend on us holding
//!    keyboard focus.
//! 3. Does a **held mouse button** leak across a transition? If the mode flips
//!    mid-drag, the transient stroke must be cancelled and no click may reach
//!    the application underneath (FR-018).
//! 4. Does the **keyboard** behave: keys reaching us in Draw, released in
//!    PassThrough?
//!
//! Pass-through is `wl_surface.set_input_region` with an empty region. Nothing
//! is forwarded, injected, or synthesised.

use std::time::{Duration, Instant};

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::csd_frame::WindowState;
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

/// Seconds spent in Draw before the automatic flip to PassThrough.
const DEFAULT_DRAW_SECONDS: u64 = 20;
/// Seconds spent in PassThrough before flipping back to Draw.
const DEFAULT_PASSTHROUGH_SECONDS: u64 = 15;
/// Total run time before the surface withdraws itself.
const DEFAULT_TOTAL_SECONDS: u64 = 180;

/// The left mouse button, as Wayland reports it.
const BTN_LEFT: u32 = 0x110;

/// The two stable modes this probe exercises. The real state machine also has
/// Hidden, Transitioning, and Faulted; they are not part of this question and
/// are deliberately absent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Draw,
    PassThrough,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Draw => "DRAW",
            Self::PassThrough => "PASS-THROUGH",
        }
    }
}

/// How the toplevel asks to be sized.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SurfaceMode {
    /// Covers the output. Measured to lose transparency on Mutter (E002).
    Fullscreen,
    /// Fills the work area. Cannot be raised above: Mutter greys out "Always on
    /// Top" for maximized windows (E002).
    Maximized,
    /// A floating toplevel. The only mode that worked in E002, so the default.
    Windowed,
}

impl SurfaceMode {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--fullscreen") {
            Self::Fullscreen
        } else if args.iter().any(|a| a == "--maximized") {
            Self::Maximized
        } else {
            Self::Windowed
        }
    }
}

/// Reads `--on-output NAME`, e.g. `--on-output eDP-1`.
fn string_arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn seconds_arg(name: &str, default: u64) -> u64 {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let surface_mode = SurfaceMode::from_args();
    let draw_seconds = seconds_arg("--draw-seconds", DEFAULT_DRAW_SECONDS);
    let passthrough_seconds = seconds_arg("--passthrough-seconds", DEFAULT_PASSTHROUGH_SECONDS);
    let total_seconds = seconds_arg("--total-seconds", DEFAULT_TOTAL_SECONDS);

    let conn = Connection::connect_to_env().expect("no compositor answered");
    let (globals, mut queue) = registry_queue_init(&conn).expect("the registry could not be read");
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor is missing");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("xdg_wm_base is missing");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm is missing");

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    window.set_title("yappyink T007 interaction probe");
    window.set_app_id("dev.yappyink.Experiment");
    if surface_mode == SurfaceMode::Maximized {
        window.set_maximized();
    }
    window.commit();

    let pool = SlotPool::new(1920 * 1080 * 4, &shm).expect("the shm pool could not be created");

    let mut app = Probe {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        seat_state: SeatState::new(&globals, &qh),
        shm,
        pool,
        compositor,
        window,
        keyboard: None,
        pointer: None,
        width: 0,
        height: 0,
        configured: false,
        needs_redraw: false,
        mode: Mode::Draw,
        committed: Vec::new(),
        transient: None,
        button_held: false,
        exit: false,
        leak_reported: false,
        drop_fullscreen: false,
    };

    // One roundtrip so the registry has delivered the outputs and the first
    // configure has arrived.
    queue
        .roundtrip(&mut app)
        .expect("the compositor did not answer");

    // xdg-shell offers no way to place a floating window on a chosen output
    // (E002, finding 2). `set_fullscreen(Some(output))` is the only call that
    // names one, so the probe goes fullscreen on the target first and drops
    // back to floating once the compositor confirms. Whether Mutter then keeps
    // the window on that output is itself part of what this run measures.
    let wanted = string_arg("--on-output");
    if let Some(wanted) = &wanted {
        let target = app.output_state.outputs().find(|output| {
            app.output_state
                .info(output)
                .and_then(|i| i.name)
                .as_deref()
                == Some(wanted.as_str())
        });
        match target {
            Some(output) => {
                eprintln!("[note  ] steering onto output {wanted} via a brief fullscreen");
                app.window.set_fullscreen(Some(&output));
                app.window.commit();
                if surface_mode == SurfaceMode::Windowed {
                    app.drop_fullscreen = true;
                }
            }
            None => {
                let names: Vec<String> = app
                    .output_state
                    .outputs()
                    .filter_map(|o| app.output_state.info(&o).and_then(|i| i.name))
                    .collect();
                eprintln!("[note  ] no output named {wanted}. Available: {names:?}");
            }
        }
    } else if surface_mode == SurfaceMode::Fullscreen {
        app.window.set_fullscreen(None);
        app.window.commit();
    }

    print_preamble(
        surface_mode,
        draw_seconds,
        passthrough_seconds,
        total_seconds,
    );

    let started = Instant::now();
    let mut mode_since = Instant::now();
    let mut announced = false;

    while !app.exit {
        std::thread::sleep(Duration::from_millis(16));
        queue
            .roundtrip(&mut app)
            .expect("the compositor stopped responding");

        if app.configured && !announced {
            eprintln!(
                "\n[{:>3}s] {} - draw with the left mouse button. Every press is logged\n\
                 \x20        with its position, so a press on a fully transparent pixel shows\n\
                 \x20        up here even if you doubt what you saw.",
                started.elapsed().as_secs(),
                app.mode.as_str()
            );
            announced = true;
        }

        let dwell = mode_since.elapsed().as_secs();
        let due = match app.mode {
            Mode::Draw => dwell >= draw_seconds,
            Mode::PassThrough => dwell >= passthrough_seconds,
        };
        if app.configured && due {
            let next = match app.mode {
                Mode::Draw => Mode::PassThrough,
                Mode::PassThrough => Mode::Draw,
            };
            app.set_mode(next, started.elapsed().as_secs());
            mode_since = Instant::now();
        }

        if app.needs_redraw {
            app.draw();
        }

        if started.elapsed().as_secs() >= total_seconds {
            app.exit = true;
        }
    }

    drop(app.window);
    conn.roundtrip().ok();
    eprintln!(
        "\n[withdrawn] The surface is destroyed. Check that the desktop behaves\n\
         \x20        normally, with no invisible region swallowing clicks."
    );
}

fn print_preamble(mode: SurfaceMode, draw: u64, passthrough: u64, total: u64) {
    eprintln!(
        "yappyink T007 interaction probe\n\
         surface mode: {mode:?}\n\n\
         SET-UP, and it matters: press Alt+Space and choose \"Always on Top\".\n\
         Mutter offers that only for a floating window, and without it the ink is\n\
         covered the moment you click anything (E002, finding 4).\n\n\
         The mode flips on its own: {draw}s in DRAW, then {passthrough}s in\n\
         PASS-THROUGH, repeating for {total}s. It cycles automatically so that\n\
         switching does not depend on us keeping keyboard focus.\n\n\
         Keys, while the overlay has focus: d = Draw, p = PassThrough,\n\
         c = clear the ink, Esc = quit.\n\n\
         WHAT TO WATCH:\n\
         \x20 - In DRAW (cyan corner): can you draw anywhere, including over empty\n\
         \x20   transparent areas? Do clicks stay off the application underneath?\n\
         \x20 - In PASS-THROUGH (amber corner): does the ink stay visible while you\n\
         \x20   click, type and scroll in the application underneath?\n\
         \x20 - THE TRAP: hold the mouse button down and keep holding it while the\n\
         \x20   mode flips to PASS-THROUGH. The half-drawn stroke must vanish, and\n\
         \x20   the application underneath must NOT receive a click from it.\n"
    );
}

struct Probe {
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    shm: Shm,
    pool: SlotPool,
    compositor: CompositorState,
    window: Window,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    width: u32,
    height: u32,
    configured: bool,
    needs_redraw: bool,
    mode: Mode,
    /// Strokes finished by releasing the button.
    committed: Vec<Vec<(f64, f64)>>,
    /// The stroke being drawn right now. Not committed, and cancelled outright
    /// if the mode changes underneath it.
    transient: Option<Vec<(f64, f64)>>,
    button_held: bool,
    exit: bool,
    /// Set once a pointer press arrives while in PassThrough, which would mean
    /// the empty input region is not being honoured.
    leak_reported: bool,
    /// True while the window is still fullscreen purely to steer which output
    /// it lands on. Cleared after the first configure drops it back to
    /// floating.
    drop_fullscreen: bool,
}

impl Probe {
    fn set_mode(&mut self, mode: Mode, at: u64) {
        if self.mode == mode {
            return;
        }

        // FR-018: a gesture in flight is cancelled, never committed, and never
        // handed to the application underneath.
        if let Some(points) = self.transient.take() {
            eprintln!(
                "[{at:>3}s] CANCELLED a stroke of {} points that was mid-draw when the mode\n\
                 \x20        changed. It must not appear, and must not become a click below.",
                points.len()
            );
        }
        if self.button_held {
            eprintln!(
                "[{at:>3}s] NOTE: the button was still held at the switch. Watch whether the\n\
                 \x20        application underneath reacts when you release it."
            );
        }

        self.mode = mode;
        match mode {
            // `None` restores the default input region: the whole surface,
            // transparent pixels included.
            Mode::Draw => self.window.wl_surface().set_input_region(None),
            Mode::PassThrough => {
                let region = Region::new(&self.compositor).expect("wl_region failed");
                // A region with no rectangles added is empty.
                self.window
                    .wl_surface()
                    .set_input_region(Some(region.wl_region()));
            }
        }
        self.window.commit();
        self.needs_redraw = true;

        eprintln!("\n[{at:>3}s] -> {}", mode.as_str());
    }

    fn draw(&mut self) {
        let (width, height) = (self.width, self.height);
        if width == 0 || height == 0 {
            return;
        }
        self.needs_redraw = false;
        let stride = width as i32 * 4;

        let (buffer, canvas) = self
            .pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .expect("the buffer could not be created");

        // Transparent everywhere. Premultiplied alpha, so a transparent pixel is
        // all zeroes and cannot leave a dark fringe.
        canvas.fill(0);

        // Byte order for Argb8888 on a little-endian host is B, G, R, A.
        let magenta = [0xFFu8, 0x00, 0xFF, 0xFF];
        let cyan = [0xFFu8, 0xFF, 0x00, 0xFF];
        let amber = [0x00u8, 0xA5, 0xFF, 0xFF];

        // A corner marker, coloured by mode: cyan in Draw, amber in
        // PassThrough. With the log hidden behind the overlay, this is how you
        // know which mode you are in.
        let marker = match self.mode {
            Mode::Draw => cyan,
            Mode::PassThrough => amber,
        };
        for y in 20..90i64 {
            for x in 20..90i64 {
                put(canvas, width, height, x, y, marker);
            }
        }

        for stroke in &self.committed {
            draw_stroke(canvas, width, height, stroke, magenta);
        }
        if let Some(stroke) = &self.transient {
            draw_stroke(canvas, width, height, stroke, magenta);
        }

        let surface = self.window.wl_surface();
        surface.damage_buffer(0, 0, width as i32, height as i32);
        buffer
            .attach_to(surface)
            .expect("the buffer could not be attached");
        self.window.commit();
    }
}

fn put(canvas: &mut [u8], width: u32, height: u32, x: i64, y: i64, colour: [u8; 4]) {
    if x < 0 || y < 0 || x >= width as i64 || y >= height as i64 {
        return;
    }
    let offset = ((y * width as i64 + x) * 4) as usize;
    canvas[offset..offset + 4].copy_from_slice(&colour);
}

/// Draws a polyline as a chain of filled discs.
///
/// Crude on purpose. Stroke quality is a renderer question (T014, T015) and
/// nothing here should be carried into that work.
fn draw_stroke(canvas: &mut [u8], width: u32, height: u32, points: &[(f64, f64)], colour: [u8; 4]) {
    const RADIUS: i64 = 3;
    let mut blob = |x: f64, y: f64| {
        let (cx, cy) = (x as i64, y as i64);
        for dy in -RADIUS..=RADIUS {
            for dx in -RADIUS..=RADIUS {
                if dx * dx + dy * dy <= RADIUS * RADIUS {
                    put(canvas, width, height, cx + dx, cy + dy, colour);
                }
            }
        }
    };

    match points {
        [] => {}
        // A single sample is a dot, which FR-007 calls a valid pen gesture.
        [only] => blob(only.0, only.1),
        _ => {
            for pair in points.windows(2) {
                let (x0, y0) = pair[0];
                let (x1, y1) = pair[1];
                let distance = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
                let steps = distance.ceil().max(1.0) as i64;
                for step in 0..=steps {
                    let t = step as f64 / steps as f64;
                    blob(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
                }
            }
        }
    }
}

impl PointerHandler for Probe {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if self.mode == Mode::PassThrough
                && !self.leak_reported
                && matches!(event.kind, PointerEventKind::Press { .. })
            {
                self.leak_reported = true;
                eprintln!(
                    "[!!] A POINTER PRESS ARRIVED WHILE IN PASS-THROUGH. The empty input\n\
                     \x20    region is not being honoured, which breaks FR-003."
                );
            }

            let (x, y) = event.position;
            match event.kind {
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                    self.button_held = true;
                    if self.mode == Mode::Draw {
                        eprintln!("[press ] at ({x:.0}, {y:.0}) - began a stroke");
                        self.transient = Some(vec![(x, y)]);
                        self.needs_redraw = true;
                    }
                }
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                    self.button_held = false;
                    if let Some(points) = self.transient.take() {
                        eprintln!("[commit] a stroke of {} point(s)", points.len());
                        self.committed.push(points);
                        self.needs_redraw = true;
                    }
                }
                PointerEventKind::Motion { .. } => {
                    if let Some(points) = self.transient.as_mut() {
                        points.push((x, y));
                        self.needs_redraw = true;
                    }
                }
                _ => {}
            }
        }
    }
}

impl KeyboardHandler for Probe {
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
        eprintln!("[focus ] the overlay gained keyboard focus");
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
        // focus. This is also why the probe cycles modes on a timer.
        eprintln!("[focus ] the overlay lost keyboard focus");
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        match event.keysym {
            Keysym::Escape => {
                eprintln!("[key   ] Escape - quitting");
                self.exit = true;
            }
            Keysym::d | Keysym::D => self.set_mode(Mode::Draw, 0),
            Keysym::p | Keysym::P => self.set_mode(Mode::PassThrough, 0),
            Keysym::c | Keysym::C => {
                eprintln!("[key   ] c - cleared {} stroke(s)", self.committed.len());
                self.committed.clear();
                self.transient = None;
                self.needs_redraw = true;
            }
            _ => {}
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

impl SeatHandler for Probe {
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

impl CompositorHandler for Probe {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        // Recorded, not handled: scale correctness is FR-012 and T018. A probe
        // that silently ignored it could mislead about coordinates.
        eprintln!("[note  ] the compositor set a scale factor of {new_factor}");
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
        _output: &wl_output::WlOutput,
    ) {
        eprintln!("[note  ] the surface entered an output");
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

impl WindowHandler for Probe {
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let (width, height) = match configure.new_size {
            (Some(w), Some(h)) => (w.get(), h.get()),
            _ => (1280, 720),
        };
        if !self.configured {
            eprintln!(
                "[note  ] first configure: {width}x{height}, state {:?}, decorations {:?}",
                configure.state, configure.decoration_mode
            );
        }
        self.width = width;
        self.height = height;
        self.configured = true;
        self.needs_redraw = true;

        if self.drop_fullscreen && configure.state.contains(WindowState::FULLSCREEN) {
            self.drop_fullscreen = false;
            eprintln!("[note  ] leaving fullscreen, which was only used to choose the output");
            self.window.unset_fullscreen();
            self.window.commit();
        }
    }
}

impl OutputHandler for Probe {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for Probe {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Probe {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_registry!(Probe);
smithay_client_toolkit::delegate_dispatch2!(Probe);
