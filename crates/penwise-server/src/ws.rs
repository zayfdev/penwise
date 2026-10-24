//! WebSocket handling: one snapshot on connect, then every page change the server fans out.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};

use crate::state::{ServerState, StrokeMsg};

/// Runs until the client closes. Malformed input is logged and ignored; it never kills the session.
pub async fn handle(socket: WebSocket, state: Arc<ServerState>) {
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
                match message {
                    Message::Text(text) => {
                        let reply = apply(&state, text.as_str()).await;
                        if let Some(reply) = reply
                            && sink.send(Message::Text(reply.into())).await.is_err()
                        {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
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
}

/// Applies one client frame; returns a direct reply when the client needs one.
async fn apply(state: &ServerState, text: &str) -> Option<String> {
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, "ignoring malformed frame");
            return None;
        }
    };
    match value.get("t").and_then(serde_json::Value::as_str) {
        Some("hello") => {
            tracing::info!("client hello");
            Some(state.snapshot_frame().await)
        }
        Some("stroke") => match serde_json::from_value::<StrokeMsg>(value.get("stroke")?.clone()) {
            Ok(stroke) => {
                if state.append(stroke).await {
                    None
                } else {
                    Some(error_frame("malformed stroke"))
                }
            }
            Err(error) => Some(error_frame(&error.to_string())),
        },
        Some("page") => {
            let strokes = value
                .get("strokes")
                .and_then(|s| serde_json::from_value::<Vec<StrokeMsg>>(s.clone()).ok())
                .unwrap_or_default();
            let count = state.replace(strokes).await;
            Some(serde_json::json!({ "t": "ack", "strokes": count }).to_string())
        }
        other => {
            tracing::warn!(kind = ?other, "ignoring unknown frame");
            None
        }
    }
}

fn error_frame(message: &str) -> String {
    serde_json::json!({ "t": "error", "message": message }).to_string()
}
