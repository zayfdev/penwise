//! One frame opened in the shared headless browser: the route at the preset size, the mark's state recipe
//! replayed, and the binding keys resolved to nodes and viewport rects (design-harness §7.2, C2).
//!
//! The browser is the server's one Chromium ([`Inspect`]); at most [`MAX_OPEN_PAGES`] pages are open at once, and
//! a [`FrameView`] closes its page when it drops. Everything is measured at scroll 0, so a document coordinate is
//! a viewport coordinate and the screenshot crop arithmetic stays exact.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock};

use penwise_core::item::{FrameItem, ItemId};
use penwise_core::marks::{FrameLayoutUpdate, FrameLayouts, FrameRect};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::frames::registry::Registry;
use crate::inspect::{Inspect, Page};

use super::AgentError;

/// Most pages open at once across the server (the browser itself has no limit).
const MAX_OPEN_PAGES: usize = 2;
/// How long the page gets to settle after a replay click before the layout is read.
const REPLAY_SETTLE_MS: u64 = 60;

static SLOTS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(MAX_OPEN_PAGES)));

/// A binding key resolved in the page.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BridgeNode {
    /// Unique CSS selector of the element (`#id > tag:nth-of-type(n)`).
    pub(crate) selector: String,
    /// Viewport rect `[x, y, w, h]`.
    pub(crate) rect: [f32; 4],
}

/// A frame's live page: the browser target, its layout for core, and the resolved keys.
pub(crate) struct FrameView {
    pub(crate) page: Page,
    pub(crate) layouts: FrameLayouts,
    /// Key → node, or `None` when the element is gone in this state.
    pub(crate) nodes: BTreeMap<String, Option<BridgeNode>>,
    pub(crate) scroll: [f32; 2],
    /// Holds one of [`MAX_OPEN_PAGES`] slots for the page's lifetime.
    _slot: OwnedSemaphorePermit,
}

impl FrameView {
    /// Opens `item`'s route at its preset in `state`, resolves `keys` and builds the frame's layout for core.
    ///
    /// # Errors
    /// The project is not linked, Chromium failed, or the page could not be evaluated.
    pub(crate) async fn open(
        inspect: &Inspect,
        projects: &Registry,
        frame: ItemId,
        item: &FrameItem,
        keys: &[String],
        state: &str,
    ) -> Result<Self, AgentError> {
        let slot = Arc::clone(&SLOTS)
            .acquire_owned()
            .await
            .map_err(|_| AgentError::Internal("browser slots closed".to_owned()))?;
        let project = projects
            .get(&item.project)
            .await
            .map_err(|error| AgentError::Internal(error.message))?
            .ok_or_else(|| AgentError::UnknownProject(item.project.clone()))?;
        let url = format!("{}{}", projects.url(&project), item.route);
        let mut page = inspect.open(&url, item.css_w, item.css_h).await?;
        // Source mapping turns served URLs into `file:line` under the project's folder.
        page.set_root(Some(
            project
                .static_dir()
                .unwrap_or(project.root.as_path())
                .to_path_buf(),
        ));
        // Measurements are made with the document at its origin: a clip is then a viewport rect.
        inspect.eval(&page, "window.scrollTo(0, 0)").await?;
        let reached = if state.is_empty() {
            String::new()
        } else {
            replay(inspect, &page, state).await?
        };
        // A replay click may have scrolled or focused something: measure at the origin again, because a
        // screenshot clip is document coordinates and the ink is placed in viewport coordinates.
        inspect.eval(&page, "window.scrollTo(0, 0)").await?;
        let nodes = resolve(inspect, &page, keys).await?;
        let (scroll, doc, viewport) = geometry(inspect, &page).await?;
        let mut layouts = FrameLayouts::new();
        layouts.merge(
            frame,
            item.preset,
            &FrameLayoutUpdate {
                preset: Some(item.preset),
                scroll: Some(scroll),
                doc: Some(doc),
                viewport: Some(viewport),
                state: Some(reached.clone()),
                rects: keys
                    .iter()
                    .map(|key| {
                        let rect = nodes
                            .get(key)
                            .and_then(|node| node.as_ref())
                            .map(|node| FrameRect::from_array(node.rect));
                        (key.clone(), rect)
                    })
                    .collect(),
            },
        );
        Ok(Self {
            page,
            layouts,
            nodes,
            scroll,
            _slot: slot,
        })
    }

    /// The resolved node of `key` (or `None` when it is gone in this state).
    pub(crate) fn node(&self, key: &str) -> Option<&BridgeNode> {
        self.nodes.get(key)?.as_ref()
    }
}

/// Replays `state` through the bridge and returns the state the page reached.
async fn replay(inspect: &Inspect, page: &Page, state: &str) -> Result<String, AgentError> {
    let recipe = serde_json::to_string(state)
        .map_err(|error| AgentError::Internal(format!("state recipe: {error}")))?;
    let script = format!(
        "new Promise(function (resolve) {{
           try {{
             if (window.PenwiseBridge && window.PenwiseBridge.receive) {{
               window.PenwiseBridge.receive({{t: \"pw:replay\", state: {recipe}}});
             }}
           }} catch (error) {{ /* the page has no bridge */ }}
           setTimeout(function () {{
             resolve(window.PenwiseBridge && window.PenwiseBridge.stateOf
               ? window.PenwiseBridge.stateOf() : \"\");
           }}, {REPLAY_SETTLE_MS});
         }})"
    );
    let value = inspect.eval(page, &script).await?;
    Ok(value.as_str().unwrap_or_default().to_owned())
}

/// Resolves every key through the bridge (the same resolver the host uses, so a mark and its element agree).
async fn resolve(
    inspect: &Inspect,
    page: &Page,
    keys: &[String],
) -> Result<BTreeMap<String, Option<BridgeNode>>, AgentError> {
    if keys.is_empty() {
        return Ok(BTreeMap::new());
    }
    let list = serde_json::to_string(keys)
        .map_err(|error| AgentError::Internal(format!("keys: {error}")))?;
    let script = format!(
        "(function (keys) {{
           var bridge = window.PenwiseBridge;
           var out = {{}};
           for (var i = 0; i < keys.length; i += 1) {{
             var element = bridge && bridge.resolveKey ? bridge.resolveKey(keys[i]) : null;
             if (!element) {{ out[keys[i]] = null; continue; }}
             var rect = element.getBoundingClientRect();
             out[keys[i]] = {{
               selector: bridge.selectorOf(element),
               rect: [rect.left, rect.top, rect.width, rect.height]
             }};
           }}
           return out;
         }})({list})"
    );
    let value = inspect.eval(page, &script).await?;
    serde_json::from_value(value)
        .map_err(|error| AgentError::Internal(format!("resolved keys: {error}")))
}

/// Scroll offset, document size and viewport size, in CSS px.
#[allow(
    clippy::cast_possible_truncation,
    reason = "viewport metrics are small integers and exact in f32"
)]
async fn geometry(
    inspect: &Inspect,
    page: &Page,
) -> Result<([f32; 2], [f32; 2], [f32; 2]), AgentError> {
    let script = "(function () {
        var root = document.documentElement;
        var body = document.body;
        return {
          scroll: [window.pageXOffset || 0, window.pageYOffset || 0],
          doc: [
            Math.max(root ? root.scrollWidth : 0, root ? root.clientWidth : 0, body ? body.scrollWidth : 0),
            Math.max(root ? root.scrollHeight : 0, root ? root.clientHeight : 0, body ? body.scrollHeight : 0)
          ],
          viewport: [window.innerWidth, window.innerHeight]
        };
      })()";
    let value = inspect.eval(page, script).await?;
    let pair = |name: &str| -> Result<[f32; 2], AgentError> {
        let values = value[name]
            .as_array()
            .ok_or_else(|| AgentError::Internal(format!("page geometry has no {name}")))?;
        let first = values.first().and_then(Value::as_f64).unwrap_or(0.0);
        let second = values.get(1).and_then(Value::as_f64).unwrap_or(0.0);
        Ok([first as f32, second as f32])
    };
    Ok((pair("scroll")?, pair("doc")?, pair("viewport")?))
}

/// The `[selector, text]` of the element at a viewport point, for an arrow's endpoints.
pub(crate) async fn element_at_point(
    inspect: &Inspect,
    page: &Page,
    point: [f32; 2],
) -> Option<Value> {
    let info = inspect
        .element_at(page, f64::from(point[0]), f64::from(point[1]))
        .await
        .ok()?;
    Some(json!({ "selector": info.selector, "text": info.text }))
}
