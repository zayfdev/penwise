//! `POST /agent/show` (spec v2 §5): the one write tool of the agent. The body names the project and exactly one
//! of `frame`, `ghost`, `variants`, `sketch`, `image` or `focus`; each dispatches to the machinery the
//! individual routes already use, and the owner's camera follows the result ([`crate::api::FocusAnnouncer`]).

mod draw;
mod frame;
mod proposal;
#[cfg(test)]
mod tests;

use axum::Json;
use axum::extract::State;
use penwise_core::item::ItemId;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState, page_of};

use super::scene::frame_item;

pub(crate) use draw::{ImageArgs, SketchArgs};
pub(crate) use frame::FrameArgs;
pub(crate) use proposal::{GhostArgs, VariantsArgs};

/// `{"project", <one of the six>}`.
#[derive(Debug, Deserialize)]
pub(crate) struct ShowBody {
    project: String,
    #[serde(default)]
    frame: Option<FrameArgs>,
    #[serde(default)]
    ghost: Option<GhostArgs>,
    #[serde(default)]
    variants: Option<VariantsArgs>,
    #[serde(default)]
    sketch: Option<SketchArgs>,
    #[serde(default)]
    image: Option<ImageArgs>,
    #[serde(default)]
    focus: Option<FocusArgs>,
}

/// `focus {frame | page}`.
#[derive(Debug, Deserialize)]
pub(crate) struct FocusArgs {
    #[serde(default)]
    frame: Option<String>,
    #[serde(default)]
    page: Option<String>,
}

/// `POST /agent/show` — what was created (ids) and, for a ghost, the patch ops that failed.
pub(crate) async fn show(
    State(app): State<AppState>,
    Json(body): Json<ShowBody>,
) -> Result<Json<Value>, ApiError> {
    let project = app
        .projects
        .get(&body.project)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown project"))?;
    let given = [
        body.frame.is_some(),
        body.ghost.is_some(),
        body.variants.is_some(),
        body.sketch.is_some(),
        body.image.is_some(),
        body.focus.is_some(),
    ];
    if given.iter().filter(|given| **given).count() != 1 {
        return Err(ApiError::bad_request(
            "give exactly one of frame, ghost, variants, sketch, image, focus",
        ));
    }
    let reply = if let Some(args) = body.frame {
        frame::open(&app, &project, args).await?
    } else if let Some(args) = body.ghost {
        proposal::ghost(&app, args).await?
    } else if let Some(args) = body.variants {
        proposal::variants(&app, args).await?
    } else if let Some(args) = body.sketch {
        draw::sketch(&app, &project, args).await?
    } else if let Some(args) = body.image {
        draw::image(&app, &project, args).await?
    } else if let Some(args) = body.focus {
        focus(&app, args).await?
    } else {
        return Err(ApiError::bad_request("nothing to show"));
    };
    Ok(Json(reply))
}

/// `focus {frame | page}`: the owner's devices go there.
async fn focus(app: &AppState, args: FocusArgs) -> Result<Value, ApiError> {
    match (args.frame, args.page) {
        (Some(frame), None) => {
            let id = frame_id(&frame)?;
            let page = page_of_frame(app, id).await?;
            (app.announce_focus)(page, Some(id));
            Ok(json!({ "page": page.to_string(), "frame": id.to_hex() }))
        }
        (None, Some(page)) => {
            let page = page_of(app, Some(&page)).await?;
            (app.announce_focus)(page, None);
            Ok(json!({ "page": page.to_string() }))
        }
        _ => Err(ApiError::bad_request("focus takes a frame or a page")),
    }
}

/// A frame id from the agent: 32 hex characters.
pub(super) fn frame_id(text: &str) -> Result<ItemId, ApiError> {
    ItemId::from_hex(text).ok_or_else(|| ApiError::bad_request("frame id is 32 hex characters"))
}

/// The page holding the live frame `id`.
pub(super) async fn page_of_frame(
    app: &AppState,
    id: ItemId,
) -> Result<penwise_proto::PageId, ApiError> {
    let page = app
        .storage
        .page_of_item(id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown frame"))?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown frame"))?;
    if frame_item(view.replica.scene(), id).is_none() {
        return Err(ApiError::not_found("unknown frame"));
    }
    Ok(page)
}
