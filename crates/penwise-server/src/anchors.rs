//! The element map: which selectors a captured page had, and where they sat.
//!
//! A picture taken from a browser carries the page it came from. Storing the element rectangles alongside lets an
//! annotation be resolved to what it marks ("inside `.hero__cta`") instead of only to pixels, and lets a later
//! capture re-resolve selectors after the layout moved.
//!
//! The map is server-local metadata per page ([`store`]). Which marks belong to which element is not stored: it is
//! computed from the page's items on every `/annotations` read, keyed by item id, so a mark that is erased, moved
//! or promoted is attributed correctly the next time and nothing can go stale.

mod store;

use std::collections::BTreeMap;

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use penwise_core::item::{ItemId, ItemKind, Layer, LayerSet, Scene};
use penwise_proto::ItemRef;
use serde::{Deserialize, Serialize};

use crate::api::{ApiError, AppState, parse_page};
use crate::sidecar::with_connection;

/// One element of a captured page, in CSS pixels of the captured viewport.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    /// CSS selector unique enough to find the element again.
    pub selector: String,
    /// `[x, y, width, height]` in CSS px, relative to the page's top-left.
    pub rect: [f32; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Trimmed text content, for the agent's benefit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Where the capture came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Anchor {
    /// Rejects anchors that cannot be resolved later: no selector, or a rect that is not four finite numbers.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        !self.selector.is_empty()
            && self.selector.len() <= MAX_SELECTOR
            && self.rect.iter().all(|value| value.is_finite())
    }
}

/// Longest accepted selector, in bytes.
const MAX_SELECTOR: usize = 1024;
/// Most anchors accepted in one request.
const MAX_ANCHORS: usize = 5_000;

/// Marks on a page grouped by the element they sit in.
#[derive(Debug, Default, PartialEq, Eq)]
struct Attribution {
    /// Selector → ids of the marks inside it (the innermost element wins).
    by_selector: BTreeMap<String, Vec<ItemId>>,
    /// Marks outside every element.
    unanchored: usize,
}

/// Attributes every live mark (stroke, shape or fill on the ink or agent layer) to the smallest anchor
/// containing the middle of its bounds, so an agent learns *what* a mark is about rather than only where it is.
fn attribute(anchors: &[Anchor], scene: &Scene) -> Attribution {
    let mut out = Attribution::default();
    for id in scene.render_order(LayerSet::NONE.with(Layer::Ink).with(Layer::Agent)) {
        let is_mark = scene.get(id).is_some_and(|item| {
            matches!(
                item.kind,
                ItemKind::Stroke(_) | ItemKind::Shape(_) | ItemKind::Fill(_)
            )
        });
        let Some(bounds) = scene.bounds(id).filter(|_| is_mark) else {
            continue;
        };
        let (x, y) = (
            bounds.min_x.midpoint(bounds.max_x),
            bounds.min_y.midpoint(bounds.max_y),
        );
        match containing(anchors, x, y) {
            Some(anchor) => out
                .by_selector
                .entry(anchor.selector.clone())
                .or_default()
                .push(id),
            None => out.unanchored += 1,
        }
    }
    out
}

/// The smallest anchor whose rectangle contains `(x, y)`; the innermost elements win, which is what a mark on
/// a button inside a card means.
fn containing(anchors: &[Anchor], x: f32, y: f32) -> Option<&Anchor> {
    anchors
        .iter()
        .filter(|anchor| {
            let [left, top, width, height] = anchor.rect;
            x >= left && x <= left + width && y >= top && y <= top + height
        })
        .min_by(|a, b| {
            let area = |anchor: &Anchor| anchor.rect[2] * anchor.rect[3];
            area(a).total_cmp(&area(b))
        })
}

#[derive(Debug, Deserialize)]
struct AnchorRequest {
    #[serde(default)]
    anchors: Vec<Anchor>,
}

/// `GET /pages/<id>/anchors` — the element map of a captured page.
async fn anchor_list(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
) -> Result<Response, ApiError> {
    let page = parse_page(&app, &page_id).await?;
    let anchors = with_connection(&app.data_dir, move |conn| store::read(conn, page)).await?;
    Ok(
        axum::Json(serde_json::json!({ "page": page.to_string(), "anchors": anchors }))
            .into_response(),
    )
}

/// `POST /pages/<id>/anchors` — replaces the element map: `{"anchors":[{selector, rect, role?, text?, url?}]}`.
async fn anchor_put(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
    axum::Json(request): axum::Json<AnchorRequest>,
) -> Result<Response, ApiError> {
    let page = parse_page(&app, &page_id).await?;
    if app
        .storage
        .page_view(page)
        .await?
        .is_none_or(|view| view.page.deleted)
    {
        return Err(ApiError::not_found(format!("unknown page {page_id}")));
    }
    if request.anchors.len() > MAX_ANCHORS {
        return Err(ApiError::bad_request(format!(
            "more than {MAX_ANCHORS} anchors"
        )));
    }
    let anchors: Vec<Anchor> = request
        .anchors
        .into_iter()
        .filter(Anchor::is_well_formed)
        .collect();
    let stored = anchors.len();
    with_connection(&app.data_dir, move |conn| {
        store::replace(conn, page, &anchors)
    })
    .await?;
    Ok(axum::Json(serde_json::json!({ "stored": stored })).into_response())
}

/// `GET /pages/<id>/annotations` — which marks belong to which element of the captured page, by item id.
async fn annotation_list(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
) -> Result<Response, ApiError> {
    let page = parse_page(&app, &page_id).await?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    let anchors = with_connection(&app.data_dir, move |conn| store::read(conn, page)).await?;
    let attribution = attribute(&anchors, view.replica.scene());
    let elements: Vec<serde_json::Value> = anchors
        .iter()
        .map(|anchor| {
            let items = attribution
                .by_selector
                .get(&anchor.selector)
                .map_or(&[][..], Vec::as_slice);
            serde_json::json!({
                "selector": anchor.selector,
                "rect": anchor.rect,
                "role": anchor.role,
                "text": anchor.text,
                "strokes": items.len(),
                "items": items.iter().map(|id| ItemRef::from(*id).to_string()).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(axum::Json(serde_json::json!({
        "page": page.to_string(),
        "elements": elements,
        "unanchored": attribution.unanchored,
    }))
    .into_response())
}

/// The element-map routes.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/pages/{id}/anchors", get(anchor_list).post(anchor_put))
        .route("/pages/{id}/annotations", get(annotation_list))
}

#[cfg(test)]
mod tests;
