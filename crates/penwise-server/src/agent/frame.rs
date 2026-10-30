//! One frame opened in the shared headless browser: the route at the preset size, the mark's state recipe
//! replayed, and the binding keys resolved to nodes and viewport rects (design-harness §7.2, C2).
//!
//! The browser is the server's one Chromium ([`Inspect`]); at most [`MAX_OPEN_PAGES`] pages are open at once, and
//! a [`FrameView`] closes its page when it drops. The page is pre-rolled ([`crate::frames::still::preroll`]) so
//! lazy and reveal-on-scroll content is in place, then everything is measured at scroll 0, so a document
//! coordinate is a viewport coordinate and the screenshot crop arithmetic stays exact. To look at a point below
//! the first screen, [`FrameView::element_at_doc`] scrolls it into view, asks, and scrolls back.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock};

use penwise_core::item::{FrameItem, ItemId};
use penwise_core::marks::{FrameLayoutUpdate, FrameLayouts, FrameRect};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::frames::registry::Registry;
use crate::inspect::{ElementInfo, Error as InspectError, Inspect, Page};

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
    /// The document size in CSS px, after the pre-roll.
    pub(crate) doc: [f32; 2],
    /// The viewport size in CSS px (the preset).
    pub(crate) viewport: [f32; 2],
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
        let mut page = inspect.open_capture(&url, item.css_w, item.css_h).await?;
        // Source mapping turns served URLs into `file:line` under the project's folder.
        page.set_root(Some(
            project
                .static_dir()
                .unwrap_or(project.root.as_path())
                .to_path_buf(),
        ));
        // Contract §4 order. Lazy images, reveal-on-scroll sections and infinite feeds only exist once scrolled
        // to, so the document is rolled through right after load, before a state replay or a patch can move
        // anything; then animations are finished so the page holds still while it is measured.
        crate::frames::still::preroll(inspect, &page, item.css_h).await?;
        crate::frames::still::freeze_animations(inspect, &page).await?;
        let reached = prepare(inspect, &page, frame, item, state).await?;
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
            doc,
            viewport,
            _slot: slot,
        })
    }

    /// The resolved node of `key` (or `None` when it is gone in this state).
    pub(crate) fn node(&self, key: &str) -> Option<&BridgeNode> {
        self.nodes.get(key)?.as_ref()
    }

    /// Scrolls document point `point` into the viewport and returns the page's scroll offset then. The page is
    /// left there: callers that go on measuring at the origin call [`FrameView::scroll_home`].
    ///
    /// # Errors
    /// The page could not be evaluated.
    pub(crate) async fn reveal(
        &self,
        inspect: &Inspect,
        point: [f32; 2],
    ) -> Result<[f32; 2], AgentError> {
        let script = format!(
            "(function (y, h) {{
               var root = document.documentElement;
               var reach = Math.max(0, Math.max(root.scrollHeight, document.body ? document.body.scrollHeight : 0) - window.innerHeight);
               var top = Math.min(reach, Math.max(0, y - h / 2));
               window.scrollTo({{ top: top, left: 0, behavior: 'instant' }});
               var scroll = function () {{ return [window.pageXOffset || 0, window.pageYOffset || 0]; }};
               // A hit test needs the scroll committed to a frame; the timer covers a page that draws none.
               return new Promise(function (resolve) {{
                 var done = function () {{ resolve(scroll()); }};
                 setTimeout(done, 150);
                 requestAnimationFrame(function () {{ requestAnimationFrame(done); }});
               }});
             }})({}, {})",
            point[1], self.viewport[1]
        );
        let value = inspect.eval(&self.page, &script).await?;
        let pair = |index: usize| {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "scroll offsets are page-sized"
            )]
            {
                value[index].as_f64().unwrap_or(0.0) as f32
            }
        };
        Ok([pair(0), pair(1)])
    }

    /// Puts the page back at scroll 0, where the rest of the view's measurements were taken.
    pub(crate) async fn scroll_home(&self, inspect: &Inspect) {
        let _ = inspect
            .eval(
                &self.page,
                "window.scrollTo({ top: 0, left: 0, behavior: 'instant' })",
            )
            .await;
    }

    /// The element at **document** point `point`: the page is scrolled so the point is in view, asked, and put
    /// back at scroll 0.
    ///
    /// `DOM.getNodeForLocation` takes document coordinates (not viewport ones, measured against Chromium here:
    /// at scroll 2600 the viewport point finds nothing and the document point finds the element) and only finds
    /// points inside the visible part of the page, hence the scroll.
    ///
    /// # Errors
    /// No element is there, or the page could not be asked.
    pub(crate) async fn element_at_doc(
        &self,
        inspect: &Inspect,
        point: [f32; 2],
    ) -> Result<ElementInfo, InspectError> {
        if let Err(error) = self.reveal(inspect, point).await {
            return Err(InspectError::Page(error.to_string()));
        }
        let found = inspect
            .element_at(&self.page, f64::from(point[0]), f64::from(point[1]))
            .await;
        self.scroll_home(inspect).await;
        found
    }
}

/// The state recipe and the frame's patch, on the pre-rolled page; the page ends at scroll 0 and frozen.
///
/// A replay click may have scrolled or focused something: measure at the origin again, because a screenshot clip
/// is document coordinates and the ink is placed in viewport coordinates. The frame's ghost/variant patch is part
/// of what the owner sees, so the bundle and the snapshot read the patched DOM: it goes on after the replay,
/// before the keys resolve.
async fn prepare(
    inspect: &Inspect,
    page: &Page,
    frame: ItemId,
    item: &FrameItem,
    state: &str,
) -> Result<String, AgentError> {
    let reached = if state.is_empty() {
        String::new()
    } else {
        replay(inspect, page, state).await?
    };
    if let Some(text) = item.patch.as_deref() {
        match apply_patch(inspect, page, text).await {
            Ok(answer) if answer.get("ok") == Some(&Value::Bool(true)) => {}
            Ok(answer) => {
                let failed = answer.get("failed").cloned().unwrap_or(Value::Null);
                tracing::warn!(frame = %frame.to_hex(), failed = %failed, "frame patch partly applied");
            }
            Err(error) => tracing::warn!(frame = %frame.to_hex(), %error, "frame patch failed"),
        }
    }
    crate::frames::still::freeze_animations(inspect, page).await?;
    inspect.eval(page, "window.scrollTo(0, 0)").await?;
    Ok(reached)
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

/// Applies P1 patch text through the page's bridge and returns its `{ok, failed}` answer.
///
/// # Errors
/// The page could not be evaluated (a closed target, a navigation in flight).
pub(crate) async fn apply_patch(
    inspect: &Inspect,
    page: &Page,
    text: &str,
) -> Result<Value, AgentError> {
    let literal = serde_json::to_string(text)
        .map_err(|error| AgentError::Internal(format!("patch: {error}")))?;
    let script = format!(
        "(function (patch) {{
           var bridge = window.PenwiseBridge;
           if (!bridge || typeof bridge.applyPatch !== 'function') {{
             return {{ ok: false, failed: [{{ index: 0, reason: 'no bridge' }}] }};
           }}
           return bridge.applyPatch(patch, true);
         }})({literal})"
    );
    Ok(inspect.eval(page, &script).await?)
}

/// The `[selector, text]` of the element at a document point, for an arrow's endpoints.
pub(crate) async fn element_at_point(
    inspect: &Inspect,
    view: &FrameView,
    point: [f32; 2],
) -> Option<Value> {
    let info = view.element_at_doc(inspect, point).await.ok()?;
    Some(json!({ "selector": info.selector, "text": info.text }))
}
