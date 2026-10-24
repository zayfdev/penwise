//! WebSocket handling: one snapshot on connect, then every page change the server fans out.
//!
//! Live (in-progress) strokes are relayed, not stored: the sender's frames carry a server-assigned session id so
//! viewers can accumulate them, and a commit (`stroke`) tells viewers to drop that session's preview.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};

use crate::state::{ServerState, StrokeMsg};

static SESSIONS: AtomicU64 = AtomicU64::new(1);

/// Runs until the client closes. Malformed input is logged and ignored; it never kills the session.
pub async fn handle(socket: WebSocket, state: Arc<ServerState>) {
    let session = SESSIONS.fetch_add(1, Ordering::Relaxed);
    let (mut sink, mut stream) = socket.split();
    let mut events = state.subscribe();
    if sink
        .send(Message::Text(state.snapshot_frame().await.into()))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else { break };
                let Message::Text(text) = message else {
                    if matches!(message, Message::Close(_)) {
                        break;
                    }
                    continue;
                };
                let Some(reply) = on_text(&state, session, text.as_str()).await else {
                    continue;
                };
                if sink.send(Message::Text(reply.into())).await.is_err() {
                    break;
                }
            },
            event = events.recv() => match event {
                Ok(frame) => {
                    if sink.send(Message::Text(frame.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "client lagged; resending page");
                    if sink.send(Message::Text(state.snapshot_frame().await.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    state.broadcast(live_end(session));
}

/// Applies one client frame; returns a direct reply when the client needs one.
async fn on_text(state: &ServerState, session: u64, text: &str) -> Option<String> {
    let frame: Value = match serde_json::from_str(text) {
        Ok(frame) => frame,
        Err(error) => {
            tracing::warn!(%error, "ignoring malformed frame");
            return None;
        }
    };
    match frame.get("t").and_then(Value::as_str) {
        Some("hello") => Some(state.snapshot_frame().await),
        Some("live") => {
            // Relay only: the preview must not touch the page state.
            state.broadcast(with_session(&frame, session));
            None
        }
        Some("stroke") => {
            let reply = apply_stroke(state, &frame).await;
            // The stroke itself arrives as a normal frame; viewers drop the preview now.
            state.broadcast(live_end(session));
            reply
        }
        Some("page") => {
            let strokes = frame
                .get("strokes")
                .and_then(|s| serde_json::from_value::<Vec<StrokeMsg>>(s.clone()).ok())
                .unwrap_or_default();
            let count = state.replace(strokes).await;
            state.broadcast(live_end(session));
            Some(json!({ "t": "ack", "strokes": count }).to_string())
        }
        other => {
            tracing::warn!(kind = ?other, "ignoring unknown frame");
            None
        }
    }
}

async fn apply_stroke(state: &ServerState, frame: &Value) -> Option<String> {
    let Some(stroke) = frame.get("stroke").cloned() else {
        return Some(error_frame("missing stroke"));
    };
    match serde_json::from_value::<StrokeMsg>(stroke) {
        Ok(stroke) => {
            if state.append(stroke).await {
                None
            } else {
                Some(error_frame("malformed stroke"))
            }
        }
        Err(error) => Some(error_frame(&error.to_string())),
    }
}

fn with_session(frame: &Value, session: u64) -> String {
    let mut out = frame.clone();
    if let Some(object) = out.as_object_mut() {
        object.insert("session".to_owned(), Value::from(session));
    }
    out.to_string()
}

fn live_end(session: u64) -> String {
    json!({ "t": "live_end", "session": session }).to_string()
}

fn error_frame(message: &str) -> String {
    json!({ "t": "error", "message": message }).to_string()
}
