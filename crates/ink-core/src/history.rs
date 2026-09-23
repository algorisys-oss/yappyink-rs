//! Undo and redo (FR-010).
//!
//! Undo and redo act on completed user actions, never on pointer samples. An
//! eraser sweep that removes nine objects is one entry, because the user made
//! one gesture.

use crate::command::Command;
use crate::document::Document;
use crate::error::DocumentError;
use crate::limits;

/// The stacks of inverse commands.
#[derive(Debug, Default)]
pub struct History {
    /// Commands that would undo each past edit, most recent last.
    undo: Vec<Command>,
    /// Commands that would redo each undone edit, most recent last.
    redo: Vec<Command>,
    /// True once the limit has forced an old entry out, so the user can be
    /// told rather than silently losing the ability to undo (NFR-003).
    truncated: bool,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the inverse of an edit that has just been applied.
    ///
    /// Clears the redo stack: once a new edit is committed, the branch the
    /// user walked away from is gone. Cancelling a preview never reaches here,
    /// which is why cancelling preserves redo.
    pub fn record(&mut self, inverse: Command) {
        self.redo.clear();
        self.undo.push(inverse);

        // Enforced at a transaction boundary, never mid-edit, so the document
        // is always consistent with what history claims about it.
        while self.undo.len() > limits::MAX_UNDO_TRANSACTIONS {
            self.undo.remove(0);
            self.truncated = true;
        }
    }

    /// Undoes the most recent edit. Returns false when there is nothing to undo.
    pub fn undo(&mut self, document: &mut Document) -> Result<bool, DocumentError> {
        let Some(command) = self.undo.pop() else {
            return Ok(false);
        };
        let inverse = command.apply(document)?;
        self.redo.push(inverse);
        Ok(true)
    }

    /// Redoes the most recently undone edit.
    pub fn redo(&mut self, document: &mut Document) -> Result<bool, DocumentError> {
        let Some(command) = self.redo.pop() else {
            return Ok(false);
        };
        let inverse = command.apply(document)?;
        self.undo.push(inverse);
        Ok(true)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Whether the limit has discarded the oldest history.
    pub fn was_truncated(&self) -> bool {
        self.truncated
    }
}
