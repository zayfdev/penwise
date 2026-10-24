//! Editing engine: the edit vocabulary, hit testing, lasso, transforms, erasers and undo.
//!
//! Owner: slice a (this file, `patch.rs`, `error.rs`, `proximity.rs`, `hit.rs`, `lasso.rs`, `transform.rs`,
//! `select.rs`); slice b (`erase_stroke.rs`, `erase_split.rs`, `erase_precise.rs`, `erase_mask.rs`,
//! `erase_fill.rs`); slice e (`undo.rs`). Contract: `docs/contracts/m2-editing.md`.
//!
//! Every tool is a pure function `(&Scene, input) → Vec<Edit>`: it never mutates the scene. The caller applies
//! the edits ([`crate::item::Scene::apply`], or [`UndoStack::apply_local`] to record them as one undo step) and
//! hands them to `penwise-proto`, which wraps each in an op envelope (op id, device id, HLC, seq). Edits carry
//! **absolute** values only, never deltas, so per-field last-writer-wins converges.
#![allow(
    clippy::todo,
    clippy::missing_panics_doc,
    clippy::unused_self,
    clippy::needless_pass_by_value,
    unused_variables,
    dead_code,
    reason = "M2 contract stubs; slices remove this"
)]

pub mod erase_fill;
pub mod erase_mask;
pub mod erase_precise;
pub mod erase_split;
pub mod erase_stroke;
mod error;
pub mod hit;
pub mod lasso;
mod patch;
pub mod proximity;
pub mod select;
pub mod transform;
pub mod undo;

pub use erase_precise::erase_precise;
pub use erase_stroke::erase_strokes;
pub use error::EditError;
pub use hit::{hit_test, topmost};
pub use lasso::{LassoParams, lasso};
pub use patch::{Field, ItemPatch};
pub use undo::UndoStack;

use crate::item::{Item, ItemId};

/// One change to a page (PLAN "Sync protocol": `AddItem`, `DeleteItem`, `UpdateItem`).
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Insert a new item (fresh id from an [`crate::item::IdSource`]).
    Add(Item),
    /// Tombstone an item (never hard-removed: undo and LWW need it). Equivalent to
    /// `Update { deleted: Some(true) }`; kept as its own variant because it is the common case and maps to
    /// `DeleteItem`.
    Delete(ItemId),
    /// Overwrite the set fields of `patch` with absolute values.
    Update { id: ItemId, patch: ItemPatch },
}

impl Edit {
    /// The item this edit targets.
    #[must_use]
    pub fn target(&self) -> ItemId {
        todo!()
    }
}
