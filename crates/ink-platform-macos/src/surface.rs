//! The bridge between our canvas and a Core Graphics bitmap.
//!
//! Free of AppKit types, like [`crate::keys`], and for the same reason:
//! buffer layout is arithmetic and is decidable without an operating system.
//!
//! # The fact this module exists to pin down
//!
//! `ink-render` produces **premultiplied ARGB8888, little-endian**, which in
//! memory is the byte order `[B, G, R, A]`.
//!
//! A `CGImage` built with `kCGImageAlphaPremultipliedFirst` *and*
//! `kCGBitmapByteOrder32Little` reads exactly that order: "alpha first" refers
//! to the 32-bit word, and little-endian puts the word's low byte — blue —
//! first in memory. So the canvas needs no conversion, the same as on Windows.
//!
//! Getting the byte-order flag wrong does not fail. It swaps red and blue, and
//! the overlay simply draws in the wrong colours, which is why this is written
//! down rather than left to whoever reads the constant next.

/// Bytes per row for a 32-bit bitmap of this width.
///
/// Core Graphics accepts any `bytesPerRow` at least `width * 4`, and is
/// happier when it is a multiple of 16 — but the value must match how the
/// buffer is actually laid out, and `Canvas` is tightly packed. Padding here
/// without padding the canvas would shear the image diagonally.
pub const fn bytes_per_row(width: u32) -> usize {
    width as usize * 4
}

/// The bytes a bitmap of this size needs.
pub const fn buffer_len(width: u32, height: u32) -> usize {
    bytes_per_row(width) * height as usize
}

/// Whether a canvas of this size matches a bitmap of these dimensions.
pub fn layout_matches(canvas_len: usize, width: u32, height: u32) -> bool {
    canvas_len == buffer_len(width, height)
}

/// Bits per component, per the pixel format above.
pub const BITS_PER_COMPONENT: usize = 8;
/// Bits per pixel, per the pixel format above.
pub const BITS_PER_PIXEL: usize = 32;

/// The scale factor for a screen's backing scale.
///
/// AppKit reports this directly — 1.0 for a normal display, 2.0 for a Retina
/// one — rather than as a DPI, so unlike Windows there is nothing to divide.
/// The guard is against a zero or a non-finite value, which would produce an
/// infinity that travels a long way before anyone notices.
pub fn scale_for_backing(backing: f64) -> f64 {
    if backing.is_finite() && backing > 0.0 {
        backing
    } else {
        1.0
    }
}

/// A rectangle in display points, origin at the display's top-left, which is
/// how ScreenCaptureKit's `sourceRect` is expressed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The part of the display to magnify at `level`, centred on the pointer and
/// kept inside the display (FR-029).
///
/// `display` is the display's size in points; `cursor` is in the same
/// top-left coordinates. The capture scales this rectangle up to the full
/// display, which is the zoom.
pub fn zoom_source(display: (f64, f64), level: f64, cursor: (f64, f64)) -> DisplayRect {
    let level = level.max(1.0);
    let (width, height) = (display.0 / level, display.1 / level);
    let x = (cursor.0 - width / 2.0).clamp(0.0, (display.0 - width).max(0.0));
    let y = (cursor.1 - height / 2.0).clamp(0.0, (display.1 - height).max(0.0));
    DisplayRect {
        x,
        y,
        width,
        height,
    }
}

/// Converts the pointer from AppKit's global coordinates, bottom-left origin
/// on the primary display, to top-left coordinates on a screen whose frame is
/// given in the same global coordinates.
///
/// AppKit counts y upwards and ScreenCaptureKit downwards. Getting this wrong
/// does not fail: the zoom follows the pointer upside down.
pub fn cursor_on_screen(mouse: (f64, f64), origin: (f64, f64), size: (f64, f64)) -> (f64, f64) {
    (mouse.0 - origin.0, origin.1 + size.1 - mouse.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_tightly_packed() {
        assert_eq!(bytes_per_row(1), 4);
        assert_eq!(bytes_per_row(1920), 7680);
        assert_eq!(bytes_per_row(1366), 5464);
    }

    #[test]
    fn the_buffer_is_every_row() {
        assert_eq!(buffer_len(1920, 1080), 1920 * 1080 * 4);
    }

    #[test]
    fn a_canvas_of_the_same_size_fits_exactly() {
        assert!(layout_matches(800 * 600 * 4, 800, 600));
        assert!(!layout_matches(800 * 600 * 4 - 4, 800, 600));
    }

    #[test]
    fn a_retina_screen_scales() {
        assert_eq!(scale_for_backing(2.0), 2.0);
        assert_eq!(scale_for_backing(1.0), 1.0);
    }

    /// A zero or a NaN would give an infinite or non-finite scale, and
    /// `Scale::new` would reject it much further away from the cause.
    #[test]
    fn a_nonsense_backing_scale_does_not_propagate() {
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let scale = scale_for_backing(bad);
            assert!(scale.is_finite() && scale > 0.0, "{bad} gave {scale}");
        }
    }

    #[test]
    fn the_zoom_is_centred_on_the_pointer() {
        let rect = zoom_source((1440.0, 900.0), 2.0, (720.0, 450.0));
        assert_eq!(
            rect,
            DisplayRect {
                x: 360.0,
                y: 225.0,
                width: 720.0,
                height: 450.0
            }
        );
    }

    #[test]
    fn the_zoom_stays_on_the_display() {
        let corner = zoom_source((1440.0, 900.0), 2.0, (0.0, 0.0));
        assert_eq!((corner.x, corner.y), (0.0, 0.0));
        let far = zoom_source((1440.0, 900.0), 4.0, (1440.0, 900.0));
        assert_eq!((far.x, far.y), (1080.0, 675.0));
    }

    #[test]
    fn no_zoom_is_the_whole_display() {
        let rect = zoom_source((1440.0, 900.0), 0.5, (100.0, 100.0));
        assert_eq!((rect.width, rect.height), (1440.0, 900.0));
        assert_eq!((rect.x, rect.y), (0.0, 0.0));
    }

    /// AppKit's y goes up from the bottom; the capture's goes down from the
    /// top. The pointer at the bottom-left corner of the main screen is at the
    /// display's bottom-left in top-left terms.
    #[test]
    fn the_pointer_is_flipped_into_top_left_coordinates() {
        assert_eq!(
            cursor_on_screen((0.0, 0.0), (0.0, 0.0), (1440.0, 900.0)),
            (0.0, 900.0)
        );
        assert_eq!(
            cursor_on_screen((100.0, 850.0), (0.0, 0.0), (1440.0, 900.0)),
            (100.0, 50.0)
        );
        // A second screen to the right and higher up.
        assert_eq!(
            cursor_on_screen((1540.0, 1000.0), (1440.0, 200.0), (1920.0, 1080.0)),
            (100.0, 280.0)
        );
    }
}
