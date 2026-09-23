//! Identity for objects and outputs.

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

    /// Rebuilds an id from a raw value.
    ///
    /// For deserialization (T021) and for tests that need an id no source
    /// issued. Normal code takes ids from an [`IdSource`].
    pub fn from_raw(value: u64) -> Self {
        Self(value)
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
    /// the object limit in [`crate::limits`] is far below it.
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
/// A backend-supplied name, never a monitor index: output names can change
/// across reboots, so persistence treats them as best-effort hints and resolves
/// misses through an explicit remap (FR-011).
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
