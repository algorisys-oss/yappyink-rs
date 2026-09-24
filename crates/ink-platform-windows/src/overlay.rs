//! The Win32 overlay window.
//!
//! This is the part that cannot be tested without Windows, and it is kept as
//! small as it can be for that reason: the key bindings live in [`crate::keys`]
//! and the buffer arithmetic in [`crate::surface`], both of which are plain
//! integers and run their tests everywhere.
//!
//! # What is here, and what is not
//!
//! Drawing, the tools, mode switching, undo and redo, save and load, a global
//! toggle through `RegisterHotKey`, and the full toolbar — the same one the
//! Wayland backend draws, from the same code in `ink-ui`. It was lifted there
//! rather than copied: a toolbar that behaves differently per platform is two
//! products that merely resemble each other.
//!
//! Still missing: the text tool's preedit underline, which needs the font
//! metrics the Wayland adapter reaches for inline, and any input-method
//! integration at all.
//!
//! # Differences from the Wayland adapter, on purpose
//!
//! - **No control socket.** On Wayland it exists because the compositor will
//!   not give a client a global shortcut, so once pass-through is on there is
//!   no way back from inside the process. `RegisterHotKey` removes that
//!   problem at the source, so the recovery route here is a real chord.
//! - **The window takes focus in Draw.** `WS_EX_NOACTIVATE` would stop the
//!   overlay ever stealing focus, which is attractive, but it also means no
//!   keyboard at all, and the text tool needs a keyboard. In PassThrough the
//!   window is click-through, so the application underneath holds focus
//!   anyway, which is the case that actually matters.
//!
//! **None of this has been run.** Every capability stays `unknown` until it is.

use std::cell::RefCell;
use std::ffi::c_void;

use ink_app::{Action, Controller, Effect, Mode, PlatformEvent, Preview, TransitionId};
use ink_core::{IdSource, LogicalPoint, LogicalSize, Object, OutputId, Session, Shape, Style};
use ink_platform::PlatformError;
use ink_render::{Canvas, Painter, Scale};

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HBITMAP,
    HDC, ReleaseDC, SelectObject,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, ReleaseCapture, SetCapture,
    UnregisterHotKey,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWL_EXSTYLE, GetMessageW,
    GetSystemMetrics, GetWindowLongPtrW, IDC_CROSS, LoadCursorW, MSG, PostQuitMessage,
    RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_SHOW, SetWindowLongPtrW, ShowWindow,
    TranslateMessage, ULW_ALPHA, UpdateLayeredWindow, WM_CAPTURECHANGED, WM_CHAR, WM_DESTROY,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_HOTKEY, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::keys;
use crate::surface;

/// The global chord that brings the overlay back.
///
/// The counterpart to Wayland's control socket, and better: it is bound by this
/// process rather than by the user editing their desktop settings, and Windows
/// says so when another application already owns it.
const HOTKEY_TOGGLE_DRAW: i32 = 1;
const HOTKEY_HIDE: i32 = 2;

/// How the overlay is asked to start.
pub struct OverlayConfig {
    /// Requested size in logical units. Ignored if it does not fit the
    /// monitor, in which case the monitor wins and a note is printed.
    pub size: LogicalSize,
}

struct Overlay {
    hwnd: HWND,
    width: u32,
    height: u32,
    pixels: *mut u8,
    bitmap: HBITMAP,
    memory_dc: HDC,
    scale: Scale,

    controller: Controller,
    session: Session,
    ids: IdSource,
    painter: Painter,

    needs_redraw: bool,
    hidden: bool,
    pass_through: bool,
    /// Where the pointer was last seen, for the text tool's caret hint.
    ///
    /// Kept rather than read on demand because the hint follows the pointer
    /// and has to be redrawn when the *tool* changes as well as when the
    /// pointer moves. Deciding exactly when something can change is what
    /// produced six repaint bugs on the other backend; keeping the value and
    /// recomputing cheaply is the lesson from those (docs/learning.md §12).
    last_pointer: Option<LogicalPoint>,
}

thread_local! {
    static OVERLAY: RefCell<Option<Overlay>> = const { RefCell::new(None) };
}

/// Runs the overlay until it is asked to quit.
pub fn run(config: OverlayConfig) -> Result<Session, PlatformError> {
    // Before any window exists, or Windows reports scaled coordinates and the
    // overlay is stretched by the compositor rather than drawn at native
    // pixels.
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } == 0 {
        eprintln!("[dpi] per-monitor-v2 awareness was refused; coordinates may be scaled");
    }

    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(1) as u32;
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(1) as u32;
    let width = screen_width.min(config.size.width().max(1.0) as u32);
    let height = screen_height.min(config.size.height().max(1.0) as u32);
    eprintln!("[output] primary monitor {screen_width}x{screen_height}, overlay {width}x{height}");

    create(width, height)?;
    print_orientation();
    pump();
    finish()
}

fn create(width: u32, height: u32) -> Result<(), PlatformError> {
    let class_name: Vec<u16> = "YappyinkOverlay\0".encode_utf16().collect();
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };

    let class = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance as _,
        hIcon: std::ptr::null_mut(),
        hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_CROSS) },
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

    // No WS_EX_NOACTIVATE: see the module documentation. The text tool needs a
    // keyboard, and a window that never activates never gets one.
    let ex_style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
    let title: Vec<u16> = "yappyink\0".encode_utf16().collect();
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            width as i32,
            height as i32,
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

    let (bitmap, memory_dc, pixels) = create_dib(width, height)?;

    // The window is in physical pixels, because this process is
    // per-monitor-v2 aware, so the document's logical size is that divided by
    // the scale. On a 150% display, which is ordinary on Windows, getting this
    // wrong makes every logical coordinate two thirds of what it should be,
    // and the symptom is a toolbar drawn at the wrong size rather than an
    // error anyone would notice in a log.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let factor = surface::scale_for_dpi(dpi);
    let scale = Scale::new(factor).unwrap_or(Scale::ONE);
    eprintln!("[dpi] {dpi} dpi, scale {factor}");

    let output = OutputId::new("primary");
    let size = LogicalSize::new(f64::from(width) / factor, f64::from(height) / factor).ok_or_else(
        || PlatformError::unsupported("size the overlay", "the monitor reported a size of zero"),
    )?;

    let painter = match ink_render::text::TextFont::discover() {
        Ok(font) => {
            eprintln!("[font] {}", font.source().display());
            for fallback in font.fallbacks() {
                eprintln!("[font] fallback {}", fallback.display());
            }
            Painter::new().with_font(font)
        }
        Err(reason) => {
            // Not fatal: everything except the text tool works without a font.
            eprintln!("[font] no usable font was found, so the text tool is unavailable: {reason}");
            Painter::new()
        }
    };

    OVERLAY.with(|slot| {
        *slot.borrow_mut() = Some(Overlay {
            hwnd,
            width,
            height,
            pixels,
            bitmap,
            memory_dc,
            scale,
            controller: Controller::new(),
            session: Session::new(output, size),
            ids: IdSource::starting_at(1),
            painter,
            needs_redraw: true,
            hidden: false,
            pass_through: false,
            last_pointer: None,
        });
    });

    register_hotkeys(hwnd);
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
    // which is the registration feedback the requirement asks for and which
    // Wayland cannot give because it has no registration to refuse.
    let modifiers = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
    for (id, key, name) in [
        (HOTKEY_TOGGLE_DRAW, keys::vk::D, "Ctrl+Alt+D"),
        (HOTKEY_HIDE, keys::vk::H, "Ctrl+Alt+H"),
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
        // Needed for WM_CHAR, which is how typed text reaches the text tool.
        // Turning a key press into a character depends on the active keyboard
        // layout and is the OS's job, not ours.
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
            DeleteDC(overlay.memory_dc);
            DeleteObject(overlay.bitmap as _);
        }
        Ok(overlay.session)
    })
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_LBUTTONDOWN => {
            // Capture, so a drag that leaves the window still reports its end.
            // Without it a stroke released outside never finishes and the
            // gesture is left open forever.
            unsafe { SetCapture(hwnd) };
            dispatch(PlatformEvent::PointerDown { at: point(lparam) });
            0
        }
        WM_MOUSEMOVE => {
            dispatch(PlatformEvent::PointerMoved { at: point(lparam) });
            0
        }
        WM_LBUTTONUP => {
            unsafe { ReleaseCapture() };
            dispatch(PlatformEvent::PointerUp { at: point(lparam) });
            0
        }
        // FR-018. The pointer was taken away mid-gesture; the stroke is
        // cancelled rather than committed from wherever it happened to be.
        WM_CAPTURECHANGED | WM_KILLFOCUS => {
            dispatch(PlatformEvent::PointerCancelled);
            0
        }
        WM_KEYDOWN => {
            on_key(hwnd, wparam as u32);
            0
        }
        WM_CHAR => {
            on_char(wparam as u32);
            0
        }
        WM_HOTKEY => {
            let action = match wparam as i32 {
                HOTKEY_TOGGLE_DRAW => Some(Action::ToggleDraw),
                HOTKEY_HIDE => Some(Action::ToggleVisibility),
                _ => None,
            };
            if let Some(action) = action {
                act(action);
            }
            0
        }
        // The window moved to a monitor with a different scale. The pixels
        // stay the same size; what changes is how many logical units they
        // are, so only the scale is updated. Not resizing the surface with it
        // is a known gap, recorded rather than papered over.
        WM_DPICHANGED => {
            on_dpi_changed(hwnd);
            0
        }
        // FR-019/FR-022: the monitor arrangement changed under us. Reported
        // rather than guessed at, because resizing the surface is real work
        // and pretending otherwise would draw at the wrong size.
        WM_DISPLAYCHANGE => {
            eprintln!(
                "[output] the display configuration changed. The overlay keeps its size; \
                 following a resize is not implemented (T018)."
            );
            0
        }
        WM_DESTROY => {
            unsafe {
                UnregisterHotKey(hwnd, HOTKEY_TOGGLE_DRAW);
                UnregisterHotKey(hwnd, HOTKEY_HIDE);
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Window-relative pixels, turned into logical units.
///
/// Signed, so a drag off the left edge is negative rather than 65000. Divided
/// by the scale, because Win32 reports physical pixels and everything above
/// the adapter works in logical units. Skipping that puts the pointer in the
/// wrong place on any scaled display, and the toolbar stops being clickable
/// where it is drawn.
fn point(lparam: LPARAM) -> LogicalPoint {
    let x = f64::from((lparam & 0xFFFF) as i16);
    let y = f64::from(((lparam >> 16) & 0xFFFF) as i16);
    let factor = OVERLAY.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(1.0, |overlay| overlay.scale.get())
    });
    // Both came from an i16 and the scale is finite and positive, so this
    // cannot fail.
    LogicalPoint::new(x / factor, y / factor).expect("a finite coordinate")
}

/// The window moved to a monitor with a different scale.
fn on_dpi_changed(hwnd: HWND) {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let factor = surface::scale_for_dpi(dpi);
    OVERLAY.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(overlay) = borrowed.as_mut() else {
            return;
        };
        overlay.scale = Scale::new(factor).unwrap_or(Scale::ONE);
        overlay.needs_redraw = true;
    });
    eprintln!("[dpi] changed to {dpi} dpi, scale {factor}");
    eprintln!("[dpi] the surface keeps its pixel size; resizing with the move is T018");
    flush();
}

fn on_key(hwnd: HWND, virtual_key: u32) {
    let editing = OVERLAY.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|overlay| overlay.controller.is_editing_text())
    });

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
        unsafe { DestroyWindow(hwnd) };
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

fn act(action: Action) {
    let effects = OVERLAY.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .map(|overlay| overlay.controller.act(action))
            .unwrap_or_default()
    });
    apply(effects);
    flush();
}

fn dispatch(event: PlatformEvent) {
    let effects = OVERLAY.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(overlay) = borrowed.as_mut() else {
            return Vec::new();
        };
        // Recorded before the controller sees the event, and compared against
        // the tool afterwards, so the caret hint redraws when either changes.
        let moved = match event {
            PlatformEvent::PointerDown { at }
            | PlatformEvent::PointerMoved { at }
            | PlatformEvent::PointerUp { at } => {
                let changed = overlay.last_pointer != Some(at);
                overlay.last_pointer = Some(at);
                changed
            }
            _ => false,
        };
        if moved && overlay.controller.tool() == ink_app::Tool::Text {
            overlay.needs_redraw = true;
        }
        overlay.controller.handle(event)
    });
    apply(effects);
    flush();
}

/// Repaints if anything asked for it.
///
/// Called after every message rather than only where a redraw seems likely.
/// Working out exactly when the screen can change is what produced six
/// separate "correct in the model, absent on screen" bugs in the Wayland
/// adapter (`docs/learning.md` §1 and §12); recomputing and making the write
/// cheap is the lesson from those.
fn flush() {
    let wanted = OVERLAY.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .is_some_and(|overlay| std::mem::take(&mut overlay.needs_redraw))
    });
    if wanted {
        repaint();
    }
}

fn apply(effects: Vec<Effect>) {
    for effect in effects {
        OVERLAY.with(|slot| {
            let mut borrowed = slot.borrow_mut();
            let Some(overlay) = borrowed.as_mut() else {
                return;
            };
            match effect {
                Effect::ApplyMode { mode, transition } => {
                    apply_mode(overlay, mode, transition);
                }
                Effect::WithdrawImmediately => {
                    // FR-018: no confirmation is awaited. The surface stops
                    // taking input now, and anything held is already cancelled
                    // by the controller.
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
                Effect::Quit => unsafe {
                    DestroyWindow(overlay.hwnd);
                },
                // Wayland needs these because the compositor owns window
                // placement and the Always on Top setting. Windows does not:
                // the overlay is already top-most and covers the monitor, so
                // there is nothing to ask a window manager for.
                Effect::ShowWindowMenu { .. }
                | Effect::BeginWindowDrag
                | Effect::BeginWindowResize => {}
                Effect::Faulted { error } => {
                    eprintln!("[failed {}] {error}", error.class());
                }
            }
        });
    }
}

fn apply_mode(overlay: &mut Overlay, mode: Mode, transition: TransitionId) {
    let show = |hwnd: HWND, visible: bool| unsafe {
        ShowWindow(hwnd, if visible { SW_SHOW } else { SW_HIDE });
    };

    match mode {
        Mode::Hidden => {
            show(overlay.hwnd, false);
            overlay.hidden = true;
        }
        Mode::Draw | Mode::PassThrough | Mode::Parked => {
            if overlay.hidden {
                show(overlay.hwnd, true);
                overlay.hidden = false;
            }
            // The whole pass-through mechanism: one extended style bit. The
            // window manager routes the click to whatever is underneath, so
            // nothing is forwarded or synthesised, which is what FR-003
            // requires and AGENTS.md forbids faking.
            let transparent = mode != Mode::Draw;
            let current = unsafe { GetWindowLongPtrW(overlay.hwnd, GWL_EXSTYLE) } as u32;
            let updated = if transparent {
                current | WS_EX_TRANSPARENT
            } else {
                current & !WS_EX_TRANSPARENT
            };
            unsafe { SetWindowLongPtrW(overlay.hwnd, GWL_EXSTYLE, updated as isize) };
            overlay.pass_through = transparent;
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
    OVERLAY.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(overlay) = borrowed.as_mut() else {
            return;
        };
        if overlay.hidden {
            return;
        }

        let length = surface::buffer_len(overlay.width, overlay.height);
        // The renderer's premultiplied ARGB8888 little-endian is byte-for-byte
        // what a 32-bit DIB wants, so the canvas is built directly over the
        // bitmap's own memory and there is no copy and no conversion. See
        // `surface` for why that is written down rather than assumed.
        let bytes = unsafe { std::slice::from_raw_parts_mut(overlay.pixels, length) };
        debug_assert!(surface::layout_matches(
            bytes.len(),
            overlay.width,
            overlay.height
        ));
        let Some(mut canvas) = Canvas::new(bytes, overlay.width, overlay.height) else {
            eprintln!("[paint] the canvas and the bitmap disagree about size");
            return;
        };
        canvas.clear();

        overlay
            .painter
            .paint(overlay.session.document(), &mut canvas, overlay.scale);

        // A gesture in flight, so a stroke appears while it is being drawn
        // rather than only on release. The Wayland adapter learned this one
        // the hard way: "are there freehand samples?" is not the same question
        // as "is something being drawn", and shapes stayed invisible until the
        // button came up (docs/learning.md §1).
        let built = match overlay.controller.preview() {
            Some(Preview::Stroke {
                points,
                kind,
                style,
            }) => Shape::stroke(kind, points.to_vec())
                .ok()
                .map(|shape| (shape, style)),
            Some(Preview::Shape { shape, style }) => Some((shape, style)),
            // The eraser's sweep, drawn faint and at the full width it will
            // clear, so the user can see what it is about to take. It was
            // missing entirely on Wayland for days because the redraw test
            // asked the wrong question (docs/learning.md §3).
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
            // Text in progress needs a caret and the preedit underline, which
            // live in the Wayland adapter's private chrome. It arrives with
            // the shared chrome rather than being copied into a second place.
            Some(Preview::Text { .. }) | None => None,
        };
        if let Some((shape, style)) = built {
            let id = overlay.ids.next_id();
            let output = overlay.session.document().output().clone();
            let object = Object::new(id, output, style, shape);
            overlay
                .painter
                .paint_object(&object, &mut canvas, overlay.scale);
        }

        // The same chrome the Wayland backend draws, from the same code. It
        // was lifted into `ink-ui` rather than copied, because a toolbar that
        // behaves differently per platform is two products (see that crate's
        // documentation).
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
            overlay.scale,
        );
        ink_ui::paint_selection(
            &mut canvas,
            &overlay.controller,
            overlay.session.document(),
            &mut overlay.painter,
            overlay.scale,
        );

        if overlay.controller.toolbar_visible() {
            ink_ui::paint_toolbar(
                &mut canvas,
                overlay.controller.toolbar(),
                overlay.controller.tool(),
                overlay.controller.style().color,
                overlay.scale,
            );
            if let Some(corner) = overlay.controller.resize_corner() {
                ink_ui::paint_resize_corner(&mut canvas, corner, overlay.scale);
            }
            ink_ui::paint_swatches(&mut canvas, &overlay.controller, overlay.scale);
            if let Some(button) = overlay.controller.hovered_button() {
                ink_ui::paint_tooltip(
                    &mut canvas,
                    button,
                    overlay.controller.toolbar().bounds(),
                    overlay.scale,
                );
            }
        }

        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let position = POINT { x: 0, y: 0 };
        let size = SIZE {
            cx: overlay.width as i32,
            cy: overlay.height as i32,
        };
        let source = POINT { x: 0, y: 0 };
        if unsafe {
            UpdateLayeredWindow(
                overlay.hwnd,
                std::ptr::null_mut(),
                &position,
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
    eprintln!("transparent. The small square in the corner is amber in draw mode and");
    eprintln!("cyan in pass-through.");
    eprintln!();
    eprintln!("  Ctrl+Alt+D   draw or pass through, from anywhere");
    eprintln!("  Ctrl+Alt+H   hide the ink, from anywhere");
    eprintln!();
    eprintln!("With the overlay focused: d/p/h modes, 1-9 tools, u/r undo and redo,");
    eprintln!("w/o write and open, x clear, c colour, q quit.");
    eprintln!();
    eprintln!("The toolbar is the same one the Linux build draws, from the same code.");
    eprintln!();
}
