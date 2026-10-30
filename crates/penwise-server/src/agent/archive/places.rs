//! Where a round's frames stand on the board: the live frame behind each touched frame, and the variant groups
//! beside it.

use std::collections::BTreeSet;

use penwise_core::item::{FrameItem, ItemId, ItemKind, Scene};

use super::super::scene::frame_item;

fn same_place(a: &FrameItem, b: &FrameItem) -> bool {
    a.project == b.project && a.route == b.route && a.preset == b.preset
}

/// The live frames behind `touched`: a plain frame is itself; a variant stands for the plain frame of its
/// project, route and preset (the rightmost one).
pub(super) fn live_frames(scene: &Scene, touched: &BTreeSet<ItemId>) -> Vec<ItemId> {
    let mut live = BTreeSet::new();
    for id in touched {
        let Some(frame) = frame_item(scene, *id) else {
            continue;
        };
        if frame.group.is_none() {
            live.insert(*id);
        } else if let Some(base) = base_frame(scene, frame) {
            live.insert(base);
        }
    }
    live.into_iter().collect()
}

fn base_frame(scene: &Scene, like: &FrameItem) -> Option<ItemId> {
    scene
        .live_ids()
        .into_iter()
        .filter_map(|id| {
            let item = scene.get(id)?;
            match &item.kind {
                ItemKind::Frame(frame) if frame.group.is_none() && same_place(frame, like) => {
                    Some((item.transform.e, id))
                }
                _ => None,
            }
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

/// The variant groups sitting beside any of the `live` frames (same project, route and preset).
pub(crate) fn groups_beside(scene: &Scene, live: &[ItemId]) -> BTreeSet<String> {
    let bases: Vec<&FrameItem> = live
        .iter()
        .filter_map(|id| frame_item(scene, *id))
        .collect();
    scene
        .live_ids()
        .into_iter()
        .filter_map(|id| frame_item(scene, id))
        .filter(|frame| bases.iter().any(|base| same_place(frame, base)))
        .filter_map(|frame| frame.group.clone())
        .collect()
}

/// Items that must make room when `frame` moves right: every live item whose box starts at or beyond the
/// frame's right edge and overlaps the frame's vertical band (its lane), except those in `except` (frames about
/// to be deleted). Marks, erase masks and strokes bound to a frame are left alone: they follow their targets.
pub(crate) fn lane_items(scene: &Scene, frame: ItemId, except: &BTreeSet<ItemId>) -> Vec<ItemId> {
    let Some(item) = scene.get(frame) else {
        return Vec::new();
    };
    let ItemKind::Frame(live) = &item.kind else {
        return Vec::new();
    };
    let [width, height] = live.box_size(None);
    let (left, top) = (item.transform.e, item.transform.f);
    let right = left + width - EDGE_SLACK;
    scene
        .live_ids()
        .into_iter()
        .filter(|id| *id != frame && !except.contains(id))
        .filter(|id| {
            let Some(other) = scene.get(*id) else {
                return false;
            };
            let moves_alone = match &other.kind {
                ItemKind::Mark(_) | ItemKind::Erase(_) => false,
                ItemKind::Stroke(stroke) => stroke.binding.is_none(),
                _ => true,
            };
            moves_alone
                && scene.bounds(*id).is_some_and(|bounds| {
                    bounds.min_x >= right && bounds.min_y < top + height && bounds.max_y > top
                })
        })
        .collect()
}

/// A little tolerance so an item that touches the frame's right edge counts as being right of it.
const EDGE_SLACK: f32 = 0.5;
