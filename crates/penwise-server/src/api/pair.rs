//! Pairing routes: `POST /pair` (public; the code is the credential), `GET /pair/local` (the token for a
//! viewer on this machine) and the token-gated code/QR views.

use std::net::{IpAddr, SocketAddr};

use axum::Json;
use axum::Router;
use axum::extract::{ConnectInfo, Extension, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
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
    Json(json!({ "ok": true, "token": *app.token, "urls": app.urls.get().await })).into_response()
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
    let payload = qr::current_payload(&app.pairing, &app.urls.get().await);
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

/// `GET /pair/local` — the token, for a viewer running on this machine (the desktop launcher), so nobody
/// copies it into a URL.
///
/// Answered only when the TCP peer is loopback *and* the `Host` header names loopback: a web page elsewhere
/// cannot read it (no CORS headers), and the `Host` check defeats DNS rebinding (an attacker's hostname
/// pointed at 127.0.0.1 still carries the attacker's `Host`). Anything else gets 403.
pub(super) async fn local(
    State(app): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
) -> Response {
    let peer = peer.map(|Extension(ConnectInfo(addr))| addr.ip());
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    if !same_machine(peer, host) {
        return (StatusCode::FORBIDDEN, "only for a viewer on this machine").into_response();
    }
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "token": *app.token })),
    )
        .into_response()
}

/// Loopback peer and a loopback `Host` (`127.0.0.1`, `[::1]` or `localhost`, any port).
fn same_machine(peer: Option<IpAddr>, host: Option<&str>) -> bool {
    let Some(host) = host else { return false };
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or_default(),
        None => host.split(':').next().unwrap_or_default(),
    };
    let loopback_name = name.eq_ignore_ascii_case("localhost")
        || name.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
    peer.is_some_and(|ip| ip.is_loopback()) && loopback_name
}
