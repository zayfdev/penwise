//! Snapshot rows in and out of the [`Catalog`]: register-wise merge of `NotebookState`/`PageState` rows and
//! export of every row with its stamps.

use super::{Catalog, NotebookEntry, PageEntry};
use crate::error::ReplicaError;
use crate::lww::Register;
use crate::snapshot::{NotebookState, PageState};

impl Catalog {
    /// Folds a snapshot row into the catalog: inserts an unknown notebook, or merges the registers of a known
    /// one with the same creation stamp. Returns true when a visible value changed.
    ///
    /// # Errors
    /// `Duplicate` when the id exists with another creation stamp.
    pub fn merge_notebook_state(&mut self, state: &NotebookState) -> Result<bool, ReplicaError> {
        let Some(entry) = self.notebooks.get_mut(&state.id) else {
            self.notebooks.insert(
                state.id,
                NotebookEntry {
                    created: state.created,
                    title: Register::new(state.title.clone(), state.title_hlc),
                    position: Register::new(state.position, state.position_hlc),
                    deleted: Register::new(state.deleted, state.deleted_hlc),
                },
            );
            return Ok(true);
        };
        if entry.created != state.created {
            return Err(ReplicaError::Duplicate);
        }
        let before = entry.view(state.id);
        entry.title.set(state.title.clone(), state.title_hlc);
        entry.position.set(state.position, state.position_hlc);
        entry.deleted.set(state.deleted, state.deleted_hlc);
        Ok(entry.view(state.id) != before)
    }

    /// As [`Catalog::merge_notebook_state`] for a page row.
    ///
    /// # Errors
    /// `UnknownNotebook` (create-before-reference), `Duplicate` (other creation stamp, notebook or kind).
    pub fn merge_page_state(&mut self, state: &PageState) -> Result<bool, ReplicaError> {
        let Some(entry) = self.pages.get_mut(&state.id) else {
            if !self.notebooks.contains_key(&state.notebook) {
                return Err(ReplicaError::UnknownNotebook);
            }
            self.pages.insert(
                state.id,
                PageEntry {
                    created: state.created,
                    notebook: state.notebook,
                    kind: state.kind,
                    title: Register::new(state.title.clone(), state.title_hlc),
                    position: Register::new(state.position, state.position_hlc),
                    background: Register::new(state.background, state.background_hlc),
                    deleted: Register::new(state.deleted, state.deleted_hlc),
                },
            );
            return Ok(true);
        };
        if entry.created != state.created || entry.notebook != state.notebook {
            return Err(ReplicaError::Duplicate);
        }
        let before = entry.view(state.id);
        entry.title.set(state.title.clone(), state.title_hlc);
        entry.position.set(state.position, state.position_hlc);
        entry.background.set(state.background, state.background_hlc);
        entry.deleted.set(state.deleted, state.deleted_hlc);
        Ok(entry.view(state.id) != before)
    }

    /// Every notebook row with its register stamps, ascending id (persistence / snapshot export).
    #[must_use]
    pub fn notebook_states(&self) -> Vec<NotebookState> {
        self.notebooks
            .iter()
            .map(|(&id, e)| NotebookState {
                id,
                created: e.created,
                title: e.title.value.clone(),
                title_hlc: e.title.stamp,
                position: e.position.value,
                position_hlc: e.position.stamp,
                deleted: e.deleted.value,
                deleted_hlc: e.deleted.stamp,
            })
            .collect()
    }

    /// Every page row with its register stamps, ascending id (persistence / snapshot export).
    #[must_use]
    pub fn page_states(&self) -> Vec<PageState> {
        self.pages
            .iter()
            .map(|(&id, e)| PageState {
                id,
                notebook: e.notebook,
                kind: e.kind,
                created: e.created,
                title: e.title.value.clone(),
                title_hlc: e.title.stamp,
                position: e.position.value,
                position_hlc: e.position.stamp,
                background: e.background.value,
                background_hlc: e.background.stamp,
                deleted: e.deleted.value,
                deleted_hlc: e.deleted.stamp,
            })
            .collect()
    }
}
