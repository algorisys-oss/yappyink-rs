//! Annotation objects.
//!
//! One completed user gesture is one object (FR-007, FR-008). The number of
//! pointer samples behind a stroke is not visible to undo.

use crate::error::DocumentError;
use crate::geometry::{LogicalPoint, LogicalRect, segment_distance};
use crate::id::{ObjectId, OutputId};
use crate::limits;
use crate::style::Style;

/// Whether a stroke was drawn with the pen or the highlighter.
///
/// Recorded in the document because it changes how the stroke is painted, and a
/// saved document must round-trip it (FR-015).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeKind {
    Pen,
    Highlighter,
}

/// The geometry of an object, in output-local logical units.
///
/// Two-point shapes store the drag's endpoints as given. Use [`Shape::bounds`]
/// for a normalised rectangle; the raw endpoints matter for an arrow, whose
/// head belongs at `to`.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Stroke {
        kind: StrokeKind,
        points: Vec<LogicalPoint>,
    },
    Line {
        from: LogicalPoint,
        to: LogicalPoint,
    },
    Arrow {
        from: LogicalPoint,
        to: LogicalPoint,
    },
    Rectangle {
        a: LogicalPoint,
        b: LogicalPoint,
    },
    Ellipse {
        a: LogicalPoint,
        b: LogicalPoint,
    },
}

/// Below this, a drag is treated as having gone nowhere.
///
/// In logical units. A shape this small would be invisible and unselectable,
/// so FR-008 requires it not to become an object at all.
const DEGENERATE_EXTENT: f64 = 1.0;

impl Shape {
    /// A freehand stroke.
    ///
    /// A single point is valid: FR-007 calls a dot a legitimate pen gesture.
    pub fn stroke(kind: StrokeKind, points: Vec<LogicalPoint>) -> Result<Self, DocumentError> {
        if points.is_empty() {
            return Err(DocumentError::EmptyStroke);
        }
        if points.len() > limits::MAX_STROKE_POINTS {
            return Err(DocumentError::StrokeTooLong {
                points: points.len(),
                limit: limits::MAX_STROKE_POINTS,
            });
        }
        Ok(Self::Stroke { kind, points })
    }

    pub fn line(from: LogicalPoint, to: LogicalPoint) -> Result<Self, DocumentError> {
        check_extent("line", from, to)?;
        Ok(Self::Line { from, to })
    }

    pub fn arrow(from: LogicalPoint, to: LogicalPoint) -> Result<Self, DocumentError> {
        check_extent("arrow", from, to)?;
        Ok(Self::Arrow { from, to })
    }

    pub fn rectangle(a: LogicalPoint, b: LogicalPoint) -> Result<Self, DocumentError> {
        check_extent("rectangle", a, b)?;
        Ok(Self::Rectangle { a, b })
    }

    pub fn ellipse(a: LogicalPoint, b: LogicalPoint) -> Result<Self, DocumentError> {
        check_extent("ellipse", a, b)?;
        Ok(Self::Ellipse { a, b })
    }

    /// The geometry as a polyline, for hit testing.
    ///
    /// Everything reduces to connected points: a stroke already is one, a
    /// rectangle is its four closed edges, and an ellipse is sampled. Having
    /// one representation means the eraser has one case to handle rather than
    /// five.
    pub fn outline(&self) -> Vec<LogicalPoint> {
        match self {
            Self::Stroke { points, .. } => points.clone(),
            Self::Line { from, to } | Self::Arrow { from, to } => vec![*from, *to],
            Self::Rectangle { a, b } => {
                let rect = LogicalRect::from_corners(*a, *b);
                let (x0, y0) = (rect.min.x, rect.min.y);
                let (x1, y1) = (rect.max.x, rect.max.y);
                vec![
                    LogicalPoint { x: x0, y: y0 },
                    LogicalPoint { x: x1, y: y0 },
                    LogicalPoint { x: x1, y: y1 },
                    LogicalPoint { x: x0, y: y1 },
                    LogicalPoint { x: x0, y: y0 },
                ]
            }
            Self::Ellipse { a, b } => {
                let rect = LogicalRect::from_corners(*a, *b);
                let (cx, cy) = (
                    (rect.min.x + rect.max.x) / 2.0,
                    (rect.min.y + rect.max.y) / 2.0,
                );
                let (rx, ry) = (rect.width() / 2.0, rect.height() / 2.0);
                (0..=ELLIPSE_HIT_SEGMENTS)
                    .map(|step| {
                        let angle =
                            std::f64::consts::TAU * step as f64 / ELLIPSE_HIT_SEGMENTS as f64;
                        LogicalPoint {
                            x: cx + rx * angle.cos(),
                            y: cy + ry * angle.sin(),
                        }
                    })
                    .collect()
            }
        }
    }

    /// The normalised bounding rectangle of the geometry.
    ///
    /// Stroke width is not included: that is a rendering concern, and hit
    /// testing for the eraser (T017) applies its own tolerance.
    pub fn bounds(&self) -> LogicalRect {
        match self {
            Self::Stroke { points, .. } => LogicalRect::around(points),
            Self::Line { from, to } | Self::Arrow { from, to } => {
                LogicalRect::from_corners(*from, *to)
            }
            Self::Rectangle { a, b } | Self::Ellipse { a, b } => LogicalRect::from_corners(*a, *b),
        }
    }
}

/// Rejects a drag whose extent is under [`DEGENERATE_EXTENT`] in both axes.
fn check_extent(what: &'static str, a: LogicalPoint, b: LogicalPoint) -> Result<(), DocumentError> {
    let rect = LogicalRect::from_corners(a, b);
    if rect.width() < DEGENERATE_EXTENT && rect.height() < DEGENERATE_EXTENT {
        return Err(DocumentError::DegenerateShape { shape: what });
    }
    Ok(())
}

/// How many segments an ellipse is approximated with for hit testing.
///
/// Coarser than the renderer's, because a tolerance of half a stroke width
/// swamps the difference and hit testing runs over every object.
const ELLIPSE_HIT_SEGMENTS: usize = 64;

/// One annotation: an id, the output it belongs to, how it looks, and where it
/// is.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    id: ObjectId,
    output: OutputId,
    style: Style,
    shape: Shape,
}

impl Object {
    pub fn new(id: ObjectId, output: OutputId, style: Style, shape: Shape) -> Self {
        Self {
            id,
            output,
            style,
            shape,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn output(&self) -> &OutputId {
        &self.output
    }

    pub fn style(&self) -> Style {
        self.style
    }

    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    pub fn bounds(&self) -> LogicalRect {
        self.shape.bounds()
    }

    /// Whether an eraser sweep along `path` with the given radius touches this
    /// object (FR-009).
    ///
    /// The whole object is hit or missed: MVP erasing removes objects, never
    /// parts of them. The tolerance includes half the object's stroke width,
    /// so a thick line is hit where it looks hit rather than only along its
    /// mathematical centre.
    pub fn intersects_sweep(&self, path: &[LogicalPoint], radius: f64) -> bool {
        if path.is_empty() {
            return false;
        }
        let tolerance = radius + self.style.width.get() / 2.0;

        // Bounding-box rejection first. Hit testing runs over every object in
        // the document on every eraser gesture, and most of them are nowhere
        // near the sweep.
        let bounds = self.bounds();
        let sweep = LogicalRect::around(path);
        if sweep.min.x - tolerance > bounds.max.x
            || sweep.max.x + tolerance < bounds.min.x
            || sweep.min.y - tolerance > bounds.max.y
            || sweep.max.y + tolerance < bounds.min.y
        {
            return false;
        }

        let outline = self.shape.outline();
        let (first_outline, first_path) = (outline.first(), path.first());
        let (Some(first_outline), Some(first_path)) = (first_outline, first_path) else {
            return false;
        };

        // A single-point gesture or a single-point stroke is a degenerate
        // segment, which the distance function handles, so a tap with the
        // eraser still erases and a dot can still be erased.
        let outline_segments: Vec<(LogicalPoint, LogicalPoint)> = if outline.len() == 1 {
            vec![(*first_outline, *first_outline)]
        } else {
            outline.windows(2).map(|w| (w[0], w[1])).collect()
        };
        let path_segments: Vec<(LogicalPoint, LogicalPoint)> = if path.len() == 1 {
            vec![(*first_path, *first_path)]
        } else {
            path.windows(2).map(|w| (w[0], w[1])).collect()
        };

        for (a0, a1) in &outline_segments {
            for (b0, b1) in &path_segments {
                if segment_distance(*a0, *a1, *b0, *b1) <= tolerance {
                    return true;
                }
            }
        }
        false
    }
}
