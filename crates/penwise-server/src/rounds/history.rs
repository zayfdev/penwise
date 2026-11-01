//! `GET /projects/<id>/history?route=<r>&preset=<p>`: the generations a lane has kept, newest first.

use axum::Json;
use axum::extract::{Path, Query, State};
use penwise_core::item::FramePreset;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState};
use crate::storage::{Generation, GenerationRole};

use super::require_project;

/// The query: the lane is a route and a preset (`desktop` unless named).
#[derive(Debug, Deserialize)]
pub(super) struct HistoryQuery {
    route: Option<String>,
    preset: Option<String>,
}

/// `GET /projects/<id>/history?route=&preset=` — `{"route","preset","generations":[{"round","resolved_ms",
/// "og":{"item","asset","w","h"},"variants":[{"label","item","asset","w","h","picked"}]}]}`, newest round first.
///
/// `item` is the picture's Image item on the board and `asset` the id of its bytes: the tablet fetches them with
/// `GET /assets/<asset>` (the route [`crate::pictures::routes`] serves). Only resolved rounds are listed. Same
/// auth as the other `/projects` routes (token, or a same-machine request).
pub(super) async fn history(
    State(app): State<AppState>,
    Path(project): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Value>, ApiError> {
    require_project(&app, &project).await?;
    let route = query
        .route
        .filter(|route| !route.is_empty())
        .ok_or_else(|| ApiError::bad_request("route is required (e.g. route=/)"))?;
    let preset = FramePreset::parse(query.preset.as_deref().unwrap_or("desktop"))
        .ok_or_else(|| ApiError::bad_request("preset is one of phone, tablet, desktop, custom"))?
        .as_str();
    let rows = app
        .storage
        .generations(project, Some((route.clone(), preset.to_owned())))
        .await?;
    Ok(Json(json!({
        "route": route,
        "preset": preset,
        "generations": by_round(&rows),
    })))
}

/// The rows (newest round first) folded into one entry per round.
fn by_round(rows: &[Generation]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut at = 0;
    while let Some(first) = rows.get(at) {
        let end = rows
            .iter()
            .skip(at)
            .position(|row| row.round != first.round)
            .map_or(rows.len(), |offset| at + offset);
        let round = rows.get(at..end).unwrap_or_default();
        let picture = |row: &Generation| json!({ "item": row.item, "asset": row.asset, "w": row.w, "h": row.h });
        let variants: Vec<Value> = round
            .iter()
            .filter(|row| row.role == GenerationRole::Variant)
            .map(|row| {
                json!({
                    "label": row.label, "item": row.item, "asset": row.asset,
                    "w": row.w, "h": row.h, "picked": row.picked,
                })
            })
            .collect();
        out.push(json!({
            "round": first.round,
            "resolved_ms": first.resolved_ms,
            "og": round.iter().find(|row| row.role == GenerationRole::Og).map(picture),
            "variants": variants,
        }));
        at = end;
    }
    out
}
