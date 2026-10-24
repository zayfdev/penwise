//! Client-side replica of the op log: catalog + one LWW-materialized scene per page.
//!
//! Owner: slice d.
//!
//! The browser viewer (via `penwise-wasm`) and the tablet (via `penwise-ffi`, M4) feed it every op they
//! receive. The server materializes into SQLite instead but renders through [`PageReplica::from_items`].
//! Applying the same set of ops in any order (each item's `Add` first, per the create-before-reference
//! invariant) gives bit-identical scenes.

use std::collections::BTreeMap;

use penwise_core::item::{ItemId, Scene};

use crate::catalog::Catalog;
use crate::error::ReplicaError;
use crate::ids::PageId;
use crate::lww::MaterializedItem;
use crate::notebook::PageKind;
use crate::op::{AnyOp, Op, SeqOp};

/// One page: winning item states and the core scene they produce.
#[derive(Debug, Clone)]
pub struct PageReplica {
    scene: Scene,
    items: BTreeMap<ItemId, MaterializedItem>,
}

/// What one applied op changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Applied {
    /// Page whose scene or page registers changed (`None`: notebook op or no visible change).
    pub page: Option<PageId>,
    /// Notebook list or a page list changed (UI lists need a refresh).
    pub catalog_changed: bool,
}

/// Every page plus the catalog, tracking the highest applied `seq`.
#[derive(Debug, Clone)]
pub struct Replica {
    catalog: Catalog,
    pages: BTreeMap<PageId, PageReplica>,
    head_seq: u64,
}

impl PageReplica {
    /// Empty page of `kind`.
    #[must_use]
    pub fn new(kind: PageKind) -> Self {
        todo!()
    }

    /// Page from stored states (server render, tablet page open): `Scene::from_items` over
    /// `(item, deleted)` pairs.
    ///
    /// # Errors
    /// [`ReplicaError::Edit`] from `Scene::from_items` (duplicate id, invalid item).
    pub fn from_items(
        kind: PageKind,
        items: impl IntoIterator<Item = MaterializedItem>,
    ) -> Result<Self, ReplicaError> {
        todo!()
    }

    /// Applies one item op: `Add` → [`MaterializedItem::from_add`] + `Scene::apply(Add)`; `Delete`/`Update`
    /// → [`MaterializedItem::merge`] and `Scene::apply` of the winning sub-edit. Returns true when the scene
    /// changed. Re-applying the same `Add` (same id, same stamp) is `Ok(false)`.
    ///
    /// # Errors
    /// `Duplicate` (Add of an existing id with another stamp), `UnknownItem`, `Edit` (scene rejected the
    /// edit; the replica is unchanged).
    pub fn apply(&mut self, op: &Op) -> Result<bool, ReplicaError> {
        todo!()
    }

    /// The page scene (render order, geometry, hit tests).
    #[must_use]
    pub fn scene(&self) -> &Scene {
        todo!()
    }

    /// Winning state of an item (tombstones included).
    #[must_use]
    pub fn item(&self, id: ItemId) -> Option<&MaterializedItem> {
        todo!()
    }
}

impl Replica {
    /// Empty replica, `head_seq = 0`.
    #[must_use]
    pub fn new() -> Self {
        todo!()
    }

    /// Applies an op (pushed locally or received). Page `Create` creates its [`PageReplica`]; item ops go to
    /// their page.
    ///
    /// # Errors
    /// `UnknownPage` for an item op on a page never created, plus the catalog/page errors.
    pub fn apply(&mut self, op: &AnyOp) -> Result<Applied, ReplicaError> {
        todo!()
    }

    /// Applies a logged op: skipped (`Ok` with nothing changed) when `seq ≤ head_seq`; otherwise
    /// [`Replica::apply`] and `head_seq = seq` — also when `apply` fails, so one bad op never wedges sync
    /// (the error is still returned for logging).
    ///
    /// # Errors
    /// As [`Replica::apply`].
    pub fn apply_seq(&mut self, op: &SeqOp) -> Result<Applied, ReplicaError> {
        todo!()
    }

    /// Highest applied `seq` (the client's `last_seen_seq`).
    #[must_use]
    pub fn head_seq(&self) -> u64 {
        todo!()
    }

    /// Notebooks and pages.
    #[must_use]
    pub fn catalog(&self) -> &Catalog {
        todo!()
    }

    /// A page's replica (`None` before its `Create`).
    #[must_use]
    pub fn page(&self, id: PageId) -> Option<&PageReplica> {
        todo!()
    }
}

impl Default for Replica {
    fn default() -> Self {
        Self::new()
    }
}
