//! What a picture stands for ("hero section", a URL): `image_labels(item, label)`, keyed by the image item's id.
//! Not synced, so it lives in the sidecar tables rather than in the op log.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use penwise_core::item::ItemId;
use penwise_proto::{ItemRef, PageId};

use crate::api::ApiError;
use crate::sidecar::with_connection;

/// Longest stored label, in bytes.
pub(super) const MAX_LABEL_BYTES: usize = 512;

pub(super) async fn put(
    data_dir: &Arc<PathBuf>,
    item: ItemId,
    label: String,
) -> Result<(), ApiError> {
    with_connection(data_dir, move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO image_labels (item, label) VALUES (?1, ?2)",
            rusqlite::params![&item.0[..], label],
        )
        .map(drop)
    })
    .await
}

/// The labels of `page`'s pictures.
pub(super) async fn of_page(
    data_dir: &Arc<PathBuf>,
    page: PageId,
) -> Result<BTreeMap<ItemId, String>, ApiError> {
    with_connection(data_dir, move |conn| {
        let mut stmt = conn.prepare(
            "SELECT l.item, l.label FROM image_labels l JOIN items i ON i.id = l.item WHERE i.page = ?1",
        )?;
        let rows = stmt.query_map([&page.0[..]], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut labels = BTreeMap::new();
        for row in rows {
            let (item, label) = row?;
            if let Some(item) = ItemRef::from_slice(&item) {
                labels.insert(item.into(), label);
            }
        }
        Ok(labels)
    })
    .await
}
