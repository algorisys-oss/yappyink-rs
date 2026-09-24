//! T003 step 1: can a layered top-most window meet the overlay contract on
//! Windows?
//!
//! Throwaway, like `experiments/gnome-xdg-shell`. It exists to be run on a real
//! Windows machine and produce evidence, not to become the adapter. Nothing
//! here should be imported by `ink-*`; the answers it produces go into an
//! evidence file, and the adapter is written afterwards from those.
//!
//! It deliberately depends on nothing from this workspace. The point is to test
//! what Windows does, and borrowing our own renderer would mean a failure could
//! be ours rather than the platform's.
//!
//! # What it is trying to find out
//!
//! - **Q1 (FR-001)** Does a layered window show per-pixel alpha above other
//!   applications while they keep updating underneath?
//! - **Q2 (FR-002)** In Draw, does the window receive clicks on *transparent*
//!   pixels, rather than them falling through?
//! - **Q3 (FR-003)** Does `WS_EX_TRANSPARENT` give real pass-through — the
//!   click reaching the application below, with no forwarding or synthesising
//!   on our part?
//! - **Q4 (FR-018)** With a mouse button held down, does switching mode leak
//!   that button into a click underneath?
//! - **Q5** Can we choose which monitor, and is per-monitor DPI reported
//!   correctly? Both are impossible on GNOME (see `docs/evidence/E002`), so
//!   this is where Windows is expected to be *better*, not merely equal.
//! - **Q6 (FR-005)** Does `RegisterHotKey` give a real global shortcut, and
//!   does it fail loudly when another application already owns the chord?
//!   Wayland has no equivalent at all.
//!
//! # A trap worth naming
//!
//! GDI text and most GDI drawing write zero into the alpha byte, which in a
//! per-pixel-alpha layered window means *invisible*. Everything here is written
//! into the DIB by hand as premultiplied BGRA for that reason. It is also a
//! genuine test of the format our renderer already produces.

fn main() -> std::process::ExitCode {
    #[cfg(windows)]
    {
        windows_probe::run()
    }
    #[cfg(not(windows))]
    {
        eprintln!(
            "This experiment measures Windows behaviour and only builds there. \
             It is compiled on {} as an empty program so that `cargo build \
             --workspace` keeps working on the development machine.",
            std::env::consts::OS
        );
        eprintln!("Build it on Windows with: cargo run -p exp-windows-layered");
        std::process::ExitCode::from(2)
    }
}

#[cfg(windows)]
mod windows_probe {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::process::ExitCode;

    use windows_sys::Win32::Foundation::{
        COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE as WSIZE, WPARAM,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
        CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject,
        EnumDisplayMonitors, GetDC, GetMonitorInfoW, HBITMAP, HDC, HMONITOR, MONITORINFO,
        ReleaseDC, SelectObject,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI,
        SetProcessDpiAwarenessContext,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWL_EXSTYLE, GetMessageW,
        GetWindowLongPtrW, IDC_ARROW, LoadCursorW, MSG, PostQuitMessage, RegisterClassW,
        SW_SHOWNOACTIVATE, SetWindowLongPtrW, ShowWindow, ULW_ALPHA, UpdateLayeredWindow,
        WM_DESTROY, WM_HOTKEY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WNDCLASSW,
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
        WS_POPUP,
    };

    /// Hot-key identifiers. Arbitrary, but distinct.
    const HOTKEY_TOGGLE: i32 = 1;
    const HOTKEY_QUIT: i32 = 2;
    /// `D` and `Q`.
    const VK_D: u32 = 0x44;
    const VK_Q: u32 = 0x51;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Mode {
        Draw,
        PassThrough,
    }

    struct Probe {
        hwnd: HWND,
        width: i32,
        height: i32,
        origin: (i32, i32),
        /// The DIB's pixels, premultiplied BGRA, top-down.
        pixels: *mut u8,
        bitmap: HBITMAP,
        memory_dc: HDC,
        mode: Mode,
        /// Committed strokes, in window-relative pixels.
        strokes: Vec<Vec<(i32, i32)>>,
        drawing: Option<Vec<(i32, i32)>>,
        /// Set when a mode change happened with the button still held. Q4.
        released_after_transition: bool,
    }

    thread_local! {
        static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) };
    }

    pub fn run() -> ExitCode {
        // Per-monitor v2, before any window exists. Without it Windows lies
        // about monitor geometry on a scaled display and the overlay would be
        // stretched by the compositor rather than drawn at native pixels.
        let dpi_aware =
            unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        println!(
            "[dpi] per-monitor-v2 awareness: {}",
            if dpi_aware != 0 {
                "set"
            } else {
                "REFUSED (already set by a manifest, or unsupported)"
            }
        );

        let monitors = enumerate_monitors();
        if monitors.is_empty() {
            eprintln!("[failed] no monitors were enumerated, which should be impossible");
            return ExitCode::FAILURE;
        }
        println!("[monitors] {} found", monitors.len());
        for (index, monitor) in monitors.iter().enumerate() {
            println!(
                "[monitors]   {index}: {}x{} at ({},{}), {} dpi{}",
                monitor.width,
                monitor.height,
                monitor.x,
                monitor.y,
                monitor.dpi,
                if monitor.primary { ", primary" } else { "" }
            );
        }

        // Q5: choosing an output. On GNOME this is impossible by any route.
        let wanted: usize = std::env::args()
            .nth(1)
            .and_then(|a| a.parse().ok())
            .unwrap_or(0);
        let target = monitors.get(wanted).unwrap_or(&monitors[0]);
        println!(
            "[monitors] asked for {wanted}, using {}x{} at ({},{})",
            target.width, target.height, target.x, target.y
        );

        match create_overlay(target) {
            Ok(()) => {}
            Err(reason) => {
                eprintln!("[failed] {reason}");
                return ExitCode::FAILURE;
            }
        }

        print_instructions();
        pump_messages();
        report();
        ExitCode::SUCCESS
    }

    struct Monitor {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        dpi: u32,
        primary: bool,
    }

    fn enumerate_monitors() -> Vec<Monitor> {
        // Collected through a raw pointer because the callback is `extern
        // "system"` and cannot capture. The Vec outlives the enumeration, which
        // is synchronous, so the pointer stays valid for its whole life.
        let mut found: Vec<Monitor> = Vec::new();
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(monitor_callback),
                &mut found as *mut Vec<Monitor> as isize,
            );
        }
        found
    }

    unsafe extern "system" fn monitor_callback(
        monitor: HMONITOR,
        _dc: HDC,
        _rect: *mut RECT,
        userdata: LPARAM,
    ) -> i32 {
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            // Reported rather than skipped silently: a monitor we cannot
            // describe is a finding, not a non-event.
            eprintln!("[monitors] GetMonitorInfoW failed for one monitor; it is omitted");
            return 1;
        }

        let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
        let dpi = if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }
            == 0
        {
            dpi_x
        } else {
            0
        };

        let area = info.rcMonitor;
        let list = unsafe { &mut *(userdata as *mut Vec<Monitor>) };
        list.push(Monitor {
            x: area.left,
            y: area.top,
            width: area.right - area.left,
            height: area.bottom - area.top,
            dpi,
            // MONITORINFOF_PRIMARY
            primary: info.dwFlags & 1 != 0,
        });
        1
    }

    fn create_overlay(target: &Monitor) -> Result<(), String> {
        let class_name: Vec<u16> = "YappyinkProbe\0".encode_utf16().collect();
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };

        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance as _,
            hIcon: std::ptr::null_mut(),
            hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
            // Null: a layered window's pixels come entirely from
            // UpdateLayeredWindow, and a background brush would be painted
            // opaque underneath them.
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err(format!(
                "RegisterClassW failed, error {}",
                std::io::Error::last_os_error()
            ));
        }

        // WS_EX_LAYERED     per-pixel alpha, which is the whole overlay.
        // WS_EX_TOPMOST     above other windows without asking the user, which
        //                   is the thing GNOME cannot do (E002).
        // WS_EX_NOACTIVATE  never take focus, so clicking the overlay does not
        //                   deactivate whatever the user was working in. It
        //                   also means we get no keyboard, which is why the
        //                   controls are global hot keys.
        // WS_EX_TOOLWINDOW  keep it out of the taskbar and Alt-Tab.
        let ex_style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
        let title: Vec<u16> = "yappyink probe\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                target.x,
                target.y,
                target.width,
                target.height,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance as _,
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err(format!(
                "CreateWindowExW failed, error {}",
                std::io::Error::last_os_error()
            ));
        }

        let (bitmap, memory_dc, pixels) = create_dib(target.width, target.height)?;

        PROBE.with(|slot| {
            *slot.borrow_mut() = Some(Probe {
                hwnd,
                width: target.width,
                height: target.height,
                origin: (target.x, target.y),
                pixels,
                bitmap,
                memory_dc,
                mode: Mode::Draw,
                strokes: Vec::new(),
                drawing: None,
                released_after_transition: true,
            });
        });

        register_hotkeys(hwnd);
        repaint();
        unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
        Ok(())
    }

    /// A top-down 32-bit DIB, which is the only surface `UpdateLayeredWindow`
    /// will take per-pixel alpha from.
    fn create_dib(width: i32, height: i32) -> Result<(HBITMAP, HDC, *mut u8), String> {
        let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative means top-down, so row 0 is the top. A positive height
            // gives a bottom-up bitmap and a vertically mirrored overlay.
            biHeight: -height,
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
            return Err("CreateDIBSection returned nothing".to_owned());
        }
        unsafe { SelectObject(memory_dc, bitmap as _) };
        Ok((bitmap, memory_dc, bits.cast::<u8>()))
    }

    fn register_hotkeys(hwnd: HWND) {
        // Q6, and FR-005's "registration/conflict feedback". A chord another
        // application already owns fails here with a real error, which is
        // exactly the feedback Wayland cannot give because it has no
        // registration at all.
        let modifiers = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
        for (id, key, name) in [
            (HOTKEY_TOGGLE, VK_D, "Ctrl+Alt+D"),
            (HOTKEY_QUIT, VK_Q, "Ctrl+Alt+Q"),
        ] {
            if unsafe { RegisterHotKey(hwnd, id, modifiers, key) } == 0 {
                println!(
                    "[hotkey] {name} REFUSED: {}. Another application owns it.",
                    std::io::Error::last_os_error()
                );
            } else {
                println!("[hotkey] {name} registered");
            }
        }
    }

    fn print_instructions() {
        println!();
        println!("A translucent frame should now be above everything, with a filled");
        println!("square in the top-left corner: cyan in draw mode, amber in pass-through.");
        println!("The inside is fully transparent.");
        println!();
        println!("  Ctrl+Alt+D   switch between draw and pass-through");
        println!("  Ctrl+Alt+Q   quit");
        println!();
        println!("What to check, and please record failures as carefully as successes:");
        println!("  Q1  Is the desktop underneath visible through it and still updating?");
        println!("  Q2  In draw mode, does dragging inside the empty middle draw a line,");
        println!("      rather than clicking whatever is underneath?");
        println!("  Q3  In pass-through, does clicking do something to the window below,");
        println!("      while the ink stays visible?");
        println!("  Q4  Hold the left button down, press Ctrl+Alt+D, then release.");
        println!("      Does anything underneath get clicked? It must not.");
        println!("  Q5  Pass a monitor number as an argument and check it appears there.");
        println!();
    }

    fn pump_messages() {
        let mut message: MSG = unsafe { std::mem::zeroed() };
        // No TranslateMessage: this window never has keyboard focus, so there
        // are no key messages to translate. Every control is a hot key.
        while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
            unsafe { DispatchMessageW(&message) };
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_LBUTTONDOWN => {
                with_probe(|probe| {
                    if probe.mode == Mode::Draw {
                        probe.released_after_transition = false;
                        probe.drawing = Some(vec![point_of(lparam)]);
                    }
                });
                0
            }
            WM_MOUSEMOVE => {
                let mut changed = false;
                with_probe(|probe| {
                    if let Some(stroke) = probe.drawing.as_mut() {
                        stroke.push(point_of(lparam));
                        changed = true;
                    }
                });
                if changed {
                    repaint();
                }
                0
            }
            WM_LBUTTONUP => {
                with_probe(|probe| {
                    probe.released_after_transition = true;
                    if let Some(stroke) = probe.drawing.take() {
                        probe.strokes.push(stroke);
                    }
                });
                repaint();
                0
            }
            WM_HOTKEY => {
                match wparam as i32 {
                    HOTKEY_TOGGLE => toggle_mode(),
                    HOTKEY_QUIT => unsafe {
                        DestroyWindow(hwnd);
                    },
                    _ => {}
                }
                0
            }
            WM_DESTROY => {
                unsafe {
                    UnregisterHotKey(hwnd, HOTKEY_TOGGLE);
                    UnregisterHotKey(hwnd, HOTKEY_QUIT);
                    PostQuitMessage(0);
                }
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    fn point_of(lparam: LPARAM) -> (i32, i32) {
        let x = (lparam & 0xFFFF) as i16 as i32;
        let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
        (x, y)
    }

    fn with_probe(f: impl FnOnce(&mut Probe)) {
        PROBE.with(|slot| {
            if let Some(probe) = slot.borrow_mut().as_mut() {
                f(probe);
            }
        });
    }

    fn toggle_mode() {
        let mut leaked_button = false;
        let (hwnd, next) = PROBE.with(|slot| {
            let mut borrowed = slot.borrow_mut();
            let probe = borrowed.as_mut().expect("the window exists");

            // Q4/FR-018: a stroke in flight is abandoned rather than committed,
            // and the fact that a button was down across the change is
            // recorded so the run can report it.
            if probe.drawing.take().is_some() {
                leaked_button = true;
            }
            probe.mode = match probe.mode {
                Mode::Draw => Mode::PassThrough,
                Mode::PassThrough => Mode::Draw,
            };
            (probe.hwnd, probe.mode)
        });

        // The entire pass-through mechanism: one extended style bit. Windows
        // routes the input to whatever is underneath itself, so nothing is
        // forwarded or synthesised, which is what FR-003 requires.
        let current = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
        let updated = match next {
            Mode::PassThrough => current | WS_EX_TRANSPARENT,
            Mode::Draw => current & !WS_EX_TRANSPARENT,
        };
        unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated as isize) };

        println!("[mode] {next:?}");
        if leaked_button {
            println!(
                "[mode] a stroke was in flight and was discarded. Check nothing underneath \
                 was clicked when you released."
            );
        }
        repaint();
    }

    /// Paints the whole surface and hands it to the compositor.
    fn repaint() {
        PROBE.with(|slot| {
            let mut borrowed = slot.borrow_mut();
            let Some(probe) = borrowed.as_mut() else {
                return;
            };

            let count = (probe.width * probe.height) as usize;
            let buffer =
                unsafe { std::slice::from_raw_parts_mut(probe.pixels, count.saturating_mul(4)) };
            buffer.fill(0);

            let (frame, badge) = match probe.mode {
                // Premultiplied: with alpha 0.5 the colour channels are halved
                // too. Writing full-intensity colour with a half alpha is the
                // classic layered-window mistake and shows as a bright halo.
                Mode::Draw => ((0, 96, 96, 192), (0, 128, 128, 255)),
                Mode::PassThrough => ((0, 64, 96, 192), (0, 96, 160, 255)),
            };

            let width = probe.width;
            let height = probe.height;
            let mut put = |x: i32, y: i32, c: (u8, u8, u8, u8)| {
                if x < 0 || y < 0 || x >= width || y >= height {
                    return;
                }
                let offset = ((y * width + x) * 4) as usize;
                // BGRA order, premultiplied, which is what a 32-bit DIB and
                // UpdateLayeredWindow expect.
                buffer[offset] = c.0;
                buffer[offset + 1] = c.1;
                buffer[offset + 2] = c.2;
                buffer[offset + 3] = c.3;
            };

            // A frame, so the thing can be found on screen at all. The first
            // Wayland overlay was fully transparent and could not be located,
            // which made it unusable; see docs/learning.md §2.
            for thickness in 0..6 {
                for x in 0..width {
                    put(x, thickness, frame);
                    put(x, height - 1 - thickness, frame);
                }
                for y in 0..height {
                    put(thickness, y, frame);
                    put(width - 1 - thickness, y, frame);
                }
            }

            // Mode badge: cyan for draw, amber for pass-through, matching the
            // Wayland build so the two can be described the same way.
            for y in 12..60 {
                for x in 12..60 {
                    put(x, y, badge);
                }
            }

            let ink = (0, 0, 255, 255);
            let strokes = probe.strokes.iter().chain(probe.drawing.iter());
            for stroke in strokes {
                for pair in stroke.windows(2) {
                    draw_line(pair[0], pair[1], ink, &mut put);
                }
                if stroke.len() == 1 {
                    draw_line(stroke[0], stroke[0], ink, &mut put);
                }
            }

            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let position = POINT {
                x: probe.origin.0,
                y: probe.origin.1,
            };
            let size = WSIZE {
                cx: probe.width,
                cy: probe.height,
            };
            let source = POINT { x: 0, y: 0 };

            let ok = unsafe {
                UpdateLayeredWindow(
                    probe.hwnd,
                    std::ptr::null_mut(),
                    &position,
                    &size,
                    probe.memory_dc,
                    &source,
                    COLORREF::default(),
                    &blend,
                    ULW_ALPHA,
                )
            };
            if ok == 0 {
                eprintln!(
                    "[paint] UpdateLayeredWindow failed: {}",
                    std::io::Error::last_os_error()
                );
            }
        });
    }

    /// Bresenham, thickened into a small square per step.
    ///
    /// Crude on purpose. This is measuring the platform, and a nicer
    /// rasteriser already exists in `ink-render` for when the adapter is real.
    fn draw_line(
        from: (i32, i32),
        to: (i32, i32),
        colour: (u8, u8, u8, u8),
        put: &mut impl FnMut(i32, i32, (u8, u8, u8, u8)),
    ) {
        let (mut x, mut y) = from;
        let dx = (to.0 - x).abs();
        let dy = -(to.1 - y).abs();
        let sx = if x < to.0 { 1 } else { -1 };
        let sy = if y < to.1 { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            for oy in -2..=2 {
                for ox in -2..=2 {
                    put(x + ox, y + oy, colour);
                }
            }
            if x == to.0 && y == to.1 {
                break;
            }
            let doubled = 2 * error;
            if doubled >= dy {
                error += dy;
                x += sx;
            }
            if doubled <= dx {
                error += dx;
                y += sy;
            }
        }
    }

    fn report() {
        PROBE.with(|slot| {
            if let Some(probe) = slot.borrow_mut().take() {
                println!();
                println!("[exit] {} stroke(s) were drawn.", probe.strokes.len());
                if !probe.released_after_transition {
                    println!(
                        "[exit] a button was still held when the run ended, which is worth \
                         noting against Q4."
                    );
                }
                unsafe {
                    DeleteDC(probe.memory_dc);
                    DeleteObject(probe.bitmap as _);
                }
                println!("[exit] the window and its bitmap were released.");
            }
        });
        println!("[exit] Record the answers in docs/evidence/, including whatever failed.");
    }
}
