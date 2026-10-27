//! Restore (design §3.5): compensating ops from the server device, never an overwrite.
//!
//! State at `to_seq` is rebuilt by replaying the page's retained item ops with `seq ≤ to_seq`. Items added
//! at or before the log floor have no `Add` left in the log, so their field history before the floor is
//! unknowable: they are left as they are (only items added after `to_seq` are deleted and items whose whole
//! history is retained are rolled back field by field).

use std::collections::HashMap;

use penwise_core::edit::{Edit, Field, ItemPatch};
use penwise_core::item::ItemId;
use penwise_proto::{AnyOp, MaterializedItem, Op, OpId, PageId, PageReplica, decode_op};
use rusqlite::params;

use super::Appended;
use super::error::{RestoreError, StorageError};
use super::random::random_u128;
use super::read::head_and_floor;
use super::rows::{corrupt, to_i64, to_u64};
use super::snapshot::{for_item_batches, page_states};
use super::writer::Writer;

pub(crate) fn run(
    w: &mut Writer,
    page: PageId,
    to_seq: u64,
) -> Result<Vec<Appended>, RestoreError> {
    let Some(state) = page_states(&w.conn, Some(page))?.into_iter().next() else {
        return Err(RestoreError::UnknownPage);
    };
    let (_, floor) = head_and_floor(&w.conn)?;
    if to_seq < floor {
        return Err(RestoreError::BelowFloor { floor });
    }
    let then = replay(w, page, state.kind, to_seq)?;
    let added = added_seqs(w, page)?;
    let mut current = Vec::new();
    for_item_batches(&w.conn, Some(page), |batch| {
        for row in batch {
            current.push(row.to_item().map_err(|e| corrupt("item row", &e))?);
        }
        Ok(())
    })?;
    let mut edits = Vec::new();
    for cur in &current {
        let added_seq = added.get(&cur.item.id).copied().unwrap_or(0);
        compensate(cur, added_seq, to_seq, then.item(cur.item.id), &mut edits);
    }
    if edits.is_empty() {
        return Ok(Vec::new());
    }
    let now = (w.now_ms)();
    let device = w.server_device;
    let ops: Vec<AnyOp> = edits
        .into_iter()
        .map(|edit| {
            AnyOp::Item(Box::new(Op {
                op_id: OpId::from_parts(now, random_u128()),
                device,
                hlc: w.clock.send(now),
                page,
                edit,
            }))
        })
        .collect();
    Ok(w.append(device, &ops)?)
}

fn replay(
    w: &Writer,
    page: PageId,
    kind: penwise_proto::PageKind,
    to_seq: u64,
) -> Result<PageReplica, StorageError> {
    let mut replica = PageReplica::new(kind);
    let mut stmt = w
        .conn
        .prepare("SELECT body FROM ops WHERE page = ?1 AND kind = 0 AND seq <= ?2 ORDER BY seq")?;
    let rows = stmt.query_map(params![&page.0[..], to_i64(to_seq)], |r| {
        r.get::<_, Vec<u8>>(0)
    })?;
    for body in rows {
        if let AnyOp::Item(op) = decode_op(&body?).map_err(|e| corrupt("stored op", &e))? {
            // A refused op (it was valid when logged, but its target may be gone) changes nothing.
            let _ = replica.apply(&op);
        }
    }
    Ok(replica)
}

fn added_seqs(w: &Writer, page: PageId) -> Result<HashMap<ItemId, u64>, StorageError> {
    let mut stmt = w
        .conn
        .prepare("SELECT id, added_seq FROM items WHERE page = ?1")?;
    let rows = stmt.query_map([&page.0[..]], |r| {
        Ok((r.get::<_, [u8; 16]>(0)?, r.get::<_, i64>(1)?))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, seq) = row?;
        out.insert(ItemId(id), to_u64(seq));
    }
    Ok(out)
}

/// Edits turning `cur` back into its state at `to_seq` (`then`, when the replay knows the item).
fn compensate(
    cur: &MaterializedItem,
    added_seq: u64,
    to_seq: u64,
    then: Option<&MaterializedItem>,
    out: &mut Vec<Edit>,
) {
    let id = cur.item.id;
    if added_seq > to_seq {
        if !cur.deleted {
            out.push(Edit::Delete(id));
        }
        return;
    }
    let Some(then) = then else { return };
    if then.deleted {
        if !cur.deleted {
            out.push(Edit::Delete(id));
        }
        return;
    }
    let tag = cur.item.kind.tag();
    let differing: Vec<Field> = Field::ALL
        .into_iter()
        .filter(|f| f.applies_to(tag))
        .filter(|&f| {
            ItemPatch::capture(&cur.item, cur.deleted, &[f])
                != ItemPatch::capture(&then.item, then.deleted, &[f])
        })
        .collect();
    if !differing.is_empty() {
        out.push(Edit::Update {
            id,
            patch: ItemPatch::capture(&then.item, then.deleted, &differing),
        });
    }
}
