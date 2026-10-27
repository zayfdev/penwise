//! Snapshot rows: the server's materialized `notebooks` / `pages` / `items` rows as wire types (design §4.2).
//!
//! A snapshot is a row dump, so a replica loads it without replaying history: [`NotebookState`] and
//! [`PageState`] carry every register with its stamp, [`ItemState`] carries the stored `body`/`patch`/
//! `stamps` of one item. Replicas **merge** rows register by register ([`crate::Replica::apply_snapshot_chunk`]),
//! which is commutative and idempotent, so a chunk delivered twice or after live ops is harmless.

use penwise_core::codec::edit::{decode_edit, encode_edit};
use penwise_core::codec::item::{decode_item, encode_item};
use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::item::ItemId;
use serde::{Deserialize, Serialize};

use crate::error::ProtoError;
use crate::hlc::Hlc;
use crate::ids::{NotebookId, PageId};
use crate::lww::{FieldStamps, MaterializedItem};
use crate::notebook::{Background, PageKind};

/// What a snapshot covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    /// Whole database: catalog and every page (tablet).
    All,
    /// Notebooks and page registers only, no items (browser, first frame).
    Catalog,
    /// One page's items (browser, after `Watch`).
    Page(PageId),
}

/// One notebook row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotebookState {
    pub id: NotebookId,
    pub created: Hlc,
    pub title: String,
    pub title_hlc: Hlc,
    pub position: i64,
    pub position_hlc: Hlc,
    pub deleted: bool,
    pub deleted_hlc: Hlc,
}

/// One page row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageState {
    pub id: PageId,
    pub notebook: NotebookId,
    pub kind: PageKind,
    pub created: Hlc,
    pub title: String,
    pub title_hlc: Hlc,
    pub position: i64,
    pub position_hlc: Hlc,
    pub background: Background,
    pub background_hlc: Hlc,
    pub deleted: bool,
    pub deleted_hlc: Hlc,
}

/// One item row: the creating `Add`'s item (`body`, PWIT blob) overlaid with `patch` (PWED `Update` blob
/// holding the winning value of every overwritten field except `deleted`), the field `stamps`
/// ([`FieldStamps::encode`]) and the winning tombstone flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemState {
    pub page: PageId,
    pub body: Vec<u8>,
    pub patch: Option<Vec<u8>>,
    pub stamps: Vec<u8>,
    pub deleted: bool,
}

/// One snapshot batch. Catalog rows before the item rows that reference them (create-before-reference); the
/// server sends notebooks, then pages, then items.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SnapshotChunk {
    pub notebooks: Vec<NotebookState>,
    pub pages: Vec<PageState>,
    pub items: Vec<ItemState>,
}

/// `PWED` `Update` blob for `patch` of item `id` (`items.patch`).
#[must_use]
pub fn encode_patch_blob(id: ItemId, patch: &ItemPatch) -> Vec<u8> {
    let mut out = Vec::new();
    encode_edit(
        &Edit::Update {
            id,
            patch: patch.clone(),
        },
        &mut out,
    );
    out
}

/// Inverse of [`encode_patch_blob`].
///
/// # Errors
/// `Codec` for a bad blob; `Invalid` when it is not an `Update` of `id`.
pub fn decode_patch_blob(bytes: &[u8], id: ItemId) -> Result<ItemPatch, ProtoError> {
    match decode_edit(bytes)? {
        Edit::Update { id: found, patch } if found == id => Ok(patch),
        _ => Err(ProtoError::Invalid("item patch blob")),
    }
}

impl ItemState {
    /// Row for a materialized item, collapsed: `body` is the item with every winning value, `patch` empty.
    /// (The server keeps the original `Add` body plus a patch instead; both load identically.)
    #[must_use]
    pub fn from_item(page: PageId, item: &MaterializedItem) -> Self {
        let mut body = Vec::new();
        encode_item(&item.item, &mut body);
        Self {
            page,
            body,
            patch: None,
            stamps: item.stamps.encode(),
            deleted: item.deleted,
        }
    }

    /// Rebuilds the materialized item: `decode(body)` ⊕ `patch`, with `stamps` and `deleted` as stored.
    ///
    /// # Errors
    /// `Codec` (bad `body`/`patch` blob), `Invalid` (patch of another item, a patch that sets `deleted`, or
    /// one the item rejects), plus the [`FieldStamps::decode`] errors.
    pub fn to_item(&self) -> Result<MaterializedItem, ProtoError> {
        let item = decode_item(&self.body)?;
        let stamps = FieldStamps::decode(&self.stamps)?;
        let item = match &self.patch {
            None => item,
            Some(blob) => {
                let patch = decode_patch_blob(blob, item.id)?;
                if patch.deleted.is_some() {
                    return Err(ProtoError::Invalid("item patch sets deleted"));
                }
                patch
                    .applied(&item, self.deleted)
                    .map_err(|_| ProtoError::Invalid("item patch"))?
                    .0
            }
        };
        Ok(MaterializedItem {
            item,
            deleted: self.deleted,
            stamps,
        })
    }
}
