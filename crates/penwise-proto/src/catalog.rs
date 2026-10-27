//! Notebook and page catalog materialized from notebook/page ops (LWW registers, [`crate::lww`]).
//!
//! Used by replicas (browser viewer, tablet). The server keeps the same registers in `SQLite`
//! (`notebooks`/`pages` tables with `*_hlc` columns) and must apply the identical rule.

use std::collections::BTreeMap;

use crate::error::ReplicaError;
use crate::hlc::Hlc;
use crate::ids::{NotebookId, PageId};
use crate::lww::Register;
use crate::notebook::{Background, Notebook, NotebookChange, Page, PageChange, PageKind};
use crate::op::{NotebookOp, PageOp};

mod state;

/// Notebook registers.
#[derive(Debug, Clone, PartialEq)]
struct NotebookEntry {
    created: Hlc,
    title: Register<String>,
    position: Register<i64>,
    deleted: Register<bool>,
}

/// Page registers and immutables.
#[derive(Debug, Clone, PartialEq)]
struct PageEntry {
    created: Hlc,
    notebook: NotebookId,
    kind: PageKind,
    title: Register<String>,
    position: Register<i64>,
    background: Register<Background>,
    deleted: Register<bool>,
}

/// All notebooks and pages a replica has seen (tombstones included).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    notebooks: BTreeMap<NotebookId, NotebookEntry>,
    pages: BTreeMap<PageId, PageEntry>,
}

impl NotebookEntry {
    fn new(created: Hlc, title: String, position: i64) -> Self {
        Self {
            created,
            title: Register::new(title, created),
            position: Register::new(position, created),
            deleted: Register::new(false, created),
        }
    }

    fn view(&self, id: NotebookId) -> Notebook {
        Notebook {
            id,
            title: self.title.value.clone(),
            position: self.position.value,
            deleted: self.deleted.value,
        }
    }
}

impl PageEntry {
    fn view(&self, id: PageId) -> Page {
        Page {
            id,
            notebook: self.notebook,
            title: self.title.value.clone(),
            position: self.position.value,
            kind: self.kind,
            background: self.background.value,
            deleted: self.deleted.value,
        }
    }
}

impl Catalog {
    /// Empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies a notebook op: `Create` inserts (all registers at `op.hlc`); `Update` LWW-sets each present
    /// register. Returns true when any visible value changed. Re-applying an op is a no-op `Ok(false)`.
    ///
    /// # Errors
    /// `Duplicate` for a `Create` of an existing id with a different stamp (the server never sends one);
    /// `UnknownNotebook` for an `Update` of an unknown id. The catalog is unchanged on error.
    pub fn apply_notebook(&mut self, op: &NotebookOp) -> Result<bool, ReplicaError> {
        match &op.change {
            NotebookChange::Create { title, position } => {
                if let Some(existing) = self.notebooks.get(&op.notebook) {
                    return same_creation(existing.created, op.hlc);
                }
                self.notebooks.insert(
                    op.notebook,
                    NotebookEntry::new(op.hlc, title.clone(), *position),
                );
                Ok(true)
            }
            NotebookChange::Update(patch) => {
                let entry = self
                    .notebooks
                    .get_mut(&op.notebook)
                    .ok_or(ReplicaError::UnknownNotebook)?;
                let before = entry.view(op.notebook);
                if let Some(title) = &patch.title {
                    entry.title.set(title.clone(), op.hlc);
                }
                if let Some(position) = patch.position {
                    entry.position.set(position, op.hlc);
                }
                if let Some(deleted) = patch.deleted {
                    entry.deleted.set(deleted, op.hlc);
                }
                Ok(entry.view(op.notebook) != before)
            }
        }
    }

    /// Applies a page op (as [`Catalog::apply_notebook`]; `Create` additionally requires the notebook to exist).
    ///
    /// # Errors
    /// `Duplicate`, `UnknownNotebook` (create in an unknown notebook), `UnknownPage` (update of an unknown page).
    pub fn apply_page(&mut self, op: &PageOp) -> Result<bool, ReplicaError> {
        match &op.change {
            PageChange::Create {
                notebook,
                title,
                position,
                kind,
                background,
            } => {
                if let Some(existing) = self.pages.get(&op.page) {
                    return same_creation(existing.created, op.hlc);
                }
                if !self.notebooks.contains_key(notebook) {
                    return Err(ReplicaError::UnknownNotebook);
                }
                let at = op.hlc;
                self.pages.insert(
                    op.page,
                    PageEntry {
                        created: at,
                        notebook: *notebook,
                        kind: *kind,
                        title: Register::new(title.clone(), at),
                        position: Register::new(*position, at),
                        background: Register::new(*background, at),
                        deleted: Register::new(false, at),
                    },
                );
                Ok(true)
            }
            PageChange::Update(patch) => {
                let entry = self
                    .pages
                    .get_mut(&op.page)
                    .ok_or(ReplicaError::UnknownPage)?;
                let before = entry.view(op.page);
                if let Some(title) = &patch.title {
                    entry.title.set(title.clone(), op.hlc);
                }
                if let Some(position) = patch.position {
                    entry.position.set(position, op.hlc);
                }
                if let Some(background) = patch.background {
                    entry.background.set(background, op.hlc);
                }
                if let Some(deleted) = patch.deleted {
                    entry.deleted.set(deleted, op.hlc);
                }
                Ok(entry.view(op.page) != before)
            }
        }
    }

    /// Drops notebook `id` (its pages stay) so a replacing `Create` can be applied.
    pub(crate) fn forget_notebook(&mut self, id: NotebookId) {
        self.notebooks.remove(&id);
    }

    /// Drops page `id`'s catalog entry so a replacing `Create` can be applied.
    pub(crate) fn forget_page(&mut self, id: PageId) {
        self.pages.remove(&id);
    }

    /// Materialized notebook (tombstones included).
    #[must_use]
    pub fn notebook(&self, id: NotebookId) -> Option<Notebook> {
        self.notebooks.get(&id).map(|e| e.view(id))
    }

    /// Materialized page (tombstones included).
    #[must_use]
    pub fn page(&self, id: PageId) -> Option<Page> {
        self.pages.get(&id).map(|e| e.view(id))
    }

    /// Live notebooks, ascending `(position, id)`.
    #[must_use]
    pub fn notebooks(&self) -> Vec<Notebook> {
        self.notebooks_where(false)
    }

    /// Tombstoned notebooks ("recently deleted"), ascending `(position, id)`.
    #[must_use]
    pub fn deleted_notebooks(&self) -> Vec<Notebook> {
        self.notebooks_where(true)
    }

    /// Live pages of `notebook`, ascending `(position, id)`. Empty for an unknown notebook.
    #[must_use]
    pub fn pages(&self, notebook: NotebookId) -> Vec<Page> {
        self.pages_where(notebook, false)
    }

    /// Tombstoned pages of `notebook` ("recently deleted"), ascending `(position, id)`.
    #[must_use]
    pub fn deleted_pages(&self, notebook: NotebookId) -> Vec<Page> {
        self.pages_where(notebook, true)
    }

    fn notebooks_where(&self, deleted: bool) -> Vec<Notebook> {
        let mut out: Vec<Notebook> = self
            .notebooks
            .iter()
            .map(|(&id, e)| e.view(id))
            .filter(|n| n.deleted == deleted)
            .collect();
        out.sort_by_key(|n| (n.position, n.id));
        out
    }

    fn pages_where(&self, notebook: NotebookId, deleted: bool) -> Vec<Page> {
        let mut out: Vec<Page> = self
            .pages
            .iter()
            .filter(|(_, e)| e.notebook == notebook)
            .map(|(&id, e)| e.view(id))
            .filter(|p| p.deleted == deleted)
            .collect();
        out.sort_by_key(|p| (p.position, p.id));
        out
    }
}

/// A `Create` of an existing id: no-op for the same stamp, `Duplicate` otherwise.
fn same_creation(existing: Hlc, stamp: Hlc) -> Result<bool, ReplicaError> {
    if existing == stamp {
        Ok(false)
    } else {
        Err(ReplicaError::Duplicate)
    }
}
