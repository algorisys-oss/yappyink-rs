//! The per-output document.
//!
//! FR-011: annotations are owned by an output and stored in that output's
//! logical coordinates. Each output has its own document, and switching
//! between outputs preserves all of them.
//!
//! History is not here. Undo, redo, and the command model are T017, and the
//! document is deliberately the thing commands act on rather than a thing that
//! records its own past.

use crate::error::DocumentError;
use crate::geometry::LogicalSize;
use crate::id::{ObjectId, OutputId};
use crate::limits;
use crate::object::Object;

/// The annotations on one output.
#[derive(Clone, Debug)]
pub struct Document {
    output: OutputId,
    output_size: LogicalSize,
    /// In paint order: a later object draws over an earlier one.
    objects: Vec<Object>,
}

impl Document {
    /// An empty document bound to `output`, whose logical size is recorded so
    /// the document can be remapped later if that output is missing or has
    /// changed (FR-011, T021).
    pub fn new(output: OutputId, output_size: LogicalSize) -> Self {
        Self {
            output,
            output_size,
            objects: Vec::new(),
        }
    }

    pub fn output(&self) -> &OutputId {
        &self.output
    }

    pub fn output_size(&self) -> LogicalSize {
        self.output_size
    }

    /// Appends an object, returning its id.
    ///
    /// Objects are never clamped to the output: a stroke can legitimately run
    /// past the edge because the pointer was there. What is visible is a
    /// rendering question.
    pub fn add(&mut self, object: Object) -> Result<ObjectId, DocumentError> {
        if object.output() != &self.output {
            return Err(DocumentError::OutputMismatch {
                object: object.id(),
                expected: self.output.clone(),
                found: object.output().clone(),
            });
        }
        if self.objects.len() >= limits::MAX_OBJECTS_PER_OUTPUT {
            return Err(DocumentError::ObjectLimitReached {
                limit: limits::MAX_OBJECTS_PER_OUTPUT,
            });
        }
        let id = object.id();
        self.objects.push(object);
        Ok(id)
    }

    /// Removes the named objects and returns them in document order.
    ///
    /// Ids that are not present are ignored. The returned objects are what an
    /// inverse command needs in order to put them back (T017), which is why
    /// they are handed over rather than dropped.
    pub fn remove(&mut self, ids: &[ObjectId]) -> Vec<Object> {
        let mut removed = Vec::new();
        self.objects.retain(|object| {
            if ids.contains(&object.id()) {
                removed.push(object.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    /// Removes every object, returning them in document order.
    ///
    /// Clear is one undoable command (FR-010), so the caller keeps what came
    /// back in order to invert it.
    pub fn clear(&mut self) -> Vec<Object> {
        std::mem::take(&mut self.objects)
    }

    pub fn objects(&self) -> impl Iterator<Item = &Object> {
        self.objects.iter()
    }

    pub fn get(&self, id: ObjectId) -> Option<&Object> {
        self.objects.iter().find(|object| object.id() == id)
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}
