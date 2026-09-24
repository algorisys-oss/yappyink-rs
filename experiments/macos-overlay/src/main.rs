//! T004 step 1: can a borderless, high-level `NSWindow` meet the overlay
//! contract on macOS?
//!
//! Throwaway, like `experiments/gnome-xdg-shell` and
//! `experiments/windows-layered`. It exists to be run on a real Mac and produce
//! evidence. Nothing in `ink-*` may import it.
//!
//! # Read this before trusting anything here
//!
//! **This is the least verified code in the repository.** It type-checks for
//! `aarch64-apple-darwin` and CI compiles it on a macOS runner. Nobody involved
//! in writing it has access to a Mac, so it has never been launched, and a
//! compiler cannot tell you whether a window appears. Treat every claim below
//! as a *hypothesis to test*, not a description of behaviour.
//!
//! # What it is trying to find out
//!
//! - **Q1 (FR-001)** Does a borderless window at screen-saver level show
//!   through to the applications underneath while they keep updating?
//! - **Q2 (FR-002)** Does the view receive clicks on fully transparent pixels?
//! - **Q3 (FR-003)** Does `setIgnoresMouseEvents:` give real pass-through, with
//!   AppKit routing the click to the application below and nothing forwarded or
//!   synthesised by us?
//! - **Q4 (FR-018)** With the button held, does switching mode leak a click?
//! - **Q5** Can we choose which screen? `NSScreen` says yes, unlike GNOME.
//! - **Q7** What happens over a **fullscreen** application and when the user
//!   switches **Spaces**? This is the macOS-specific question and the one most
//!   likely to produce a nasty answer. `NSWindowCollectionBehavior` is set to
//!   join all Spaces and to be a fullscreen auxiliary; whether that is honoured
//!   is exactly what needs watching.
//!
//! # A question this probe deliberately does not answer
//!
//! **FR-005's global shortcut.** On Windows `RegisterHotKey` is one call. macOS
//! has no equivalent that works without either Carbon's `RegisterEventHotKey`
//! or an accessibility-permission grant for a global event monitor, and a
//! permission prompt is a product decision, not a detail.
//!
//! So the controls here are ordinary key presses, which forces the window to be
//! able to become key — and *that* is the tension worth recording: a window
//! that accepts keys takes focus from the application being annotated, which is
//! the opposite of what `WS_EX_NOACTIVATE` buys on Windows. Whether macOS can
//! have both is an open question and the reason this is a probe.

fn main() -> std::process::ExitCode {
    #[cfg(target_os = "macos")]
    {
        macos_probe::run()
    }
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!(
            "This experiment measures macOS behaviour and only builds there. It is \
             compiled on {} as an empty program so that `cargo build --workspace` \
             keeps working on the development machine.",
            std::env::consts::OS
        );
        eprintln!("Build it on a Mac with: cargo run -p exp-macos-overlay");
        std::process::ExitCode::from(2)
    }
}

#[cfg(target_os = "macos")]
mod macos_probe {
    use std::cell::RefCell;
    use std::process::ExitCode;

    use objc2::rc::Retained;
    use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSEvent,
        NSRectFill, NSScreen, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
    };
    use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};

    /// The window level to sit at.
    ///
    /// `kCGScreenSaverWindowLevel`. Chosen over floating (3) or status (25)
    /// because the overlay must be above ordinary windows *and* above menus,
    /// and the screen saver level is the highest ordinary applications are
    /// expected to use. Whether it survives a fullscreen application is Q7 and
    /// is unknown.
    const SCREEN_SAVER_LEVEL: isize = 1000;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Mode {
        Draw,
        PassThrough,
    }

    /// Everything the view draws, and the strokes it has collected.
    pub struct ProbeState {
        mode: RefCell<Mode>,
        strokes: RefCell<Vec<Vec<NSPoint>>>,
        drawing: RefCell<Option<Vec<NSPoint>>>,
    }

    define_class!(
        #[unsafe(super(NSView))]
        #[thread_kind = MainThreadOnly]
        #[name = "YappyinkProbeView"]
        #[ivars = ProbeState]
        struct ProbeView;

        impl ProbeView {
            /// Painted by hand rather than with a layer, so that what reaches
            /// the screen is unambiguous: if nothing appears, nothing was
            /// drawn, rather than something being drawn into a layer that was
            /// never composited.
            #[unsafe(method(drawRect:))]
            fn draw_rect(&self, _dirty: NSRect) {
                let bounds = self.bounds();
                let state = self.ivars();
                let mode = *state.mode.borrow();

                // A frame, so the overlay can be found at all. The first
                // Wayland build was fully transparent and therefore unusable;
                // see docs/learning.md §2.
                let frame_colour = match mode {
                    Mode::Draw => NSColor::colorWithSRGBRed_green_blue_alpha(0.0, 0.8, 0.8, 0.75),
                    Mode::PassThrough => NSColor::colorWithSRGBRed_green_blue_alpha(0.9, 0.6, 0.1, 0.75),
                };
                frame_colour.set();
                let thickness = 6.0;
                for rect in [
                    NSRect::new(
                        NSPoint::new(0.0, 0.0),
                        NSSize::new(bounds.size.width, thickness),
                    ),
                    NSRect::new(
                        NSPoint::new(0.0, bounds.size.height - thickness),
                        NSSize::new(bounds.size.width, thickness),
                    ),
                    NSRect::new(
                        NSPoint::new(0.0, 0.0),
                        NSSize::new(thickness, bounds.size.height),
                    ),
                    NSRect::new(
                        NSPoint::new(bounds.size.width - thickness, 0.0),
                        NSSize::new(thickness, bounds.size.height),
                    ),
                ] {
                    NSRectFill(rect);
                }

                // Mode badge, top-left, matching the Wayland and Windows
                // builds so all three can be described the same way. macOS
                // origin is bottom-left, hence the subtraction.
                let badge = NSRect::new(
                    NSPoint::new(12.0, bounds.size.height - 60.0),
                    NSSize::new(48.0, 48.0),
                );
                NSRectFill(badge);

                let ink = NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 0.0, 1.0, 1.0);
                ink.set();
                let committed = state.strokes.borrow();
                let in_flight = state.drawing.borrow();
                for stroke in committed.iter().chain(in_flight.iter()) {
                    for point in stroke {
                        // Crude on purpose: this measures the platform, and
                        // ink-render already rasterises properly for when the
                        // adapter is real.
                        NSRectFill(NSRect::new(
                            NSPoint::new(point.x - 2.0, point.y - 2.0),
                            NSSize::new(5.0, 5.0),
                        ));
                    }
                }
            }

            /// Q2: this firing at all is the answer, because the pixels under
            /// the pointer are fully transparent.
            #[unsafe(method(mouseDown:))]
            fn mouse_down(&self, event: &NSEvent) {
                let point = self.convert_from_window(event);
                *self.ivars().drawing.borrow_mut() = Some(vec![point]);
                self.setNeedsDisplay(true);
            }

            #[unsafe(method(mouseDragged:))]
            fn mouse_dragged(&self, event: &NSEvent) {
                let point = self.convert_from_window(event);
                if let Some(stroke) = self.ivars().drawing.borrow_mut().as_mut() {
                    stroke.push(point);
                }
                self.setNeedsDisplay(true);
            }

            #[unsafe(method(mouseUp:))]
            fn mouse_up(&self, _event: &NSEvent) {
                let taken = self.ivars().drawing.borrow_mut().take();
                if let Some(stroke) = taken {
                    self.ivars().strokes.borrow_mut().push(stroke);
                }
                self.setNeedsDisplay(true);
            }

            #[unsafe(method(keyDown:))]
            fn key_down(&self, event: &NSEvent) {
                let characters = event.charactersIgnoringModifiers();
                let typed = characters.map(|c| c.to_string()).unwrap_or_default();
                match typed.as_str() {
                    "d" => self.toggle_mode(),
                    "q" => {
                        let mtm = MainThreadMarker::from(self);
                        report(self);
                        NSApplication::sharedApplication(mtm).terminate(None);
                    }
                    _ => {}
                }
            }

            /// Without this the borderless window can never be key and no key
            /// press arrives. It is also exactly the focus-stealing this
            /// experiment is meant to measure the cost of.
            #[unsafe(method(acceptsFirstResponder))]
            fn accepts_first_responder(&self) -> bool {
                true
            }
        }
    );

    impl ProbeView {
        fn convert_from_window(&self, event: &NSEvent) -> NSPoint {
            let in_window = event.locationInWindow();
            self.convertPoint_fromView(in_window, None)
        }

        fn toggle_mode(&self) {
            let state = self.ivars();

            // FR-018: a stroke in flight is abandoned rather than committed.
            let interrupted = state.drawing.borrow_mut().take().is_some();

            let next = {
                let mut mode = state.mode.borrow_mut();
                *mode = match *mode {
                    Mode::Draw => Mode::PassThrough,
                    Mode::PassThrough => Mode::Draw,
                };
                *mode
            };

            if let Some(window) = self.window() {
                // The whole pass-through mechanism. AppKit stops hit-testing
                // the window and routes the event to whatever is underneath,
                // so nothing is forwarded or synthesised, which is what FR-003
                // requires and AGENTS.md forbids faking.
                window.setIgnoresMouseEvents(next == Mode::PassThrough);
            }

            println!("[mode] {next:?}");
            if interrupted {
                println!(
                    "[mode] a stroke was in flight and was discarded. Check that nothing \
                     underneath was clicked when you released."
                );
            }
            self.setNeedsDisplay(true);
        }
    }

    fn report(view: &ProbeView) {
        let strokes = view.ivars().strokes.borrow().len();
        println!();
        println!("[exit] {strokes} stroke(s) were drawn.");
        println!("[exit] Record the answers in docs/evidence/, including whatever failed.");
    }

    pub fn run() -> ExitCode {
        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("[failed] AppKit must be driven from the main thread");
            return ExitCode::FAILURE;
        };

        let app = NSApplication::sharedApplication(mtm);
        // Accessory: no Dock icon, no menu bar, and the application does not
        // become active on launch. The nearest macOS has to WS_EX_NOACTIVATE,
        // though see the module documentation: accepting keys undoes part of
        // it, and reconciling the two is an open question.
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

        let screens = NSScreen::screens(mtm);
        if screens.is_empty() {
            eprintln!("[failed] no screens were reported, which should be impossible");
            return ExitCode::FAILURE;
        }
        println!("[screens] {} found", screens.len());
        for (index, screen) in screens.iter().enumerate() {
            let frame = screen.frame();
            let scale = screen.backingScaleFactor();
            println!(
                "[screens]   {index}: {}x{} at ({},{}), backing scale {scale}",
                frame.size.width, frame.size.height, frame.origin.x, frame.origin.y
            );
        }

        // Q5: choosing an output, which GNOME cannot do by any route.
        let wanted: usize = std::env::args()
            .nth(1)
            .and_then(|a| a.parse().ok())
            .unwrap_or(0);
        // NSArray is not a slice, so it is walked rather than indexed.
        let all: Vec<_> = screens.iter().collect();
        let screen = all.get(wanted).unwrap_or_else(|| &all[0]);
        let frame = screen.frame();
        println!(
            "[screens] asked for {wanted}, using {}x{} at ({},{})",
            frame.size.width, frame.size.height, frame.origin.x, frame.origin.y
        );

        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            )
        };

        // Transparency is three separate settings and all of them are needed.
        // Leaving the background opaque is the usual reason a "transparent"
        // AppKit window turns out grey.
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setLevel(SCREEN_SAVER_LEVEL);
        window.setIgnoresMouseEvents(false);
        // Q7. Joining all Spaces and being a fullscreen auxiliary is the
        // documented way to stay visible across Spaces and over a fullscreen
        // application. Whether macOS honours it for a window at this level is
        // the single most uncertain thing here.
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary,
        );
        // Otherwise the window is released when closed and the pointer to it
        // dangles. Marked unsafe by the bindings for exactly that reason.
        unsafe { window.setReleasedWhenClosed(false) };

        let view = create_view(mtm, frame);
        window.setContentView(Some(&view));
        window.makeFirstResponder(Some(&view));
        window.makeKeyAndOrderFront(None);

        print_instructions();
        app.run();
        ExitCode::SUCCESS
    }

    fn create_view(mtm: MainThreadMarker, frame: NSRect) -> Retained<ProbeView> {
        let state = ProbeState {
            mode: RefCell::new(Mode::Draw),
            strokes: RefCell::new(Vec::new()),
            drawing: RefCell::new(None),
        };
        let view = ProbeView::alloc(mtm).set_ivars(state);
        let bounds = NSRect::new(NSPoint::new(0.0, 0.0), frame.size);
        unsafe { msg_send![super(view), initWithFrame: bounds] }
    }

    fn print_instructions() {
        println!();
        println!("A translucent frame should now be above everything, with a filled");
        println!("square in the top-left corner: cyan in draw mode, amber in pass-through.");
        println!("The inside is fully transparent.");
        println!();
        println!("  d   switch between draw and pass-through");
        println!("  q   quit");
        println!();
        println!("What to check, and please record failures as carefully as successes:");
        println!("  Q1  Is the desktop underneath visible through it and still updating?");
        println!("  Q2  In draw mode, does dragging inside the empty middle draw, rather");
        println!("      than clicking whatever is underneath?");
        println!("  Q3  In pass-through, does clicking reach the window below while the");
        println!("      ink stays visible?");
        println!("  Q4  Hold the button down, press d, then release. Does anything");
        println!("      underneath get clicked? It must not.");
        println!("  Q5  Pass a screen number as an argument and check it appears there.");
        println!("  Q7  Put an application into fullscreen, and switch Spaces. Does the");
        println!("      overlay stay? This is the one most likely to fail.");
        println!();
        println!("Also worth noting: whether the application you were using lost focus");
        println!("when the overlay appeared. It should not have, and the fact that this");
        println!("window accepts keys makes that doubtful.");
        println!();
    }
}
