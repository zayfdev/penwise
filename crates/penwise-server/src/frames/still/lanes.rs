//! Lanes (spec v2 §2.1): every screen (project, route, preset) is one row of the board, and rows stack
//! downward with [`LANE_GAP`] between them. A frame's box is as tall as its document, so when a document grows
//! the lanes below must make room: [`restack`] returns the edits that move them down.
//!
//! A lane is the frames of one screen (its live frame and variants) plus the history pictures that sit at the
//! lane's top edge. Only lanes strictly below the grown one move, and only as far as needed to leave a gap again;
//! everything else on the board (sketches, notes, other columns) stays where the owner put it.

use std::collections::BTreeMap;

use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::item::{Affine, FramePreset, ItemId, ItemKind, Scene};

/// Space between a lane's lowest box and the next lane, in page px (the same as `frame_open` places with).
pub(crate) const LANE_GAP: f32 = 200.0;
/// Tops closer than this are the same row.
const SAME_ROW: f32 = 0.5;

/// One row of the board.
struct Lane {
    top: f32,
    bottom: f32,
    members: Vec<(ItemId, Affine)>,
}

/// The edits that move every lane below `frame`'s lane down so that none overlaps it, given that the frame's
/// document is now `doc_h` px tall. Empty when nothing overlaps (or `frame` is not a frame).
pub(crate) fn restack(scene: &Scene, frame: ItemId, doc_h: u32) -> Vec<Edit> {
    let mut lanes: BTreeMap<(String, String, FramePreset), Lane> = BTreeMap::new();
    let mut home = None;
    for id in scene.live_ids() {
        let Some(item) = scene.get(id) else { continue };
        let ItemKind::Frame(frame_item) = &item.kind else {
            continue;
        };
        let mut sized = frame_item.clone();
        if id == frame {
            sized.doc_h = doc_h;
        }
        let top = item.transform.f;
        let key = (
            frame_item.project.clone(),
            frame_item.route.clone(),
            frame_item.preset,
        );
        if id == frame {
            home = Some(key.clone());
        }
        let lane = lanes.entry(key).or_insert(Lane {
            top,
            bottom: top,
            members: Vec::new(),
        });
        lane.top = lane.top.min(top);
        lane.bottom = lane.bottom.max(top + sized.box_size(None)[1]);
        lane.members.push((id, item.transform));
    }
    let Some(home) = home else {
        return Vec::new();
    };
    attach_pictures(scene, &mut lanes);
    let mut ordered: Vec<(bool, Lane)> = lanes
        .into_iter()
        .map(|(key, lane)| (key == home, lane))
        .collect();
    ordered.sort_by(|a, b| a.1.top.total_cmp(&b.1.top));
    let Some(start) = ordered.iter().position(|(is_home, _)| *is_home) else {
        return Vec::new();
    };
    let home_top = ordered[start].1.top;
    let mut floor = ordered[start].1.bottom;
    let mut edits = Vec::new();
    for (_, lane) in ordered.iter_mut().skip(start + 1) {
        if lane.top <= home_top + SAME_ROW {
            continue;
        }
        let mut shift = 0.0;
        if lane.top < floor {
            shift = floor + LANE_GAP - lane.top;
            for (id, transform) in &lane.members {
                let mut moved = *transform;
                moved.f += shift;
                edits.push(Edit::Update {
                    id: *id,
                    patch: ItemPatch {
                        transform: Some(moved),
                        ..ItemPatch::default()
                    },
                });
            }
        }
        floor = floor.max(lane.bottom + shift);
    }
    edits
}

/// History pictures sit where the live frame stood: at their lane's top edge. They move with the lane and count
/// towards its height.
fn attach_pictures(scene: &Scene, lanes: &mut BTreeMap<(String, String, FramePreset), Lane>) {
    for id in scene.live_ids() {
        let Some(item) = scene.get(id) else { continue };
        let ItemKind::Image(image) = &item.kind else {
            continue;
        };
        let top = item.transform.f;
        if let Some(lane) = lanes
            .values_mut()
            .find(|lane| (lane.top - top).abs() <= SAME_ROW)
        {
            lane.bottom = lane.bottom.max(top + image.h);
            lane.members.push((id, item.transform));
        }
    }
}
