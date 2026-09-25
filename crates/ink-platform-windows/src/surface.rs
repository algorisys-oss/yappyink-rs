//! The bridge between our canvas and a Win32 DIB.
//!
//! Also free of Windows types, and for the same reason: everything here is
//! arithmetic about buffer layout, which is decidable without an operating
//! system. Only the calls that hand the buffer to the window manager need
//! Win32, and those live in `overlay`.
//!
//! # The one fact this module exists to pin down
//!
//! `ink-render` produces **premultiplied ARGB8888, little-endian**, which in
//! memory is the byte order `[B, G, R, A]`. A 32-bit Windows DIB drawn through
//! `UpdateLayeredWindow` with `AC_SRC_ALPHA` wants **premultiplied BGRA**,
//! which is the same byte order.
//!
//! So there is no conversion: the canvas can be written straight into the DIB's
//! pixels. That is worth stating loudly, because "no code" is exactly the kind
//! of claim that rots silently if either side ever changes its convention, and
//! the symptom would be swapped red and blue channels rather than a failure.

/// How many bytes one row of a 32-bit DIB occupies.
///
/// DIB rows are aligned to four bytes. At 32 bits per pixel every row is
/// already a multiple of four, so this is width × 4 — but it is written out
/// rather than assumed, because the day someone tries a 24-bit surface this is
/// the line that would otherwise be quietly wrong.
pub const fn stride(width: u32) -> usize {
    (width as usize * 4).div_ceil(4) * 4
}

/// The bytes a DIB of this size needs.
pub const fn buffer_len(width: u32, height: u32) -> usize {
    stride(width) * height as usize
}

/// The `biHeight` to ask for.
///
/// Negative means top-down, so row zero is the top of the screen and matches
/// how `Canvas` is indexed. A positive height gives a bottom-up bitmap and a
/// vertically mirrored overlay, which is a mistake that looks like a rendering
/// bug rather than a layout one.
pub const fn dib_height(height: u32) -> i32 {
    -(height as i32)
}

/// Whether a canvas of this size can be written into a DIB of this size
/// without conversion.
///
/// The answer is yes whenever the dimensions agree, because the byte orders
/// already match. Phrased as a check rather than an assumption so that the
/// caller has somewhere to fail loudly if a resize is ever missed.
pub fn layout_matches(canvas_len: usize, width: u32, height: u32) -> bool {
    canvas_len == buffer_len(width, height)
}

/// The scale factor for a DPI, relative to the Windows baseline of 96.
///
/// Per-monitor DPI is reported as an integer where 96 means 100%, 144 means
/// 150%, and so on. Logical units in this application are 96ths of an inch, so
/// this is the number `ink-render` needs.
pub fn scale_for_dpi(dpi: u32) -> f64 {
    if dpi == 0 {
        // A monitor that reports nothing is a monitor we know nothing about.
        // Guessing 96 is the documented default and is better than dividing by
        // zero, but it is a guess and the caller should say so in a log.
        return 1.0;
    }
    f64::from(dpi) / 96.0
}

/// A rectangle in screen pixels, edges exclusive on the right and bottom, as
/// Win32's `RECT` has them. Our own type so the arithmetic below needs no
/// Windows headers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PixelRect {
    pub const fn width(self) -> i32 {
        self.right - self.left
    }

    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }
}

/// One monitor, as enumeration reported it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Monitor {
    pub bounds: PixelRect,
    pub dpi: u32,
    pub primary: bool,
}

/// Which monitor to cover, from a 1-based number the user typed.
///
/// With nothing asked for, the primary. A number that names no monitor is an
/// error that lists what exists, rather than a silent fallback to the primary:
/// an overlay on the wrong screen during a presentation is exactly the surprise
/// FR-015 is meant to prevent.
pub fn choose_monitor(monitors: &[Monitor], requested: Option<usize>) -> Result<usize, String> {
    if monitors.is_empty() {
        return Err("Windows reported no monitors at all".to_owned());
    }
    match requested {
        None => Ok(monitors.iter().position(|m| m.primary).unwrap_or(0)),
        Some(number) if (1..=monitors.len()).contains(&number) => Ok(number - 1),
        Some(number) => Err(format!(
            "there is no monitor {number}; this machine has {} (numbered from 1)",
            monitors.len()
        )),
    }
}

/// Where the overlay goes on a monitor.
///
/// The whole monitor unless a size was asked for, because on Windows nothing
/// stops a transparent window covering a screen. (Mutter does, which is why the
/// Linux build defaults to a smaller floating window.) A requested size is in
/// logical units and becomes pixels at this monitor's scale, then is clamped so
/// the overlay never extends off the monitor it was put on.
pub fn fit(monitor: Monitor, requested: Option<(f64, f64)>) -> PixelRect {
    let bounds = monitor.bounds;
    let Some((width, height)) = requested else {
        return bounds;
    };
    let scale = scale_for_dpi(monitor.dpi);
    let clamp =
        |logical: f64, limit: i32| ((logical * scale).round() as i32).clamp(1, limit.max(1));
    PixelRect {
        left: bounds.left,
        top: bounds.top,
        right: bounds.left + clamp(width, bounds.width()),
        bottom: bounds.top + clamp(height, bounds.height()),
    }
}

/// The window size while Parked: the toolbar, with the same margin on the far
/// side as it has on the near one.
///
/// The same arithmetic the Wayland adapter uses, so parking looks alike on both.
pub fn parked_size(toolbar: ink_core::LogicalRect, scale: f64) -> (i32, i32) {
    let side = |near: f64, far: f64| (((far + near) * scale).round() as i32).max(1);
    (
        side(toolbar.min.x, toolbar.max.x),
        side(toolbar.min.y, toolbar.max.y),
    )
}

/// The smallest the overlay can be resized to, in pixels. Smaller and the
/// toolbar no longer fits, which would leave nothing to click to get it back.
pub const MIN_RESIZE: (i32, i32) = (240, 120);

/// A window dragged by its grip: the same size, offset by how far the pointer
/// has moved since the press. Positions are screen pixels.
pub const fn moved(start: PixelRect, from: (i32, i32), to: (i32, i32)) -> PixelRect {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    PixelRect {
        left: start.left + dx,
        top: start.top + dy,
        right: start.right + dx,
        bottom: start.bottom + dy,
    }
}

/// A window resized from its bottom-right corner, never below `minimum`.
pub fn resized(
    start: PixelRect,
    from: (i32, i32),
    to: (i32, i32),
    minimum: (i32, i32),
) -> PixelRect {
    PixelRect {
        right: (start.right + to.0 - from.0).max(start.left + minimum.0),
        bottom: (start.bottom + to.1 - from.1).max(start.top + minimum.1),
        ..start
    }
}

/// The system cursor resource for a controller cursor.
///
/// These are the numeric ids behind `IDC_ARROW` and friends, which Win32
/// defines as `MAKEINTRESOURCE` values. Kept as numbers so the mapping is
/// testable here; `overlay` hands them to `LoadCursorW`.
pub const fn cursor_resource(cursor: ink_app::Cursor) -> u16 {
    match cursor {
        ink_app::Cursor::Default => 32512,           // IDC_ARROW
        ink_app::Cursor::Text => 32513,              // IDC_IBEAM
        ink_app::Cursor::Crosshair => 32515,         // IDC_CROSS
        ink_app::Cursor::ResizeBottomRight => 32642, // IDC_SIZENWSE
        ink_app::Cursor::Move => 32646,              // IDC_SIZEALL
    }
}

/// Where the magnified view starts, for a zoom that follows the pointer
/// (FR-029, T038).
///
/// `MagSetFullscreenTransform` takes the top-left corner of the part of the
/// desktop to magnify, in unmagnified screen pixels. The view is centred on
/// the pointer and kept inside the monitor, so near an edge the pointer moves
/// towards the edge of the view instead of the view showing off-screen space.
pub fn zoom_offset(monitor: PixelRect, level: f64, cursor: (i32, i32)) -> (i32, i32) {
    let level = level.max(1.0);
    let view_w = (f64::from(monitor.width()) / level).round() as i32;
    let view_h = (f64::from(monitor.height()) / level).round() as i32;
    (
        (cursor.0 - view_w / 2).clamp(monitor.left, monitor.right - view_w),
        (cursor.1 - view_h / 2).clamp(monitor.top, monitor.bottom - view_h),
    )
}

/// `WM_APP`, the first message number an application may define for itself.
const WM_APP: u32 = 0x8000;

/// How many remote verbs there can be. Anything posted above this range is
/// not ours and is ignored.
pub const MAX_REMOTE: u32 = 32;

/// The window message that carries remote verb number `index`.
pub const fn remote_message(index: u32) -> Option<u32> {
    if index < MAX_REMOTE {
        Some(WM_APP + index)
    } else {
        None
    }
}

/// The remote verb number a message carries, if it is one of ours.
pub const fn remote_index(message: u32) -> Option<u32> {
    if message >= WM_APP && message < WM_APP + MAX_REMOTE {
        Some(message - WM_APP)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_32_bit_row_needs_no_padding() {
        assert_eq!(stride(1), 4);
        assert_eq!(stride(1920), 7680);
        // The awkward widths, where a 24-bit surface would need padding and
        // this one does not.
        assert_eq!(stride(3), 12);
        assert_eq!(stride(1366), 5464);
    }

    #[test]
    fn the_buffer_is_every_row() {
        assert_eq!(buffer_len(1920, 1080), 1920 * 1080 * 4);
        assert_eq!(buffer_len(1366, 768), 1366 * 768 * 4);
    }

    /// A positive height silently mirrors the overlay. Cheap to assert, and
    /// the failure is confusing enough to be worth a test of its own.
    #[test]
    fn the_dib_is_top_down() {
        assert_eq!(dib_height(1080), -1080);
        assert!(dib_height(1) < 0);
    }

    #[test]
    fn a_canvas_of_the_same_size_fits_exactly() {
        let width = 800;
        let height = 600;
        assert!(layout_matches((width * height * 4) as usize, width, height));
        assert!(!layout_matches(
            (width * height * 4) as usize - 4,
            width,
            height
        ));
        assert!(!layout_matches(
            (width * height * 4) as usize + 4,
            width,
            height
        ));
    }

    #[test]
    fn dpi_becomes_a_scale() {
        assert_eq!(scale_for_dpi(96), 1.0);
        assert_eq!(scale_for_dpi(144), 1.5);
        assert_eq!(scale_for_dpi(192), 2.0);
        assert_eq!(scale_for_dpi(120), 1.25);
    }

    /// Nothing sensible can be computed from a zero, and a division would give
    /// an infinity that travels a long way before it is noticed.
    #[test]
    fn an_unknown_dpi_does_not_produce_an_infinity() {
        let scale = scale_for_dpi(0);
        assert!(scale.is_finite());
        assert_eq!(scale, 1.0);
    }

    fn monitor(left: i32, width: i32, dpi: u32, primary: bool) -> Monitor {
        Monitor {
            bounds: PixelRect {
                left,
                top: 0,
                right: left + width,
                bottom: 1080,
            },
            dpi,
            primary,
        }
    }

    /// The primary is not always first in enumeration order, and assuming it
    /// is puts the overlay on the laptop screen instead of the projector.
    #[test]
    fn with_nothing_asked_for_the_primary_is_chosen() {
        let monitors = [monitor(-1920, 1920, 96, false), monitor(0, 1920, 96, true)];
        assert_eq!(choose_monitor(&monitors, None), Ok(1));
    }

    #[test]
    fn a_monitor_is_chosen_by_its_number_from_one() {
        let monitors = [monitor(0, 1920, 96, true), monitor(1920, 2560, 144, false)];
        assert_eq!(choose_monitor(&monitors, Some(1)), Ok(0));
        assert_eq!(choose_monitor(&monitors, Some(2)), Ok(1));
    }

    #[test]
    fn a_monitor_that_does_not_exist_is_refused_rather_than_guessed() {
        let monitors = [monitor(0, 1920, 96, true)];
        let refusal = choose_monitor(&monitors, Some(2)).unwrap_err();
        assert!(refusal.contains("has 1"), "{refusal}");
        assert!(choose_monitor(&monitors, Some(0)).is_err());
        assert!(choose_monitor(&[], None).is_err());
    }

    #[test]
    fn with_no_size_asked_for_the_whole_monitor_is_covered() {
        let second = monitor(1920, 2560, 144, false);
        assert_eq!(fit(second, None), second.bounds);
    }

    /// Logical units become pixels at the monitor's own scale, and the result
    /// starts on that monitor rather than at the desktop origin.
    #[test]
    fn a_requested_size_is_scaled_and_placed_on_its_monitor() {
        let second = monitor(1920, 2560, 144, false);
        let rect = fit(second, Some((800.0, 400.0)));
        assert_eq!((rect.left, rect.top), (1920, 0));
        assert_eq!((rect.width(), rect.height()), (1200, 600));
    }

    #[test]
    fn a_size_larger_than_the_monitor_is_clamped_to_it() {
        let small = monitor(0, 1366, 96, true);
        let rect = fit(small, Some((5000.0, 5000.0)));
        assert_eq!((rect.width(), rect.height()), (1366, 1080));
    }

    #[test]
    fn parking_keeps_the_toolbar_and_its_margin() {
        let toolbar = ink_core::LogicalRect {
            min: ink_core::LogicalPoint { x: 12.0, y: 12.0 },
            max: ink_core::LogicalPoint { x: 412.0, y: 56.0 },
        };
        assert_eq!(parked_size(toolbar, 1.0), (424, 68));
        assert_eq!(parked_size(toolbar, 1.5), (636, 102));
    }

    #[test]
    fn a_drag_moves_without_resizing() {
        let start = PixelRect {
            left: 100,
            top: 100,
            right: 900,
            bottom: 700,
        };
        let after = moved(start, (150, 120), (170, 90));
        assert_eq!((after.left, after.top), (120, 70));
        assert_eq!((after.width(), after.height()), (800, 600));
    }

    #[test]
    fn a_resize_moves_only_the_far_corner_and_stops_at_the_minimum() {
        let start = PixelRect {
            left: 100,
            top: 100,
            right: 900,
            bottom: 700,
        };
        let bigger = resized(start, (900, 700), (1000, 750), MIN_RESIZE);
        assert_eq!((bigger.left, bigger.top), (100, 100));
        assert_eq!((bigger.width(), bigger.height()), (900, 650));

        let crushed = resized(start, (900, 700), (0, 0), MIN_RESIZE);
        assert_eq!((crushed.width(), crushed.height()), MIN_RESIZE);
    }

    #[test]
    fn every_cursor_has_a_distinct_system_resource() {
        use ink_app::Cursor;
        let all = [
            Cursor::Default,
            Cursor::Text,
            Cursor::Crosshair,
            Cursor::ResizeBottomRight,
            Cursor::Move,
        ];
        let mut ids: Vec<u16> = all.iter().map(|c| cursor_resource(*c)).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        assert_eq!(cursor_resource(Cursor::Default), 32512);
    }

    #[test]
    fn remote_messages_round_trip_and_stay_in_their_range() {
        for index in 0..MAX_REMOTE {
            let message = remote_message(index).unwrap();
            assert_eq!(remote_index(message), Some(index));
        }
        assert_eq!(remote_message(MAX_REMOTE), None);
        // Ordinary window messages are not mistaken for ours.
        assert_eq!(remote_index(0x0201), None);
        assert_eq!(remote_index(0x8000 + MAX_REMOTE), None);
    }

    fn laptop() -> PixelRect {
        PixelRect {
            left: 0,
            top: 0,
            right: 1366,
            bottom: 768,
        }
    }

    #[test]
    fn the_zoomed_view_is_centred_on_the_pointer() {
        // At 2x the view is 683x384, so centred on (683, 384) it starts at
        // (342, 192) (rounding the half view down).
        assert_eq!(zoom_offset(laptop(), 2.0, (683, 384)), (342, 192));
    }

    /// Near an edge the view stops at the edge rather than showing space
    /// beyond the monitor.
    #[test]
    fn the_zoomed_view_stays_on_the_monitor() {
        assert_eq!(zoom_offset(laptop(), 2.0, (0, 0)), (0, 0));
        assert_eq!(zoom_offset(laptop(), 2.0, (1366, 768)), (683, 384));
        assert_eq!(zoom_offset(laptop(), 4.0, (1366, 0)), (1366 - 342, 0));
    }

    /// A second monitor at negative coordinates, as E010's was.
    #[test]
    fn a_monitor_away_from_the_origin_is_respected() {
        let above = PixelRect {
            left: -290,
            top: -1080,
            right: 1630,
            bottom: 0,
        };
        let (x, y) = zoom_offset(above, 2.0, (-290, -1080));
        assert_eq!((x, y), (-290, -1080));
        let (x, y) = zoom_offset(above, 2.0, (1630, 0));
        assert_eq!((x, y), (1630 - 960, -540));
    }

    #[test]
    fn a_level_below_one_is_treated_as_no_zoom() {
        assert_eq!(zoom_offset(laptop(), 0.5, (700, 400)), (0, 0));
    }
}
