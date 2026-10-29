//! Variant frames (design-harness M4, P4): `POST /frames/<id>/variants`, `GET /variants/<group>` and
//! `DELETE /variants/<group>`.
//!
//! The agent proposes 2–4 frames beside the base frame, each with its own P1 patch and label, in one edit batch
//! (one undo step). The owner then taps "Use this" on the tablet or the PC window, which is an ordinary
//! `FrameChosen` update from that device; `GET /variants/<group>` reads the scene, so the pick needs no route of
//! its own. Clearing keeps the ink: the marks on the variant frames become `done` before the frames go away.

use std::collections::{BTreeMap, BTreeSet};

use axum::Json;
use axum::extract::{Path, State};
use penwise_core::edit::Edit;
use penwise_core::item::{ItemId, VariantSpec, Z_STEP, group_frames, variant_frames};
use penwise_core::marks::{FrameLayouts, MarkEntry, MarkStatus, MarkView, set_mark_status};
use penwise_proto::{ItemRef, PageId};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState, Author};
use crate::frames::patch::Patch;
use crate::storage::PageView;
use crate::storage::random::random_u128;

use super::entry::page_bundles;
use super::frame::FrameView;
use super::scene::{frame_item, frame_keys, frame_position};

/// Fewest and most variants a proposal may carry (spec §7.1).
const MIN_VARIANTS: usize = 2;
const MAX_VARIANTS: usize = 4;

/// `POST /frames/<id>/variants {"variants": [{"label", "patch"}]}`.
#[derive(Debug, Deserialize)]
pub(crate) struct VariantsBody {
    variants: Vec<VariantRequest>,
}

#[derive(Debug, Deserialize)]
struct VariantRequest {
    label: String,
    patch: Value,
}

/// Creates the variant frames beside the base frame; `{"group", "frames": [ids]}`.
pub(crate) async fn create(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<VariantsBody>,
) -> Result<Json<Value>, ApiError> {
    let frame_id = frame_id(&id)?;
    if !(MIN_VARIANTS..=MAX_VARIANTS).contains(&body.variants.len()) {
        return Err(ApiError::bad_request("variants is 2 to 4"));
    }
    let page = page_of(&app, frame_id).await?;
    let view = live_view(&app, page).await?;
    let scene = view.replica.scene();
    if frame_item(scene, frame_id).is_none() {
        return Err(ApiError::not_found("unknown frame"));
    }
    let group = format!("vg_{:032x}", random_u128());
    let now = app.storage.now_ms();
    let specs = body
        .variants
        .into_iter()
        .map(|variant| variant_spec(variant, now))
        .collect::<Result<Vec<_>, _>>()?;
    let z = scene.next_z().unwrap_or(Z_STEP);
    let edits = variant_frames(scene, frame_id, &specs, &group, z)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let mut author = Author::new(&app.storage);
    for edit in edits {
        author.item(page, edit).await?;
    }
    author.commit().await?;
    let frames: Vec<String> = specs.iter().map(|spec| spec.id.to_hex()).collect();
    Ok(Json(json!({ "group": group, "frames": frames })))
}

fn variant_spec(request: VariantRequest, now: u64) -> Result<VariantSpec, ApiError> {
    let text = serde_json::to_string(&request.patch)
        .map_err(|error| ApiError::bad_request(format!("patch: {error}")))?;
    let patch =
        Patch::parse(&text).map_err(|error| ApiError::bad_request(format!("patch: {error}")))?;
    Ok(VariantSpec {
        id: ItemRef::from_parts(now, random_u128()).into(),
        label: request.label,
        patch: patch.to_text(),
    })
}

/// `GET /variants/<group>` — the frames, the pick, and the bundles of ink drawn on them.
pub(crate) async fn show(
    State(app): State<AppState>,
    Path(group): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let (_, view) = locate(&app, &group)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown variant group"))?;
    let scene = view.replica.scene();
    let ids = group_frames(scene, &group);
    if ids.is_empty() {
        return Err(ApiError::not_found("unknown variant group"));
    }
    let mut frames = Vec::with_capacity(ids.len());
    let mut chosen = Value::Null;
    for id in &ids {
        let Some(frame) = frame_item(scene, *id) else {
            continue;
        };
        if frame.chosen {
            chosen = json!(id.to_hex());
        }
        frames.push(json!({
            "id": id.to_hex(),
            "label": frame.label,
            "chosen": frame.chosen,
            "patch": patch_value(frame.patch.as_deref()),
        }));
    }
    let marks = variant_marks(&app, &view, &ids).await;
    Ok(Json(json!({
        "group": group,
        "frames": frames,
        "chosen": chosen,
        "marks": marks,
    })))
}

/// `DELETE /variants/<group>` — removes the variant frames, their marks left `done`.
pub(crate) async fn clear(
    State(app): State<AppState>,
    Path(group): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let (page, view) = locate(&app, &group)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown variant group"))?;
    let scene = view.replica.scene();
    let ids = group_frames(scene, &group);
    if ids.is_empty() {
        return Err(ApiError::not_found("unknown variant group"));
    }
    let wanted: BTreeSet<ItemId> = ids.iter().copied().collect();
    let marks: Vec<ItemId> = MarkView::new(scene, &FrameLayouts::new(), true)
        .mark_entries()
        .into_iter()
        .filter(|entry| wanted.contains(&entry.frame))
        .map(|entry| entry.id)
        .collect();
    let mut author = Author::new(&app.storage);
    for mark in &marks {
        for edit in set_mark_status(scene, *mark, MarkStatus::Done)
            .map_err(|error| ApiError::bad_request(error.to_string()))?
        {
            author.item(page, edit).await?;
        }
    }
    for frame in &ids {
        author.item(page, Edit::Delete(*frame)).await?;
    }
    author.commit().await?;
    Ok(Json(json!({
        "ok": true,
        "removed": ids.len(),
        "marks": marks.len(),
    })))
}

/// The page and view holding `group`; the first live page with a frame of it (groups are unique).
async fn locate(app: &AppState, group: &str) -> Result<Option<(PageId, PageView)>, ApiError> {
    if group.is_empty() {
        return Ok(None);
    }
    let catalog = app.storage.catalog().await?;
    for notebook in catalog.notebooks() {
        for page in catalog.pages(notebook.id) {
            if page.deleted {
                continue;
            }
            let Some(view) = app.storage.page_view(page.id).await? else {
                continue;
            };
            if !group_frames(view.replica.scene(), group).is_empty() {
                return Ok(Some((page.id, view)));
            }
        }
    }
    Ok(None)
}

/// One mark bundle per (variant frame, state), with crops — the same machinery `GET /marks` uses.
async fn variant_marks(app: &AppState, view: &PageView, ids: &[ItemId]) -> Vec<Value> {
    let scene = view.replica.scene();
    let wanted: BTreeSet<ItemId> = ids.iter().copied().collect();
    let mut groups: BTreeMap<(ItemId, String), Vec<MarkEntry>> = BTreeMap::new();
    for entry in MarkView::new(scene, &FrameLayouts::new(), true).mark_entries() {
        if wanted.contains(&entry.frame) {
            groups
                .entry((entry.frame, entry.state.clone()))
                .or_default()
                .push(entry);
        }
    }
    let mut out = Vec::new();
    for ((frame_id, state), entries) in groups {
        let Some(frame) = frame_item(scene, frame_id).cloned() else {
            continue;
        };
        let position = frame_position(scene, frame_id);
        let keys = frame_keys(scene, frame_id);
        match FrameView::open(
            app.inspect.as_ref(),
            &app.projects,
            frame_id,
            &frame,
            &keys,
            &state,
        )
        .await
        {
            Ok(browser) => {
                out.extend(
                    page_bundles(
                        app,
                        browser,
                        scene,
                        &frame,
                        position,
                        &entries,
                        view.updated_seq,
                    )
                    .await,
                );
            }
            Err(error) => {
                tracing::warn!(frame = %frame_id.to_hex(), %error, "no variant page; bundle without an element");
                for entry in &entries {
                    out.push(super::entry::bundle(app, scene, entry, &frame, None).await);
                }
            }
        }
    }
    out
}

/// A stored patch text as the JSON object it names (`null` when absent or unreadable).
fn patch_value(text: Option<&str>) -> Value {
    text.and_then(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or(Value::Null)
}

fn frame_id(text: &str) -> Result<ItemId, ApiError> {
    ItemId::from_hex(text).ok_or_else(|| ApiError::bad_request("frame id is 32 hex characters"))
}

async fn page_of(app: &AppState, frame: ItemId) -> Result<PageId, ApiError> {
    app.storage
        .page_of_item(frame)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown frame"))
}

async fn live_view(app: &AppState, page: PageId) -> Result<PageView, ApiError> {
    app.storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown frame"))
}
