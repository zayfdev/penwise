//! The capture pipeline: open the frame's page, measure, cut strips, remember the still, write back what the
//! server learned (document height, infinite-feed fallback).

use std::collections::BTreeMap;
use std::sync::Arc;

use penwise_core::item::{
    FrameExtent, FrameItem, ItemId, MAX_FRAME_SIDE, set_doc_height, set_extent,
};
use penwise_proto::PageId;

use crate::agent::{FrameView, frame_item, frame_keys, frame_position};
use crate::api::Author;

use super::hitmap;
use super::lanes;
use super::model::{INFINITE_NOTE, Still, StripMeta, hash_of};
use super::preroll::MAX_STILL_DOC;
use super::strips;
use super::{Inner, StillError};

/// A change of document height of at most this many CSS px is not worth an op.
const DOC_HEIGHT_SLACK: i64 = 1;

/// The frame as the scene holds it now.
pub(super) struct Loaded {
    pub(super) page: PageId,
    pub(super) frame: FrameItem,
    pub(super) position: [f32; 2],
    pub(super) keys: Vec<String>,
}

/// Reads frame `id` from storage.
///
/// # Errors
/// [`StillError::UnknownFrame`] when it is gone or not a frame.
pub(super) async fn load(inner: &Inner, id: ItemId) -> Result<Loaded, StillError> {
    let page = inner
        .storage
        .page_of_item(id)
        .await?
        .ok_or(StillError::UnknownFrame)?;
    let view = inner
        .storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or(StillError::UnknownFrame)?;
    let scene = view.replica.scene();
    let frame = frame_item(scene, id)
        .cloned()
        .ok_or(StillError::UnknownFrame)?;
    Ok(Loaded {
        page,
        frame,
        position: frame_position(scene, id),
        keys: frame_keys(scene, id),
    })
}

/// Makes and stores a new still of `id`. The caller holds the frame's gate (see [`super::Stills`]).
///
/// # Errors
/// The frame is unknown, or its page could not be opened, measured or shot.
pub(super) async fn run(inner: &Inner, id: ItemId) -> Result<Arc<Still>, StillError> {
    let loaded = load(inner, id).await?;
    let view = FrameView::open(
        &inner.inspect,
        &inner.projects,
        id,
        &loaded.frame,
        &loaded.keys,
        "",
    )
    .await?;
    let frame = &loaded.frame;
    let doc_h = view.doc[1];
    let infinite = doc_h > MAX_STILL_DOC;
    // A viewport-extent frame shows the first screen only; so does an infinite feed (about to become one).
    let span = if infinite || frame.extent == FrameExtent::Viewport {
        frame.css_h
    } else {
        ceil_px(doc_h).max(frame.css_h)
    };
    // Contract order: everything is measured on the pre-rolled page first, the hit map before the strips.
    let hits = match hitmap::collect(&inner.inspect, &view.page).await {
        Ok(hits) => hits,
        Err(error) => {
            tracing::warn!(frame = %id.to_hex(), %error, "no hit map for the still");
            Arc::default()
        }
    };
    let shot = strips::capture(&inner.inspect, &view.page, frame.css_w, span).await?;
    let rects = document_rects(&view, &loaded.keys);
    let doc = [ceil_px(view.doc[0]), ceil_px(doc_h.min(MAX_STILL_DOC))];
    drop(view);
    let mut metas = Vec::with_capacity(shot.len());
    for strip in shot {
        let hash = hash_of(&strip.png);
        inner.keep_blob(&hash, strip.png).await;
        metas.push(StripMeta {
            y: strip.y,
            h: strip.h,
            hash,
        });
    }
    let candidate = Still {
        version: 0,
        project: frame.project.clone(),
        doc,
        viewport: [frame.css_w, frame.css_h],
        strips: metas,
        rects,
        state: String::new(),
        note: infinite.then_some(INFINITE_NOTE),
        hits,
    };
    let still = store(inner, id, candidate);
    if let Err(error) = write_back(
        inner,
        id,
        &loaded,
        (!infinite).then(|| ceil_px(doc_h)),
        infinite,
    )
    .await
    {
        tracing::warn!(frame = %id.to_hex(), %error, "still taken, but its measurements were not saved");
    }
    Ok(still)
}

/// Gives `candidate` its version (the previous one when nothing changed) and caches it.
fn store(inner: &Inner, id: ItemId, mut candidate: Still) -> Arc<Still> {
    let mut cache = inner.cache();
    if let Some(previous) = cache.get(id)
        && previous.same_content(&candidate)
    {
        return previous;
    }
    candidate.version = cache.next_version(id, inner.storage.now_ms());
    let still = Arc::new(candidate);
    cache.put(id, Arc::clone(&still));
    still
}

/// Watched key → rect in document coordinates (the view is at scroll `view.scroll`, normally 0).
fn document_rects(view: &FrameView, keys: &[String]) -> BTreeMap<String, Option<[f32; 4]>> {
    keys.iter()
        .map(|key| {
            let rect = view.node(key).map(|node| {
                [
                    node.rect[0] + view.scroll[0],
                    node.rect[1] + view.scroll[1],
                    node.rect[2],
                    node.rect[3],
                ]
            });
            (key.clone(), rect)
        })
        .collect()
}

/// `FrameDocHeight` when the document height moved by more than a pixel (clamped so the box stays inside the
/// frame side limit), and the switch to `viewport` for an infinite feed. Server-device ops, like every other
/// server write.
async fn write_back(
    inner: &Inner,
    id: ItemId,
    loaded: &Loaded,
    doc_h: Option<u32>,
    infinite: bool,
) -> Result<(), StillError> {
    let view = inner
        .storage
        .page_view(loaded.page)
        .await?
        .ok_or(StillError::UnknownFrame)?;
    let scene = view.replica.scene();
    let Some(frame) = frame_item(scene, id) else {
        return Ok(());
    };
    let mut author = Author::new(&inner.storage);
    if let Some(measured) = doc_h {
        let room = MAX_FRAME_SIDE.saturating_sub(ceil_px(loaded.position[1].max(0.0)));
        let height = measured.min(room);
        if (i64::from(height) - i64::from(frame.doc_h)).abs() > DOC_HEIGHT_SLACK {
            let edits = set_doc_height(scene, id, height)
                .map_err(|error| StillError::Write(error.to_string()))?;
            for edit in edits {
                author.item(loaded.page, edit).await.map_err(api_error)?;
            }
            // The box grew into the lane below: move the lanes below down, in the same batch.
            for edit in lanes::restack(scene, id, height) {
                author.item(loaded.page, edit).await.map_err(api_error)?;
            }
        }
    }
    let mut flipped = false;
    if infinite {
        let edits = set_extent(scene, id, FrameExtent::Viewport);
        flipped = !edits.is_empty();
        for edit in edits {
            author.item(loaded.page, edit).await.map_err(api_error)?;
        }
    }
    // Remembered before the commit (the op can reach the trigger task first), forgotten if it never lands.
    if flipped {
        inner.flips().insert(id);
    }
    if let Err(error) = author.commit().await {
        if flipped {
            inner.flips().remove(&id);
        }
        return Err(api_error(error));
    }
    Ok(())
}

fn api_error(error: crate::api::ApiError) -> StillError {
    StillError::Write(error.message)
}

/// `value` rounded up to whole CSS px (never negative).
fn ceil_px(value: f32) -> u32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=2²⁰ first, exact in f32"
    )]
    {
        value.clamp(0.0, 1_048_576.0).ceil() as u32
    }
}
