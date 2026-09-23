//! Platform-free annotation domain.
//!
//! Scope is fixed by the constitution (§3) and NFR-004: no OS handle, window,
//! GPU, egui, portal, or Objective-C type may appear here, and every test must
//! run headlessly and deterministically.
//!
//! T009 provides the document model: objects, styles, output-local geometry,
//! and per-output documents with validation. The interaction reducer is T010
//! and command history is T017; neither belongs in this crate's `document`
//! module, which is the thing commands act on rather than a thing that records
//! its own past.

#![forbid(unsafe_code)]

pub mod document;
pub mod error;
pub mod geometry;
pub mod id;
pub mod limits;
pub mod object;
pub mod style;

pub use document::Document;
pub use error::DocumentError;
pub use geometry::{LogicalPoint, LogicalRect, LogicalSize};
pub use id::{IdSource, ObjectId, OutputId};
pub use object::{Object, Shape, StrokeKind};
pub use style::{Opacity, Rgb, Style, Width};
