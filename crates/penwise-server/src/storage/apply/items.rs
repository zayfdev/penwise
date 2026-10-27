//! Item ops: `Add` creates the row, `Delete`/`Update` merge per-field LWW into `patch` + `stamps`.

use penwise_core::codec::item::encode_item;
use penwise_core::edit::Edit;
use penwise_core::edit::{Field, ItemPatch};
use penwise_core::item::{Item, ItemId, ItemKind, ItemKindTag};
use penwise_proto::snapshot::{ItemState, encode_patch_blob};
use penwise_proto::{FieldStamps, Op, OpRejection, PageId};
use rusqlite::{OptionalExtension, Transaction, params};

use super::{Done, Fail};
use crate::storage::error::StorageError;
use crate::storage::rows::{corrupt, to_i64, to_u64};

pub(super) fn apply(tx: &Transaction<'_>, op: &Op, seq: u64) -> Result<Done, Fail> {
    if !page_exists(tx, op.page)? {
        return Err(OpRejection::UnknownPage.into());
    }
    match &op.edit {
        Edit::Add(item) => add(tx, op, item, seq),
        Edit::Delete(id) => {
            let patch = ItemPatch {
                deleted: Some(true),
                ..ItemPatch::default()
            };
            merge(tx, op, *id, &patch, seq)
        }
        Edit::Update { id, patch } => merge(tx, op, *id, patch, seq),
    }
}

fn page_exists(tx: &Transaction<'_>, page: PageId) -> Result<bool, rusqlite::Error> {
    tx.prepare_cached("SELECT 1 FROM pages WHERE id = ?1")?
        .exists([&page.0[..]])
}

/// The page an item lives on, `None` for an unknown id.
fn item_page(tx: &Transaction<'_>, id: ItemId) -> Result<Option<[u8; 16]>, rusqlite::Error> {
    tx.prepare_cached("SELECT page FROM items WHERE id = ?1")?
        .query_row([&id.0[..]], |r| r.get::<_, [u8; 16]>(0))
        .optional()
}

/// `id` must exist on `page` (erase target, trim origin).
fn require_on_page(tx: &Transaction<'_>, id: ItemId, page: PageId) -> Result<(), Fail> {
    match item_page(tx, id)? {
        None => Err(OpRejection::UnknownItem.into()),
        Some(found) if found != page.0 => Err(OpRejection::WrongPage.into()),
        Some(_) => Ok(()),
    }
}

fn add(tx: &Transaction<'_>, op: &Op, item: &Item, seq: u64) -> Result<Done, Fail> {
    match &item.kind {
        ItemKind::Erase(erase) => require_on_page(tx, erase.target, op.page)?,
        ItemKind::Stroke(stroke) => {
            if let Some(trim) = &stroke.trim {
                require_on_page(tx, ItemId(trim.origin.0), op.page)?;
            }
        }
        _ => {}
    }
    let existing = tx
        .prepare_cached("SELECT page, stamps, added_seq FROM items WHERE id = ?1")?
        .query_row([&item.id.0[..]], |r| {
            Ok((
                r.get::<_, [u8; 16]>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .optional()?;
    if let Some((page, stamps, added_seq)) = existing {
        let stamps = FieldStamps::decode(&stamps).map_err(|e| corrupt("item stamps", &e))?;
        return if page == op.page.0 && stamps.created == op.hlc {
            Ok(Done::Existing(to_u64(added_seq)))
        } else {
            Err(OpRejection::Duplicate.into())
        };
    }
    let purged = tx
        .prepare_cached("SELECT 1 FROM purged_items WHERE id = ?1")?
        .exists([&item.id.0[..]])?;
    if purged {
        // Compaction purged this item's tombstone: re-adding it would resurrect erased ink.
        return Err(OpRejection::Duplicate.into());
    }
    let mut body = Vec::new();
    encode_item(item, &mut body);
    tx.prepare_cached(
        "INSERT INTO items (id, page, kind, layer, z, deleted, body, patch, stamps, added_seq, updated_seq)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, NULL, ?7, ?8, ?8)",
    )?
    .execute(params![
        &item.id.0[..],
        &op.page.0[..],
        i64::from(item.kind.tag().code()),
        i64::from(item.layer.code()),
        item.z,
        body,
        FieldStamps::new(op.hlc).encode(),
        to_i64(seq),
    ])?;
    Ok(Done::Applied)
}

struct Row {
    page: [u8; 16],
    kind: i64,
    deleted: bool,
    stamps: Vec<u8>,
}

fn merge(
    tx: &Transaction<'_>,
    op: &Op,
    id: ItemId,
    patch: &ItemPatch,
    seq: u64,
) -> Result<Done, Fail> {
    let row = tx
        .prepare_cached("SELECT page, kind, deleted, stamps FROM items WHERE id = ?1")?
        .query_row([&id.0[..]], |r| {
            Ok(Row {
                page: r.get(0)?,
                kind: r.get(1)?,
                deleted: r.get::<_, i64>(2)? != 0,
                stamps: r.get(3)?,
            })
        })
        .optional()?
        .ok_or(OpRejection::UnknownItem)?;
    if row.page != op.page.0 {
        return Err(OpRejection::WrongPage.into());
    }
    let tag = u8::try_from(row.kind)
        .ok()
        .and_then(ItemKindTag::from_code)
        .ok_or_else(|| StorageError::Corrupt("item kind".into()))?;
    if patch.fields().into_iter().any(|f| !f.applies_to(tag)) {
        return Err(OpRejection::Invalid.into());
    }
    let mut stamps = FieldStamps::decode(&row.stamps).map_err(|e| corrupt("item stamps", &e))?;
    let fields = patch.fields();
    let winners = stamps.winners(&fields, op.hlc);
    if winners.is_empty() {
        if !fields.is_empty() && fields.iter().all(|&f| stamps.get(f) == op.hlc) {
            // Every register already carries this op's stamp: a retry whose log row was compacted away.
            return Ok(Done::Existing(super::log_floor(tx)?));
        }
        return Ok(Done::Applied);
    }
    for &field in &winners {
        stamps.set(field, op.hlc);
    }
    let winning = patch.restrict(&winners);
    if winners == [Field::Deleted] {
        write_deleted(tx, id, &stamps, winning.deleted == Some(true), seq)?;
        return Ok(Done::Applied);
    }
    write_update(tx, id, &row, &stamps, &winning, seq)
}

fn write_deleted(
    tx: &Transaction<'_>,
    id: ItemId,
    stamps: &FieldStamps,
    deleted: bool,
    seq: u64,
) -> Result<(), rusqlite::Error> {
    tx.prepare_cached(
        "UPDATE items SET deleted = ?2, stamps = ?3, updated_seq = ?4 WHERE id = ?1",
    )?
    .execute(params![
        &id.0[..],
        i64::from(deleted),
        stamps.encode(),
        to_i64(seq)
    ])?;
    Ok(())
}

/// Update with at least one non-tombstone field: validate against the materialized item, store the new patch.
fn write_update(
    tx: &Transaction<'_>,
    id: ItemId,
    row: &Row,
    stamps: &FieldStamps,
    winning: &ItemPatch,
    seq: u64,
) -> Result<Done, Fail> {
    let (page, body, old_patch): (Vec<u8>, Vec<u8>, Option<Vec<u8>>) = tx
        .prepare_cached("SELECT page, body, patch FROM items WHERE id = ?1")?
        .query_row([&id.0[..]], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    let page =
        PageId::from_slice(&page).ok_or_else(|| StorageError::Corrupt("item page".into()))?;
    let state = ItemState {
        page,
        body,
        patch: old_patch,
        stamps: Vec::new(),
        deleted: row.deleted,
    };
    // `to_item` needs stamps only to decode them; the current ones are valid.
    let state = ItemState {
        stamps: stamps.encode(),
        ..state
    };
    let current = state.to_item().map_err(|e| corrupt("item", &e))?;
    let (item, deleted) = winning
        .applied(&current.item, current.deleted)
        .map_err(|_| OpRejection::Invalid)?;
    let mut merged = match &state.patch {
        Some(blob) => penwise_proto::snapshot::decode_patch_blob(blob, current.item.id)
            .map_err(|e| corrupt("item patch", &e))?,
        None => ItemPatch::default(),
    };
    let mut stored = winning.clone();
    stored.deleted = None;
    merged.merge(&stored);
    tx.prepare_cached(
        "UPDATE items SET layer = ?2, z = ?3, deleted = ?4, patch = ?5, stamps = ?6, updated_seq = ?7
         WHERE id = ?1",
    )?
    .execute(params![
        &id.0[..],
        i64::from(item.layer.code()),
        item.z,
        i64::from(deleted),
        encode_patch_blob(id, &merged),
        stamps.encode(),
        to_i64(seq),
    ])?;
    Ok(Done::Applied)
}
