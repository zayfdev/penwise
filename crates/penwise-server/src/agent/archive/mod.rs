//! Freezing a round into history (spec v2 §4): for every frame the round touched, the frame as it looks with
//! the round's ink becomes a picture where the live frame stood, the live frame moves one column right (taking
//! everything to its right in its lane with it), its ghost clears, its variants go, and the round's marks
//! become `done`.
//!
//! It runs in two phases. First every snapshot of every page is taken; a failure there changes nothing, so the
//! call can simply be retried. Then each page commits one batch (one undo step). A frame that already has its
//! `Round n — …` picture is skipped, so a retry after a partial commit never freezes a frame twice. The
//! snapshot is [`super::frames::frame_snapshot`] without ink plus only the round's strokes ([`ink`]), and the
//! variants go through [`super::variants::clear_plan`], which spares variants carrying marks newer than the
//! round.

mod ink;
mod lane;
mod places;
mod store;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::item::{
    Affine, ImageItem, Item, ItemId, ItemKind, Layer, Scene, Z_STEP, set_patch,
};
use penwise_core::marks::{MarkStatus, set_mark_status};
use penwise_proto::PageId;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState, Author};
use crate::assets;
use crate::rounds::RoundMark;

use super::variants::clear_plan;

use lane::{place_pictures, shifts};
use places::live_frames;
use store::picture_of;

pub(super) use places::groups_beside;

/// Gap between a frozen picture and the live frame that moved off it, in page px.
const COLUMN_GAP: f32 = 40.0;
/// Largest side the asset store accepts; a taller snapshot is stored scaled down and shown at full size.
const STORED_SIDE: u32 = 9_000;
/// Snapshots larger than this many bytes are refused.
const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;

/// What a freeze changed.
#[derive(Debug, Default)]
pub(crate) struct Frozen {
    pub(crate) snapshots: Vec<Value>,
    pub(crate) marks_done: usize,
    pub(crate) variants_removed: usize,
    /// Frames that already had this round's picture (a retry) and were left where they are.
    pub(crate) already: usize,
}

/// A frozen frame, ready to be added to the page.
struct Picture {
    frame: ItemId,
    id: ItemId,
    asset: String,
    position: [f32; 2],
    size: [f32; 2],
    label: String,
}

/// One page's share of a freeze, after its snapshots are taken.
struct PagePlan {
    page: PageId,
    /// `(mark, frame)` of the round's marks on this page.
    marks: Vec<(ItemId, ItemId)>,
    pictures: Vec<Picture>,
    already: BTreeSet<ItemId>,
}

/// The label of the history picture of a frame in round `n`.
fn label_of(n: u32, preset: &str, route: &str) -> String {
    format!("Round {n} \u{2014} {preset} {route}")
}

/// Freezes round `n`: `marks` are the round's marks (any pages) and `newer` the project's marks drawn after the
/// round was sent, which neither the picture nor the variants clear may touch.
///
/// # Errors
/// 503 when a frame cannot be snapshotted (nothing is changed), 5xx on storage failures.
pub(crate) async fn freeze_round(
    app: &AppState,
    n: u32,
    marks: &[&RoundMark],
    newer: &BTreeSet<ItemId>,
) -> Result<Frozen, ApiError> {
    let mut pages: BTreeMap<PageId, Vec<&RoundMark>> = BTreeMap::new();
    for mark in marks {
        pages.entry(mark.page).or_default().push(mark);
    }
    let mut plans = Vec::new();
    for (page, marks) in pages {
        plans.push(prepare_page(app, n, page, &marks).await?);
    }
    let mut frozen = Frozen::default();
    for plan in plans {
        commit_page(app, plan, newer, &mut frozen).await?;
    }
    Ok(frozen)
}

/// Takes the snapshots of one page's live frames (skipping the frames already frozen for round `n`).
async fn prepare_page(
    app: &AppState,
    n: u32,
    page: PageId,
    marks: &[&RoundMark],
) -> Result<PagePlan, ApiError> {
    let view = live_view(app, page).await?;
    let scene = view.replica.scene();
    let touched: BTreeSet<ItemId> = marks.iter().map(|mark| mark.entry.frame).collect();
    let labels = crate::pictures::labels_of_page(app, page).await?;
    let mut plan = PagePlan {
        page,
        marks: marks
            .iter()
            .map(|mark| (mark.entry.id, mark.entry.frame))
            .collect(),
        pictures: Vec::new(),
        already: BTreeSet::new(),
    };
    for frame in live_frames(scene, &touched) {
        let Some(live) = super::scene::frame_item(scene, frame) else {
            continue;
        };
        let label = label_of(n, live.preset.as_str(), &live.route);
        let frozen_before = labels
            .iter()
            .any(|(id, text)| *text == label && scene.get(*id).is_some());
        if frozen_before {
            plan.already.insert(frame);
            continue;
        }
        let strokes: BTreeSet<ItemId> = marks
            .iter()
            .filter(|mark| mark.entry.frame == frame)
            .flat_map(|mark| mark.entry.strokes.iter().copied())
            .collect();
        plan.pictures
            .push(picture_of(app, scene, frame, label, &strokes).await?);
    }
    Ok(plan)
}

async fn live_view(app: &AppState, page: PageId) -> Result<crate::storage::PageView, ApiError> {
    app.storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown page"))
}

/// Commits one page: pictures, moves (with the lane), ghost and variants cleared, marks `done`. The scene is
/// read again, so edits made while the snapshots were taken are respected.
async fn commit_page(
    app: &AppState,
    plan: PagePlan,
    newer: &BTreeSet<ItemId>,
    frozen: &mut Frozen,
) -> Result<(), ApiError> {
    let page = plan.page;
    let view = live_view(app, page).await?;
    let scene = view.replica.scene();
    let touched: BTreeSet<ItemId> = plan.marks.iter().map(|(_, frame)| *frame).collect();
    let live = live_frames(scene, &touched);
    let mut clears = Vec::new();
    let mut deleted: BTreeSet<ItemId> = BTreeSet::new();
    let mut done: BTreeSet<ItemId> = BTreeSet::new();
    for group in groups_beside(scene, &live) {
        if let Some(cleared) = clear_plan(scene, &group, newer)? {
            frozen.variants_removed += cleared.frames;
            deleted.extend(cleared.deleted.iter().copied());
            done.extend(cleared.marks.iter().copied());
            clears.push(cleared.edits);
        }
    }
    let mut author = Author::new(&app.storage);
    let pictures: Vec<&Picture> = plan
        .pictures
        .iter()
        .filter(|picture| scene.get(picture.frame).is_some())
        .collect();
    let mut z = lowest_z(scene);
    for (picture, position) in place_pictures(scene, &pictures, &deleted) {
        author
            .item(page, Edit::Add(picture_item(picture, position, z)))
            .await?;
        z = z.saturating_sub(Z_STEP);
        for edit in set_patch(scene, picture.frame, None)
            .map_err(|error| ApiError::bad_request(error.to_string()))?
        {
            author.item(page, edit).await?;
        }
    }
    for (id, dx) in shifts(scene, &pictures, &deleted) {
        if let Some(item) = scene.get(id) {
            let mut transform = item.transform;
            transform.e += dx;
            author.item(page, update_transform(id, transform)).await?;
        }
    }
    for edit in clears.into_iter().flatten() {
        author.item(page, edit).await?;
    }
    for (mark, _) in &plan.marks {
        if done.insert(*mark) {
            for edit in set_mark_status(scene, *mark, MarkStatus::Done)
                .map_err(|error| ApiError::bad_request(error.to_string()))?
            {
                author.item(page, edit).await?;
            }
        }
    }
    author.commit().await?;
    frozen.marks_done += plan.marks.len();
    frozen.already += plan.already.len();
    for picture in &plan.pictures {
        if scene.get(picture.frame).is_none() {
            continue;
        }
        frozen.snapshots.push(json!({
            "frame": picture.frame.to_hex(),
            "image": picture.id.to_hex(),
            "asset": picture.asset,
            "w": picture.size[0],
            "h": picture.size[1],
            "label": picture.label,
        }));
        crate::pictures::store_label(app, picture.id, picture.label.clone()).await;
    }
    Ok(())
}

fn update_transform(id: ItemId, transform: Affine) -> Edit {
    Edit::Update {
        id,
        patch: ItemPatch {
            transform: Some(transform),
            ..ItemPatch::default()
        },
    }
}

fn picture_item(picture: &Picture, position: [f32; 2], z: i64) -> Item {
    Item {
        id: picture.id,
        layer: Layer::Ink,
        z,
        transform: Affine::translate(position[0], position[1]),
        kind: ItemKind::Image(ImageItem {
            asset: assets::digest_of(&picture.asset).unwrap_or([0; 32]),
            w: picture.size[0],
            h: picture.size[1],
        }),
    }
}

/// The z that puts a picture beneath everything on the page.
fn lowest_z(scene: &Scene) -> i64 {
    scene
        .live_ids()
        .into_iter()
        .filter_map(|id| scene.get(id))
        .map(|item| item.z)
        .min()
        .map_or(0, |lowest| lowest.saturating_sub(Z_STEP))
}
