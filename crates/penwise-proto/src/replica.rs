//! Client-side replica of the op log: catalog + one LWW-materialized scene per page.
//!
//! The browser viewer (via `penwise-wasm`) and the tablet (via `penwise-ffi`) feed it every op they
//! receive. The server materializes into `SQLite` instead but renders through [`PageReplica::from_items`].
//! Applying the same set of ops in any order (each item's `Add` first, per the create-before-reference
//! invariant), with duplicates, gives equal replicas.

use std::collections::{BTreeMap, BTreeSet};

use penwise_core::item::ItemId;

use crate::catalog::Catalog;
use crate::error::ReplicaError;
use crate::ids::PageId;
use crate::notebook::PageChange;
use crate::op::{AnyOp, SeqOp};
use crate::snapshot::{Scope, SnapshotChunk};

mod page;

pub use page::PageReplica;

/// What one applied op changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Applied {
    /// Page whose scene or page registers changed (`None`: notebook op or no visible change).
    pub page: Option<PageId>,
    /// The item whose scene state changed (`None` for page/notebook ops and no-ops): lets a renderer
    /// invalidate one item instead of the page.
    pub item: Option<ItemId>,
    /// Notebook list or a page list changed (UI lists need a refresh).
    pub catalog_changed: bool,
}

/// What one snapshot chunk changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapshotApplied {
    /// Notebook list or a page list changed.
    pub catalog_changed: bool,
    /// Pages that gained or changed items.
    pub pages: BTreeSet<PageId>,
}

/// Every page plus the catalog, tracking the highest applied `seq`.
#[derive(Debug, Clone)]
pub struct Replica {
    catalog: Catalog,
    pages: BTreeMap<PageId, PageReplica>,
    head_seq: u64,
}

impl Replica {
    /// Empty replica, `head_seq = 0`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            catalog: Catalog::new(),
            pages: BTreeMap::new(),
            head_seq: 0,
        }
    }

    /// Applies an op (pushed locally or received). Page `Create` creates its [`PageReplica`]; item ops go to
    /// their page.
    ///
    /// # Errors
    /// `UnknownPage` for an item op on a page never created, plus the catalog/page errors.
    pub fn apply(&mut self, op: &AnyOp) -> Result<Applied, ReplicaError> {
        match op {
            AnyOp::Notebook(op) => {
                let changed = self.catalog.apply_notebook(op)?;
                Ok(Applied {
                    page: None,
                    item: None,
                    catalog_changed: changed,
                })
            }
            AnyOp::Page(op) => {
                let changed = self.catalog.apply_page(op)?;
                if let PageChange::Create { kind, .. } = &op.change {
                    self.pages
                        .entry(op.page)
                        .or_insert_with(|| PageReplica::new(*kind));
                }
                Ok(Applied {
                    page: changed.then_some(op.page),
                    item: None,
                    catalog_changed: changed,
                })
            }
            AnyOp::Item(op) => {
                let page = self
                    .pages
                    .get_mut(&op.page)
                    .ok_or(ReplicaError::UnknownPage)?;
                let changed = page.apply(op)?;
                Ok(Applied {
                    page: changed.then_some(op.page),
                    item: changed.then(|| op.edit.target()),
                    catalog_changed: false,
                })
            }
        }
    }

    /// Applies a logged op: skipped (`Ok` with nothing changed) when `seq ≤ head_seq`; otherwise
    /// [`Replica::apply`] and `head_seq = seq` — also when `apply` fails, so one bad op never wedges sync
    /// (the error is still returned for logging).
    ///
    /// # Errors
    /// As [`Replica::apply`].
    pub fn apply_seq(&mut self, op: &SeqOp) -> Result<Applied, ReplicaError> {
        if op.seq <= self.head_seq {
            return Ok(Applied {
                page: None,
                item: None,
                catalog_changed: false,
            });
        }
        let result = self.apply(&op.op);
        self.head_seq = op.seq;
        result
    }

    /// Moves the cursor to `upto_seq` after an `OpsSince` batch: a browser-scope batch is filtered, so its
    /// `upto_seq` can exceed the last op's seq. Never moves backwards.
    pub fn advance_to(&mut self, upto_seq: u64) {
        self.head_seq = self.head_seq.max(upto_seq);
    }

    /// Starts a snapshot: drops what the snapshot will replace. `All` resets the whole replica (cursor
    /// included; re-apply the outbox afterwards), `Catalog` drops the catalog and every page's items, `Page(p)`
    /// empties page `p` (it must exist in the catalog).
    ///
    /// # Errors
    /// `UnknownPage` for `Page(p)` with `p` not in the catalog.
    pub fn begin_snapshot(&mut self, scope: Scope) -> Result<(), ReplicaError> {
        match scope {
            Scope::All => *self = Self::new(),
            Scope::Catalog => {
                self.catalog = Catalog::new();
                self.pages.clear();
            }
            Scope::Page(page) => {
                let kind = self
                    .catalog
                    .page(page)
                    .ok_or(ReplicaError::UnknownPage)?
                    .kind;
                self.pages.insert(page, PageReplica::new(kind));
            }
        }
        Ok(())
    }

    /// Folds one snapshot chunk in (rows merge register by register, so re-delivery is harmless): notebooks,
    /// then pages (creating their [`PageReplica`]s), then items. A failing row aborts the chunk; rows before it
    /// stay applied.
    ///
    /// # Errors
    /// The catalog errors, `UnknownPage` for an item row of a page not yet in the catalog, `Edit`/`Duplicate`
    /// from the page, and `Snapshot` for an undecodable item row.
    pub fn apply_snapshot_chunk(
        &mut self,
        chunk: &SnapshotChunk,
    ) -> Result<SnapshotApplied, ReplicaError> {
        let mut applied = SnapshotApplied::default();
        for state in &chunk.notebooks {
            applied.catalog_changed |= self.catalog.merge_notebook_state(state)?;
        }
        for state in &chunk.pages {
            applied.catalog_changed |= self.catalog.merge_page_state(state)?;
            self.pages
                .entry(state.id)
                .or_insert_with(|| PageReplica::new(state.kind));
        }
        for row in &chunk.items {
            let item = row.to_item()?;
            let page = self
                .pages
                .get_mut(&row.page)
                .ok_or(ReplicaError::UnknownPage)?;
            if page.merge_state(item)? {
                applied.pages.insert(row.page);
            }
        }
        Ok(applied)
    }

    /// Ends a snapshot taken at `head_seq`: the cursor becomes `max(cursor, head_seq)`.
    pub fn end_snapshot(&mut self, head_seq: u64) {
        self.advance_to(head_seq);
    }

    /// Highest applied `seq` (the client's `last_seen_seq`).
    #[must_use]
    pub fn head_seq(&self) -> u64 {
        self.head_seq
    }

    /// Notebooks and pages.
    #[must_use]
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// A page's replica (`None` before its `Create`).
    #[must_use]
    pub fn page(&self, id: PageId) -> Option<&PageReplica> {
        self.pages.get(&id)
    }
}

/// Replicas are equal when catalog, per-page item states and cursor are.
impl PartialEq for Replica {
    fn eq(&self, other: &Self) -> bool {
        self.head_seq == other.head_seq
            && self.catalog == other.catalog
            && self.pages == other.pages
    }
}

impl Default for Replica {
    fn default() -> Self {
        Self::new()
    }
}
