//! Live zoom on macOS (FR-029, ADR-008 as amended for macOS, T039).
//!
//! macOS has no public API that drives its own Zoom, so this draws the zoom
//! itself: ScreenCaptureKit captures a rectangle of the display around the
//! pointer, scaled to the whole display, and each frame's IOSurface is shown
//! in a borderless window just below the overlay. The owner chose this over
//! deferring to the system Zoom, knowing it costs a Screen Recording prompt.
//!
//! # What it is not
//!
//! Unlike GNOME and Windows, the compositor is not magnifying, so two things
//! differ and are recorded in ADR-008 rather than hidden:
//!
//! - **The ink is not magnified.** Both of our windows are excluded from the
//!   capture (otherwise the magnifier would capture itself), so the overlay
//!   stays at its own scale above the magnified picture.
//! - **Clicks in pass-through reach the real, unmagnified positions.** Moving
//!   them to match the picture would mean injecting input, which AGENTS.md
//!   forbids.
//!
//! Frames live only in the IOSurfaces ScreenCaptureKit recycles. Nothing is
//! stored, written or sent anywhere.
//!
//! # Threads
//!
//! ScreenCaptureKit's completion handlers run on queues of its choosing, and
//! the stream is created and started from one. Frames are delivered on the
//! main queue, so the layer is only ever touched on the main thread, where the
//! window and layer also live (thread-locals below). The stream itself is
//! kept in one mutex; see [`Capture`] for the single `Send` this asserts.
//!
//! **Never run.** Nobody on this project has a Mac.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, NSObject, ProtocolObject};
use objc2::{AnyThread, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSEvent, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGMainDisplayID;
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_core_video::CVPixelBufferGetIOSurface;
use objc2_foundation::{
    MainThreadMarker, NSArray, NSError, NSObjectProtocol, NSPoint, NSRect, NSSize, NSTimer,
};
use objc2_quartz_core::CALayer;
use objc2_screen_capture_kit::{
    SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutput,
    SCStreamOutputType, SCWindow,
};

use crate::surface;

/// One below the overlay's `kCGScreenSaverWindowLevel`, so the magnified
/// picture is under the ink and over everything else.
const MAGNIFIER_LEVEL: isize = 999;

/// How often the zoom follows the pointer. Changing the capture's source
/// rectangle is a reconfiguration, so this is kept below the frame rate.
const FOLLOW_SECONDS: f64 = 1.0 / 30.0;

/// The screen the overlay is on, in AppKit points, and its backing scale.
#[derive(Clone, Copy)]
pub struct Screen {
    pub origin: (f64, f64),
    pub size: (f64, f64),
    pub backing: f64,
}

/// The running capture.
struct Capture {
    stream: Retained<SCStream>,
    config: Retained<SCStreamConfiguration>,
    _output: Retained<FrameOutput>,
}

// SAFETY: the one `Send` this module asserts. ScreenCaptureKit's objects are
// used across threads by design: the stream is built in a completion handler
// on a queue ScreenCaptureKit chooses, and Apple's own examples then control
// it from other threads. Swift declares these classes `Sendable`; the Rust
// bindings do not yet. Nothing here touches AppKit: the window and layer stay
// on the main thread in thread-locals, and frames reach them through the main
// queue. The mutex serialises every use of the stream and its configuration.
unsafe impl Send for Capture {}

static CAPTURE: Mutex<Option<Capture>> = Mutex::new(None);

/// A start is in flight: the permission prompt may be up and the stream not
/// yet built. A second `z` meanwhile must not start a second stream.
static STARTING: AtomicBool = AtomicBool::new(false);

/// Whether zoom is still wanted. Turned off by [`stop`]; a stream that comes
/// up after that is stopped at once instead of kept, or it would run hidden.
static WANTED: AtomicBool = AtomicBool::new(false);

struct Zoom {
    level: f64,
    source: surface::DisplayRect,
    timer: Retained<NSTimer>,
}

thread_local! {
    static SCREEN: Cell<Option<Screen>> = const { Cell::new(None) };
    static WINDOW: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
    static LAYER: RefCell<Option<Retained<CALayer>>> = const { RefCell::new(None) };
    static ZOOM: RefCell<Option<Zoom>> = const { RefCell::new(None) };
    static ON_FAILURE: Cell<Option<fn()>> = const { Cell::new(None) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "YappyinkMagnifierOutput"]
    #[ivars = ()]
    struct FrameOutput;

    unsafe impl NSObjectProtocol for FrameOutput {}

    unsafe impl SCStreamOutput for FrameOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn stream_did_output(
            &self,
            _stream: &SCStream,
            sample: &CMSampleBuffer,
            kind: SCStreamOutputType,
        ) {
            if kind == SCStreamOutputType::Screen {
                show_frame(sample);
            }
        }
    }
);

impl FrameOutput {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

/// Whether ScreenCaptureKit exists here at all (macOS 12.3 and later). Zoom is
/// only offered where it does, so an older Mac shows no button.
pub fn available() -> bool {
    AnyClass::get(c"SCStream").is_some()
}

/// Records the screen to magnify and what to do if capture fails, for
/// example because Screen Recording permission was refused.
pub fn prepare(screen: Screen, on_failure: fn()) {
    SCREEN.with(|slot| slot.set(Some(screen)));
    ON_FAILURE.with(|slot| slot.set(Some(on_failure)));
}

/// Zooms to a level, or stops with `None`. Main thread only.
pub fn set(mtm: MainThreadMarker, overlay: &NSWindow, factor: Option<f64>) {
    let Some(level) = factor else {
        stop();
        eprintln!("[zoom] off");
        return;
    };
    let Some(screen) = SCREEN.with(Cell::get) else {
        eprintln!("[zoom] no screen was recorded; not zooming");
        return;
    };

    let window = ensure_window(mtm, screen);
    window.orderFront(None);
    let source = source_now(screen, level);

    WANTED.store(true, Ordering::SeqCst);
    let running = CAPTURE.lock().ok().is_some_and(|capture| capture.is_some());
    if running {
        reconfigure(source);
    } else if STARTING.swap(true, Ordering::SeqCst) {
        // Already starting; the timer moves it to the new level's rectangle
        // once it is up.
    } else {
        let exclude = vec![overlay.windowNumber(), window.windowNumber()];
        start(screen, source, exclude);
        eprintln!(
            "[zoom] starting capture; macOS asks for Screen Recording permission the first time"
        );
    }

    ZOOM.with(|slot| {
        let mut zoom = slot.borrow_mut();
        let timer = match zoom.take() {
            Some(previous) => previous.timer,
            None => follow_timer(),
        };
        *zoom = Some(Zoom {
            level,
            source,
            timer,
        });
    });
    eprintln!("[zoom] {level:.0}x");
}

/// Stops the capture and hides the magnified picture. Safe to call when not
/// zoomed, and called on exit so a capture never outlives the overlay.
pub fn stop() {
    WANTED.store(false, Ordering::SeqCst);
    ZOOM.with(|slot| {
        if let Some(zoom) = slot.borrow_mut().take() {
            zoom.timer.invalidate();
        }
    });
    if let Some(capture) = CAPTURE.lock().ok().and_then(|mut slot| slot.take()) {
        unsafe { capture.stream.stopCaptureWithCompletionHandler(None) };
    }
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            window.orderOut(None);
        }
    });
    LAYER.with(|slot| {
        if let Some(layer) = slot.borrow().as_ref() {
            unsafe { layer.setContents(None) };
        }
    });
}

/// The rectangle to magnify, around where the pointer is now.
fn source_now(screen: Screen, level: f64) -> surface::DisplayRect {
    let mouse = NSEvent::mouseLocation();
    let cursor = surface::cursor_on_screen((mouse.x, mouse.y), screen.origin, screen.size);
    surface::zoom_source(screen.size, level, cursor)
}

fn to_cg(rect: surface::DisplayRect) -> CGRect {
    CGRect::new(
        CGPoint::new(rect.x, rect.y),
        CGSize::new(rect.width, rect.height),
    )
}

/// A borderless, click-through window covering the screen, just below the
/// overlay, whose layer shows the frames.
fn ensure_window(mtm: MainThreadMarker, screen: Screen) -> Retained<NSWindow> {
    if let Some(window) = WINDOW.with(|slot| slot.borrow().clone()) {
        return window;
    }
    let frame = NSRect::new(
        NSPoint::new(screen.origin.0, screen.origin.1),
        NSSize::new(screen.size.0, screen.size.1),
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
    window.setOpaque(true);
    window.setBackgroundColor(Some(&NSColor::blackColor()));
    window.setLevel(MAGNIFIER_LEVEL);
    // The picture is only a picture: every click goes to what is really
    // there, or to the overlay above in Draw.
    window.setIgnoresMouseEvents(true);
    window.setHasShadow(false);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary,
    );
    unsafe { window.setReleasedWhenClosed(false) };
    if let Some(view) = window.contentView() {
        view.setWantsLayer(true);
        LAYER.with(|slot| *slot.borrow_mut() = view.layer());
    }
    WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    window
}

/// Shows one frame. Runs on the main queue, where the layer lives.
fn show_frame(sample: &CMSampleBuffer) {
    let Some(image) = (unsafe { sample.image_buffer() }) else {
        return;
    };
    let Some(surface) = CVPixelBufferGetIOSurface(Some(&image)) else {
        return;
    };
    LAYER.with(|slot| {
        if let Some(layer) = slot.borrow().as_ref() {
            // An IOSurface is a valid layer content, and is toll-free
            // bridged to the Objective-C object the layer expects.
            let object: &AnyObject = unsafe { &*(&*surface as *const _ as *const AnyObject) };
            unsafe { layer.setContents(Some(object)) };
        }
    });
}

/// Asks for the shareable content, which is also what raises the Screen
/// Recording prompt, then builds and starts the stream in the completion
/// handler. Only plain values cross into the handler.
fn start(screen: Screen, source: surface::DisplayRect, exclude: Vec<isize>) {
    let display_id = CGMainDisplayID();
    let width = (screen.size.0 * screen.backing).round() as usize;
    let height = (screen.size.1 * screen.backing).round() as usize;

    let handler = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            let Some(content) = (unsafe { content.as_ref() }) else {
                let reason = unsafe { error.as_ref() }
                    .map(|error| error.localizedDescription().to_string())
                    .unwrap_or_else(|| "no reason given".to_owned());
                eprintln!(
                    "[zoom] the screen could not be captured: {reason}. Zoom on macOS needs \
                     Screen Recording permission: System Settings, Privacy & Security, Screen \
                     Recording, then restart yappyink"
                );
                STARTING.store(false, Ordering::SeqCst);
                DispatchQueue::main().exec_async(fail);
                return;
            };

            let displays = unsafe { content.displays() };
            let Some(display) = displays
                .iter()
                .find(|display| unsafe { display.displayID() } == display_id)
            else {
                eprintln!("[zoom] the main display is not in the shareable content");
                STARTING.store(false, Ordering::SeqCst);
                DispatchQueue::main().exec_async(fail);
                return;
            };
            let ours: Vec<Retained<SCWindow>> = unsafe { content.windows() }
                .iter()
                .filter(|window| exclude.contains(&(unsafe { window.windowID() } as isize)))
                .collect();
            let excluded = NSArray::from_retained_slice(&ours);

            let filter = unsafe {
                SCContentFilter::initWithDisplay_excludingWindows(
                    SCContentFilter::alloc(),
                    &display,
                    &excluded,
                )
            };
            let config = unsafe { SCStreamConfiguration::new() };
            unsafe {
                config.setWidth(width);
                config.setHeight(height);
                config.setSourceRect(to_cg(source));
                // The real pointer is drawn by the system above everything; a
                // magnified copy of it in the picture would be a second one.
                config.setShowsCursor(false);
                config.setMinimumFrameInterval(CMTime::new(1, 60));
                config.setQueueDepth(3);
            }

            let output = FrameOutput::new();
            let stream = unsafe {
                SCStream::initWithFilter_configuration_delegate(
                    SCStream::alloc(),
                    &filter,
                    &config,
                    None,
                )
            };
            if let Err(error) = unsafe {
                stream.addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    SCStreamOutputType::Screen,
                    Some(DispatchQueue::main()),
                )
            } {
                eprintln!(
                    "[zoom] the capture could not be set up: {}",
                    error.localizedDescription()
                );
                STARTING.store(false, Ordering::SeqCst);
                DispatchQueue::main().exec_async(fail);
                return;
            }

            let started = RcBlock::new(|error: *mut NSError| {
                if let Some(error) = unsafe { error.as_ref() } {
                    eprintln!(
                        "[zoom] the capture did not start: {}",
                        error.localizedDescription()
                    );
                    DispatchQueue::main().exec_async(fail);
                }
            });
            unsafe { stream.startCaptureWithCompletionHandler(Some(&started)) };

            // Zoom may have been turned off while the prompt was up.
            if !WANTED.load(Ordering::SeqCst) {
                unsafe { stream.stopCaptureWithCompletionHandler(None) };
            } else if let Ok(mut slot) = CAPTURE.lock() {
                *slot = Some(Capture {
                    stream,
                    config,
                    _output: output,
                });
            }
            STARTING.store(false, Ordering::SeqCst);
        },
    );
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&handler) };
}

/// Moves the magnified rectangle, if the capture is running.
fn reconfigure(source: surface::DisplayRect) {
    if let Ok(slot) = CAPTURE.lock()
        && let Some(capture) = slot.as_ref()
    {
        unsafe {
            capture.config.setSourceRect(to_cg(source));
            capture
                .stream
                .updateConfiguration_completionHandler(&capture.config, None);
        }
    }
}

/// Follows the pointer while zoomed. A timer rather than mouse events,
/// because in pass-through this application sees none.
fn follow_timer() -> Retained<NSTimer> {
    let tick = RcBlock::new(|_timer: NonNull<NSTimer>| {
        let Some(screen) = SCREEN.with(Cell::get) else {
            return;
        };
        let moved = ZOOM.with(|slot| {
            let mut zoom = slot.borrow_mut();
            let zoom = zoom.as_mut()?;
            let source = source_now(screen, zoom.level);
            (source != zoom.source).then(|| {
                zoom.source = source;
                source
            })
        });
        if let Some(source) = moved {
            reconfigure(source);
        }
    });
    unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(FOLLOW_SECONDS, true, &tick) }
}

/// Capture failed: stop, and let the overlay reset its zoom level. Runs on
/// the main queue.
fn fail() {
    stop();
    if let Some(on_failure) = ON_FAILURE.with(Cell::get) {
        on_failure();
    }
}
