//! Legacy `objects` rows → item `Add` ops, quarantine of what cannot convert, and the read-back check.

use penwise_core::codec::item::decode_item;
use penwise_core::item::{Item, ItemKind};
use penwise_proto::PageId;
use penwise_proto::legacy::{
    LegacyImage, LegacyStroke, image_to_item, legacy_add_op, legacy_seed, stroke_to_item,
};
use rusqlite::{Transaction, params};
use serde::Deserialize;

use super::emit::{Emitter, fail};
use crate::storage::error::StorageError;
use crate::storage::legacy_codec::decode_stroke;

#[derive(Default)]
pub(super) struct ObjectCounts {
    pub(super) strokes: u64,
    pub(super) images: u64,
    pub(super) unmigrated: u64,
}

/// The JSON of an `image_v1` payload (`ImageMsg`; `id` and `layer` come from the row).
#[derive(Deserialize)]
struct ImagePayload {
    asset: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: Option<String>,
}

struct ObjectRow {
    id: String,
    kind: String,
    layer: String,
    z: i64,
    codec: String,
    payload: Vec<u8>,
}

pub(super) fn migrate_objects(
    e: &mut Emitter<'_>,
    page_text: &str,
    page: PageId,
) -> Result<ObjectCounts, StorageError> {
    let rows: Vec<ObjectRow> = e
        .tx
        .prepare(
            "SELECT id, kind, layer, z, codec, payload FROM legacy_objects WHERE page = ?1 ORDER BY z, id",
        )?
        .query_map([page_text], |r| {
            Ok(ObjectRow {
                id: r.get(0)?,
                kind: r.get(1)?,
                layer: r.get(2)?,
                z: r.get(3)?,
                codec: r.get(4)?,
                payload: r.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let mut counts = ObjectCounts::default();
    for row in rows {
        match convert(page_text, &row) {
            Ok(Converted {
                item,
                stroke,
                label,
            }) => {
                let hlc = e.stamp();
                e.emit(&legacy_add_op(page, e.device, hlc, item.clone()))?;
                if let Some(stroke) = &stroke {
                    verify_stroke(e.tx, &item, stroke, row.z)?;
                    counts.strokes += 1;
                } else {
                    counts.images += 1;
                }
                if let Some(label) = label {
                    e.tx.execute(
                        "INSERT INTO image_labels (item, label) VALUES (?1, ?2)",
                        params![&item.id.0[..], label],
                    )?;
                }
                e.after_item()?;
            }
            Err(reason) => {
                tracing::warn!(page = page_text, object = %row.id, %reason, "object quarantined");
                e.tx.execute(
                    "INSERT INTO legacy_unmigrated (page, id, kind, codec, payload, reason)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![page_text, row.id, row.kind, row.codec, row.payload, reason],
                )?;
                counts.unmigrated += 1;
            }
        }
    }
    Ok(counts)
}

struct Converted {
    item: Item,
    stroke: Option<LegacyStroke>,
    label: Option<String>,
}

fn convert(page_text: &str, row: &ObjectRow) -> Result<Converted, String> {
    match (row.kind.as_str(), row.codec.as_str()) {
        ("stroke", "stroke_v1") => {
            let stroke = decode_stroke(&row.payload, &row.layer, &row.id)
                .ok_or("undecodable stroke blob")?;
            let item = stroke_to_item(page_text, &stroke, row.z).map_err(|e| e.to_string())?;
            Ok(Converted {
                item,
                stroke: Some(stroke),
                label: None,
            })
        }
        ("image", "image_v1") => {
            let payload: ImagePayload =
                serde_json::from_slice(&row.payload).map_err(|e| format!("image json: {e}"))?;
            let image = LegacyImage {
                id: row.id.clone(),
                asset: payload.asset,
                x: payload.x,
                y: payload.y,
                w: payload.w,
                h: payload.h,
                layer: row.layer.clone(),
            };
            let item = image_to_item(page_text, &image, row.z).map_err(|e| e.to_string())?;
            Ok(Converted {
                item,
                stroke: None,
                label: payload.label,
            })
        }
        (kind, codec) => Err(format!("unknown object {kind}/{codec}")),
    }
}

/// Reads the stored row back and compares it to the source (argb, seed, sample count, z, layer).
fn verify_stroke(
    tx: &Transaction<'_>,
    item: &Item,
    source: &LegacyStroke,
    z: i64,
) -> Result<(), StorageError> {
    let (body, layer, stored_z): (Vec<u8>, i64, i64) = tx.query_row(
        "SELECT body, layer, z FROM items WHERE id = ?1",
        [&item.id.0[..]],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let decoded = decode_item(&body).map_err(|e| fail(format!("stored stroke: {e}")))?;
    let ItemKind::Stroke(stroke) = &decoded.kind else {
        return Err(fail("stored stroke has another kind"));
    };
    let want_layer = i64::from(u8::from(source.layer == "agent"));
    let same = decoded == *item
        && stroke.seed == legacy_seed(&source.seed)
        && stroke.params.color.to_argb() == source.argb.cast_unsigned()
        && stroke.samples.len() == source.times.len()
        && stored_z == z
        && layer == want_layer;
    if same {
        Ok(())
    } else {
        Err(fail(format!("stroke {} does not round-trip", source.seed)))
    }
}
