//! Agent tools (design-harness M3, C2): the mark bundle an agent reads in the terminal, its crops, frame
//! creation and frame snapshots.
//!
//! The routes sit in the `/projects` group (token, or a same-machine loopback request), so `penwise-mcp` uses them
//! with the same rules as the registry. A bundle is built from the stored scene plus one headless page per
//! (frame, state) pair: the page replays the mark's state recipe, the bridge resolves the binding keys to nodes
//! and viewport rects, core places the ink exactly as the tablet draws it, and the screenshot is cropped around
//! that ink. Nothing is cached between calls except the crops the bundle names (invalidated by the page's
//! `updated_seq`), so an agent always reads the page as it is now.

mod bundle;
mod cache;
mod entry;
mod frame;
mod frames;
mod ghost;
mod handwriting;
mod images;
mod kind;
mod scene;
#[cfg(test)]
mod tests;
mod variants;

use axum::Router;
use axum::routing::{get, post};

use crate::api::AppState;

pub(crate) use cache::ImageCache;

/// Why an agent request could not be answered.
#[derive(Debug, thiserror::Error)]
pub(crate) enum AgentError {
    #[error("project {0} is not linked")]
    UnknownProject(String),
    #[error(transparent)]
    Inspect(#[from] crate::inspect::Error),
    #[error(transparent)]
    Render(#[from] crate::render::RenderError),
    #[error("internal: {0}")]
    Internal(String),
}

/// The agent routes, merged into the project group by [`crate::api::projects::routes`].
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/marks", get(bundle::list))
        .route("/marks/{id}/image/{kind}", get(images::route))
        .route("/frames/open", post(frames::frame_open))
        .route("/frames/{id}/ghost", post(ghost::ghost))
        .route("/frames/{id}/variants", post(variants::create))
        .route("/frames/{id}/snapshot.png", get(frames::frame_snapshot))
        .route(
            "/variants/{group}",
            get(variants::show).delete(variants::clear),
        )
}
