//! Annotation objects.
//!
//! One completed user gesture is one object (FR-007, FR-008). The number of
//! pointer samples behind a stroke is not visible to undo.

use crate::error::DocumentError;
use crate::geometry::{LogicalPoint, LogicalRect};
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
}
