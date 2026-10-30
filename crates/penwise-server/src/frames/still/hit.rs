//! The browser fallback of `GET /frames/<id>/hit?x&y`, for a frame that has no still (and so no hit map).

use penwise_core::item::ItemId;
use serde_json::{Value, json};

use crate::agent::FrameView;

use super::capture::load;
use super::{Inner, StillError};

/// Opens the frame like a still does (route, patch, pre-roll), scrolls document point `point` into view and
/// asks the bridge what is under it. The answer is the bridge's `pw:hit` payload with `rect` in **document**
/// coordinates: `{key, selector, rect}`, all `null` when nothing is there.
pub(super) async fn at(inner: &Inner, frame: ItemId, point: [f32; 2]) -> Result<Value, StillError> {
    let loaded = load(inner, frame).await?;
    let view = FrameView::open(
        &inner.inspect,
        &inner.projects,
        frame,
        &loaded.frame,
        &[],
        "",
    )
    .await?;
    let scroll = view.reveal(&inner.inspect, point).await?;
    let script = format!(
        "(function (x, y) {{
           var bridge = window.PenwiseBridge;
           return bridge && bridge.hitAt ? bridge.hitAt(x, y) : null;
         }})({}, {})",
        point[0] - scroll[0],
        point[1] - scroll[1]
    );
    let found = inner.inspect.eval(&view.page, &script).await;
    view.scroll_home(&inner.inspect).await;
    let found = found?;
    Ok(document_rect(&found, scroll))
}

/// The bridge's answer with its viewport rect moved by the page's scroll into document coordinates.
fn document_rect(found: &Value, scroll: [f32; 2]) -> Value {
    let rect = found["rect"].as_array().and_then(|values| {
        let at = |index: usize| values.get(index).and_then(Value::as_f64);
        Some([
            at(0)? + f64::from(scroll[0]),
            at(1)? + f64::from(scroll[1]),
            at(2)?,
            at(3)?,
        ])
    });
    json!({
        "key": found["key"],
        "selector": found["selector"],
        "rect": rect,
    })
}
