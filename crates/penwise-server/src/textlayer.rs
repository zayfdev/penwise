//! Text items as the text layer: recognized handwriting (`Layer::Text`) and typed text, read straight from the
//! stored items. `/text` and `/search` both read through here, so there is no separate text cache to go stale.

use std::collections::BTreeMap;

use penwise_core::item::{ItemId, ItemKind, Layer};
use penwise_proto::{ItemState, PageId};

use crate::storage::{PageView, Storage, StorageError};

/// One live text item.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextRow {
    pub id: ItemId,
    pub layer: Layer,
    pub content: String,
    /// Page position of the box's top-left corner.
    pub at: [f32; 2],
    /// Strokes the text was recognized from.
    pub sources: Vec<ItemId>,
}

/// The live text items of a loaded page, in reading order (top to bottom, then left to right).
pub(crate) fn rows_of(view: &PageView) -> Vec<TextRow> {
    let mut rows: Vec<TextRow> = view
        .replica
        .items()
        .filter(|state| !state.deleted)
        .filter_map(|state| row_of(&state.item))
        .collect();
    reading_order(&mut rows);
    rows
}

fn row_of(item: &penwise_core::item::Item) -> Option<TextRow> {
    let ItemKind::Text(text) = &item.kind else {
        return None;
    };
    Some(TextRow {
        id: item.id,
        layer: item.layer,
        content: text.content.clone(),
        at: [item.transform.e, item.transform.f],
        sources: text.sources.clone(),
    })
}

fn reading_order(rows: &mut [TextRow]) {
    rows.sort_by(|a, b| {
        a.at[1]
            .total_cmp(&b.at[1])
            .then(a.at[0].total_cmp(&b.at[0]))
            .then(a.id.cmp(&b.id))
    });
}

/// A page's text: its rows' content, one block per item.
pub(crate) fn joined(rows: &[TextRow]) -> String {
    rows.iter()
        .map(|row| row.content.trim())
        .filter(|content| !content.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every live text item of every live page, by page, each in reading order.
///
/// # Errors
/// `Corrupt` for a row that does not decode, `Sqlite`.
pub(crate) async fn all_rows(
    storage: &Storage,
) -> Result<BTreeMap<PageId, Vec<TextRow>>, StorageError> {
    storage
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT i.page, i.body, i.patch, i.stamps FROM items i
                 JOIN pages p ON p.id = i.page
                 WHERE i.kind = 4 AND i.deleted = 0 AND p.deleted = 0",
            )?;
            let mut rows = stmt.query([])?;
            let mut by_page: BTreeMap<PageId, Vec<TextRow>> = BTreeMap::new();
            while let Some(row) = rows.next()? {
                let page = PageId::from_slice(&row.get::<_, Vec<u8>>(0)?)
                    .ok_or_else(|| StorageError::Corrupt("items.page".into()))?;
                let state = ItemState {
                    page,
                    body: row.get(1)?,
                    patch: row.get(2)?,
                    stamps: row.get(3)?,
                    deleted: false,
                };
                let item = state
                    .to_item()
                    .map_err(|error| StorageError::Corrupt(format!("text item: {error}")))?;
                by_page.entry(page).or_default().extend(row_of(&item.item));
            }
            for rows in by_page.values_mut() {
                reading_order(rows);
            }
            Ok(by_page)
        })
        .await
}
