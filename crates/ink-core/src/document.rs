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

    /// Removes the named objects and returns them with the positions they
    /// occupied, in ascending order.
    ///
    /// The positions are what make undo exact: reinserting them in this order
    /// reproduces the original sequence, so a restored object is painted where
    /// it was rather than on top (FR-010).
    pub fn remove_with_positions(&mut self, ids: &[ObjectId]) -> Vec<(usize, Object)> {
        let mut removed = Vec::new();
        let mut index = 0;
        self.objects.retain(|object| {
            let keep = !ids.contains(&object.id());
            if !keep {
                removed.push((index, object.clone()));
            }
            index += 1;
            keep
        });
        removed
    }

    /// Puts objects back at the given positions.
    ///
    /// Positions must be ascending, which is how [`Self::remove_with_positions`]
    /// produces them. A position past the end of the document is refused
    /// rather than silently appended, because that would mean history and the
    /// document disagree about what happened.
    pub fn insert_at(&mut self, objects: Vec<(usize, Object)>) -> Result<(), DocumentError> {
        if self.objects.len() + objects.len() > limits::MAX_OBJECTS_PER_OUTPUT {
            return Err(DocumentError::ObjectLimitReached {
                limit: limits::MAX_OBJECTS_PER_OUTPUT,
            });
        }
        for (index, object) in objects {
            if object.output() != &self.output {
                return Err(DocumentError::OutputMismatch {
                    object: object.id(),
                    expected: self.output.clone(),
                    found: object.output().clone(),
                });
            }
            if index > self.objects.len() {
                return Err(DocumentError::InvalidPosition {
                    index,
                    length: self.objects.len(),
                });
            }
            self.objects.insert(index, object);
        }
        Ok(())
    }

    /// Swaps objects at the given positions, returning what was there.
    ///
    /// Positions and ids must both match what is present. A mismatch is an
    /// error rather than a silent insert, because it would mean the caller and
    /// the document disagree about what exists.
    pub fn replace_at(
        &mut self,
        objects: Vec<(usize, Object)>,
    ) -> Result<Vec<(usize, Object)>, DocumentError> {
        for (index, object) in &objects {
            match self.objects.get(*index) {
                Some(existing) if existing.id() == object.id() => {}
                Some(_) | None => {
                    return Err(DocumentError::InvalidPosition {
                        index: *index,
                        length: self.objects.len(),
                    });
                }
            }
            if object.output() != &self.output {
                return Err(DocumentError::OutputMismatch {
                    object: object.id(),
                    expected: self.output.clone(),
                    found: object.output().clone(),
                });
            }
        }

        let mut previous = Vec::with_capacity(objects.len());
        for (index, object) in objects {
            previous.push((index, std::mem::replace(&mut self.objects[index], object)));
        }
        Ok(previous)
    }

    /// The position of an object, if it is present.
    pub fn position_of(&self, id: ObjectId) -> Option<usize> {
        self.objects.iter().position(|object| object.id() == id)
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

    pub fn objects(&self) -> impl DoubleEndedIterator<Item = &Object> {
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
