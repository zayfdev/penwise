//! The handwriting of one mark for the bundle: its ink rendered on white and read by the local model. The call is
//! bounded in time, so an agent asking "what did I mark?" never waits on a slow CPU model.

use std::time::Duration;

use penwise_core::item::{ItemId, ItemKind, Scene};
use penwise_core::marks::MarkEntry;

use crate::api::AppState;

use super::entry::PageFacts;

/// How long handwriting recognition may take before the bundle reports `timeout` and no text.
const OCR_DEADLINE: Duration = Duration::from_secs(20);
/// Padding around a mark's ink when it is rendered for recognition, in page px.
const OCR_PADDING: f32 = 24.0;

/// The mark's handwriting: its ink on white through the local model, or `None` with a reason.
pub(crate) async fn read_text(
    app: &AppState,
    facts: Option<&PageFacts<'_>>,
    entry: &MarkEntry,
) -> (Option<String>, &'static str) {
    if app.ocr_url.trim().is_empty() {
        return (None, "off");
    }
    let Some(facts) = facts else {
        return (None, "unavailable");
    };
    let Some(png) = ink_png(facts.ink, &entry.strokes).await else {
        return (None, "ok");
    };
    match crate::ocr::transcribe_deadline(&app.ocr_url, png, OCR_DEADLINE).await {
        Ok(text) if text.trim().is_empty() => (None, "ok"),
        Ok(text) => (Some(text), "ok"),
        Err(crate::ocr::Unavailable::Timeout) => (None, "timeout"),
        Err(crate::ocr::Unavailable::Failed(reason)) => {
            tracing::warn!(%reason, "handwriting model unavailable");
            (None, "unavailable")
        }
    }
}

/// The mark's ink rendered on white, at most `OCR_MAX_SIDE` pixels per side.
async fn ink_png(ink: &Scene, strokes: &[ItemId]) -> Option<Vec<u8>> {
    let scene = ink.clone();
    let strokes = strokes.to_vec();
    tokio::task::spawn_blocking(move || {
        let mut bounds = penwise_core::Bounds::EMPTY;
        for id in &strokes {
            if let Some(item) = scene.bounds(*id) {
                bounds = bounds.union(&item);
            }
        }
        if bounds.is_empty() {
            return None;
        }
        let region = bounds.inflate(OCR_PADDING);
        let longest = region.width().max(region.height());
        let mut scale = 1.0;
        if longest > crate::render::OCR_MAX_SIDE {
            scale = (crate::render::OCR_MAX_SIDE - 1.0) / longest;
        }
        let mut paper = penwise_core::raster::scene_pixmap(&region, scale).ok()?;
        paper.fill(tiny_skia::Color::WHITE);
        let only = mark_scene(&scene, &strokes)?;
        penwise_core::raster::render_scene_onto(
            &mut paper,
            &only,
            penwise_core::item::LayerSet::INK,
            &region,
            scale,
            &(),
        )
        .ok()?;
        penwise_core::raster::png::encode_png(&paper).ok()
    })
    .await
    .ok()
    .flatten()
}

/// The presentation scene restricted to `strokes`.
fn mark_scene(ink: &Scene, strokes: &[ItemId]) -> Option<Scene> {
    let kept: std::collections::BTreeSet<ItemId> = strokes.iter().copied().collect();
    let picked = ink
        .live_ids()
        .into_iter()
        .filter_map(|id| ink.get(id))
        .filter(|item| match &item.kind {
            ItemKind::Stroke(_) => kept.contains(&item.id),
            ItemKind::Erase(erase) => kept.contains(&erase.target),
            _ => false,
        });
    Scene::from_items(ink.kind(), picked.map(|item| (item.clone(), false))).ok()
}
