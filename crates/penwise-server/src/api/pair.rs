//! Pairing routes: `POST /pair` (public; the code is the credential) and the token-gated code/QR views.

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use crate::qr;

/// Token-gated pairing routes (merged under the auth middleware).
pub(super) fn protected() -> Router<AppState> {
    Router::new()
        .route("/pair/code", get(code).post(refresh))
        .route("/pair.png", get(qr_png))
}

/// `POST /pair` — exchanges a short pairing code for the token and the reachable URLs.
///
/// The code is the credential, so this route takes no bearer token; it is single-use and rate limited.
pub(super) async fn exchange(State(app): State<AppState>, Json(body): Json<Value>) -> Response {
    let code = body.get("code").and_then(Value::as_str).unwrap_or_default();
    if !app.pairing.exchange(code) {
        return (StatusCode::UNAUTHORIZED, "bad pairing code").into_response();
    }
    Json(json!({ "ok": true, "token": *app.token, "urls": *app.urls })).into_response()
}

/// `GET /pair/code` — the current code, for the viewer to display.
async fn code(State(app): State<AppState>) -> Json<Value> {
    Json(json!({
        "code": app.pairing.current(),
        "expires_in": app.pairing.remaining().as_secs(),
    }))
}

/// `POST /pair/code` — mints a new code (invalidates the old one).
async fn refresh(State(app): State<AppState>) -> Json<Value> {
    Json(json!({
        "code": app.pairing.refresh(),
        "expires_in": app.pairing.remaining().as_secs(),
    }))
}

#[derive(Debug, Deserialize)]
struct QrQuery {
    size: Option<u32>,
}

/// `GET /pair.png?size=8` — the pairing deep link as a QR the device camera can open.
async fn qr_png(State(app): State<AppState>, Query(query): Query<QrQuery>) -> Response {
    let payload = qr::current_payload(&app.pairing, &app.urls);
    match qr::png(&payload, qr::module_px(query.size)) {
        Ok(png) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            png,
        )
            .into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}
