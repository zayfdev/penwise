//! `POST /frames/<id>/ghost` (design-harness M4, P4): the agent shows a temporary patch live on a frame, and
//! clears it again.
//!
//! The patch is P1 JSON: [`crate::frames::patch::Patch`] validates and sanitizes it before anything is stored.
//! The reply reports what the shared headless browser could not do — an op whose selector matches nothing, or a
//! selector the browser rejects — so the agent can fix its proposal instead of guessing. The patch itself is
//! stored even with failures, exactly like a ghost with a stale selector: the rest of the page still shows.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use penwise_core::item::{FrameItem, ItemId, set_patch};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState, Author};
use crate::frames::patch::Patch;

use super::frame::{FrameView, apply_patch};

/// `POST /frames/<id>/ghost {"patch": P1 | null}` (an absent patch clears too).
#[derive(Debug, Deserialize)]
pub(crate) struct GhostBody {
    patch: Option<Value>,
}

/// Sets or clears the frame's ghost; `{"ok", "failed"}` describes the headless validation run.
pub(crate) async fn ghost(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<GhostBody>,
) -> Result<Json<Value>, ApiError> {
    let frame_id = frame_id(&id)?;
    let page = app
        .storage
        .page_of_item(frame_id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown frame"))?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown frame"))?;
    let frame = super::scene::frame_item(view.replica.scene(), frame_id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("unknown frame"))?;
    let patch = stored_patch(body.patch)?;
    let failed = match patch.as_deref() {
        Some(text) => validate(&app, frame_id, &frame, text).await?,
        None => Vec::new(),
    };
    let edits = set_patch(view.replica.scene(), frame_id, patch)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let mut author = Author::new(&app.storage);
    for edit in edits {
        author.item(page, edit).await?;
    }
    author.commit().await?;
    Ok(Json(json!({ "ok": failed.is_empty(), "failed": failed })))
}

fn frame_id(text: &str) -> Result<ItemId, ApiError> {
    ItemId::from_hex(text).ok_or_else(|| ApiError::bad_request("frame id is 32 hex characters"))
}

/// The sanitized P1 JSON text; `None` (absent or `null`) clears the ghost.
fn stored_patch(patch: Option<Value>) -> Result<Option<String>, ApiError> {
    let Some(value) = patch.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let text = serde_json::to_string(&value)
        .map_err(|error| ApiError::bad_request(format!("patch: {error}")))?;
    let parsed =
        Patch::parse(&text).map_err(|error| ApiError::bad_request(format!("patch: {error}")))?;
    Ok(Some(parsed.to_text()))
}

/// Applies `text` in a fresh page of the frame's route and returns the bridge's `failed` list.
async fn validate(
    app: &AppState,
    frame_id: ItemId,
    frame: &FrameItem,
    text: &str,
) -> Result<Vec<Value>, ApiError> {
    // Validate against the page as the code has it: the stored ghost is not part of the proposal.
    let mut probe = frame.clone();
    probe.patch = None;
    let browser = FrameView::open(
        app.inspect.as_ref(),
        &app.projects,
        frame_id,
        &probe,
        &[],
        "",
    )
    .await
    .map_err(|error| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, error.to_string()))?;
    let result = apply_patch(app.inspect.as_ref(), &browser.page, text)
        .await
        .map_err(|error| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, error.to_string()))?;
    let failed = result
        .get("failed")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if failed.is_empty() && result.get("ok") == Some(&Value::Bool(false)) {
        // No list but not ok: report it as one failure rather than claiming success.
        return Ok(vec![json!({ "index": 0, "reason": "patch did not apply" })]);
    }
    Ok(failed)
}
