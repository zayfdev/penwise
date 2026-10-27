//! Ops: the append-only change log entries (PLAN "Sync protocol": `AddItem`, `DeleteItem`, `UpdateItem`
//! = core [`Edit`]; `PageOp`; `NotebookOp`).
//!
//! Every op carries `{op_id, device, hlc}`; `hlc.device == device` always. Ops are immutable once created; the
//! server assigns each accepted op a `seq` (its log position) without changing it.
//!
//! **Create-before-reference invariant** (enforced by the server, relied on by every replica): an accepted
//! op's target (item, page, notebook) was created by an earlier-seq op, and an item op's `page` is the page
//! the item was added to. Causality follows: a device can only reference what it has seen, and its clock
//! has observed that creation's stamp, so a reference's `hlc` is greater than its target's creation `hlc`.
//!
//! Serde: `Op::edit` is carried as a core `PWED` v1 edit blob ([`edit_blob`]); all else is plain postcard.
//! **Append-only:** new `AnyOp` variants go at the end; existing structs never change (new shapes = new
//! variants), see [`crate::envelope`].

use penwise_core::edit::Edit;
use serde::{Deserialize, Serialize};

use crate::hlc::{Hlc, MAX_WALL_MS};
use crate::ids::{DeviceId, ItemRef, NotebookId, OpId, PageId};
use crate::notebook::{NotebookChange, PageChange};

/// Item op: one core edit on one page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Op {
    pub op_id: OpId,
    pub device: DeviceId,
    pub hlc: Hlc,
    pub page: PageId,
    #[serde(with = "edit_blob")]
    pub edit: Edit,
}

/// Page create / register update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageOp {
    pub op_id: OpId,
    pub device: DeviceId,
    pub hlc: Hlc,
    pub page: PageId,
    pub change: PageChange,
}

/// Notebook create / register update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotebookOp {
    pub op_id: OpId,
    pub device: DeviceId,
    pub hlc: Hlc,
    pub notebook: NotebookId,
    pub change: NotebookChange,
}

/// Any log entry. Variant order is frozen (postcard discriminant; `SQLite` `ops.kind` = discriminant).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnyOp {
    /// Kind 0. Boxed: an [`Op`] carries the whole edit payload, far larger than the other variants.
    Item(Box<Op>),
    /// Kind 1.
    Page(PageOp),
    /// Kind 2.
    Notebook(NotebookOp),
}

/// An op with its server log position (`seq ≥ 1`, strictly increasing, never reused).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeqOp {
    pub seq: u64,
    pub op: AnyOp,
}

/// Why the server refused an op (reported in `ServerMsg::Ack::rejected`; the op is not logged). Append-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
pub enum OpRejection {
    /// `op.device` or `op.hlc.device` differs from the authenticated session device.
    #[error("op not authored by this device")]
    NotOwnDevice,
    /// `hlc.wall_ms` more than `MAX_SKEW_MS` ahead of the server clock.
    #[error("device clock too far ahead")]
    ClockSkew,
    /// Stateless validation failed ([`AnyOp::validate`]).
    #[error("invalid op")]
    Invalid,
    /// Item or page op for a page never created.
    #[error("unknown page")]
    UnknownPage,
    /// Page create in, or notebook update of, a notebook never created.
    #[error("unknown notebook")]
    UnknownNotebook,
    /// `Delete`/`Update` of an item never added.
    #[error("unknown item")]
    UnknownItem,
    /// Create of an id that already exists (item, page or notebook) under a different `op_id`.
    #[error("duplicate id")]
    Duplicate,
    /// Item op whose `page` is not the page the item was added to.
    #[error("item lives on another page")]
    WrongPage,
}

impl AnyOp {
    /// Header op id.
    #[must_use]
    pub fn op_id(&self) -> OpId {
        match self {
            Self::Item(op) => op.op_id,
            Self::Page(op) => op.op_id,
            Self::Notebook(op) => op.op_id,
        }
    }

    /// Header author.
    #[must_use]
    pub fn device(&self) -> DeviceId {
        match self {
            Self::Item(op) => op.device,
            Self::Page(op) => op.device,
            Self::Notebook(op) => op.device,
        }
    }

    /// Header stamp.
    #[must_use]
    pub fn hlc(&self) -> Hlc {
        match self {
            Self::Item(op) => op.hlc,
            Self::Page(op) => op.hlc,
            Self::Notebook(op) => op.hlc,
        }
    }

    /// The same op under a new stamp (re-stamping a refused op; `hlc.device` must stay the author's).
    #[must_use]
    pub fn with_hlc(mut self, hlc: Hlc) -> Self {
        match &mut self {
            Self::Item(op) => op.hlc = hlc,
            Self::Page(op) => op.hlc = hlc,
            Self::Notebook(op) => op.hlc = hlc,
        }
        self
    }

    /// Page an item/page op belongs to; `None` for notebook ops.
    #[must_use]
    pub fn page(&self) -> Option<PageId> {
        match self {
            Self::Item(op) => Some(op.page),
            Self::Page(op) => Some(op.page),
            Self::Notebook(_) => None,
        }
    }

    /// Target item of an item op ([`Edit::target`]); `None` otherwise.
    #[must_use]
    pub fn item(&self) -> Option<ItemRef> {
        match self {
            Self::Item(op) => Some(op.edit.target().into()),
            Self::Page(_) | Self::Notebook(_) => None,
        }
    }

    /// Frozen kind code = variant discriminant (`Item 0, Page 1, Notebook 2`).
    #[must_use]
    pub fn kind_code(&self) -> u8 {
        match self {
            Self::Item(_) => 0,
            Self::Page(_) => 1,
            Self::Notebook(_) => 2,
        }
    }

    /// Stateless checks every receiver can run without the log: `hlc.device == device`;
    /// `hlc.wall_ms ≤ MAX_WALL_MS`; item `Add` passes [`penwise_core::item::Item::validate`]; item `Update`
    /// patch non-empty; [`PageChange::is_valid`]; [`NotebookChange::is_valid`].
    ///
    /// # Errors
    /// [`OpRejection::NotOwnDevice`] for the device mismatch, else [`OpRejection::Invalid`].
    pub fn validate(&self) -> Result<(), OpRejection> {
        let hlc = self.hlc();
        if hlc.device != self.device() {
            return Err(OpRejection::NotOwnDevice);
        }
        if hlc.wall_ms > MAX_WALL_MS {
            return Err(OpRejection::Invalid);
        }
        let valid = match self {
            Self::Item(op) => match &op.edit {
                Edit::Add(item) => item.validate().is_ok(),
                Edit::Delete(_) => true,
                Edit::Update { patch, .. } => !patch.is_empty(),
            },
            Self::Page(op) => op.change.is_valid(),
            Self::Notebook(op) => op.change.is_valid(),
        };
        if valid {
            Ok(())
        } else {
            Err(OpRejection::Invalid)
        }
    }
}

/// Serde adapter carrying a core [`Edit`] as one byte string: the full `PWED` v1 blob
/// (`penwise_core::codec::edit::encode_edit`, header + FNV trailer), so the edit format versions itself
/// independently of postcard.
pub mod edit_blob {
    use core::fmt;

    use penwise_core::codec::edit::{decode_edit, encode_edit};
    use penwise_core::edit::Edit;
    use serde::de::{self, Visitor};
    use serde::{Deserializer, Serializer};

    /// Writes `encode_edit(edit)` as a postcard byte string (`serialize_bytes`).
    ///
    /// # Errors
    /// Only the serializer's own errors.
    pub fn serialize<S: Serializer>(edit: &Edit, serializer: S) -> Result<S::Ok, S::Error> {
        let mut blob = Vec::new();
        encode_edit(edit, &mut blob);
        serializer.serialize_bytes(&blob)
    }

    struct BlobVisitor;

    impl Visitor<'_> for BlobVisitor {
        type Value = Edit;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a PWED edit blob")
        }

        fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Edit, E> {
            decode_edit(bytes).map_err(E::custom)
        }
    }

    /// Reads a byte string and `decode_edit`s it; a `CodecError` becomes `D::Error::custom`.
    ///
    /// # Errors
    /// Malformed bytes or a codec error.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Edit, D::Error> {
        deserializer.deserialize_bytes(BlobVisitor)
    }
}
