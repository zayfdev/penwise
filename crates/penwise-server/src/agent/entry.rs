//! One mark's bundle entry (design-harness §7.2): the JSON an agent reads — the kind hint, the colour meaning,
//! the handwriting, the element with its source `file:line` and CSS rules, and the crop URLs.

use std::collections::BTreeMap;

use penwise_core::item::{FrameItem, ItemId, Scene};
use penwise_core::marks::{Binding, BindingMode, MarkEntry, MarkView};
use serde_json::{Map, Value, json};

use crate::api::AppState;

use super::frame::FrameView;
use super::handwriting::read_text;
use super::images::{self, BuiltImages, ImageKind, MarkCrop};
use super::kind;
use super::scene::{mark_binding, mark_strokes};

/// Bundles for one (frame, state): one presentation, one screenshot batch, then one bundle per mark.
pub(crate) async fn page_bundles(
    app: &AppState,
    browser: FrameView,
    scene: &Scene,
    frame: &FrameItem,
    position: [f32; 2],
    entries: &[MarkEntry],
    version: u64,
) -> Vec<Value> {
    let marks = MarkView::new(scene, &browser.layouts, true);
    let presentation = marks.presentation();
    let rects: BTreeMap<ItemId, Option<[f32; 4]>> = marks
        .mark_entries()
        .into_iter()
        .map(|entry| (entry.id, entry.rect))
        .collect();
    let size = frame.page_size();
    let frame_box = [position[0], position[1], size[0], size[1]];
    let mut images: BTreeMap<ItemId, BuiltImages> = BTreeMap::new();
    match crate::render::ink_only(&presentation.scene) {
        Ok(ink) => {
            let crops: Vec<MarkCrop> = entries
                .iter()
                .filter_map(|entry| {
                    let rect = rects.get(&entry.id).copied().flatten()?;
                    Some(MarkCrop {
                        mark: entry.id,
                        rect,
                        strokes: entry.strokes.clone(),
                    })
                })
                .collect();
            if !crops.is_empty() {
                match images::build(app.inspect.as_ref(), &browser, &ink, frame_box, &crops).await {
                    Ok(built) => {
                        for (mark, images) in &built {
                            app.images.put(*mark, version, images);
                        }
                        images = built;
                    }
                    Err(error) => tracing::warn!(%error, "mark crops unavailable"),
                }
            }
        }
        Err(error) => tracing::warn!(%error, "mark ink unavailable"),
    }
    let facts = PageFacts {
        browser: &browser,
        rects: &rects,
        images: &images,
        position,
        ink: &presentation.scene,
    };
    let mut out = Vec::new();
    for entry in entries {
        out.push(bundle(app, scene, entry, frame, Some(&facts)).await);
    }
    out
}

/// What an open frame page adds to a bundle.
pub(crate) struct PageFacts<'a> {
    pub(crate) browser: &'a FrameView,
    /// Mark → effective page rect (page coordinates).
    pub(crate) rects: &'a BTreeMap<ItemId, Option<[f32; 4]>>,
    pub(crate) images: &'a BTreeMap<ItemId, BuiltImages>,
    pub(crate) position: [f32; 2],
    pub(crate) ink: &'a Scene,
}

/// One mark's bundle.
#[allow(
    clippy::cast_possible_truncation,
    reason = "element rects from the browser are page-sized pixel counts, exact in f32"
)]
pub(crate) async fn bundle(
    app: &AppState,
    scene: &Scene,
    entry: &MarkEntry,
    frame: &FrameItem,
    facts: Option<&PageFacts<'_>>,
) -> Value {
    let mut out = Map::new();
    out.insert("id".to_owned(), json!(entry.id.to_hex()));
    out.insert("status".to_owned(), json!(entry.status.as_str()));
    out.insert("frame".to_owned(), json!(entry.frame.to_hex()));
    out.insert("project".to_owned(), json!(frame.project));
    out.insert("route".to_owned(), json!(frame.route));
    out.insert("preset".to_owned(), json!(entry.preset.as_str()));
    out.insert("state".to_owned(), json!(state_steps(&entry.state)));
    let binding = mark_binding(scene, entry);
    out.insert(
        "binding".to_owned(),
        json!(binding.map(|binding| binding.mode.as_str())),
    );
    out.insert(
        "rect".to_owned(),
        facts
            .and_then(|facts| facts.rects.get(&entry.id).copied().flatten())
            .map_or(Value::Null, |rect| json!(rect)),
    );
    let element = match facts {
        Some(facts) => resolve_element(app.inspect.as_ref(), facts, entry, binding).await,
        None => None,
    };
    let strokes = mark_strokes(scene, entry);
    let element_rect = element.as_ref().map(|info| {
        [
            info.rect[0] as f32,
            info.rect[1] as f32,
            info.rect[2] as f32,
            info.rect[3] as f32,
        ]
    });
    let shape = kind::shape(&strokes, element_rect);
    let (from, to) = match (shape.from, shape.to, facts) {
        (Some(from), Some(to), Some(facts)) => (
            point_element(app.inspect.as_ref(), facts, binding, from).await,
            point_element(app.inspect.as_ref(), facts, binding, to).await,
        ),
        _ => (None, None),
    };
    out.insert(
        "kind".to_owned(),
        json!({
            "guess": shape.guess,
            "confidence": shape.confidence,
            "from": from,
            "to": to,
        }),
    );
    let colour = strokes
        .first()
        .map_or("none", |stroke| kind::colour(stroke.params.color.to_argb()));
    out.insert("colour".to_owned(), json!(colour));
    out.insert(
        "element".to_owned(),
        element.map_or(Value::Null, |info| json!(info)),
    );
    let (text, status) = read_text(app, facts, entry).await;
    out.insert("text".to_owned(), text.map_or(Value::Null, Value::String));
    out.insert("text_status".to_owned(), json!(status));
    out.insert(
        "images".to_owned(),
        match facts.and_then(|facts| facts.images.get(&entry.id)) {
            Some(_) => json!({
                "with_ink": image_url(entry.id, ImageKind::WithInk),
                "without_ink": image_url(entry.id, ImageKind::WithoutInk),
                "context": image_url(entry.id, ImageKind::Context),
            }),
            None => Value::Null,
        },
    );
    Value::Object(out)
}

/// The URL of one crop (relative to the server root, which the agent client prefixes).
fn image_url(mark: ItemId, kind: ImageKind) -> String {
    format!("/marks/{}/image/{}.png", mark.to_hex(), kind.as_str())
}

/// The mark's element: the bound key's node, or the element under an area mark's centre.
async fn resolve_element(
    inspect: &crate::inspect::Inspect,
    facts: &PageFacts<'_>,
    entry: &MarkEntry,
    binding: Option<&Binding>,
) -> Option<crate::inspect::ElementInfo> {
    match binding?.mode {
        BindingMode::Bound => {
            let key = binding?.key.as_deref()?;
            let node = facts.browser.node(key)?;
            inspect
                .element(&facts.browser.page, &node.selector)
                .await
                .ok()
        }
        BindingMode::Area => {
            let rect = facts.rects.get(&entry.id).copied().flatten()?;
            let x = rect[0] - facts.position[0] + rect[2] / 2.0;
            let y = rect[1] - facts.position[1] + rect[3] / 2.0;
            inspect
                .element_at(&facts.browser.page, f64::from(x), f64::from(y))
                .await
                .ok()
        }
    }
}

/// The element at an arrow endpoint (binding-local coordinates).
async fn point_element(
    inspect: &crate::inspect::Inspect,
    facts: &PageFacts<'_>,
    binding: Option<&Binding>,
    local: [f32; 2],
) -> Option<Value> {
    let binding = binding?;
    let point = match binding.mode {
        BindingMode::Bound => {
            let node = facts.browser.node(binding.key.as_deref()?)?;
            [node.rect[0] + local[0], node.rect[1] + local[1]]
        }
        BindingMode::Area => [
            local[0] - facts.browser.scroll[0],
            local[1] - facts.browser.scroll[1],
        ],
    };
    super::frame::element_at_point(inspect, &facts.browser.page, point).await
}

/// A state recipe as its steps (`"click: a;click: b"` → `["click: a", "click: b"]`).
fn state_steps(state: &str) -> Vec<&str> {
    state
        .split(';')
        .map(str::trim)
        .filter(|step| !step.is_empty())
        .collect()
}
