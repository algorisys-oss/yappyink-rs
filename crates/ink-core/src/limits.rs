//! Defensive bounds on document size.
//!
//! These are the starting limits from `product-spec.md`, described there as
//! design defaults rather than measured optimal values. NFR-003 requires the
//! document to report a limit rather than grow without bound. Revisit with
//! benchmark evidence and an ADR (T024).

/// Objects one output's document may hold.
pub const MAX_OBJECTS_PER_OUTPUT: usize = 10_000;

/// Sampled points a single stroke may hold.
pub const MAX_STROKE_POINTS: usize = 100_000;
