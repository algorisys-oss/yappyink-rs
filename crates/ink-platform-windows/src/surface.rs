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
}
