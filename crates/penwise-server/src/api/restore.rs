//! `POST /pages/{id}/restore?to_seq=N` and `GET /pages/{id}/history` (design §3.4–3.5).
//!
//! Restore appends compensating ops from the server device, so it is an ordinary logged change and can itself be
//! undone by restoring to an earlier seq. Unknown page → 404; a seq that was never logged → 404; a seq below the
//! retained log → 410. Items whose `Add` was compacted away and that changed after `to_seq` cannot be rolled back;
//! the 200 answer lists their ids in `skipped` (empty when the restore was complete).

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use penwise_proto::{DeviceId, ItemRef, PageId};
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use super::error::ApiError;
use super::resolve::parse_page;
use crate::storage::StorageError;

/// Batches returned when `limit` is absent / the largest accepted `limit`.
const DEFAULT_HISTORY: usize = 50;
const MAX_HISTORY: usize = 500;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/pages/{id}/restore", post(restore))
        .route("/pages/{id}/history", get(history))
}

#[derive(Debug, Deserialize)]
struct RestoreQuery {
    to_seq: Option<u64>,
}

async fn restore(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<RestoreQuery>,
) -> Result<Json<Value>, ApiError> {
    let page = parse_page(&app, &id).await?;
    let to_seq = query
        .to_seq
        .ok_or_else(|| ApiError::bad_request("to_seq is required (a seq from /history)"))?;
    let head = app.storage.status().await?.head_seq;
    if to_seq == 0 || to_seq > head {
        return Err(ApiError::not_found(format!(
            "seq {to_seq} was never logged (head is {head})"
        )));
    }
    let restored = app.storage.restore_page(page, to_seq).await?;
    let skipped: Vec<String> = restored
        .skipped
        .iter()
        .map(|id| ItemRef::from(*id).to_string())
        .collect();
    Ok(Json(json!({
        "restored": restored.appended.len(),
        "to_seq": to_seq,
        "head_seq": restored.appended.last().map_or(head, |a| a.seq),
        "skipped": skipped,
    })))
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    limit: Option<usize>,
}

/// One frame of ops per entry, newest first: `{seq, last_seq, device, received_ms, ops, summary}`. Pass a
/// batch's `last_seq` to `restore` to return the page to how it was right after that batch.
async fn history(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Value>, ApiError> {
    let page = parse_page(&app, &id).await?;
    if app.storage.page_view(page).await?.is_none() {
        return Err(ApiError::not_found("unknown page"));
    }
    let limit = query.limit.unwrap_or(DEFAULT_HISTORY).clamp(1, MAX_HISTORY);
    let batches = app
        .storage
        .read(move |conn| batches(conn, page, limit))
        .await?;
    Ok(Json(
        json!({ "page": page.to_string(), "batches": batches }),
    ))
}

fn batches(
    conn: &rusqlite::Connection,
    page: PageId,
    limit: usize,
) -> Result<Vec<Value>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT batch, MAX(seq), device, MIN(received_ms), COUNT(*), SUM(kind = 0)
           FROM ops WHERE page = ?1 GROUP BY batch ORDER BY batch DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![&page.0[..], i64::try_from(limit).unwrap_or(i64::MAX)],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        },
    )?;
    let mut out = Vec::new();
    for row in rows {
        let (first, last, device, received, ops, items) = row?;
        let device = DeviceId::from_slice(&device).map(|d| d.to_string());
        out.push(json!({
            "seq": first,
            "last_seq": last,
            "device": device,
            "received_ms": received,
            "ops": ops,
            "summary": format!("{items} item op(s), {} page op(s)", ops - items),
        }));
    }
    Ok(out)
}
