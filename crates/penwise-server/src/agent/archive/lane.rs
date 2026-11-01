//! The columns of a freeze: how far each moved frame goes and what makes room for it, left to right in its lane.
//! A lane's generation is one row of columns: the OG picture, then one per variant; the live frame moves right
//! by the whole row.

use std::collections::{BTreeMap, BTreeSet};

use penwise_core::item::{ItemId, ItemKind, Scene};

use super::places::lane_items;
use super::{COLUMN_GAP, Picture};

/// How far each moved frame goes: its generation's columns, plus the columns of the frames to its left in the lane.
/// Returns, per frame in left-to-right order, the shift already applied to it before it moves itself, and the
/// frame's own distance.
fn lane_plan(
    scene: &Scene,
    pictures: &[&Picture],
    deleted: &BTreeSet<ItemId>,
) -> Vec<(ItemId, f32, f32, Vec<ItemId>)> {
    let mut ordered: Vec<&&Picture> = pictures.iter().filter(|picture| picture.is_og()).collect();
    ordered.sort_by(|a, b| {
        let x = |picture: &Picture| {
            scene
                .get(picture.frame)
                .map_or(0.0, |item| item.transform.e)
        };
        x(a).total_cmp(&x(b))
    });
    let mut carried: BTreeMap<ItemId, f32> = BTreeMap::new();
    let mut out = Vec::new();
    for picture in ordered {
        let Some(ItemKind::Frame(live)) = scene.get(picture.frame).map(|item| &item.kind) else {
            continue;
        };
        let variants = pictures
            .iter()
            .filter(|other| other.live == picture.frame && !other.is_og())
            .count();
        #[allow(clippy::cast_precision_loss, reason = "at most a handful of variants")]
        let dx = (1 + variants) as f32 * (live.page_size()[0] + COLUMN_GAP);
        let before = carried.get(&picture.frame).copied().unwrap_or(0.0);
        let right = lane_items(scene, picture.frame, deleted);
        for id in &right {
            *carried.entry(*id).or_default() += dx;
        }
        *carried.entry(picture.frame).or_default() += dx;
        out.push((picture.frame, before, dx, right));
    }
    out
}

/// Where each history picture goes: the live frame's old position, moved by the columns of frames to its left,
/// and by its own slot in the row.
pub(super) fn place_pictures<'a>(
    scene: &Scene,
    pictures: &[&'a Picture],
    deleted: &BTreeSet<ItemId>,
) -> Vec<(&'a Picture, [f32; 2])> {
    let before: BTreeMap<ItemId, f32> = lane_plan(scene, pictures, deleted)
        .into_iter()
        .map(|(frame, before, _, _)| (frame, before))
        .collect();
    pictures
        .iter()
        .map(|picture| {
            let shift = before.get(&picture.live).copied().unwrap_or(0.0);
            #[allow(clippy::cast_precision_loss, reason = "at most a handful of variants")]
            let column = picture.slot as f32 * column_of(scene, picture.live);
            (
                *picture,
                [picture.position[0] + shift + column, picture.position[1]],
            )
        })
        .collect()
}

/// One column of `frame`'s lane: its width and the gap.
fn column_of(scene: &Scene, frame: ItemId) -> f32 {
    match scene.get(frame).map(|item| &item.kind) {
        Some(ItemKind::Frame(live)) => live.page_size()[0] + COLUMN_GAP,
        _ => 0.0,
    }
}

/// Every item that moves right, and by how much: the frames that leave their pictures behind, and everything
/// in their lanes to the right.
pub(super) fn shifts(
    scene: &Scene,
    pictures: &[&Picture],
    deleted: &BTreeSet<ItemId>,
) -> BTreeMap<ItemId, f32> {
    let mut total: BTreeMap<ItemId, f32> = BTreeMap::new();
    for (frame, _, dx, right) in lane_plan(scene, pictures, deleted) {
        *total.entry(frame).or_default() += dx;
        for id in right {
            *total.entry(id).or_default() += dx;
        }
    }
    total
}
