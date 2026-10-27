//! Dead letters: ops the server refused for good, kept for the user (show, export, dismiss).
//!
//! Blob ([`encode_dead_letters`], little-endian): `u8 version = 1`, `u32 n`, then per letter `[16] op id`,
//! `u8 kind` (0 item add, 1 item update, 2 item delete, 3 page op, 4 notebook op, 255 undecodable),
//! `[16] page` (all zero when none), `u8 reason` ([`reason_code`]), `u32 size` (stored envelope bytes).

use penwise_core::edit::Edit;
use penwise_proto::{AnyOp, OpId, OpRejection, PageId, decode_op};

use super::SyncCore;
use crate::bytes::Writer;

/// Blob layout version.
pub const DEAD_VERSION: u8 = 1;

/// What the UI needs to list one dead letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadInfo {
    pub op_id: OpId,
    pub kind: u8,
    pub page: Option<PageId>,
    pub reason: u8,
    pub size: u32,
}

/// `OpRejection` declaration order: `NotOwnDevice` 0, `ClockSkew` 1, `Invalid` 2, `UnknownPage` 3,
/// `UnknownNotebook` 4, `UnknownItem` 5, `Duplicate` 6, `WrongPage` 7.
#[must_use]
pub fn reason_code(reason: OpRejection) -> u8 {
    match reason {
        OpRejection::NotOwnDevice => 0,
        OpRejection::ClockSkew => 1,
        OpRejection::Invalid => 2,
        OpRejection::UnknownPage => 3,
        OpRejection::UnknownNotebook => 4,
        OpRejection::UnknownItem => 5,
        OpRejection::Duplicate => 6,
        OpRejection::WrongPage => 7,
    }
}

fn kind_of(op: &AnyOp) -> u8 {
    match op {
        AnyOp::Item(i) => match i.edit {
            Edit::Add(_) => 0,
            Edit::Update { .. } => 1,
            Edit::Delete(_) => 2,
        },
        AnyOp::Page(_) => 3,
        AnyOp::Notebook(_) => 4,
    }
}

impl SyncCore {
    /// The dead letters, oldest first.
    #[must_use]
    pub fn dead_letters(&self) -> Vec<DeadInfo> {
        self.outbox
            .dead()
            .iter()
            .map(|d| {
                let op = decode_op(&d.bytes).ok();
                DeadInfo {
                    op_id: d.op_id,
                    kind: op.as_ref().map_or(255, kind_of),
                    page: op.as_ref().and_then(AnyOp::page),
                    reason: reason_code(d.reason),
                    size: u32::try_from(d.bytes.len()).unwrap_or(u32::MAX),
                }
            })
            .collect()
    }

    /// Stored envelope bytes of one dead letter (`penwise_proto::decode_op`), for export.
    #[must_use]
    pub fn dead_letter_bytes(&self, op_id: OpId) -> Option<Vec<u8>> {
        self.outbox
            .dead()
            .iter()
            .find(|d| d.op_id == op_id)
            .map(|d| d.bytes.clone())
    }

    /// Dismisses one dead letter (after the user exported or dismissed it).
    pub fn remove_dead_letter(&mut self, op_id: OpId) -> bool {
        let removed = self.outbox.remove_dead(op_id).is_some();
        if removed {
            self.revision += 1;
        }
        removed
    }
}

/// The blob of [`SyncCore::dead_letters`].
#[must_use]
pub fn encode_dead_letters(letters: &[DeadInfo]) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(DEAD_VERSION);
    w.count(letters.len());
    for letter in letters {
        w.raw(&letter.op_id.to_bytes());
        w.u8(letter.kind);
        w.raw(&letter.page.map_or([0; 16], PageId::to_bytes));
        w.u8(letter.reason);
        w.u32(letter.size);
    }
    w.finish()
}
