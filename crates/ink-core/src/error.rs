//! Typed document failures.
//!
//! Every variant names what was refused and why, so a caller can report it
//! rather than discarding the user's work silently (NFR-005).

use std::fmt;

use crate::id::{ObjectId, OutputId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentError {
    /// A stroke with no samples at all. Distinct from a dot, which has one.
    EmptyStroke,
    /// A stroke exceeding the sample limit (NFR-003).
    StrokeTooLong { points: usize, limit: usize },
    /// A drag that went nowhere. Storing it would create an object the user
    /// cannot see, select, or erase (FR-008).
    DegenerateShape { shape: &'static str },
    /// The object belongs to a different output than the document.
    OutputMismatch {
        object: ObjectId,
        expected: OutputId,
        found: OutputId,
    },
    /// The document is full (NFR-003).
    ObjectLimitReached { limit: usize },
}

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyStroke => f.write_str("a stroke must have at least one point"),
            Self::StrokeTooLong { points, limit } => {
                write!(
                    f,
                    "a stroke of {points} points exceeds the limit of {limit}"
                )
            }
            Self::DegenerateShape { shape } => {
                write!(f, "the {shape} is too small to be drawn or selected")
            }
            Self::OutputMismatch {
                object,
                expected,
                found,
            } => write!(
                f,
                "object {object} belongs to output {found}, not {expected}"
            ),
            Self::ObjectLimitReached { limit } => {
                write!(
                    f,
                    "this output already holds the maximum of {limit} objects"
                )
            }
        }
    }
}

impl std::error::Error for DocumentError {}
