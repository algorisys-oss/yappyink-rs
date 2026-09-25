//! The per-output document.
//!
//! FR-011: annotations are owned by an output and stored in that output's
//! logical coordinates. Each output has its own document, and switching
//! between outputs preserves all of them.
//!
//! History is not here. Undo, redo, and the command model are T017, and the
//! document is deliberately the thing commands act on rather than a thing that
//! records its own past.

use std::sync::atomic::{AtomicU64, Ordering};

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
    /// Changes whenever the document might have.
    revision: u64,
}

/// Where revisions come from. One counter for the whole process, so that no
/// two document states ever share a revision, including a document adopted
/// from a file in place of another. A per-document counter would restart at
/// zero on load, and a cache keyed on it would show the old ink.
static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_revision() -> u64 {
    NEXT_REVISION.fetch_add(1, Ordering::Relaxed)
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
            revision: next_revision(),
        }
    }

    /// A number that changes whenever the document might have changed.
    ///
    /// For caches, such as a raster of the whole document: equal revisions
    /// mean identical content. It is bumped at the start of every method that
    /// takes `&mut self`, including ones that then fail, so a false change is
    /// possible and a missed one is not.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn touch(&mut self) {
        self.revision = next_revision();
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
        self.touch();
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
        self.touch();
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
        self.touch();
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
        self.touch();
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
        self.touch();
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
        self.touch();
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

#[cfg(test)]
mod revision_tests {
    use super::*;
    use crate::{LogicalPoint, Opacity, Rgb, Shape, StrokeKind, Style, Width};

    fn doc() -> Document {
        Document::new(OutputId::new("r"), LogicalSize::new(100.0, 100.0).unwrap())
    }

    fn stroke(id: u64) -> Object {
        let points = vec![
            LogicalPoint::new(1.0, 1.0).unwrap(),
            LogicalPoint::new(9.0, 9.0).unwrap(),
        ];
        Object::new(
            ObjectId::from_raw(id),
            OutputId::new("r"),
            Style::new(
                Rgb::new(0, 0, 0),
                Width::new(2.0).unwrap(),
                Opacity::new(1.0).unwrap(),
            ),
            Shape::stroke(StrokeKind::Pen, points).unwrap(),
        )
    }

    /// A cache keyed on the revision is only correct if every change moves it.
    #[test]
    fn every_change_moves_the_revision() {
        let mut document = doc();
        let mut seen = vec![document.revision()];
        let mut check = |document: &Document| {
            assert!(
                !seen.contains(&document.revision()),
                "a change kept an old revision"
            );
            seen.push(document.revision());
        };
        document.add(stroke(1)).unwrap();
        check(&document);
        document.add(stroke(2)).unwrap();
        check(&document);
        let removed = document.remove_with_positions(&[ObjectId::from_raw(1)]);
        check(&document);
        document.insert_at(removed).unwrap();
        check(&document);
        document.remove(&[ObjectId::from_raw(2)]);
        check(&document);
        document.clear();
        check(&document);
    }

    /// Two documents never share a revision, so adopting a loaded file in
    /// place of the current one cannot look unchanged to a cache.
    #[test]
    fn a_new_document_never_reuses_a_revision() {
        let first = doc();
        let second = doc();
        assert_ne!(first.revision(), second.revision());
    }

    #[test]
    fn reading_does_not_move_it() {
        let document = doc();
        let before = document.revision();
        let _ = document.len();
        let _ = document.objects().count();
        assert_eq!(document.revision(), before);
    }
}
