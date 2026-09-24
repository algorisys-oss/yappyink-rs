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
}
