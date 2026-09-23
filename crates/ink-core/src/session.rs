//! A document with its history.
//!
//! This is what an editor holds. Every edit goes through here so that history
//! cannot drift out of step with the document: there is no way to change one
//! without the other.
//!
//! No-op edits are refused before they reach history (FR-010). Erasing nothing,
//! clearing an empty document, or restoring nothing leaves the undo stack
//! alone, so a user pressing undo never has to press it twice to skip over
//! something that did not happen.

use crate::command::Command;
use crate::document::Document;
use crate::error::DocumentError;
use crate::geometry::{LogicalPoint, LogicalRect, LogicalSize};
use crate::history::History;
use crate::id::{ObjectId, OutputId};
use crate::object::Object;

#[derive(Debug)]
pub struct Session {
    document: Document,
    history: History,
}

impl Session {
    pub fn new(output: OutputId, output_size: LogicalSize) -> Self {
        Self {
            document: Document::new(output, output_size),
            history: History::new(),
        }
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    /// Adds an object as one undoable edit.
    pub fn add(&mut self, object: Object) -> Result<ObjectId, DocumentError> {
        let id = object.id();
        let inverse = Command::Add { object }.apply(&mut self.document)?;
        self.history.record(inverse);
        Ok(id)
    }

    /// Erases every object the sweep touches, as **one** transaction.
    ///
    /// FR-009: an eraser drag is one undoable action, however many objects it
    /// removed. Undoing it brings all of them back, in order.
    ///
    /// Returns how many objects were removed. Zero means nothing was recorded.
    pub fn erase_along(
        &mut self,
        path: &[LogicalPoint],
        radius: f64,
    ) -> Result<usize, DocumentError> {
        let ids: Vec<ObjectId> = self
            .document
            .objects()
            .filter(|object| object.intersects_sweep(path, radius))
            .map(Object::id)
            .collect();
        self.run(Command::Remove { ids })
    }

    /// Removes every object as one undoable edit (FR-010).
    pub fn clear(&mut self) -> Result<usize, DocumentError> {
        let ids: Vec<ObjectId> = self.document.objects().map(Object::id).collect();
        self.run(Command::Remove { ids })
    }

    /// Applies a command unless it would change nothing.
    fn run(&mut self, command: Command) -> Result<usize, DocumentError> {
        if command.is_noop(&self.document) {
            return Ok(0);
        }
        let weight = command.weight();
        let inverse = command.apply(&mut self.document)?;
        self.history.record(inverse);
        Ok(weight)
    }

    /// The topmost object at a point, if any.
    ///
    /// Topmost because later objects paint over earlier ones, so the one the
    /// user can see is the one they mean.
    pub fn hit_test(&self, at: LogicalPoint, tolerance: f64) -> Option<ObjectId> {
        self.document
            .objects()
            .rfind(|object| object.contains_point(at, tolerance))
            .map(Object::id)
    }

    /// The combined bounds of some objects, ignoring ids that are not present.
    pub fn bounds_of(&self, ids: &[ObjectId]) -> Option<LogicalRect> {
        let mut found: Option<LogicalRect> = None;
        for object in self.document.objects().filter(|o| ids.contains(&o.id())) {
            let bounds = object.bounds();
            found = Some(match found {
                None => bounds,
                Some(so_far) => LogicalRect {
                    min: LogicalPoint {
                        x: so_far.min.x.min(bounds.min.x),
                        y: so_far.min.y.min(bounds.min.y),
                    },
                    max: LogicalPoint {
                        x: so_far.max.x.max(bounds.max.x),
                        y: so_far.max.y.max(bounds.max.y),
                    },
                },
            });
        }
        found
    }

    /// Removes the named objects as one undoable edit.
    pub fn delete(&mut self, ids: &[ObjectId]) -> Result<usize, DocumentError> {
        self.run(Command::Remove { ids: ids.to_vec() })
    }

    /// Moves objects as one undoable edit.
    pub fn move_objects(
        &mut self,
        ids: &[ObjectId],
        dx: f64,
        dy: f64,
    ) -> Result<usize, DocumentError> {
        self.transform(ids, |shape| shape.translated(dx, dy))
    }

    /// Scales objects about an anchor as one undoable edit.
    pub fn scale_objects(
        &mut self,
        ids: &[ObjectId],
        anchor: LogicalPoint,
        sx: f64,
        sy: f64,
    ) -> Result<usize, DocumentError> {
        self.transform(ids, |shape| shape.scaled(anchor, sx, sy))
    }

    /// Applies a geometry change to each named object.
    ///
    /// All or nothing. If the change would make any object invalid, such as
    /// scaling a rectangle down to nothing, the whole edit is refused and
    /// history is untouched. A partly applied transform would leave the user
    /// with a selection that no longer matches what they dragged.
    fn transform(
        &mut self,
        ids: &[ObjectId],
        change: impl Fn(&crate::object::Shape) -> Option<crate::object::Shape>,
    ) -> Result<usize, DocumentError> {
        let mut updated = Vec::new();
        for id in ids {
            let Some(index) = self.document.position_of(*id) else {
                continue;
            };
            let Some(object) = self.document.get(*id) else {
                continue;
            };
            let Some(shape) = change(object.shape()) else {
                return Err(DocumentError::DegenerateShape {
                    shape: "transformed object",
                });
            };
            updated.push((index, object.with_shape(shape)));
        }
        self.run(Command::Replace { objects: updated })
    }

    /// Undoes the most recent edit. False means there was nothing to undo.
    pub fn undo(&mut self) -> Result<bool, DocumentError> {
        self.history.undo(&mut self.document)
    }

    /// Redoes the most recently undone edit.
    pub fn redo(&mut self) -> Result<bool, DocumentError> {
        self.history.redo(&mut self.document)
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Rebinds to a different output, discarding history.
    ///
    /// Only valid while empty. History refers to objects on the old output, so
    /// keeping it across a rebind would let undo reinsert an object the
    /// document would then refuse. Moving a populated document between outputs
    /// is T027.
    pub fn rebind(&mut self, output: OutputId) -> bool {
        if !self.document.is_empty() {
            return false;
        }
        let size = self.document.output_size();
        self.document = Document::new(output, size);
        self.history = History::new();
        true
    }
}
