//! Platform-free annotation domain vocabulary.
//!
//! Scope of this crate is fixed by the constitution (§3) and NFR-004: no OS
//! handle, window, GPU, egui, portal, or Objective-C type may appear here, and
//! every test must run headlessly and deterministically.
//!
//! T001 creates only the identity and logical-unit vocabulary that later tasks
//! build on. The document objects, command reducer, and undo history are T009
//! and T010; do not add them here ahead of those tasks.

#![forbid(unsafe_code)]

use std::fmt;

/// Identity of an annotation object within a document.
///
/// Values come from an [`IdSource`] so that tests are deterministic (NFR-004);
/// the domain never reads a clock or a random generator itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(u64);

impl ObjectId {
    /// The raw value, for serialization and diagnostics only.
    pub fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Injected, monotonic source of [`ObjectId`] values.
#[derive(Clone, Debug)]
pub struct IdSource {
    next: u64,
}

impl IdSource {
    /// Starts issuing ids at `seed`.
    pub fn starting_at(seed: u64) -> Self {
        Self { next: seed }
    }

    /// Issues the next id.
    ///
    /// # Panics
    ///
    /// Panics only on `u64` exhaustion, which no reachable document can cause:
    /// FR-011's object limit is far below it.
    pub fn next_id(&mut self) -> ObjectId {
        let id = ObjectId(self.next);
        self.next = self.next.checked_add(1).expect("ObjectId space exhausted");
        id
    }

    /// Issues the next `count` ids. Convenience for tests and batch commands.
    pub fn take(&mut self, count: usize) -> Vec<ObjectId> {
        (0..count).map(|_| self.next_id()).collect()
    }
}

/// Identity of a display output that a document is bound to.
///
/// This is a backend-supplied stable-ish name, never a monitor index: output
/// names can change across reboots, so persistence treats them as best-effort
/// hints and resolves misses through an explicit remap (FR-011, architecture
/// "Persistence").
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputId(String);

impl OutputId {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OutputId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A point in output-local logical units, origin at the output's top-left.
///
/// Logical units are not physical pixels. Scaling, rotation, and the macOS
/// coordinate-direction flip are applied at the backend boundary (FR-012).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalPoint {
    pub x: f64,
    pub y: f64,
}

impl LogicalPoint {
    /// Returns `None` for a non-finite coordinate.
    ///
    /// Negative and out-of-bounds values are accepted here: an input device can
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_monotonic_from_the_seed() {
        let mut ids = IdSource::starting_at(7);
        assert_eq!(ids.next_id().get(), 7);
        assert_eq!(ids.next_id().get(), 8);
    }

    #[test]
    fn logical_size_keeps_its_extents() {
        let size = LogicalSize::new(1920.0, 1080.0).expect("valid extents");
        assert_eq!(size.width(), 1920.0);
        assert_eq!(size.height(), 1080.0);
    }
}
