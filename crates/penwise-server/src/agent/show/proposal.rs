//! `show {ghost}` and `show {variants}`: the proposals before any file edit.

use std::collections::BTreeSet;

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState, Author};

use super::super::archive::groups_beside;
use super::super::ghost::{GhostBody, ghost as set_ghost};
use super::super::variants::{VariantsBody, clear_plan, create};
use super::{frame_id, page_of_frame};

/// `ghost {frame, patch | clear}`.
#[derive(Debug, Deserialize)]
pub(crate) struct GhostArgs {
    frame: String,
    #[serde(default)]
    patch: Option<Value>,
    #[serde(default)]
    clear: bool,
}

/// `variants {frame, options:[{label, patch}] | clear}`.
#[derive(Debug, Deserialize)]
pub(crate) struct VariantsArgs {
    frame: String,
    #[serde(default)]
    options: Option<Vec<Value>>,
    #[serde(default)]
    clear: bool,
}

/// Sets or clears the ghost; `{ok, failed}` lists the patch ops the headless browser could not apply.
pub(super) async fn ghost(app: &AppState, args: GhostArgs) -> Result<Value, ApiError> {
    let patch = match (args.patch, args.clear) {
        (Some(patch), false) if !patch.is_null() => patch,
        (None, true) => Value::Null,
        _ => return Err(ApiError::bad_request("ghost takes a patch or clear")),
    };
    let body: GhostBody = serde_json::from_value(json!({ "patch": patch }))
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let Json(reply) = set_ghost(State(app.clone()), Path(args.frame), Json(body)).await?;
    Ok(reply)
}

/// Creates 2-4 variants beside the frame (the owner's view fits the whole board), or removes them.
pub(super) async fn variants(app: &AppState, args: VariantsArgs) -> Result<Value, ApiError> {
    let frame = frame_id(&args.frame)?;
    match (args.options, args.clear) {
        (Some(options), false) => {
            let body: VariantsBody = serde_json::from_value(json!({ "variants": options }))
                .map_err(|error| ApiError::bad_request(error.to_string()))?;
            let Json(mut reply) = create(State(app.clone()), Path(args.frame), Json(body)).await?;
            let page = page_of_frame(app, frame).await?;
            (app.announce_focus)(page, None);
            reply["page"] = json!(page.to_string());
            Ok(reply)
        }
        (None, true) => clear(app, frame).await,
        _ => Err(ApiError::bad_request(
            "variants takes options (2 to 4) or clear",
        )),
    }
}

/// Removes every variant group beside `frame`; their marks stay as `done`.
async fn clear(app: &AppState, frame: penwise_core::item::ItemId) -> Result<Value, ApiError> {
    let page = page_of_frame(app, frame).await?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown frame"))?;
    let scene = view.replica.scene();
    let mut author = Author::new(&app.storage);
    let (mut removed, mut marks) = (0, BTreeSet::new());
    for group in groups_beside(scene, &[frame]) {
        if let Some(plan) = clear_plan(scene, &group, &BTreeSet::new())? {
            removed += plan.frames;
            marks.extend(plan.marks);
            for edit in plan.edits {
                author.item(page, edit).await?;
            }
        }
    }
    author.commit().await?;
    Ok(json!({ "ok": true, "removed": removed, "marks": marks.len() }))
}
