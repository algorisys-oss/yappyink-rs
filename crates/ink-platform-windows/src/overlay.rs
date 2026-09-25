//! The Win32 overlay window.
//!
//! This is the part that cannot be tested without Windows, and it is kept as
//! small as it can be for that reason: the key bindings live in [`crate::keys`]
//! and every piece of arithmetic — monitor choice, fitting, parking, dragging,
//! cursors, remote message numbers — in [`crate::surface`], all of which run
//! their tests everywhere.
//!
//! # What is here
//!
//! Drawing, every tool, mode switching, undo and redo, save and load, the full
//! toolbar from `ink-ui`, text with its live preview and input-method
//! composition, a monitor chosen by number, Parked shrinking the window to the
//! toolbar, a cursor per tool, the window dragged by its grip and resized from
//! its corner, a DPI change or display change followed by a resize, global
//! `Ctrl+Alt+D` and `Ctrl+Alt+H` through `RegisterHotKey`, and the
//! `yappyink toggle-draw` family of verbs delivered as window messages.
//!
//! # How messages are handled, and why it is not the obvious way
//!
//! The window procedure never touches the overlay's state. It turns each
//! message into an [`Input`], queues it, and drains the queue one input at a
//! time. The obvious way — borrowing the state in each handler — crashes:
//! `ShowWindow`, `DestroyWindow` and `SetWindowPos` deliver messages such as
//! `WM_KILLFOCUS` to this same procedure *synchronously, before they return*.
//! The first version called them while holding the borrow, so hiding or
//! quitting would re-enter, borrow again, panic, and a panic inside an
//! `extern "system"` callback aborts the process. With the queue, a message
//! that arrives mid-handling waits its turn instead.
//!
//! # Differences from the Wayland adapter, on purpose
//!
//! - **No control socket.** `RegisterHotKey` supplies a real global chord, and
//!   the CLI verbs arrive as `WM_APP` messages posted to the overlay's window,
//!   found by its class name.
//! - **Input capture is painted, not declared.** Win32 lets a click through a
//!   layered window wherever alpha is zero, so a capturing frame is covered
//!   with an invisible alpha-1 floor (`ink_ui::clear`). Wayland declares an
//!   input region instead.
//! - **The toolbar is not clickable in PassThrough.** Wayland narrows the
//!   input region to the toolbar. Here pass-through is `WS_EX_TRANSPARENT` on
//!   the whole window, and a window cannot be transparent in one place and not
//!   another; a second window for the toolbar would be the fix. The hotkey is
//!   the way back meanwhile.
//! - **The window takes focus in Draw.** `WS_EX_NOACTIVATE` would stop the
//!   overlay ever stealing focus, but it also means no keyboard, and the text
//!   tool needs one. In PassThrough the window is click-through, so the
//!   application underneath keeps focus, which is the case that matters.
//!
//! **None of this has been run.** Every capability stays `unknown` until it is.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::c_void;

use ink_app::{Action, Controller, Effect, Mode, PlatformEvent, TransitionId};
use ink_core::{IdSource, LogicalPoint, LogicalSize, Object, OutputId, Session, Style};
use ink_platform::PlatformError;
use ink_render::{Canvas, Painter, Scale};

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject,
    EnumDisplayMonitors, GetDC, GetMonitorInfoW, HBITMAP, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST,
    MONITORINFO, MonitorFromWindow, ReleaseDC, SelectObject,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, GetDpiForWindow,
    MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::Ime::{
    CANDIDATEFORM, CFS_CANDIDATEPOS, CFS_POINT, COMPOSITIONFORM, GCS_COMPSTR, GCS_RESULTSTR,
    IACE_DEFAULT, IME_COMPOSITION_STRING, ImmAssociateContextEx, ImmGetCompositionStringW,
    ImmGetContext, ImmReleaseContext, ImmSetCandidateWindow, ImmSetCompositionWindow,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, ReleaseCapture, SetCapture,
    UnregisterHotKey,
};
use windows_sys::Win32::UI::Magnification::{
    MagInitialize, MagSetFullscreenTransform, MagUninitialize,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW, GWL_EXSTYLE,
    GetCursorPos, GetMessageW, GetWindowLongPtrW, HTCLIENT, IDC_ARROW, KillTimer, LoadCursorW,
    MONITORINFOF_PRIMARY, MSG, PostMessageW, PostQuitMessage, RegisterClassW, SW_HIDE, SW_SHOW,
    SWP_NOACTIVATE, SWP_NOZORDER, SetCursor, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TranslateMessage, ULW_ALPHA, UpdateLayeredWindow, WM_CAPTURECHANGED, WM_CHAR, WM_DESTROY,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_HOTKEY, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION,
    WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_SETCURSOR, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP, WindowFromPoint,
};

use crate::keys;
use crate::surface::{self, Monitor, PixelRect};

/// The window class, which is also how `yappyink toggle-draw` finds a running
/// overlay to post its message to.
pub const CLASS_NAME: &str = "YappyinkOverlay";

const HOTKEY_TOGGLE_DRAW: i32 = 1;
const HOTKEY_HIDE: i32 = 2;
const HOTKEY_ZOOM: i32 = 3;
const HOTKEY_ZOOM_OFF: i32 = 4;

/// The timer that moves the magnified view with the pointer while zoomed.
/// A timer rather than mouse messages, because in pass-through the pointer
/// is over other applications and this window hears nothing from it.
const ZOOM_TIMER: usize = 1;
/// About sixty times a second: smooth enough to follow a pointer, and only
/// running while zoomed, so idle cost is unchanged (NFR-002).
const ZOOM_TICK_MS: u32 = 16;

/// What a remote verb asks the overlay to do.
///
/// The verbs themselves belong to the application, which also owns them on
/// Linux; the overlay is handed a decoder rather than a copy of the list, so
/// there is one list.
pub enum Remote {
    Act(Action),
    Quit,
}

/// How the overlay is asked to start.
pub struct OverlayConfig {
    /// Which monitor to cover, numbered from 1 in the order they are printed
    /// at startup. `None` is the primary.
    pub monitor: Option<usize>,
    /// A size in logical units, or `None` for the whole monitor.
    pub size: Option<LogicalSize>,
    /// Turns a remote verb number into what it means.
    pub remote: fn(u32) -> Option<Remote>,
}

/// One window message, reduced to what the overlay needs from it.
///
/// Pointer positions are client pixels. They become logical units only when
/// the input is handled, because that needs the scale, which is overlay state
/// the window procedure is not allowed to touch.
enum Input {
    PointerDown(i32, i32),
    PointerMoved(i32, i32),
    PointerUp(i32, i32),
    PointerCancelled,
    Key(u32),
    Char(u32),
    Hotkey(i32),
    Remote(u32),
    DpiChanged(u32, PixelRect),
    DisplayChanged,
    CompositionStarted,
    Composing(String),
    Composed(String),
    ZoomTick,
}

#[derive(Clone, Copy)]
enum DragKind {
    Move,
    Resize,
}

/// The window being moved or resized by hand.
///
/// Done here rather than by handing the button to the system's own move and
/// size loop, because that loop only resizes windows with a sizing border, and
/// this one has none: it is a borderless popup.
#[derive(Clone, Copy)]
struct WindowDrag {
    kind: DragKind,
    from: (i32, i32),
    start: PixelRect,
}

struct Overlay {
    hwnd: HWND,
    /// Where the window is, in screen pixels.
    rect: PixelRect,
    /// The bitmap's size, which follows `rect` after every resize.
    width: u32,
    height: u32,
    pixels: *mut u8,
    bitmap: HBITMAP,
    memory_dc: HDC,
    scale: Scale,
    /// What was asked for at startup, so a display change can fit again.
    requested: Option<(f64, f64)>,
    remote: fn(u32) -> Option<Remote>,

    controller: Controller,
    session: Session,
    ids: IdSource,
    painter: Painter,
    /// The committed ink, rasterised once and reused until it changes, so a
    /// pointer move does not re-rasterise every stroke on the page (E015).
    ink: ink_ui::InkLayer,

    needs_redraw: bool,
    hidden: bool,
    /// The full-size rectangle to return to when leaving Parked.
    restore: Option<PixelRect>,
    drag: Option<WindowDrag>,
    /// The cursor resource currently shown, so it is only set on a change.
    cursor: Option<u16>,
    /// Whether the input method is attached. Only while text is being edited:
    /// otherwise a Japanese or Chinese input method in its native mode would
    /// turn `d` into the start of a composition instead of a mode switch.
    ime_enabled: bool,
    /// Where the pointer was last seen, in logical units.
    ///
    /// Kept rather than read on demand because the caret hint and the cursor
    /// both depend on it and have to be recomputed when the *tool* changes as
    /// well as when the pointer moves (docs/learning.md §12).
    last_pointer: Option<LogicalPoint>,
    /// The monitor the overlay was put on, which the zoom stays within.
    monitor_bounds: PixelRect,
    /// Whether `MagInitialize` succeeded, which is what offering zoom means.
    magnifier: bool,
    /// The zoom level, and the offset last applied, while zoomed.
    zoom: Option<(f64, (i32, i32))>,
}

thread_local! {
    static OVERLAY: RefCell<Option<Overlay>> = const { RefCell::new(None) };
    static INBOX: RefCell<VecDeque<Input>> = const { RefCell::new(VecDeque::new()) };
    static DRAINING: Cell<bool> = const { Cell::new(false) };
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn pixel_rect(rect: RECT) -> PixelRect {
    PixelRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

/// Sends remote verb number `index` to a running overlay.
///
/// Posted, not sent, so a hung overlay cannot hang the command too. Refused
/// when no overlay is running, which is the ordinary case of pressing the
/// chord before starting one.
pub fn send_remote(index: u32) -> Result<(), String> {
    let message = surface::remote_message(index)
        .ok_or_else(|| format!("remote verb {index} is out of range"))?;
    let class = wide(CLASS_NAME);
    let hwnd = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    if hwnd.is_null() {
        return Err("no overlay is running; start one with `yappyink draw`".to_owned());
    }
    if unsafe { PostMessageW(hwnd, message, 0, 0) } == 0 {
        return Err(format!(
            "the overlay could not be reached: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// Every monitor, in the order Windows enumerates them.
///
/// That order is what the numbers printed at startup refer to. It is stable
/// for a given arrangement, and is not promised to survive plugging a monitor
/// in; the startup listing is there so nobody has to guess.
pub fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn collect(
        handle: HMONITOR,
        _dc: HDC,
        _clip: *mut RECT,
        data: LPARAM,
    ) -> windows_sys::core::BOOL {
        let found = unsafe { &mut *(data as *mut Vec<Monitor>) };
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(handle, &mut info) } != 0 {
            let (mut dpi, mut unused) = (0u32, 0u32);
            if unsafe { GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused) } != 0 {
                dpi = 0;
            }
            found.push(Monitor {
                bounds: pixel_rect(info.rcMonitor),
                dpi,
                primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
            });
        }
        1
    }

    let mut found: Vec<Monitor> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(collect),
            &mut found as *mut Vec<Monitor> as LPARAM,
        );
    }
    found
}

/// Runs the overlay until it is asked to quit.
pub fn run(config: OverlayConfig) -> Result<Session, PlatformError> {
    // Before any window exists, or Windows reports scaled coordinates and the
    // overlay is stretched by the compositor rather than drawn at native
    // pixels.
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } == 0 {
        eprintln!("[dpi] per-monitor-v2 awareness was refused; coordinates may be scaled");
    }

    let all = monitors();
    for (index, monitor) in all.iter().enumerate() {
        let bounds = monitor.bounds;
        eprintln!(
            "[output] monitor {}: {}x{} at {},{}, {} dpi{}",
            index + 1,
            bounds.width(),
            bounds.height(),
            bounds.left,
            bounds.top,
            monitor.dpi,
            if monitor.primary { ", primary" } else { "" }
        );
    }
    let index = surface::choose_monitor(&all, config.monitor)
        .map_err(|reason| PlatformError::invalid_data("the requested monitor", reason))?;
    let requested = config.size.map(|size| (size.width(), size.height()));
    let rect = surface::fit(all[index], requested);
    eprintln!(
        "[output] covering monitor {}: {}x{} at {},{}",
        index + 1,
        rect.width(),
        rect.height(),
        rect.left,
        rect.top
    );

    create(index, all[index].bounds, rect, requested, config.remote)?;
    print_orientation();

    // The same first step as the Wayland adapter. The controller starts
    // Hidden, and a hidden overlay paints nothing and takes nothing, so
    // without this the window opened as an invisible rectangle that ignored
    // the mouse. Neither this backend nor the macOS one did it before.
    act(Action::EnterDraw);
    flush();

    pump();
    finish()
}

fn create(
    monitor: usize,
    bounds: PixelRect,
    rect: PixelRect,
    requested: Option<(f64, f64)>,
    remote: fn(u32) -> Option<Remote>,
) -> Result<(), PlatformError> {
    let class_name = wide(CLASS_NAME);
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };

    let class = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance as _,
        hIcon: std::ptr::null_mut(),
        // Only a fallback: WM_SETCURSOR sets the real one from the tool.
        hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
        // Null deliberately: every pixel comes from UpdateLayeredWindow, and a
        // background brush would be painted opaque underneath them.
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(PlatformError::unsupported(
            "register the overlay window class",
            std::io::Error::last_os_error().to_string(),
        ));
    }

    // No WS_EX_NOACTIVATE: see the module documentation.
    let ex_style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
    let title = wide("yappyink");
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance as _,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err(PlatformError::unsupported(
            "create the overlay window",
            std::io::Error::last_os_error().to_string(),
        ));
    }

    let width = rect.width().max(1) as u32;
    let height = rect.height().max(1) as u32;
    let (bitmap, memory_dc, pixels) = create_dib(width, height)?;

    // The window is in physical pixels, because this process is
    // per-monitor-v2 aware, so the document's logical size is that divided by
    // the scale.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let factor = surface::scale_for_dpi(dpi);
    let scale = Scale::new(factor).unwrap_or(Scale::ONE);
    eprintln!("[dpi] {dpi} dpi, scale {factor}");

    let output = OutputId::new(format!("monitor-{}", monitor + 1));
    let size = LogicalSize::new(f64::from(width) / factor, f64::from(height) / factor).ok_or_else(
        || PlatformError::unsupported("size the overlay", "the monitor reported a size of zero"),
    )?;

    let painter = match ink_render::text::TextFont::discover() {
        Ok(font) => {
            eprintln!("[font] {}", font.source().display());
            for fallback in font.fallbacks() {
                eprintln!(
                    "[font] fallback {}, loaded when first needed",
                    fallback.display()
                );
            }
            Painter::new().with_font(font)
        }
        Err(reason) => {
            // Not fatal: everything except the text tool works without a font.
            eprintln!("[font] no usable font was found, so the text tool is unavailable: {reason}");
            Painter::new()
        }
    };

    let mut controller = Controller::new();
    // The controller offers a resize corner only once it knows the surface's
    // size. The Wayland adapter never tells it, which is why that backend has
    // no corner; this one does, and keeps it current after every resize.
    controller.set_surface_size(size);

    // FR-029 through the Magnification API (ADR-008, T038). Windows does the
    // magnifying, so no pixels reach this process. Offered only when it
    // initialises, so a system without it shows no zoom button.
    let magnifier = unsafe { MagInitialize() } != 0;
    if magnifier {
        controller.offer_zoom();
        eprintln!("[zoom] the Magnification API is available: z or Ctrl+Alt+Z steps 2x, 3x, 4x");
    } else {
        eprintln!(
            "[zoom] MagInitialize failed ({}); zoom is not offered",
            std::io::Error::last_os_error()
        );
    }

    OVERLAY.with(|slot| {
        *slot.borrow_mut() = Some(Overlay {
            hwnd,
            rect,
            width,
            height,
            pixels,
            bitmap,
            memory_dc,
            scale,
            requested,
            remote,
            controller,
            session: Session::new(output, size),
            ids: IdSource::starting_at(1),
            painter,
            ink: ink_ui::InkLayer::new(),
            needs_redraw: true,
            hidden: false,
            restore: None,
            drag: None,
            cursor: None,
            ime_enabled: true,
            last_pointer: None,
            monitor_bounds: bounds,
            magnifier,
            zoom: None,
        });
    });

    register_hotkeys(hwnd);
    sync_input_method();
    repaint();
    unsafe { ShowWindow(hwnd, SW_SHOW) };
    Ok(())
}

fn create_dib(width: u32, height: u32) -> Result<(HBITMAP, HDC, *mut u8), PlatformError> {
    let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width as i32,
        // Top-down, so row zero is the top and matches how Canvas is indexed.
        biHeight: surface::dib_height(height),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };

    let mut bits: *mut c_void = std::ptr::null_mut();
    let screen_dc = unsafe { GetDC(std::ptr::null_mut()) };
    let bitmap = unsafe {
        CreateDIBSection(
            screen_dc,
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        )
    };
    let memory_dc = unsafe { CreateCompatibleDC(screen_dc) };
    unsafe { ReleaseDC(std::ptr::null_mut(), screen_dc) };

    if bitmap.is_null() || bits.is_null() {
        unsafe { DeleteDC(memory_dc) };
        return Err(PlatformError::unsupported(
            "create the overlay bitmap",
            "a 32-bit device-independent bitmap could not be created",
        ));
    }
    unsafe { SelectObject(memory_dc, bitmap as _) };
    Ok((bitmap, memory_dc, bits.cast::<u8>()))
}

fn register_hotkeys(hwnd: HWND) {
    // FR-005. A chord another application owns fails here with a real error,
    // which is the registration feedback the requirement asks for.
    let modifiers = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
    for (id, key, name) in [
        (HOTKEY_TOGGLE_DRAW, keys::vk::D, "Ctrl+Alt+D"),
        (HOTKEY_HIDE, keys::vk::H, "Ctrl+Alt+H"),
        (HOTKEY_ZOOM, keys::vk::Z, "Ctrl+Alt+Z"),
        (HOTKEY_ZOOM_OFF, keys::vk::KEY_0, "Ctrl+Alt+0"),
    ] {
        if unsafe { RegisterHotKey(hwnd, id, modifiers, key) } == 0 {
            eprintln!(
                "[hotkey] {name} was refused: {}. Another application owns it, so that \
                 recovery route is unavailable this run.",
                std::io::Error::last_os_error()
            );
        } else {
            eprintln!("[hotkey] {name} registered");
        }
    }
}

fn pump() {
    let mut message: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        // Needed for WM_CHAR. Turning a key press into a character depends on
        // the active keyboard layout and is the OS's job, not ours.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn finish() -> Result<Session, PlatformError> {
    OVERLAY.with(|slot| {
        let overlay = slot.borrow_mut().take().ok_or_else(|| {
            PlatformError::unsupported("close the overlay", "it was never created")
        })?;
        unsafe {
            // Never leave the desktop magnified after quitting.
            if overlay.zoom.is_some() {
                MagSetFullscreenTransform(1.0, 0, 0);
            }
            if overlay.magnifier {
                MagUninitialize();
            }
            DeleteDC(overlay.memory_dc);
            DeleteObject(overlay.bitmap as _);
        }
        Ok(overlay.session)
    })
}

fn client_point(lparam: LPARAM) -> (i32, i32) {
    // Signed, so a drag off the left edge is negative rather than 65000.
    (
        i32::from((lparam & 0xFFFF) as i16),
        i32::from(((lparam >> 16) & 0xFFFF) as i16),
    )
}

/// Reads one of the input method's strings.
///
/// Done in the window procedure rather than when the input is drained,
/// because the context only holds the string that belongs to the message
/// being handled.
fn composition_string(hwnd: HWND, which: IME_COMPOSITION_STRING) -> Option<String> {
    let context = unsafe { ImmGetContext(hwnd) };
    if context.is_null() {
        return None;
    }
    // A length in bytes, of UTF-16.
    let bytes = unsafe { ImmGetCompositionStringW(context, which, std::ptr::null_mut(), 0) };
    let text = if bytes >= 0 {
        let mut buffer = vec![0u16; bytes as usize / 2];
        let read = unsafe {
            ImmGetCompositionStringW(context, which, buffer.as_mut_ptr().cast(), bytes as u32)
        };
        (read >= 0).then(|| String::from_utf16_lossy(&buffer[..read as usize / 2]))
    } else {
        None
    };
    unsafe { ImmReleaseContext(hwnd, context) };
    text
}

/// Turns a message into an input and queues it, or answers it directly when
/// the answer cannot wait.
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if let Some(index) = surface::remote_index(message) {
        post(Input::Remote(index));
        return 0;
    }
    let input = match message {
        WM_LBUTTONDOWN => {
            // Capture, so a drag that leaves the window still reports its end.
            unsafe { SetCapture(hwnd) };
            let (x, y) = client_point(lparam);
            Input::PointerDown(x, y)
        }
        WM_MOUSEMOVE => {
            let (x, y) = client_point(lparam);
            Input::PointerMoved(x, y)
        }
        WM_LBUTTONUP => {
            let (x, y) = client_point(lparam);
            Input::PointerUp(x, y)
        }
        // FR-018. The pointer was taken away mid-gesture; the stroke is
        // cancelled rather than committed from wherever it happened to be.
        WM_CAPTURECHANGED | WM_KILLFOCUS => Input::PointerCancelled,
        WM_KEYDOWN => Input::Key(wparam as u32),
        WM_CHAR => Input::Char(wparam as u32),
        WM_HOTKEY => Input::Hotkey(wparam as i32),
        WM_SETCURSOR if (lparam & 0xFFFF) as u32 == HTCLIENT => {
            // Answered now, because returning nonzero is what stops the class
            // cursor overwriting ours, and that has to be decided here.
            if let Some(resource) = wanted_cursor() {
                show_cursor(resource);
                return 1;
            }
            return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
        }
        WM_DPICHANGED => {
            // Windows suggests where the window should go at the new scale,
            // and following the suggestion is what keeps it on the monitor
            // it was moved to.
            let suggested = unsafe { *(lparam as *const RECT) };
            Input::DpiChanged((wparam & 0xFFFF) as u32, pixel_rect(suggested))
        }
        WM_DISPLAYCHANGE => Input::DisplayChanged,
        WM_TIMER if wparam == ZOOM_TIMER => Input::ZoomTick,
        // Not passed on: the default would open the input method's own
        // composition window, and the composition is drawn inline instead.
        WM_IME_STARTCOMPOSITION => Input::CompositionStarted,
        WM_IME_COMPOSITION => {
            // A commit and the start of the next composition can arrive in one
            // message, so both are read, commit first.
            let flags = lparam as u32;
            if flags & GCS_RESULTSTR != 0
                && let Some(text) = composition_string(hwnd, GCS_RESULTSTR)
            {
                post(Input::Composed(text));
            }
            if flags & GCS_COMPSTR != 0
                && let Some(text) = composition_string(hwnd, GCS_COMPSTR)
            {
                post(Input::Composing(text));
            }
            // Not passed on either: the default turns the result into
            // WM_IME_CHAR and then WM_CHAR, which would type it twice.
            return 0;
        }
        WM_IME_ENDCOMPOSITION => Input::Composing(String::new()),
        WM_DESTROY => {
            unsafe {
                UnregisterHotKey(hwnd, HOTKEY_TOGGLE_DRAW);
                UnregisterHotKey(hwnd, HOTKEY_HIDE);
                UnregisterHotKey(hwnd, HOTKEY_ZOOM);
                UnregisterHotKey(hwnd, HOTKEY_ZOOM_OFF);
                KillTimer(hwnd, ZOOM_TIMER);
                PostQuitMessage(0);
            }
            return 0;
        }
        _ => return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    };
    post(input);
    0
}

/// Queues an input and drains the queue, unless a drain is already running
/// further up this same stack, in which case that one will get to it.
fn post(input: Input) {
    INBOX.with(|inbox| inbox.borrow_mut().push_back(input));
    if DRAINING.with(|draining| draining.replace(true)) {
        return;
    }
    while let Some(input) = INBOX.with(|inbox| inbox.borrow_mut().pop_front()) {
        handle(input);
        flush();
    }
    DRAINING.with(|draining| draining.set(false));
}

fn handle(input: Input) {
    match input {
        Input::PointerDown(x, y) => {
            let at = logical(x, y);
            dispatch(PlatformEvent::PointerDown { at });
        }
        Input::PointerMoved(x, y) => {
            if !drag_window() {
                let at = logical(x, y);
                dispatch(PlatformEvent::PointerMoved { at });
            }
        }
        Input::PointerUp(x, y) => {
            let dragging = with_overlay(|overlay| overlay.drag.take().is_some());
            unsafe { ReleaseCapture() };
            if dragging == Some(true) {
                eprintln!("[window] placed");
            }
            let at = logical(x, y);
            dispatch(PlatformEvent::PointerUp { at });
        }
        Input::PointerCancelled => {
            // Losing capture ends a window drag too, where it is.
            with_overlay(|overlay| overlay.drag = None);
            dispatch(PlatformEvent::PointerCancelled);
        }
        Input::Key(virtual_key) => on_key(virtual_key),
        Input::Char(code) => on_char(code),
        Input::Hotkey(id) => match id {
            HOTKEY_TOGGLE_DRAW => act(Action::ToggleDraw),
            HOTKEY_HIDE => act(Action::ToggleVisibility),
            HOTKEY_ZOOM => act(Action::CycleZoom),
            HOTKEY_ZOOM_OFF => act(Action::ZoomOff),
            _ => {}
        },
        Input::Remote(index) => {
            let decoded = with_overlay(|overlay| (overlay.remote)(index)).flatten();
            match decoded {
                Some(Remote::Act(action)) => act(action),
                Some(Remote::Quit) => quit(),
                None => eprintln!("[remote] message {index} is not a verb; ignored"),
            }
        }
        Input::DpiChanged(dpi, suggested) => on_dpi_changed(dpi, suggested),
        Input::DisplayChanged => on_display_changed(),
        Input::CompositionStarted => place_candidate_window(),
        Input::Composing(text) => {
            dispatch(PlatformEvent::Preedit(text));
            place_candidate_window();
        }
        Input::Composed(text) => dispatch(PlatformEvent::CommitPreedit(text)),
        Input::ZoomTick => {
            with_overlay(follow_pointer);
        }
    }
}

fn with_overlay<T>(f: impl FnOnce(&mut Overlay) -> T) -> Option<T> {
    OVERLAY.with(|slot| slot.borrow_mut().as_mut().map(f))
}

/// Client pixels, turned into logical units.
///
/// Divided by the scale, because Win32 reports physical pixels and everything
/// above the adapter works in logical units. Skipping that puts the pointer in
/// the wrong place on any scaled display, and the toolbar stops being
/// clickable where it is drawn.
fn logical(x: i32, y: i32) -> LogicalPoint {
    let factor = with_overlay(|overlay| overlay.scale.get()).unwrap_or(1.0);
    // Both came from an i16 and the scale is finite and positive, so this
    // cannot fail.
    LogicalPoint::new(f64::from(x) / factor, f64::from(y) / factor).expect("a finite coordinate")
}

fn screen_cursor() -> (i32, i32) {
    let mut point = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut point) };
    (point.x, point.y)
}

/// Moves or resizes the window if a drag of it is under way. Returns whether
/// one was, so the motion is not also handed to the controller.
fn drag_window() -> bool {
    let Some(Some(drag)) = with_overlay(|overlay| overlay.drag) else {
        return false;
    };
    let now = screen_cursor();
    let target = match drag.kind {
        DragKind::Move => surface::moved(drag.start, drag.from, now),
        DragKind::Resize => surface::resized(drag.start, drag.from, now, surface::MIN_RESIZE),
    };
    with_overlay(|overlay| place(overlay, target));
    true
}

/// Puts the window at a rectangle, resizing the bitmap if its size changed.
fn place(overlay: &mut Overlay, target: PixelRect) {
    let resized =
        target.width() != overlay.rect.width() || target.height() != overlay.rect.height();
    unsafe {
        SetWindowPos(
            overlay.hwnd,
            std::ptr::null_mut(),
            target.left,
            target.top,
            target.width(),
            target.height(),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    overlay.rect = target;
    if !resized {
        return;
    }

    let width = target.width().max(1) as u32;
    let height = target.height().max(1) as u32;
    match create_dib(width, height) {
        Ok((bitmap, memory_dc, pixels)) => {
            unsafe {
                DeleteDC(overlay.memory_dc);
                DeleteObject(overlay.bitmap as _);
            }
            overlay.bitmap = bitmap;
            overlay.memory_dc = memory_dc;
            overlay.pixels = pixels;
            overlay.width = width;
            overlay.height = height;
        }
        Err(error) => {
            // The old bitmap is kept, and repaint notices the mismatch and
            // says so, rather than writing past the end of it.
            eprintln!("[failed {}] {error}", error.class());
        }
    }
    let factor = overlay.scale.get();
    if let Some(size) = LogicalSize::new(f64::from(width) / factor, f64::from(height) / factor) {
        overlay.controller.set_surface_size(size);
    }
    overlay.needs_redraw = true;
}

/// The window moved to a monitor with a different scale.
fn on_dpi_changed(dpi: u32, suggested: PixelRect) {
    let factor = surface::scale_for_dpi(dpi);
    with_overlay(|overlay| {
        overlay.scale = Scale::new(factor).unwrap_or(Scale::ONE);
        // Parked has its own size, worked out from the toolbar at the new
        // scale; the full size to return to is what Windows suggested.
        if overlay.restore.is_some() {
            overlay.restore = Some(suggested);
            let (width, height) =
                surface::parked_size(overlay.controller.toolbar().bounds(), factor);
            let parked = PixelRect {
                right: suggested.left + width,
                bottom: suggested.top + height,
                ..suggested
            };
            place(overlay, parked);
        } else {
            place(overlay, suggested);
        }
        overlay.needs_redraw = true;
    });
    eprintln!("[dpi] changed to {dpi} dpi, scale {factor}");
}

/// The monitor arrangement changed: resolution, orientation, or a monitor
/// added or removed. FR-019 and FR-022.
///
/// Fitted again to whichever monitor the window is now nearest, because the
/// one it was on may be gone.
/// Zooms to a level, or back to normal with `None`.
fn set_zoom(overlay: &mut Overlay, factor: Option<f64>) {
    match factor {
        Some(level) => {
            let offset = surface::zoom_offset(overlay.monitor_bounds, level, screen_cursor());
            if unsafe { MagSetFullscreenTransform(level as f32, offset.0, offset.1) } == 0 {
                eprintln!(
                    "[zoom] MagSetFullscreenTransform failed: {}",
                    std::io::Error::last_os_error()
                );
                return;
            }
            if overlay.zoom.is_none() {
                unsafe { SetTimer(overlay.hwnd, ZOOM_TIMER, ZOOM_TICK_MS, None) };
            }
            overlay.zoom = Some((level, offset));
            eprintln!("[zoom] {level:.0}x");
        }
        None => {
            unsafe {
                KillTimer(overlay.hwnd, ZOOM_TIMER);
                MagSetFullscreenTransform(1.0, 0, 0);
            }
            overlay.zoom = None;
            eprintln!("[zoom] off");
        }
    }
}

/// Moves the magnified view with the pointer, if it has moved.
fn follow_pointer(overlay: &mut Overlay) {
    let Some((level, last)) = overlay.zoom else {
        return;
    };
    let offset = surface::zoom_offset(overlay.monitor_bounds, level, screen_cursor());
    if offset != last && unsafe { MagSetFullscreenTransform(level as f32, offset.0, offset.1) } != 0
    {
        overlay.zoom = Some((level, offset));
    }
}

fn on_display_changed() {
    with_overlay(|overlay| {
        let handle = unsafe { MonitorFromWindow(overlay.hwnd, MONITOR_DEFAULTTONEAREST) };
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(handle, &mut info) } == 0 {
            eprintln!("[output] the display changed and the new arrangement could not be read");
            return;
        }
        let dpi = unsafe { GetDpiForWindow(overlay.hwnd) };
        overlay.scale = Scale::new(surface::scale_for_dpi(dpi)).unwrap_or(Scale::ONE);
        let monitor = Monitor {
            bounds: pixel_rect(info.rcMonitor),
            dpi,
            primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
        };
        let full = surface::fit(monitor, overlay.requested);
        eprintln!(
            "[output] the display changed; now {}x{} at {},{}",
            full.width(),
            full.height(),
            full.left,
            full.top
        );
        if overlay.restore.is_some() {
            overlay.restore = Some(full);
        } else {
            place(overlay, full);
        }
    });
}

fn on_key(virtual_key: u32) {
    let editing = with_overlay(|overlay| overlay.controller.is_editing_text()).unwrap_or(false);
    let Some(key) = keys::translate(virtual_key) else {
        return;
    };

    if editing {
        // FR-023: while the editor is open a key is a character, not a
        // shortcut. Only the keys that cannot be characters keep their
        // meaning; everything else arrives as WM_CHAR.
        if let Some(action) = ink_app::keymap::editing(key) {
            act(action);
        }
        return;
    }

    if ink_app::keymap::quits(key) {
        quit();
        return;
    }
    if let Some(action) = ink_app::keymap::command(key) {
        // Wayland needs this to ask the compositor for Always on Top. Windows
        // is already top-most by its own extended style, so the key is
        // accepted and explains itself rather than being silently missing on
        // one platform.
        if action == Action::ShowWindowMenu {
            eprintln!("[window] this overlay is already top-most; there is nothing to ask for");
            return;
        }
        act(action);
    }
}

fn on_char(code: u32) {
    let Some(character) = char::from_u32(code) else {
        return;
    };
    // Control characters arrive here too; backspace and return were already
    // handled as virtual keys, and passing them on would insert them twice.
    if character.is_control() {
        return;
    }
    act(Action::TypeText(character));
}

/// Destroys the window, outside any borrow, so the messages that destruction
/// sends back into the window procedure find nothing held.
fn quit() {
    if let Some(hwnd) = with_overlay(|overlay| overlay.hwnd) {
        unsafe { DestroyWindow(hwnd) };
    }
}

fn act(action: Action) {
    let effects = with_overlay(|overlay| overlay.controller.act(action)).unwrap_or_default();
    apply(effects);
}

fn dispatch(event: PlatformEvent) {
    let effects = with_overlay(|overlay| {
        // Recorded before the controller sees the event, so the caret hint
        // and the cursor follow the pointer.
        if let PlatformEvent::PointerDown { at }
        | PlatformEvent::PointerMoved { at }
        | PlatformEvent::PointerUp { at } = event
        {
            overlay.last_pointer = Some(at);
        }
        // Every pointer event repaints. Until 0.8.4 only the text tool did,
        // so a stroke being dragged was not drawn until release, and a
        // tooltip never appeared on hover: both change controller state and
        // produce no effect, and nothing else asked for a frame. Predicting
        // which events change the screen is the mistake docs/learning.md §12
        // records; since the committed ink is cached, a frame is cheap enough
        // not to try.
        if matches!(
            event,
            PlatformEvent::PointerDown { .. }
                | PlatformEvent::PointerMoved { .. }
                | PlatformEvent::PointerUp { .. }
                | PlatformEvent::PointerCancelled
        ) {
            overlay.needs_redraw = true;
        }
        let preedit_changed = matches!(
            event,
            PlatformEvent::Preedit(_) | PlatformEvent::CommitPreedit(_)
        );
        let effects = overlay.controller.handle(event);
        if preedit_changed {
            overlay.needs_redraw = true;
        }
        effects
    })
    .unwrap_or_default();
    apply(effects);
}

/// Brings the screen, the cursor and the input method into line with the
/// state, after every input.
///
/// Called after every input rather than only where a change seems likely.
/// Working out exactly when the screen can change is what produced six
/// separate "correct in the model, absent on screen" bugs in the Wayland
/// adapter (`docs/learning.md` §1 and §12); recomputing and making the write
/// cheap is the lesson from those.
fn flush() {
    let wanted = with_overlay(|overlay| std::mem::take(&mut overlay.needs_redraw)).unwrap_or(false);
    if wanted {
        repaint();
    }
    refresh_cursor();
    sync_input_method();
}

/// The cursor resource the pointer should show over the overlay right now.
fn wanted_cursor() -> Option<u16> {
    OVERLAY.with(|slot| {
        // `try_borrow`: WM_SETCURSOR can arrive while the state is held, and
        // the class cursor is a fine answer for that one message.
        let borrowed = slot.try_borrow().ok()?;
        let overlay = borrowed.as_ref()?;
        let cursor = match overlay.last_pointer {
            Some(at) => overlay.controller.cursor_at(at),
            None => overlay.controller.cursor(),
        };
        Some(surface::cursor_resource(cursor))
    })
}

fn show_cursor(resource: u16) {
    let handle = unsafe { LoadCursorW(std::ptr::null_mut(), resource as usize as *const u16) };
    if !handle.is_null() {
        unsafe { SetCursor(handle) };
    }
    with_overlay(|overlay| overlay.cursor = Some(resource));
}

/// Updates the cursor after a tool or mode change without waiting for the
/// pointer to move, if the pointer is over this window.
///
/// Waiting for the next motion is what left the Wayland cursor stale after a
/// tool was chosen from the keyboard.
fn refresh_cursor() {
    let Some(resource) = wanted_cursor() else {
        return;
    };
    let Some((hwnd, current)) = with_overlay(|overlay| (overlay.hwnd, overlay.cursor)) else {
        return;
    };
    if current == Some(resource) {
        return;
    }
    let (x, y) = screen_cursor();
    if unsafe { WindowFromPoint(POINT { x, y }) } == hwnd {
        show_cursor(resource);
    }
}

/// Attaches the input method while text is being edited and detaches it
/// otherwise; each once, on the change. The same rule as the Wayland
/// adapter's `zwp_text_input_v3` enable and disable.
fn sync_input_method() {
    let Some((hwnd, editing, enabled)) = with_overlay(|overlay| {
        (
            overlay.hwnd,
            overlay.controller.is_editing_text(),
            overlay.ime_enabled,
        )
    }) else {
        return;
    };
    if editing == enabled {
        return;
    }
    // A null context with no flags detaches; IACE_DEFAULT restores the
    // window's default context.
    let flags = if editing { IACE_DEFAULT } else { 0 };
    unsafe { ImmAssociateContextEx(hwnd, std::ptr::null_mut(), flags) };
    with_overlay(|overlay| overlay.ime_enabled = editing);
    if editing {
        place_candidate_window();
    }
}

/// Tells the input method where the caret is, so its candidate list opens next
/// to the text rather than wherever it guesses.
fn place_candidate_window() {
    let Some(Some((hwnd, x, y, height))) = with_overlay(|overlay| {
        let (at, _, height) = overlay.controller.caret_rectangle()?;
        let scale = overlay.scale.get();
        Some((
            overlay.hwnd,
            (at.x * scale).round() as i32,
            (at.y * scale).round() as i32,
            (height * scale).round() as i32,
        ))
    }) else {
        return;
    };
    let context = unsafe { ImmGetContext(hwnd) };
    if context.is_null() {
        return;
    }
    let empty = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let composition = COMPOSITIONFORM {
        dwStyle: CFS_POINT,
        ptCurrentPos: POINT { x, y },
        rcArea: empty,
    };
    // Below the line, so the list does not cover what is being composed.
    let candidate = CANDIDATEFORM {
        dwIndex: 0,
        dwStyle: CFS_CANDIDATEPOS,
        ptCurrentPos: POINT { x, y: y + height },
        rcArea: empty,
    };
    unsafe {
        ImmSetCompositionWindow(context, &composition);
        ImmSetCandidateWindow(context, &candidate);
        ImmReleaseContext(hwnd, context);
    }
}

fn apply(effects: Vec<Effect>) {
    for effect in effects {
        if matches!(effect, Effect::Quit) {
            quit();
            continue;
        }
        with_overlay(|overlay| match effect {
            Effect::ApplyMode { mode, transition } => {
                apply_mode(overlay, mode, transition);
            }
            Effect::WithdrawImmediately => {
                // FR-018: no confirmation is awaited. The surface stops taking
                // input now, and anything held is already cancelled by the
                // controller.
                unsafe { ShowWindow(overlay.hwnd, SW_HIDE) };
                overlay.hidden = true;
            }
            Effect::CommitObject { shape, style } => {
                commit(overlay, shape, style);
            }
            Effect::EraseAlong { path, radius } => {
                match overlay.session.erase_along(&path, radius) {
                    Ok(0) => eprintln!("[erase] the sweep touched nothing"),
                    Ok(count) => {
                        eprintln!("[erase] removed {count} object(s) as one undoable action");
                        overlay.needs_redraw = true;
                    }
                    Err(error) => eprintln!("[rejected] {error}"),
                }
            }
            Effect::Undo => match overlay.session.undo() {
                Ok(true) => overlay.needs_redraw = true,
                Ok(false) => eprintln!("[undo] nothing left to undo"),
                Err(error) => eprintln!("[rejected] {error}"),
            },
            Effect::Redo => match overlay.session.redo() {
                Ok(true) => overlay.needs_redraw = true,
                Ok(false) => eprintln!("[redo] nothing left to redo"),
                Err(error) => eprintln!("[rejected] {error}"),
            },
            Effect::Clear => match overlay.session.clear() {
                Ok(0) => eprintln!("[clear] there was nothing to clear"),
                Ok(count) => {
                    eprintln!("[clear] removed {count} object(s); undo brings them back");
                    overlay.needs_redraw = true;
                }
                Err(error) => eprintln!("[rejected] {error}"),
            },
            Effect::GestureDiscarded { reason } => {
                // FR-008: the user finished this one, so they are told why
                // nothing appeared instead of being left guessing.
                eprintln!("[discarded] {reason}");
                overlay.needs_redraw = true;
            }
            Effect::GestureCancelled { points } => {
                eprintln!("[cancelled] a gesture of {points} sample(s) was discarded");
                overlay.needs_redraw = true;
            }
            Effect::MoveSelection { ids, dx, dy } => {
                match overlay.session.move_objects(&ids, dx, dy) {
                    Ok(0) => {}
                    Ok(_) => overlay.needs_redraw = true,
                    Err(error) => eprintln!("[rejected] {error}"),
                }
            }
            Effect::ScaleSelection {
                ids,
                anchor,
                sx,
                sy,
            } => match overlay.session.scale_objects(&ids, anchor, sx, sy) {
                Ok(0) => {}
                Ok(_) => overlay.needs_redraw = true,
                Err(error) => eprintln!("[rejected] {error}"),
            },
            Effect::DeleteSelection { ids } => match overlay.session.delete(&ids) {
                Ok(0) => {}
                Ok(count) => {
                    eprintln!("[delete] removed {count} object(s); undo brings them back");
                    overlay.needs_redraw = true;
                }
                Err(error) => eprintln!("[rejected] {error}"),
            },
            Effect::Save => save(overlay),
            Effect::Load => load(overlay),
            // The window is top-most already; there is no menu to ask for.
            Effect::ShowWindowMenu { .. } => {}
            // Issued on the press, while the capture SetCapture took is still
            // held, so every motion until release reaches this window.
            Effect::BeginWindowDrag | Effect::BeginWindowResize => {
                let kind = if matches!(effect, Effect::BeginWindowDrag) {
                    DragKind::Move
                } else {
                    DragKind::Resize
                };
                overlay.drag = Some(WindowDrag {
                    kind,
                    from: screen_cursor(),
                    start: overlay.rect,
                });
            }
            Effect::Faulted { error } => {
                eprintln!("[failed {}] {error}", error.class());
            }
            Effect::Zoom { factor } => set_zoom(overlay, factor),
            Effect::ZoomUnavailable => {
                eprintln!("[zoom] not available: MagInitialize failed at startup");
            }
            Effect::Quit => unreachable!("handled before borrowing"),
        });
    }
}

fn apply_mode(overlay: &mut Overlay, mode: Mode, transition: TransitionId) {
    if mode == Mode::Hidden {
        unsafe { ShowWindow(overlay.hwnd, SW_HIDE) };
        overlay.hidden = true;
    } else {
        // Parked pins the window to the toolbar; anything else returns it to
        // the size it had.
        if mode == Mode::Parked {
            if overlay.restore.is_none() {
                overlay.restore = Some(overlay.rect);
            }
            let (width, height) =
                surface::parked_size(overlay.controller.toolbar().bounds(), overlay.scale.get());
            let parked = PixelRect {
                right: overlay.rect.left + width,
                bottom: overlay.rect.top + height,
                ..overlay.rect
            };
            place(overlay, parked);
        } else if let Some(full) = overlay.restore.take() {
            place(overlay, full);
        }

        // The whole pass-through mechanism: one extended style bit. The window
        // manager routes the click to whatever is underneath, so nothing is
        // forwarded or synthesised, which is what FR-003 requires and
        // AGENTS.md forbids faking. Parked is *not* transparent: the window is
        // only the toolbar by then, and the toolbar has to take clicks.
        let transparent = mode == Mode::PassThrough;
        let current = unsafe { GetWindowLongPtrW(overlay.hwnd, GWL_EXSTYLE) } as u32;
        let updated = if transparent {
            current | WS_EX_TRANSPARENT
        } else {
            current & !WS_EX_TRANSPARENT
        };
        unsafe { SetWindowLongPtrW(overlay.hwnd, GWL_EXSTYLE, updated as isize) };

        if overlay.hidden {
            unsafe { ShowWindow(overlay.hwnd, SW_SHOW) };
            overlay.hidden = false;
        }
    }

    eprintln!("[mode] {mode:?}");
    overlay.needs_redraw = true;

    // Reported as applied immediately, because every call above is
    // synchronous: unlike Wayland there is no round trip to wait for. The
    // transition id still matters, so a stale confirmation cannot overwrite a
    // newer one.
    let effects = overlay
        .controller
        .handle(PlatformEvent::ModeApplied { transition });
    if !effects.is_empty() {
        eprintln!("[mode] {} follow-up effect(s) were produced", effects.len());
    }
}

fn commit(overlay: &mut Overlay, shape: ink_core::Shape, style: Style) {
    let id = overlay.ids.next_id();
    let output = overlay.session.document().output().clone();
    match overlay.session.add(Object::new(id, output, style, shape)) {
        Ok(_) => overlay.needs_redraw = true,
        Err(error) => eprintln!("[rejected] {error}"),
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
            overlay.needs_redraw = true;
        }
        Err(error) => {
            // Nothing is adopted unless the whole file validated, so what is
            // on screen is still exactly what it was.
            eprintln!("[load {}] {error}", error.class());
            eprintln!("[load] what is on screen is unchanged");
        }
    }
}

/// Paints everything and hands the pixels to the window manager.
fn repaint() {
    with_overlay(|overlay| {
        if overlay.hidden {
            return;
        }

        let length = surface::buffer_len(overlay.width, overlay.height);
        // The renderer's premultiplied ARGB8888 little-endian is byte-for-byte
        // what a 32-bit DIB wants, so the canvas is built directly over the
        // bitmap's own memory and there is no copy and no conversion.
        let bytes = unsafe { std::slice::from_raw_parts_mut(overlay.pixels, length) };
        let Some(mut canvas) = Canvas::new(bytes, overlay.width, overlay.height) else {
            eprintln!("[paint] the canvas and the bitmap disagree about size");
            return;
        };

        // Every mode but PassThrough takes the pointer, and on a layered
        // window that means no pixel may have alpha zero. See `ink_ui::clear`.
        let mode = overlay.controller.mode();
        ink_ui::clear(&mut canvas, mode != Mode::PassThrough);

        let scale = overlay.scale;
        let output = overlay.session.document().output().clone();
        overlay.ink.paint(
            &mut canvas,
            &overlay.controller,
            overlay.session.document(),
            &mut overlay.painter,
            scale,
        );
        ink_ui::paint_preview(
            &mut canvas,
            &overlay.controller,
            &output,
            &mut overlay.painter,
            scale,
        );
        ink_ui::paint_chrome(
            &mut canvas,
            mode,
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
            // Not while Parked: the window is the toolbar and nothing else,
            // so there is no canvas corner to offer.
            if mode != Mode::Parked
                && let Some(corner) = overlay.controller.resize_corner()
            {
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

        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // Null destination: the window stays where SetWindowPos put it. The
        // first version passed (0, 0) here, which is the corner of the primary
        // monitor, and would have dragged the overlay back there from any
        // other monitor on every frame.
        let size = SIZE {
            cx: overlay.width as i32,
            cy: overlay.height as i32,
        };
        let source = POINT { x: 0, y: 0 };
        if unsafe {
            UpdateLayeredWindow(
                overlay.hwnd,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
                overlay.memory_dc,
                &source,
                COLORREF::default(),
                &blend,
                ULW_ALPHA,
            )
        } == 0
        {
            eprintln!(
                "[paint] UpdateLayeredWindow failed: {}",
                std::io::Error::last_os_error()
            );
        }
    });
}

fn print_orientation() {
    eprintln!();
    eprintln!("The overlay is the thin outlined rectangle; everything inside it is");
    eprintln!("transparent. The frame and the square in the corner are cyan in draw");
    eprintln!("mode and amber in pass-through.");
    eprintln!();
    eprintln!("  Ctrl+Alt+D   draw or pass through, from anywhere");
    eprintln!("  Ctrl+Alt+H   hide the ink, from anywhere");
    eprintln!("  Ctrl+Alt+Z   zoom 2x, 3x, 4x, from anywhere; Ctrl+Alt+0 back to normal");
    eprintln!("  yappyink toggle-draw, hide, save, quit, ...   the same, from a terminal");
    eprintln!();
    eprintln!("With the overlay focused: d/p/h modes, g park, 1-9 tools, u/r undo and");
    eprintln!("redo, w/o write and open, x clear, c colour, q quit.");
    eprintln!();
    eprintln!("Drag the toolbar's grip to move the overlay, and its bottom-right corner");
    eprintln!("to resize it. `yappyink draw --monitor 2` puts it on another monitor.");
    eprintln!();
}
