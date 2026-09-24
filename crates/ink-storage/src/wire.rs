//! The file format.
//!
//! These types describe what is on disk, and nothing else. They are not the
//! domain types and deliberately do not derive from them: the format has to
//! stay stable across changes to `ink-core`, and the gap between the two is
//! where validation happens. Everything here is plain data with no invariants,
//! so a hostile or corrupt file can be parsed without constructing anything the
//! rest of the program would trust.
//!
//! Field order is the order it is written, which keeps saved files stable and
//! diffable.

use serde::{Deserialize, Serialize};

/// The schema this build writes and is willing to read.
///
/// A file claiming a higher number is refused rather than guessed at.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireDocument {
    pub schema_version: u32,
    /// What wrote it. Diagnostics only; nothing branches on it.
    pub app: String,
    /// The output the annotations were drawn on. A hint, not an identity:
    /// output names change across reboots (FR-011).
    pub output: String,
    pub output_width: f64,
    pub output_height: f64,
    pub objects: Vec<WireObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireObject {
    pub id: u64,
    pub color: [u8; 3],
    pub width: f64,
    pub opacity: f64,
    pub shape: WireShape,
}

/// Externally tagged, so a shape reads as `{"stroke": {...}}`.
///
/// An unknown tag fails to parse rather than being skipped, which is what
/// FR-017 wants: a file containing something this build does not understand is
/// refused, not silently loaded with pieces missing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireShape {
    Stroke {
        highlighter: bool,
        points: Vec<[f64; 2]>,
    },
    Line {
        from: [f64; 2],
        to: [f64; 2],
    },
    Arrow {
        from: [f64; 2],
        to: [f64; 2],
    },
    Rectangle {
        a: [f64; 2],
        b: [f64; 2],
    },
    Ellipse {
        a: [f64; 2],
        b: [f64; 2],
    },
    Text {
        at: [f64; 2],
        content: String,
        size: f64,
    },
}
