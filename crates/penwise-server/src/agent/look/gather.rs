//! What `look` reads before it writes: the pages behind the marks, their bundles, the board's frames and the
//! owner's variant picks.

use std::collections::BTreeMap;

use penwise_core::item::{ItemId, ItemKind, Scene};
use penwise_proto::PageId;
use serde_json::Value;

use crate::api::{ApiError, AppState};
use crate::rounds::RoundMark;
use crate::storage::PageView;

use super::collect::bundles_of;
use super::text::{FrameLine, Pick};

/// The pages the answer reads, by id.
pub(super) type Views = BTreeMap<PageId, PageView>;

/// The page views the answer reads: the marks' pages and the board.
pub(super) async fn load_views(
    app: &AppState,
    board: Option<PageId>,
    shown: &[&RoundMark],
) -> Result<Views, ApiError> {
    let mut views = Views::new();
    for page in shown.iter().map(|mark| mark.page).chain(board) {
        if views.contains_key(&page) {
            continue;
        }
        if let Some(view) = app
            .storage
            .page_view(page)
            .await?
            .filter(|v| !v.page.deleted)
        {
            views.insert(page, view);
        }
    }
    Ok(views)
}

/// Mark id → bundle, one batch of headless pages per page.
pub(super) async fn bundles_by_mark(
    app: &AppState,
    views: &Views,
    shown: &[&RoundMark],
) -> BTreeMap<ItemId, Value> {
    let mut out = BTreeMap::new();
    for (page, view) in views {
        let entries: Vec<_> = shown
            .iter()
            .filter(|mark| mark.page == *page)
            .map(|mark| mark.entry.clone())
            .collect();
        if entries.is_empty() {
            continue;
        }
        for bundle in bundles_of(app, view, entries).await {
            if let Some(id) = bundle["id"].as_str().and_then(ItemId::from_hex) {
                out.insert(id, bundle);
            }
        }
    }
    out
}

/// The board's plain frames and the frames of the shown marks, in board order, with their `f<n>` index.
pub(super) fn frame_lines(
    views: &Views,
    board: Option<PageId>,
    shown: &[&RoundMark],
) -> (Vec<FrameLine>, BTreeMap<ItemId, usize>) {
    let mut wanted: Vec<(PageId, ItemId)> = Vec::new();
    for (page, view) in views {
        let scene = view.replica.scene();
        let on_board = board == Some(*page);
        for id in scene.render_order(penwise_core::item::LayerSet::ALL) {
            let is_plain = matches!(scene.get(id).map(|item| &item.kind),
                Some(ItemKind::Frame(frame)) if frame.group.is_none());
            let marked = shown.iter().any(|mark| mark.entry.frame == id);
            if (on_board && is_plain) || marked {
                wanted.push((*page, id));
            }
        }
    }
    let mut lines = Vec::new();
    let mut tags = BTreeMap::new();
    for (page, id) in wanted {
        let Some(view) = views.get(&page) else {
            continue;
        };
        let scene = view.replica.scene();
        let Some(ItemKind::Frame(frame)) = scene.get(id).map(|item| &item.kind) else {
            continue;
        };
        if tags.contains_key(&id) {
            continue;
        }
        tags.insert(id, lines.len());
        lines.push(FrameLine {
            id: id.to_hex(),
            preset: frame.preset.as_str(),
            route: frame.route.clone(),
            label: frame.label.clone(),
            css: [frame.css_w, frame.css_h],
            box_size: frame.box_size(None),
        });
    }
    (lines, tags)
}

/// The variant groups of the board with the owner's pick.
pub(super) fn picks(scene: &Scene) -> Vec<Pick> {
    let mut groups: BTreeMap<String, Vec<(String, String, bool)>> = BTreeMap::new();
    for id in scene.render_order(penwise_core::item::LayerSet::ALL) {
        if let Some(ItemKind::Frame(frame)) = scene.get(id).map(|item| &item.kind)
            && let Some(group) = &frame.group
        {
            groups.entry(group.clone()).or_default().push((
                frame.label.clone(),
                id.to_hex(),
                frame.chosen,
            ));
        }
    }
    groups
        .into_iter()
        .map(|(group, variants)| Pick { group, variants })
        .collect()
}

/// `[x, y, w, h]` of a bundle's page rect, when the mark is visible.
#[allow(
    clippy::cast_possible_truncation,
    reason = "page rects are small pixel counts, exact in f32"
)]
pub(super) fn rect_of(bundle: &Value) -> Option<[f32; 4]> {
    let values = bundle.get("rect")?.as_array()?;
    let at = |index: usize| values.get(index)?.as_f64().map(|value| value as f32);
    Some([at(0)?, at(1)?, at(2)?, at(3)?])
}
