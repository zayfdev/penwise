//! A lane's generation: the OG picture and one picture per unprotected variant, and the rows that keep them
//! (locked-frames contract §2).

use std::collections::{BTreeMap, BTreeSet};

use penwise_core::item::{ItemId, Scene};
use serde_json::{Value, json};

use crate::api::{ApiError, AppState};
use crate::rounds::RoundMark;
use crate::storage::{Generation, GenerationRole};

use super::super::scene::frame_item;
use super::super::variants::clearable_frames;
use super::Picture;
use super::places::groups_beside;
use super::store::picture_of;

/// The label of a variant's history picture: its lane's OG label and the variant's own.
fn variant_label_of(og: &str, variant: &str) -> String {
    format!("{og} \u{2014} {variant}")
}

/// The variant frames a freeze pictures for `live`, by label (then id): every variant of the groups beside it
/// that a clear would remove, so a variant spared for a mark drawn after Send is neither pictured nor deleted.
fn variants_of(scene: &Scene, live: ItemId, newer: &BTreeSet<ItemId>) -> Vec<(String, ItemId)> {
    let mut out: Vec<(String, ItemId)> = groups_beside(scene, &[live])
        .iter()
        .flat_map(|group| clearable_frames(scene, group, newer))
        .filter_map(|id| Some((frame_item(scene, id)?.label.clone(), id)))
        .collect();
    out.sort();
    out
}

/// The round's strokes on `frame`.
fn strokes_on(marks: &[&RoundMark], frame: ItemId) -> BTreeSet<ItemId> {
    marks
        .iter()
        .filter(|mark| mark.entry.frame == frame)
        .flat_map(|mark| mark.entry.strokes.iter().copied())
        .collect()
}

/// The pictures of one lane: the live frame `live` first, then its variants, each with its own ink. A picture
/// whose label is already on the board (`labels`, a retry) is not made again.
pub(super) async fn lane_pictures(
    app: &AppState,
    scene: &Scene,
    marks: &[&RoundMark],
    live: ItemId,
    label: &str,
    labels: &BTreeMap<ItemId, String>,
    newer: &BTreeSet<ItemId>,
) -> Result<Vec<Picture>, ApiError> {
    let here = |text: &str| {
        labels
            .iter()
            .any(|(id, known)| known == text && scene.get(*id).is_some())
    };
    let origin = picture_of(app, scene, live, label.to_owned(), &strokes_on(marks, live)).await?;
    let position = origin.position;
    let mut pictures = vec![origin];
    for (slot, (name, variant)) in variants_of(scene, live, newer).into_iter().enumerate() {
        let text = variant_label_of(label, &name);
        if here(&text) {
            continue;
        }
        let mut picture =
            picture_of(app, scene, variant, text, &strokes_on(marks, variant)).await?;
        picture.live = live;
        picture.slot = slot + 1;
        picture.variant = Some(name);
        // The row stands where the live frame stood; `lane` moves it by its slot and its lane's shift.
        picture.position = position;
        pictures.push(picture);
    }
    Ok(pictures)
}

/// The pictures whose frames are still on the page at commit time, with each lane's variant slots made
/// contiguous again (a variant deleted since its snapshot leaves no hole where the live frame will land).
pub(super) fn survivors(scene: &Scene, pictures: Vec<Picture>) -> Vec<Picture> {
    let mut kept: Vec<Picture> = pictures
        .into_iter()
        .filter(|picture| scene.get(picture.frame).is_some() && scene.get(picture.live).is_some())
        .collect();
    renumber(&mut kept);
    kept
}

/// Gives the variant pictures of each lane slots 1, 2, 3.. in their current order.
fn renumber(pictures: &mut [Picture]) {
    let lanes: BTreeSet<ItemId> = pictures.iter().map(|picture| picture.live).collect();
    for live in lanes {
        let mut variants: Vec<&mut Picture> = pictures
            .iter_mut()
            .filter(|picture| picture.live == live && !picture.is_og())
            .collect();
        variants.sort_by_key(|picture| picture.slot);
        for (index, picture) in variants.into_iter().enumerate() {
            picture.slot = index + 1;
        }
    }
}

/// The `generations` rows of `pictures`: `picked` is whether the variant's frame carries the owner's pick now.
pub(super) fn records(
    project: &str,
    round: u32,
    scene: &Scene,
    pictures: &[&Picture],
) -> Vec<Generation> {
    pictures
        .iter()
        .map(|picture| Generation {
            project: project.to_owned(),
            round,
            route: picture.route.clone(),
            preset: picture.preset.clone(),
            role: if picture.is_og() {
                GenerationRole::Og
            } else {
                GenerationRole::Variant
            },
            label: picture.variant.clone().unwrap_or_default(),
            picked: picked(scene, picture),
            item: picture.id.to_hex(),
            asset: picture.asset.clone(),
            w: pixels(picture.size[0]),
            h: pixels(picture.size[1]),
            resolved_ms: 0,
        })
        .collect()
}

/// Whether `picture` is a variant the owner has chosen.
fn picked(scene: &Scene, picture: &Picture) -> bool {
    picture.variant.is_some() && frame_item(scene, picture.frame).is_some_and(|frame| frame.chosen)
}

/// A size in page px as a whole number.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "frame boxes are positive and far below u32::MAX px"
)]
fn pixels(size: f32) -> u32 {
    size.max(0.0).round() as u32
}

/// One entry of a resolve's `snapshots`.
pub(super) fn snapshot_json(scene: &Scene, picture: &Picture) -> Value {
    json!({
        "frame": picture.frame.to_hex(),
        "image": picture.id.to_hex(),
        "asset": picture.asset,
        "w": picture.size[0],
        "h": picture.size[1],
        "label": picture.label,
        "role": if picture.is_og() { "og" } else { "variant" },
        "variant": picture.variant,
        "picked": picked(scene, picture),
    })
}
