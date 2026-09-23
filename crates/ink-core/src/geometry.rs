//! Output-local logical geometry.
//!
//! Logical units are not physical pixels. Scaling, rotation, and the macOS
//! coordinate-direction flip are applied at the backend boundary (FR-012), so
//! nothing in this module may know a device pixel ratio.

/// A point in output-local logical units, origin at the output's top-left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalPoint {
    pub x: f64,
    pub y: f64,
}

impl LogicalPoint {
    /// Returns `None` for a non-finite coordinate.
    ///
    /// Negative and out-of-bounds values are accepted: an input device can
    /// legitimately report them, and clamping is a backend policy decision.
    /// FR-017 relies on this rejection when validating loaded documents.
    pub fn new(x: f64, y: f64) -> Option<Self> {
        (x.is_finite() && y.is_finite()).then_some(Self { x, y })
    }
}

/// A size in output-local logical units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalSize {
    width: f64,
    height: f64,
}

impl LogicalSize {
    /// Returns `None` unless both extents are finite and strictly positive.
    ///
    /// A zero-extent output is rejected rather than silently accepted, because
    /// it would make every coordinate conversion degenerate.
    pub fn new(width: f64, height: f64) -> Option<Self> {
        let usable = |v: f64| v.is_finite() && v > 0.0;
        (usable(width) && usable(height)).then_some(Self { width, height })
    }

    pub fn width(self) -> f64 {
        self.width
    }

    pub fn height(self) -> f64 {
        self.height
    }
}

/// An axis-aligned rectangle, normalised so `min` is the top-left corner.
///
/// Used for object bounds. A drag that went right-to-left produces the same
/// bounds as one that went left-to-right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalRect {
    pub min: LogicalPoint,
    pub max: LogicalPoint,
}

impl LogicalRect {
    /// Builds the smallest rectangle containing both points.
    pub fn from_corners(a: LogicalPoint, b: LogicalPoint) -> Self {
        Self {
            min: LogicalPoint {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
            },
            max: LogicalPoint {
                x: a.x.max(b.x),
                y: a.y.max(b.y),
            },
        }
    }

    /// Builds the smallest rectangle containing every point.
    ///
    /// # Panics
    ///
    /// Panics on an empty slice. Callers hold a non-empty stroke by
    /// construction; [`crate::Shape::stroke`] rejects an empty one.
    pub fn around(points: &[LogicalPoint]) -> Self {
        let first = *points
            .first()
            .expect("bounds of an empty point set are undefined");
        points.iter().skip(1).fold(
            Self {
                min: first,
                max: first,
            },
            |rect, p| Self {
                min: LogicalPoint {
                    x: rect.min.x.min(p.x),
                    y: rect.min.y.min(p.y),
                },
                max: LogicalPoint {
                    x: rect.max.x.max(p.x),
                    y: rect.max.y.max(p.y),
                },
            },
        )
    }

    pub fn width(self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(self) -> f64 {
        self.max.y - self.min.y
    }
}

/// The shortest distance between two line segments.
///
/// Used for eraser hit testing. Segment-to-segment rather than point-to-point
/// because a fast sweep produces widely spaced samples: testing only those
/// points would let the eraser skip straight over a stroke between two
/// reports, which `specs/002-drawing-history/spec.md` calls out by name.
pub fn segment_distance(
    a0: LogicalPoint,
    a1: LogicalPoint,
    b0: LogicalPoint,
    b1: LogicalPoint,
) -> f64 {
    // A crossing means distance zero, and the four endpoint-to-segment
    // distances below would not find it.
    if segments_cross(a0, a1, b0, b1) {
        return 0.0;
    }
    let candidates = [
        point_segment_distance(a0, b0, b1),
        point_segment_distance(a1, b0, b1),
        point_segment_distance(b0, a0, a1),
        point_segment_distance(b1, a0, a1),
    ];
    candidates.into_iter().fold(f64::INFINITY, f64::min)
}

/// The shortest distance from a point to a segment.
pub fn point_segment_distance(p: LogicalPoint, a: LogicalPoint, b: LogicalPoint) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f64::EPSILON {
        // A degenerate segment is a point, which is what a single-sample
        // gesture produces.
        return ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt();
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length_squared).clamp(0.0, 1.0);
    let (cx, cy) = (a.x + t * dx, a.y + t * dy);
    ((p.x - cx).powi(2) + (p.y - cy).powi(2)).sqrt()
}

fn orientation(a: LogicalPoint, b: LogicalPoint, c: LogicalPoint) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn segments_cross(a0: LogicalPoint, a1: LogicalPoint, b0: LogicalPoint, b1: LogicalPoint) -> bool {
    let d1 = orientation(a0, a1, b0);
    let d2 = orientation(a0, a1, b1);
    let d3 = orientation(b0, b1, a0);
    let d4 = orientation(b0, b1, a1);
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}
