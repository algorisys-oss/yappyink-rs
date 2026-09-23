//! Document edits that can be undone.
//!
//! Three commands, each the inverse of another: [`Command::Add`] inverts to
//! [`Command::Remove`], `Remove` inverts to [`Command::Insert`], and `Insert`
//! inverts back to `Remove`. Applying one returns the command that would undo
//! it, so history never has to reason about what an edit meant.
//!
//! Clear is not a fourth variant. It is `Remove` over everything, which is why
//! clearing an empty document is naturally a no-op rather than a special case
//! (FR-010).
//!
//! Inverses carry whole objects and their positions rather than a recipe for
//! rebuilding them. Undo has to restore order and style exactly, and the
//! cheapest way to guarantee that is to keep what was there.

use crate::document::Document;
use crate::error::DocumentError;
use crate::id::ObjectId;
use crate::object::Object;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Append one object.
    Add { object: Object },
    /// Remove objects by id. Ids that are not present are ignored.
    Remove { ids: Vec<ObjectId> },
    /// Put objects back at the positions they came from.
    ///
    /// Positions are ascending, which is what makes the restoration exact:
    /// inserting in that order reproduces the original sequence.
    Insert { objects: Vec<(usize, Object)> },
}

impl Command {
    /// Applies the command and returns the command that would undo it.
    pub fn apply(self, document: &mut Document) -> Result<Command, DocumentError> {
        match self {
            Self::Add { object } => {
                let id = document.add(object)?;
                Ok(Self::Remove { ids: vec![id] })
            }
            Self::Remove { ids } => {
                let removed = document.remove_with_positions(&ids);
                Ok(Self::Insert { objects: removed })
            }
            Self::Insert { objects } => {
                let ids = objects.iter().map(|(_, object)| object.id()).collect();
                document.insert_at(objects)?;
                Ok(Self::Remove { ids })
            }
        }
    }

    /// Whether applying this would change nothing, so history should not
    /// record it (FR-010).
    pub fn is_noop(&self, document: &Document) -> bool {
        match self {
            Self::Add { .. } => false,
            Self::Remove { ids } => !ids.iter().any(|id| document.get(*id).is_some()),
            Self::Insert { objects } => objects.is_empty(),
        }
    }

    /// How many objects this command carries, for history budgeting.
    pub fn weight(&self) -> usize {
        match self {
            Self::Add { .. } => 1,
            Self::Remove { ids } => ids.len(),
            Self::Insert { objects } => objects.len(),
        }
    }
}
