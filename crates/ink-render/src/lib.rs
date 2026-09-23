//! Scene to pixels.
//!
//! A CPU rasteriser with no GPU, window, or OS dependency, so its output can be
//! asserted pixel by pixel in a headless test. That is the point: a renderer
//! you can only check by looking at a screen is a renderer nobody checks.
//!
//! **Scope.** T011 needs something that paints so the vertical slice can exist.
//! T014 owns the real renderer: the wgpu path, geometry caches, demand-driven
//! redraw, and the highlighter's whole-stroke opacity. What is fixed here and
//! must survive that change is the *convention*, not the implementation.
//!
//! **The convention is premultiplied alpha, ARGB8888, little-endian.** A pixel
//! is four bytes in memory order B, G, R, A, and the colour channels are
//! already multiplied by the alpha. A fully transparent pixel is four zero
//! bytes, which is why a transparent background cannot leave a dark fringe.

#![forbid(unsafe_code)]

use ink_core::{Document, LogicalPoint, Object, Shape, Style};

/// A mutable rectangle of premultiplied ARGB8888 pixels.
pub struct Canvas<'a> {
    pixels: &'a mut [u8],
    width: u32,
    height: u32,
}

impl<'a> Canvas<'a> {
    /// Wraps a pixel buffer.
    ///
    /// Returns `None` if the buffer is not exactly `width * height * 4` bytes,
    /// rather than painting into whatever happens to be there.
    pub fn new(pixels: &'a mut [u8], width: u32, height: u32) -> Option<Self> {
        let needed = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        (pixels.len() == needed).then_some(Self {
            pixels,
            width,
            height,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Fills with fully transparent pixels.
    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }

    /// The four bytes at a pixel, or `None` if out of bounds. For tests.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let offset = ((y as usize * self.width as usize) + x as usize) * 4;
        Some([
            self.pixels[offset],
            self.pixels[offset + 1],
            self.pixels[offset + 2],
            self.pixels[offset + 3],
        ])
    }

    /// Source-over blend of a premultiplied colour at a pixel.
    fn blend(&mut self, x: i64, y: i64, colour: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        let offset = ((y as usize * self.width as usize) + x as usize) * 4;
        let inverse = 255 - u32::from(colour[3]);
        let destination = &mut self.pixels[offset..offset + 4];
        for (under, over) in destination.iter_mut().zip(colour) {
            // +127 rounds to nearest rather than truncating, which keeps a
            // repeated blend from drifting darker.
            let blended = u32::from(over) + (u32::from(*under) * inverse + 127) / 255;
            *under = blended.min(255) as u8;
        }
    }
}

/// Maps output-local logical units to canvas pixels.
///
/// FR-012: the document is in logical units and knows nothing about pixels.
/// The scale is applied here, at the boundary, and nowhere else.
#[derive(Clone, Copy, Debug)]
pub struct Scale(f64);

impl Scale {
    pub const ONE: Self = Self(1.0);

    /// Returns `None` unless the factor is finite and strictly positive.
    pub fn new(factor: f64) -> Option<Self> {
        (factor.is_finite() && factor > 0.0).then_some(Self(factor))
    }

    pub fn get(self) -> f64 {
        self.0
    }

    fn apply(self, point: LogicalPoint) -> (f64, f64) {
        (point.x * self.0, point.y * self.0)
    }
}

/// Paints every object in the document onto the canvas, in document order.
///
/// The canvas is not cleared first: the caller decides whether this is a fresh
/// frame or a layer over something else.
pub fn paint(document: &Document, canvas: &mut Canvas, scale: Scale) {
    for object in document.objects() {
        paint_object(object, canvas, scale);
    }
}

/// Paints one object. Used for the transient gesture preview, which is not in
/// the document yet and must not be.
pub fn paint_object(object: &Object, canvas: &mut Canvas, scale: Scale) {
    let colour = premultiplied(object.style());
    let radius = (object.style().width.get() * scale.get() / 2.0).max(0.5);

    match object.shape() {
        Shape::Stroke { points, .. } => paint_polyline(points, canvas, scale, radius, colour),
        Shape::Line { from, to } | Shape::Arrow { from, to } => {
            // The arrowhead is T016's work. Painting the shaft alone here would
            // be a silent half-implementation, so an arrow is drawn as its line
            // and T016 adds the head.
            paint_polyline(&[*from, *to], canvas, scale, radius, colour);
        }
        Shape::Rectangle { a, b } => {
            let (x0, y0) = scale.apply(*a);
            let (x1, y1) = scale.apply(*b);
            let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)];
            for pair in corners.windows(2) {
                paint_segment(pair[0], pair[1], canvas, radius, colour);
            }
        }
        Shape::Ellipse { a, b } => {
            let (x0, y0) = scale.apply(*a);
            let (x1, y1) = scale.apply(*b);
            let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            let (rx, ry) = ((x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0);
            // Enough segments that the outline reads as a curve at any size the
            // limits allow. A proper curve renderer is T014.
            let steps = 128;
            let mut previous = (cx + rx, cy);
            for step in 1..=steps {
                let angle = std::f64::consts::TAU * f64::from(step) / f64::from(steps);
                let next = (cx + rx * angle.cos(), cy + ry * angle.sin());
                paint_segment(previous, next, canvas, radius, colour);
                previous = next;
            }
        }
    }
}

/// Premultiplies the style's colour by its opacity, in memory order B, G, R, A.
fn premultiplied(style: Style) -> [u8; 4] {
    let alpha = style.opacity.get();
    let scale = |channel: u8| (f64::from(channel) * alpha).round().clamp(0.0, 255.0) as u8;
    [
        scale(style.color.b),
        scale(style.color.g),
        scale(style.color.r),
        (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

fn paint_polyline(
    points: &[LogicalPoint],
    canvas: &mut Canvas,
    scale: Scale,
    radius: f64,
    colour: [u8; 4],
) {
    match points {
        [] => {}
        [only] => paint_disc(scale.apply(*only), canvas, radius, colour),
        _ => {
            for pair in points.windows(2) {
                paint_segment(
                    scale.apply(pair[0]),
                    scale.apply(pair[1]),
                    canvas,
                    radius,
                    colour,
                );
            }
        }
    }
}

/// Draws a thick segment as a chain of discs.
///
/// Crude and deliberately so. Stroke tessellation with correct joins and the
/// highlighter's single-alpha compositing are T014 and T015.
fn paint_segment(
    from: (f64, f64),
    to: (f64, f64),
    canvas: &mut Canvas,
    radius: f64,
    colour: [u8; 4],
) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let distance = (dx * dx + dy * dy).sqrt();
    let steps = distance.ceil().max(1.0) as i64;
    for step in 0..=steps {
        let t = step as f64 / steps as f64;
        paint_disc((from.0 + dx * t, from.1 + dy * t), canvas, radius, colour);
    }
}

fn paint_disc(centre: (f64, f64), canvas: &mut Canvas, radius: f64, colour: [u8; 4]) {
    let limit = radius.ceil() as i64;
    let (cx, cy) = (centre.0.round() as i64, centre.1.round() as i64);
    let radius_squared = radius * radius;
    for dy in -limit..=limit {
        for dx in -limit..=limit {
            if (dx * dx + dy * dy) as f64 <= radius_squared {
                canvas.blend(cx + dx, cy + dy, colour);
            }
        }
    }
}
