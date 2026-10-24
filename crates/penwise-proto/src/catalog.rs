//! Notebook and page catalog materialized from notebook/page ops (LWW registers, [`crate::lww`]).
//!
//! Owner: slice d.
//!
//! Used by replicas (browser viewer, tablet). The server keeps the same registers in SQLite
//! (`notebooks`/`pages` tables with `*_hlc` columns) and must apply the identical rule.

use std::collections::BTreeMap;

use crate::error::ReplicaError;
use crate::ids::{NotebookId, PageId};
use crate::lww::Register;
use crate::notebook::{Background, Notebook, Page, PageKind};
use crate::op::{NotebookOp, PageOp};
use crate::hlc::Hlc;

/// Notebook registers.
#[derive(Debug, Clone, PartialEq)]
struct NotebookEntry {
    created: Hlc,
    title: Register<String>,
    deleted: Register<bool>,
}

/// Page registers and immutables.
#[derive(Debug, Clone, PartialEq)]
struct PageEntry {
    created: Hlc,
    notebook: NotebookId,
    kind: PageKind,
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

impl Catalog {
    /// Empty catalog.
    #[must_use]
    pub fn new() -> Self {
        todo!()
    }

    /// Applies a notebook op: `Create` inserts (all registers at `op.hlc`); `Update` LWW-sets each present
    /// register. Returns true when any visible value changed. Re-applying an op is a no-op `Ok(false)`.
    ///
    /// # Errors
    /// `Duplicate` for a `Create` of an existing id with a different stamp (the server never sends one);
    /// `UnknownNotebook` for an `Update` of an unknown id. The catalog is unchanged on error.
    pub fn apply_notebook(&mut self, op: &NotebookOp) -> Result<bool, ReplicaError> {
        todo!()
    }

    /// Applies a page op (as [`Catalog::apply_notebook`]; `Create` additionally requires the notebook to exist).
    ///
    /// # Errors
    /// `Duplicate`, `UnknownNotebook` (create in an unknown notebook), `UnknownPage` (update of an unknown page).
    pub fn apply_page(&mut self, op: &PageOp) -> Result<bool, ReplicaError> {
        todo!()
    }

    /// Materialized notebook (tombstones included).
    #[must_use]
    pub fn notebook(&self, id: NotebookId) -> Option<Notebook> {
        todo!()
    }

    /// Materialized page (tombstones included).
    #[must_use]
    pub fn page(&self, id: PageId) -> Option<Page> {
        todo!()
    }

    /// Live notebooks, ascending id (= creation time).
    #[must_use]
    pub fn notebooks(&self) -> Vec<Notebook> {
        todo!()
    }

    /// Live pages of `notebook`, ascending `(position, id)`. Empty for an unknown notebook.
    #[must_use]
    pub fn pages(&self, notebook: NotebookId) -> Vec<Page> {
        todo!()
    }
}
