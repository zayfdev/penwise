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
mod pages;
mod pair;
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

use crate::assets::Assets;
use crate::pairing::Pairing;
use crate::storage::Storage;

pub(crate) use author::Author;
pub(crate) use error::ApiError;
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
    /// URLs this machine is reachable at (handed out when pairing).
    pub urls: Arc<Vec<String>>,
    pub assets: Arc<Assets>,
    /// Base URL of the handwriting model (`ServerConfig::ocr_url`).
    pub ocr_url: Arc<String>,
}

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
        .merge(pair::protected())
        .layer(middleware::from_fn_with_state(
            Arc::clone(&app.token),
            crate::auth::require,
        ));
    Router::new()
        .route("/pair", post(pair::exchange))
        .route("/pair/local", get(pair::local))
        .merge(protected)
        .fallback(get(file))
        .with_state(app)
}

async fn file(State(app): State<AppState>, uri: Uri) -> Response {
    crate::http::serve((*app.web).clone(), uri).await
}
