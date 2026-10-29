//! HTTP routes (JSON) over the op-log storage: catalog CRUD, annotate/clear, restore, status — plus the
//! feature modules' routes (pictures, text, search, PNG), pairing and the static viewer. One [`router`] serves
//! both the binary and the in-process tests.
//!
//! Every handler that reveals or changes a page sits behind the token middleware ([`crate::auth::require`]);
//! only `POST /pair` (the code is the credential), `GET /pair/local` (loopback only), `/sync` (authenticates
//! in its handshake) and the static viewer are public. Writes are ops from the server device
//! ([`crate::storage::Storage::server_device`]), so they reach tablets and browsers through the normal
//! broadcast.

mod author;
mod error;
mod marks;
mod notebooks;
pub(crate) mod pages;
mod pair;
mod project_marks;
pub(crate) mod projects;
mod resolve;
mod restore;
mod status;
mod views;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::Uri;
use axum::middleware;
use axum::response::Response;
use axum::routing::{get, post};

use penwise_proto::PageId;

use crate::assets::Assets;
use crate::pairing::Pairing;
use crate::storage::Storage;

pub(crate) use author::Author;
pub(crate) use error::ApiError;
pub(crate) use pair::same_machine;
pub(crate) use resolve::{page_of, parse_page};

/// Everything a handler needs. Cheap to clone.
#[allow(
    missing_debug_implementations,
    reason = "holding the token makes a derived Debug a leak risk; nothing logs AppState"
)]
#[derive(Clone)]
pub struct AppState {
    pub storage: Storage,
    pub token: Arc<String>,
    pub data_dir: Arc<PathBuf>,
    /// Directory of the static viewer.
    pub web: Arc<PathBuf>,
    pub pairing: Arc<Pairing>,
    /// Where this machine is reachable (handed out when pairing; recomputed per request).
    pub urls: Arc<crate::urls::Urls>,
    pub assets: Arc<Assets>,
    /// Base URL of the handwriting model (`ServerConfig::ocr_url`).
    pub ocr_url: Arc<String>,
    /// The headless browser behind `/inspect/*`, source mapping and the agent bundle.
    pub(crate) inspect: Arc<crate::inspect::Inspect>,
    /// Linked projects and their live ports.
    pub(crate) projects: Arc<crate::frames::registry::Registry>,
    /// Broadcasts a page switch to the connected devices (`POST /pages/<id>/select` and the agent's
    /// `penwise_switch_page`).
    pub(crate) announce_page: PageAnnouncer,
    /// Mark crops built by `GET /marks`, served by `GET /marks/<id>/image/<kind>.png`.
    pub(crate) images: Arc<crate::agent::ImageCache>,
}

/// Tells every sync session to switch to a page.
pub(crate) type PageAnnouncer = Arc<dyn Fn(PageId) + Send + Sync>;

/// The HTTP router (without `/sync`, which `serve` merges).
pub fn router(app: AppState) -> Router {
    let protected = Router::new()
        .merge(status::routes())
        .merge(pages::routes())
        .merge(notebooks::routes())
        .merge(marks::routes())
        .merge(restore::routes())
        .merge(crate::pictures::routes())
        .merge(crate::ocr::routes())
        .merge(crate::search::routes())
        .merge(crate::render::routes())
        .merge(crate::anchors::routes())
        .merge(crate::inspect::routes())
        .merge(pair::protected())
        .layer(middleware::from_fn_with_state(
            Arc::clone(&app.token),
            crate::auth::require,
        ));
    Router::new()
        .route("/pair", post(pair::exchange))
        .route("/pair/local", get(pair::local))
        .merge(protected)
        // Projects may be linked by a same-machine tool without the token (the rule of `GET /pair/local`).
        .merge(projects::routes(Arc::clone(&app.token)))
        .fallback(get(file))
        .with_state(app)
}

async fn file(State(app): State<AppState>, uri: Uri) -> Response {
    crate::http::serve((*app.web).clone(), uri).await
}
