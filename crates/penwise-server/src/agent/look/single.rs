//! `look {mark}`: one mark's full bundle with its crops.

use serde_json::{Value, json};

use crate::api::{ApiError, AppState};
use crate::rounds::Ledger;
use penwise_core::item::ItemId;

use super::super::images::ImageKind;
use super::collect::bundles_of;
use super::gather::Views;
use super::{b64, mark_seen};

/// `mark=<id>`: the full v1 bundle of that mark and its crops.
pub(super) async fn one_mark(app: &AppState, ledger: &Ledger, id: &str) -> Result<Value, ApiError> {
    let mark_id = ItemId::from_hex(id)
        .ok_or_else(|| ApiError::bad_request("mark id is 32 hex characters"))?;
    let mark = ledger
        .find(mark_id)
        .ok_or_else(|| ApiError::not_found("unknown mark"))?;
    let view = app
        .storage
        .page_view(mark.page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown mark"))?;
    let bundles = bundles_of(app, &view, vec![mark.entry.clone()]).await;
    let bundle = bundles
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::not_found("unknown mark"))?;
    let mut images = Vec::new();
    for (kind, name) in [
        (ImageKind::WithInk, "with_ink"),
        (ImageKind::Context, "context"),
    ] {
        if let Some(png) = app.images.get(mark_id, kind, view.updated_seq) {
            images.push(json!({ "label": format!("{name} {id}"), "png": b64::encode(&png) }));
        }
    }
    mark_seen(app, &Views::from([(mark.page, view)]), &[mark]).await?;
    Ok(json!({
        "text": serde_json::to_string_pretty(&bundle).unwrap_or_default(),
        "images": images,
        "round": ledger.current(),
        "marks": [id],
    }))
}
