//! The AppKit overlay window.
//!
//! The part that cannot be tested without a Mac, kept as small as it can be:
//! the key translation lives in [`crate::keys`] and the bitmap arithmetic in
//! [`crate::surface`], and both run their tests on any machine.
//!
//! # What is here
//!
//! Drawing, every tool, mode switching, undo and redo, save and load, and the
//! full toolbar, which is the same `ink-ui` chrome the other two backends draw.
//!
//! # What is not, and why
//!
//! - **No global shortcut.** There is no `RegisterHotKey` here. The options are
//!   Carbon's `RegisterEventHotKey` or an accessibility-permission grant, and a
//!   permission prompt is a product decision that needs its own ADR. The
//!   consequence is real: in PassThrough this window gets no input, so **the
//!   only way back is the terminal that launched it**. That is worse than
//!   either other platform and it is not hidden.
//! - **No input methods.** Latin only, like the Windows backend.
//! - **One screen.** `NSScreen` makes choosing easy and the adapter does not.
//!
//! # The focus tension, which is unresolved
//!
//! `Accessory` keeps the application out of the Dock and stops it activating on
//! launch. But a window that accepts a key press must be able to become key,
//! and becoming key takes focus from whatever is being annotated. Windows
//! escapes this by taking no keyboard at all and putting every control on a hot
//! key; macOS has no free equivalent. See `docs/adr/ADR-006-macos-bindings.md`.
//!
//! **Nothing here has been run.** Nobody on this project has a Mac.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;

use ink_app::{Action, Controller, Effect, Mode, PlatformEvent, Preview, TransitionId, keymap};
use ink_core::{IdSource, LogicalPoint, LogicalSize, Object, OutputId, Session, Shape, Style};
use ink_platform::PlatformError;
use ink_render::{Canvas, Painter, Scale};

use objc2::rc::Retained;
use objc2::{MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSEvent,
    NSGraphicsContext, NSScreen, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect as CFCGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGContext, CGDataProvider, CGImage,
    CGImageAlphaInfo, CGImageByteOrderInfo,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};

use crate::keys;
use crate::surface;

/// `kCGScreenSaverWindowLevel`.
///
/// Above ordinary windows and above the menu bar. Whether it survives a
/// fullscreen application is the most uncertain thing in this backend.
const SCREEN_SAVER_LEVEL: isize = 1000;

/// How the overlay is asked to start.
pub struct OverlayConfig {
    /// Requested size in logical units. The screen wins if it is smaller.
    pub size: LogicalSize,
}

struct Overlay {
    window: Retained<NSWindow>,
    view: Option<Retained<OverlayView>>,
    /// Physical pixels.
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    scale: Scale,

    controller: Controller,
    session: Session,
    ids: IdSource,
    painter: Painter,

    hidden: bool,
    pass_through: bool,
    last_pointer: Option<LogicalPoint>,
}

thread_local! {
    static OVERLAY: RefCell<Option<Overlay>> = const { RefCell::new(None) };
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "YappyinkOverlayView"]
    #[ivars = ()]
    struct OverlayView;

    impl OverlayView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            paint(self);
        }

        /// Top-left origin, so the view agrees with `Canvas`, which is indexed
        /// from the top. Without this every pointer coordinate is mirrored
        /// vertically and the toolbar is unreachable where it is drawn.
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            dispatch(PlatformEvent::PointerDown {
                at: self.logical(event),
            });
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            dispatch(PlatformEvent::PointerMoved {
                at: self.logical(event),
            });
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            dispatch(PlatformEvent::PointerMoved {
                at: self.logical(event),
            });
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            dispatch(PlatformEvent::PointerUp {
                at: self.logical(event),
            });
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            self.on_key(event);
        }

        /// Without this the borderless window can never be key and no key
        /// press arrives at all. It is also the focus-stealing the module
        /// documentation describes; both halves are true at once.
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }
    }
);

impl OverlayView {
    /// Pointer position in logical units.
    ///
    /// The view is flipped, so `convertPoint:fromView:` already gives top-left
    /// coordinates in points. Points *are* logical units here: AppKit reports
    /// a Retina screen as the same number of points and a backing scale of 2,
    /// which is the opposite of Win32, where coordinates are physical pixels
    /// and have to be divided. Dividing again here would halve every
    /// coordinate on a Retina display.
    fn logical(&self, event: &NSEvent) -> LogicalPoint {
        let in_window = event.locationInWindow();
        let local = self.convertPoint_fromView(in_window, None);
        LogicalPoint::new(local.x, local.y)
            .unwrap_or_else(|| LogicalPoint::new(0.0, 0.0).expect("origin is finite"))
    }

    fn on_key(&self, event: &NSEvent) {
        let Some(characters) = event.charactersIgnoringModifiers() else {
            return;
        };
        let text = characters.to_string();
        let Some(first) = text.chars().next() else {
            return;
        };

        let editing = OVERLAY.with(|slot| {
            slot.borrow()
                .as_ref()
                .is_some_and(|overlay| overlay.controller.is_editing_text())
        });

        let Some(key) = keys::translate(first) else {
            return;
        };

        if editing {
            // FR-023: the editor owns the keyboard, so a key is a character
            // rather than a shortcut. Only the keys that cannot be characters
            // keep a meaning.
            if let Some(action) = keymap::editing(key) {
                act(action);
            } else if let Some(character) = key.as_char() {
                act(Action::TypeText(character));
            }
            return;
        }

        if keymap::quits(key) {
            let mtm = MainThreadMarker::from(self);
            NSApplication::sharedApplication(mtm).terminate(None);
            return;
        }
        if let Some(action) = keymap::command(key) {
            if action == Action::ShowWindowMenu {
                // Wayland needs this to ask for Always on Top. This window is
                // already at screen-saver level, so the key explains itself
                // rather than being silently missing on one platform.
                eprintln!("[window] this overlay is already above other windows");
                return;
            }
            act(action);
        }
    }
}

/// Runs the overlay until it is asked to quit.
pub fn run(config: OverlayConfig) -> Result<Session, PlatformError> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Err(PlatformError::unsupported(
            "start the overlay",
            "AppKit must be driven from the main thread",
        ));
    };

    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon, no menu bar, and no activation on launch: the nearest
    // macOS has to not stealing focus. See the module documentation for why
    // that is only half true here.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let screens = NSScreen::screens(mtm);
    let all: Vec<_> = screens.iter().collect();
    let Some(screen) = all.first() else {
        return Err(PlatformError::unsupported(
            "find a screen",
            "NSScreen reported none, which should be impossible",
        ));
    };
    let frame = screen.frame();
    let backing = surface::scale_for_backing(screen.backingScaleFactor());
    eprintln!(
        "[output] screen {}x{} points, backing scale {backing}",
        frame.size.width, frame.size.height
    );

    let logical_width = frame.size.width.min(config.size.width().max(1.0));
    let logical_height = frame.size.height.min(config.size.height().max(1.0));
    let content = NSRect::new(
        NSPoint::new(frame.origin.x, frame.origin.y),
        NSSize::new(logical_width, logical_height),
    );

    // The bitmap is in physical pixels; the document is in points. The two
    // differ by the backing scale on a Retina display, and conflating them
    // draws everything at half size.
    let width = (logical_width * backing).round().max(1.0) as u32;
    let height = (logical_height * backing).round().max(1.0) as u32;

    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            content,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };

    // Transparency is three separate settings and all three are needed.
    // Leaving the background opaque is the usual reason an AppKit window that
    // should be see-through comes out grey.
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setLevel(SCREEN_SAVER_LEVEL);
    window.setIgnoresMouseEvents(false);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary,
    );
    // Otherwise the window is released on close and the retained pointer here
    // dangles.
    unsafe { window.setReleasedWhenClosed(false) };

    let size = LogicalSize::new(logical_width, logical_height).ok_or_else(|| {
        PlatformError::unsupported("size the overlay", "the screen reported a size of zero")
    })?;
    let scale = Scale::new(backing).unwrap_or(Scale::ONE);

    let painter = match ink_render::text::TextFont::discover() {
        Ok(font) => {
            eprintln!("[font] {}", font.source().display());
            for fallback in font.fallbacks() {
                eprintln!("[font] fallback {}", fallback.display());
            }
            Painter::new().with_font(font)
        }
        Err(reason) => {
            eprintln!("[font] no usable font was found, so the text tool is unavailable: {reason}");
            Painter::new()
        }
    };

    OVERLAY.with(|slot| {
        *slot.borrow_mut() = Some(Overlay {
            window: window.clone(),
            view: None,
            width,
            height,
            pixels: vec![0u8; surface::buffer_len(width, height)],
            scale,
            controller: Controller::new(),
            session: Session::new(OutputId::new("primary"), size),
            ids: IdSource::starting_at(1),
            painter,
            hidden: false,
            pass_through: false,
            last_pointer: None,
        });
    });

    let view = make_view(mtm, NSRect::new(NSPoint::new(0.0, 0.0), content.size));
    window.setContentView(Some(&view));
    window.makeFirstResponder(Some(&view));
    window.makeKeyAndOrderFront(None);
    OVERLAY.with(|slot| {
        if let Some(overlay) = slot.borrow_mut().as_mut() {
            overlay.view = Some(view);
        }
    });

    print_orientation();
    app.run();
    finish()
}

fn make_view(mtm: MainThreadMarker, frame: NSRect) -> Retained<OverlayView> {
    let view = OverlayView::alloc(mtm).set_ivars(());
    unsafe { msg_send![super(view), initWithFrame: frame] }
}

fn finish() -> Result<Session, PlatformError> {
    OVERLAY.with(|slot| {
        slot.borrow_mut()
            .take()
            .map(|overlay| overlay.session)
            .ok_or_else(|| PlatformError::unsupported("close the overlay", "it was never created"))
    })
}

fn act(action: Action) {
    let effects = OVERLAY.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .map(|overlay| overlay.controller.act(action))
            .unwrap_or_default()
    });
    apply(effects);
    redraw();
}

fn dispatch(event: PlatformEvent) {
    let effects = OVERLAY.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(overlay) = borrowed.as_mut() else {
            return Vec::new();
        };
        if let PlatformEvent::PointerDown { at }
        | PlatformEvent::PointerMoved { at }
        | PlatformEvent::PointerUp { at } = event
        {
            overlay.last_pointer = Some(at);
        }
        overlay.controller.handle(event)
    });
    apply(effects);
    redraw();
}

/// Asks AppKit for a repaint.
///
/// Unconditional rather than guarded by a "did anything change" test. Working
/// out exactly when the screen can change is what produced six separate
/// "correct in the model, absent on screen" bugs on the first backend
/// (`docs/learning.md` §1 and §12); AppKit coalesces these, so the cost of
/// simply always asking is small and the class of bug disappears.
fn redraw() {
    OVERLAY.with(|slot| {
        if let Some(overlay) = slot.borrow().as_ref()
            && let Some(view) = &overlay.view
        {
            view.setNeedsDisplay(true);
        }
    });
}

fn apply(effects: Vec<Effect>) {
    for effect in effects {
        OVERLAY.with(|slot| {
            let mut borrowed = slot.borrow_mut();
            let Some(overlay) = borrowed.as_mut() else {
                return;
            };
            match effect {
                Effect::ApplyMode { mode, transition } => apply_mode(overlay, mode, transition),
                Effect::WithdrawImmediately => {
                    // FR-018: no confirmation is awaited.
                    overlay.window.orderOut(None);
                    overlay.hidden = true;
                }
                Effect::CommitObject { shape, style } => {
                    let id = overlay.ids.next_id();
                    let output = overlay.session.document().output().clone();
                    if let Err(error) = overlay.session.add(Object::new(id, output, style, shape)) {
                        eprintln!("[rejected] {error}");
                    }
                }
                Effect::EraseAlong { path, radius } => {
                    match overlay.session.erase_along(&path, radius) {
                        Ok(0) => eprintln!("[erase] the sweep touched nothing"),
                        Ok(count) => {
                            eprintln!("[erase] removed {count} object(s) as one undoable action")
                        }
                        Err(error) => eprintln!("[rejected] {error}"),
                    }
                }
                Effect::Undo => match overlay.session.undo() {
                    Ok(true) => {}
                    Ok(false) => eprintln!("[undo] nothing left to undo"),
                    Err(error) => eprintln!("[rejected] {error}"),
                },
                Effect::Redo => match overlay.session.redo() {
                    Ok(true) => {}
                    Ok(false) => eprintln!("[redo] nothing left to redo"),
                    Err(error) => eprintln!("[rejected] {error}"),
                },
                Effect::Clear => match overlay.session.clear() {
                    Ok(0) => eprintln!("[clear] there was nothing to clear"),
                    Ok(count) => {
                        eprintln!("[clear] removed {count} object(s); undo brings them back")
                    }
                    Err(error) => eprintln!("[rejected] {error}"),
                },
                Effect::GestureDiscarded { reason } => {
                    // FR-008: the user finished this one, so they are told why
                    // nothing appeared.
                    eprintln!("[discarded] {reason}");
                }
                Effect::GestureCancelled { points } => {
                    eprintln!("[cancelled] a gesture of {points} sample(s) was discarded");
                }
                Effect::MoveSelection { ids, dx, dy } => {
                    if let Err(error) = overlay.session.move_objects(&ids, dx, dy) {
                        eprintln!("[rejected] {error}");
                    }
                }
                Effect::ScaleSelection {
                    ids,
                    anchor,
                    sx,
                    sy,
                } => {
                    if let Err(error) = overlay.session.scale_objects(&ids, anchor, sx, sy) {
                        eprintln!("[rejected] {error}");
                    }
                }
                Effect::DeleteSelection { ids } => match overlay.session.delete(&ids) {
                    Ok(0) => {}
                    Ok(count) => {
                        eprintln!("[delete] removed {count} object(s); undo brings them back")
                    }
                    Err(error) => eprintln!("[rejected] {error}"),
                },
                Effect::Save => save(overlay),
                Effect::Load => load(overlay),
                Effect::Quit => {
                    overlay.window.close();
                    if let Some(mtm) = MainThreadMarker::new() {
                        NSApplication::sharedApplication(mtm).terminate(None);
                    }
                }
                // The window is borderless and covers the screen, so there is
                // no frame to drag and no compositor to ask for anything.
                Effect::ShowWindowMenu { .. }
                | Effect::BeginWindowDrag
                | Effect::BeginWindowResize => {}
                Effect::Faulted { error } => eprintln!("[failed {}] {error}", error.class()),
            }
        });
    }
}

fn apply_mode(overlay: &mut Overlay, mode: Mode, transition: TransitionId) {
    match mode {
        Mode::Hidden => {
            overlay.window.orderOut(None);
            overlay.hidden = true;
        }
        Mode::Draw | Mode::PassThrough | Mode::Parked => {
            if overlay.hidden {
                overlay.window.makeKeyAndOrderFront(None);
                overlay.hidden = false;
            }
            // The whole pass-through mechanism. AppKit stops hit-testing the
            // window and routes the event to whatever is underneath, so
            // nothing is forwarded or synthesised, which is what FR-003
            // requires and AGENTS.md forbids faking.
            let transparent = mode != Mode::Draw;
            overlay.window.setIgnoresMouseEvents(transparent);
            overlay.pass_through = transparent;
            if transparent {
                eprintln!(
                    "[mode] this window now ignores input, and there is no global shortcut on \
                     macOS, so the only way back is this terminal"
                );
            }
        }
    }

    eprintln!("[mode] {mode:?}");

    // Reported applied immediately: every call above is synchronous and there
    // is no round trip to wait for. The transition id still matters, so a
    // stale confirmation cannot overwrite a newer one.
    let effects = overlay
        .controller
        .handle(PlatformEvent::ModeApplied { transition });
    if !effects.is_empty() {
        eprintln!("[mode] {} follow-up effect(s) were produced", effects.len());
    }
}

fn save(overlay: &mut Overlay) {
    let path = match ink_storage::default_session_path() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("[save] {error}");
            return;
        }
    };
    match ink_storage::save(overlay.session.document(), &path) {
        Ok(()) => eprintln!(
            "[save] {} object(s) written to {}",
            overlay.session.document().len(),
            path.display()
        ),
        // FR-017: the previous file is untouched on any failure.
        Err(error) => eprintln!("[save {}] {error}", error.class()),
    }
}

fn load(overlay: &mut Overlay) {
    let path = match ink_storage::default_session_path() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("[load] {error}");
            return;
        }
    };
    match ink_storage::load(&path) {
        Ok(loaded) => {
            eprintln!(
                "[load] {} object(s) from {}",
                loaded.document.len(),
                path.display()
            );
            overlay.session.adopt(loaded.document);
        }
        Err(error) => {
            // Nothing is adopted unless the whole file validated, so what is
            // on screen is still exactly what it was.
            eprintln!("[load {}] {error}", error.class());
            eprintln!("[load] what is on screen is unchanged");
        }
    }
}

/// Paints the document and the chrome, then blits the result.
fn paint(view: &OverlayView) {
    let image = OVERLAY.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let overlay = borrowed.as_mut()?;

        let (width, height, scale) = (overlay.width, overlay.height, overlay.scale);
        debug_assert!(surface::layout_matches(overlay.pixels.len(), width, height));
        let mut canvas = Canvas::new(&mut overlay.pixels, width, height)?;
        canvas.clear();

        overlay
            .painter
            .paint(overlay.session.document(), &mut canvas, scale);

        // A gesture in flight, so a stroke appears while it is being drawn
        // rather than only on release.
        let built = match overlay.controller.preview() {
            Some(Preview::Stroke {
                points,
                kind,
                style,
            }) => Shape::stroke(kind, points.to_vec())
                .ok()
                .map(|shape| (shape, style)),
            Some(Preview::Shape { shape, style }) => Some((shape, style)),
            Some(Preview::Erase { path, radius }) => {
                Shape::stroke(ink_core::StrokeKind::Pen, path.to_vec())
                    .ok()
                    .and_then(|shape| {
                        let width = ink_core::Width::new(radius * 2.0)?;
                        let faint = ink_core::Opacity::new(0.35)?;
                        let grey = ink_core::Rgb::new(200, 200, 200);
                        Some((shape, Style::new(grey, width, faint)))
                    })
            }
            // Text in progress needs the caret and preedit underline, which
            // the Wayland adapter still computes inline. It arrives when that
            // moves into `ink-ui` too.
            Some(Preview::Text { .. }) | None => None,
        };
        if let Some((shape, style)) = built {
            let id = overlay.ids.next_id();
            let output = overlay.session.document().output().clone();
            let object = Object::new(id, output, style, shape);
            overlay.painter.paint_object(&object, &mut canvas, scale);
        }

        // The same chrome the other backends draw, from the same code.
        ink_ui::paint_chrome(
            &mut canvas,
            overlay.controller.mode(),
            overlay.controller.style(),
            overlay.controller.tool(),
        );
        ink_ui::paint_caret_hint(
            &mut canvas,
            &overlay.controller,
            overlay.last_pointer,
            scale,
        );
        ink_ui::paint_selection(
            &mut canvas,
            &overlay.controller,
            overlay.session.document(),
            &mut overlay.painter,
            scale,
        );
        if overlay.controller.toolbar_visible() {
            ink_ui::paint_toolbar(
                &mut canvas,
                overlay.controller.toolbar(),
                overlay.controller.tool(),
                overlay.controller.style().color,
                scale,
            );
            if let Some(corner) = overlay.controller.resize_corner() {
                ink_ui::paint_resize_corner(&mut canvas, corner, scale);
            }
            ink_ui::paint_swatches(&mut canvas, &overlay.controller, scale);
            if let Some(button) = overlay.controller.hovered_button() {
                ink_ui::paint_tooltip(
                    &mut canvas,
                    button,
                    overlay.controller.toolbar().bounds(),
                    scale,
                );
            }
        }

        make_image(&overlay.pixels, width, height)
    });

    let Some(image) = image else {
        return;
    };
    let Some(context) = NSGraphicsContext::currentContext() else {
        return;
    };
    let cg = context.CGContext();
    let bounds = view.bounds();
    let rect = CFCGRect::new(
        CGPoint::new(0.0, 0.0),
        CGSize::new(bounds.size.width, bounds.size.height),
    );

    // The view is flipped so that pointer coordinates match the canvas, but
    // Core Graphics draws images bottom-up regardless, so an image blitted
    // into a flipped context arrives upside down. Undoing the flip around the
    // image's own height is the standard correction, and it is bracketed so
    // nothing else inherits the transform.
    CGContext::save_g_state(Some(&cg));
    CGContext::translate_ctm(Some(&cg), 0.0, bounds.size.height);
    CGContext::scale_ctm(Some(&cg), 1.0, -1.0);
    CGContext::draw_image(Some(&cg), rect, Some(&image));
    CGContext::restore_g_state(Some(&cg));
}

/// Wraps a copy of the canvas in a `CGImage`.
///
/// The pixels are copied and handed to Core Graphics, which frees them through
/// the callback below. Lending it the live buffer instead would be faster and
/// would depend on Core Graphics not keeping the provider past this call,
/// which is not documented anywhere; a full-screen copy per frame is a poor
/// trade for a use-after-free that would appear only under load.
fn make_image(pixels: &[u8], width: u32, height: u32) -> Option<CFRetained<CGImage>> {
    let boxed: Box<[u8]> = pixels.to_vec().into_boxed_slice();
    let length = boxed.len();
    let raw = Box::into_raw(boxed);

    unsafe extern "C-unwind" fn release(_info: *mut c_void, data: NonNull<c_void>, size: usize) {
        // Reconstituted exactly as it was leaked, and dropped.
        let slice = std::ptr::slice_from_raw_parts_mut(data.as_ptr().cast::<u8>(), size);
        drop(unsafe { Box::from_raw(slice) });
    }

    let provider = unsafe {
        CGDataProvider::with_data(
            std::ptr::null_mut(),
            raw.cast::<c_void>().cast_const(),
            length,
            Some(release),
        )
    }?;

    let space = CGColorSpace::new_device_rgb()?;
    // Premultiplied, alpha first in the 32-bit word, little-endian byte order:
    // together that is the memory order B, G, R, A, which is exactly what
    // `ink-render` produces. Dropping the byte-order flag does not fail, it
    // swaps red and blue. See `surface`.
    let info = CGBitmapInfo(
        CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0,
    );

    unsafe {
        CGImage::new(
            width as usize,
            height as usize,
            surface::BITS_PER_COMPONENT,
            surface::BITS_PER_PIXEL,
            surface::bytes_per_row(width),
            Some(&space),
            info,
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
}

fn print_orientation() {
    eprintln!();
    eprintln!("The overlay is the thin outlined rectangle; everything inside it is");
    eprintln!("transparent. The corner square is amber in draw mode, cyan in pass-through.");
    eprintln!();
    eprintln!("  d / p / h   draw, pass through, hide");
    eprintln!("  1-9         tools, u/r undo and redo, w/o write and open, q quit");
    eprintln!();
    eprintln!("The toolbar is the same one the Linux and Windows builds draw.");
    eprintln!();
    eprintln!("WARNING: macOS has no global shortcut here. Once you switch to");
    eprintln!("pass-through this window stops receiving keys, and the only way back");
    eprintln!("is to quit from this terminal. See docs/adr/ADR-006-macos-bindings.md.");
    eprintln!();
}
