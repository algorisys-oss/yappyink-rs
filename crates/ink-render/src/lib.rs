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

    /// Source-over blend of a premultiplied colour over a rectangle.
    ///
    /// For application chrome, not document content: the mode indicator and
    /// frame are drawn with this. Anything painted this way is a control, so
    /// an ink-only export (FR-024) must leave it out.
    pub fn fill_rect(&mut self, x: i64, y: i64, width: i64, height: i64, colour: [u8; 4]) {
        for row in y..y.saturating_add(height) {
            for column in x..x.saturating_add(width) {
                self.blend(column, row, colour);
            }
        }
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

/// Paints objects, reusing its scratch buffers between frames.
///
/// Holding this across frames matters: the coverage mask is the size of the
/// canvas, and allocating it per frame would show up in NFR-001's budget.
#[derive(Default)]
pub struct Painter {
    /// Per-pixel coverage of the object being drawn, 0 to 255.
    ///
    /// This is what makes FR-007 work. The samples of a stroke overlap
    /// heavily, so blending them one at a time makes a 50% highlighter come
    /// out opaque. Coverage is accumulated for the whole object first and
    /// composited once, so the opacity the user chose is the opacity they get,
    /// however many samples the gesture took.
    coverage: Vec<u8>,
}

impl Painter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Paints every object in the document, in document order.
    ///
    /// The canvas is not cleared first: the caller decides whether this is a
    /// fresh frame or a layer over something else.
    pub fn paint(&mut self, document: &Document, canvas: &mut Canvas, scale: Scale) {
        for object in document.objects() {
            self.paint_object(object, canvas, scale);
        }
    }

    /// Paints one object.
    ///
    /// Also used for the transient gesture preview, which is not in the
    /// document and must not be.
    pub fn paint_object(&mut self, object: &Object, canvas: &mut Canvas, scale: Scale) {
        let radius = (object.style().width.get() * scale.get() / 2.0).max(0.5);
        let Some(rect) = mask_rect(object, canvas, scale, radius) else {
            return;
        };

        let needed = canvas.width() as usize * canvas.height() as usize;
        if self.coverage.len() < needed {
            self.coverage.resize(needed, 0);
        }
        // Only the object's own rectangle is cleared, not the whole buffer, so
        // a document of many small objects does not cost one full-canvas clear
        // each.
        clear_rect(&mut self.coverage, canvas.width(), rect);

        self.mark(object, canvas.width(), rect, scale, radius);
        composite(
            &mut self.coverage,
            canvas,
            rect,
            premultiplied(object.style()),
        );
    }

    /// Rasterises the object's geometry into the coverage mask.
    fn mark(&mut self, object: &Object, stride: u32, rect: PixelRect, scale: Scale, radius: f64) {
        let mut pen = Pen {
            coverage: &mut self.coverage,
            stride,
            rect,
            radius,
        };
        match object.shape() {
            Shape::Stroke { points, .. } => pen.polyline(points, scale),
            Shape::Line { from, to } | Shape::Arrow { from, to } => {
                // The arrowhead is T016's work. Drawing the shaft alone here
                // would be a silent half-implementation, so an arrow is its
                // line until T016 adds the head.
                pen.polyline(&[*from, *to], scale);
            }
            Shape::Rectangle { a, b } => {
                let (x0, y0) = scale.apply(*a);
                let (x1, y1) = scale.apply(*b);
                let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)];
                for pair in corners.windows(2) {
                    pen.segment(pair[0], pair[1]);
                }
            }
            Shape::Ellipse { a, b } => {
                let (x0, y0) = scale.apply(*a);
                let (x1, y1) = scale.apply(*b);
                let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                let (rx, ry) = ((x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0);
                // Enough segments to read as a curve at any size the limits
                // allow. A proper curve renderer is a T014 follow-up.
                let steps = 128;
                let mut previous = (cx + rx, cy);
                for step in 1..=steps {
                    let angle = std::f64::consts::TAU * f64::from(step) / f64::from(steps);
                    let next = (cx + rx * angle.cos(), cy + ry * angle.sin());
                    pen.segment(previous, next);
                    previous = next;
                }
            }
        }
    }
}

/// Paints a document with a throwaway painter.
///
/// Convenient for tests and one-off renders. A caller drawing frames should
/// keep a [`Painter`] instead.
pub fn paint(document: &Document, canvas: &mut Canvas, scale: Scale) {
    Painter::new().paint(document, canvas, scale);
}

/// Paints one object with a throwaway painter.
pub fn paint_object(object: &Object, canvas: &mut Canvas, scale: Scale) {
    Painter::new().paint_object(object, canvas, scale);
}

/// A rectangle of canvas pixels, `min` inclusive and `max` exclusive.
#[derive(Clone, Copy, Debug)]
struct PixelRect {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

/// The canvas pixels an object can touch, or `None` if it misses entirely.
fn mask_rect(object: &Object, canvas: &Canvas, scale: Scale, radius: f64) -> Option<PixelRect> {
    let bounds = object.bounds();
    let margin = radius.ceil() + 1.0;
    let left = bounds.min.x * scale.get() - margin;
    let top = bounds.min.y * scale.get() - margin;
    let right = bounds.max.x * scale.get() + margin;
    let bottom = bounds.max.y * scale.get() + margin;

    if right < 0.0 || bottom < 0.0 {
        return None;
    }
    let x0 = left.max(0.0).floor() as u32;
    let y0 = top.max(0.0).floor() as u32;
    let x1 = (right.ceil().max(0.0) as u32 + 1).min(canvas.width());
    let y1 = (bottom.ceil().max(0.0) as u32 + 1).min(canvas.height());
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    Some(PixelRect { x0, y0, x1, y1 })
}

fn clear_rect(coverage: &mut [u8], stride: u32, rect: PixelRect) {
    for y in rect.y0..rect.y1 {
        let row = y as usize * stride as usize;
        coverage[row + rect.x0 as usize..row + rect.x1 as usize].fill(0);
    }
}

/// Blends the colour onto the canvas once, wherever the mask has coverage.
fn composite(coverage: &mut [u8], canvas: &mut Canvas, rect: PixelRect, colour: [u8; 4]) {
    for y in rect.y0..rect.y1 {
        let row = y as usize * canvas.width() as usize;
        for x in rect.x0..rect.x1 {
            if coverage[row + x as usize] == 0 {
                continue;
            }
            canvas.blend(i64::from(x), i64::from(y), colour);
        }
    }
}

/// Writes coverage for thick geometry.
///
/// Deliberately hard-edged: coverage is 0 or 255, so strokes have aliased
/// edges. Anti-aliasing is a quality improvement that belongs with the real
/// renderer; getting the compositing model right comes first, because that is
/// what the document format and FR-007 depend on.
struct Pen<'a> {
    coverage: &'a mut [u8],
    stride: u32,
    rect: PixelRect,
    radius: f64,
}

impl Pen<'_> {
    fn polyline(&mut self, points: &[LogicalPoint], scale: Scale) {
        match points {
            [] => {}
            // A single sample is a dot, which FR-007 calls a valid gesture.
            [only] => self.disc(scale.apply(*only)),
            _ => {
                for pair in points.windows(2) {
                    self.segment(scale.apply(pair[0]), scale.apply(pair[1]));
                }
            }
        }
    }

    fn segment(&mut self, from: (f64, f64), to: (f64, f64)) {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let distance = (dx * dx + dy * dy).sqrt();
        let steps = distance.ceil().max(1.0) as i64;
        for step in 0..=steps {
            let t = step as f64 / steps as f64;
            self.disc((from.0 + dx * t, from.1 + dy * t));
        }
    }

    fn disc(&mut self, centre: (f64, f64)) {
        let limit = self.radius.ceil() as i64;
        let (cx, cy) = (centre.0.round() as i64, centre.1.round() as i64);
        let radius_squared = self.radius * self.radius;
        for dy in -limit..=limit {
            for dx in -limit..=limit {
                if (dx * dx + dy * dy) as f64 > radius_squared {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if x < i64::from(self.rect.x0)
                    || y < i64::from(self.rect.y0)
                    || x >= i64::from(self.rect.x1)
                    || y >= i64::from(self.rect.y1)
                {
                    continue;
                }
                // Coverage saturates rather than accumulating: overlapping
                // samples within one object cover the pixel, they do not
                // darken it.
                self.coverage[y as usize * self.stride as usize + x as usize] = 255;
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
