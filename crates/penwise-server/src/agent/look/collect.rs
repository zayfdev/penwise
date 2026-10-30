//! The bundles behind `look`: the existing per-mark bundle builder, run for the marks of a round.

use std::collections::BTreeMap;

use penwise_core::item::ItemId;
use penwise_core::marks::MarkEntry;
use serde_json::Value;

use crate::api::AppState;
use crate::storage::PageView;

use super::super::entry::{bundle, page_bundles};
use super::super::frame::FrameView;
use super::super::scene::{frame_item, frame_keys, frame_position};

/// One bundle per entry of `view`'s page: one headless page per (frame, state), then one bundle per mark. A
/// frame whose page cannot be opened still gets bundles, without element or crops.
pub(super) async fn bundles_of(
    app: &AppState,
    view: &PageView,
    entries: Vec<MarkEntry>,
) -> Vec<Value> {
    let scene = view.replica.scene();
    let mut groups: BTreeMap<(ItemId, String), Vec<MarkEntry>> = BTreeMap::new();
    for entry in entries {
        groups
            .entry((entry.frame, entry.state.clone()))
            .or_default()
            .push(entry);
    }
    let mut out = Vec::new();
    for ((frame_id, state), entries) in groups {
        let Some(frame) = frame_item(scene, frame_id).cloned() else {
            continue;
        };
        let position = frame_position(scene, frame_id);
        let keys = frame_keys(scene, frame_id);
        let opened = FrameView::open(
            app.inspect.as_ref(),
            &app.projects,
            frame_id,
            &frame,
            &keys,
            &state,
        )
        .await;
        match opened {
            Ok(browser) => out.extend(
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
            ),
            Err(error) => {
                tracing::warn!(frame = %frame_id.to_hex(), %error, "no frame page; bundle without an element");
                for entry in &entries {
                    out.push(bundle(app, scene, entry, &frame, None).await);
                }
            }
        }
    }
    out
}
