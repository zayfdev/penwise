//! axum glue: one WebSocket ↔ the channel pair a [`super::session`] runs on.
//!
//! Binary frames in, binary frames out; text frames are forwarded as [`WsIn::Text`] so the session can answer
//! them with a protocol error. Socket pings/pongs are handled by axum and not visible here.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use axum::extract::{ConnectInfo, Extension, State, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use penwise_proto::MAX_FRAME_BYTES;
use tokio::sync::mpsc;

use super::hub::Hub;
use super::outbound::{Outbound, WsOut};
use super::session;
use super::store::SyncStore;

/// What the reader hands to the session.
#[derive(Debug)]
pub enum WsIn {
    Binary(Bytes),
    Text,
    /// Peer closed (or the socket failed).
    Closed,
}

/// Frames the socket accepts: twice the protocol limit so an oversized frame reaches the session (which
/// answers `Error{TooLarge}`) instead of killing the connection; beyond that axum closes with 1009.
const SOCKET_LIMIT: usize = 2 * MAX_FRAME_BYTES;
/// Inbound frames queued while the session is busy.
const INBOUND_CAPACITY: usize = 16;

/// `/sync` route bound to `hub`. Serve with `into_make_service_with_connect_info::<SocketAddr>()` to enable
/// the per-peer handshake limiter (without it every peer shares one bucket).
pub fn routes<S: SyncStore>(hub: Arc<Hub<S>>) -> Router {
    Router::new()
        .route("/sync", get(upgrade::<S>))
        .with_state(hub)
}

async fn upgrade<S: SyncStore>(
    State(hub): State<Arc<Hub<S>>>,
    ws: WebSocketUpgrade,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
) -> Response {
    let peer = peer.map(|Extension(ConnectInfo(addr))| addr.ip());
    ws.max_message_size(SOCKET_LIMIT)
        .max_frame_size(SOCKET_LIMIT)
        .on_upgrade(move |socket| pump(hub, socket, peer))
}

async fn pump<S: SyncStore>(hub: Arc<Hub<S>>, socket: WebSocket, peer: Option<IpAddr>) {
    let (mut sink, mut stream) = socket.split();
    let (in_tx, in_rx) = mpsc::channel(INBOUND_CAPACITY);
    let (out_tx, mut out_rx) = mpsc::channel(hub.config.outbound_capacity.max(1));
    let write_timeout = hub.config.idle_timeout;
    let reader = tokio::spawn(async move {
        while let Some(Ok(message)) = stream.next().await {
            let frame = match message {
                Message::Binary(bytes) => WsIn::Binary(bytes),
                Message::Text(_) => WsIn::Text,
                Message::Close(_) => break,
                Message::Ping(_) | Message::Pong(_) => continue,
            };
            if in_tx.send(frame).await.is_err() {
                return;
            }
        }
        let _ = in_tx.send(WsIn::Closed).await;
    });
    // Not aborted at the end: once the channels close the session finishes (and cancels its live strokes).
    tokio::spawn(session::run(hub, peer, in_rx, Outbound::new(out_tx)));
    while let Some(out) = out_rx.recv().await {
        let message = match out {
            WsOut::Frame(frame) => Message::Binary(Bytes::copy_from_slice(&frame)),
            WsOut::Close { code, reason } => {
                let close = Message::Close(Some(CloseFrame {
                    code,
                    reason: reason.into(),
                }));
                let _ = sink.send(close).await;
                break;
            }
        };
        // A client that stops reading must not pin the session forever.
        if !matches!(
            tokio::time::timeout(write_timeout, sink.send(message)).await,
            Ok(Ok(()))
        ) {
            break;
        }
    }
    reader.abort();
}
