//! WebSocket handling: the library index and the selected page on connect, then every change the server fans out.
//!
//! Live (in-progress) strokes are relayed, not stored: the sender's frames carry a server-assigned session id so
//! viewers can accumulate them, and a commit (`stroke`) tells viewers to drop that session's preview.
//!
//! Each session watches one page. Frames that name a page reach only the sessions showing it; frames without a
//! page (the index, the active page, end-of-preview) reach everyone.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};

use crate::library::{Library, StrokeMsg};

static SESSIONS: AtomicU64 = AtomicU64::new(1);

/// Runs until the client closes. Malformed input is logged and ignored; it never kills the session.
pub async fn handle(socket: WebSocket, library: Arc<Library>) {
    let session = SESSIONS.fetch_add(1, Ordering::Relaxed);
    let (mut sink, mut stream) = socket.split();
    let mut events = library.subscribe();
    let mut page = library.active().await;
    let opening = [
        library.meta_frame().await,
        library.active_frame().await,
        library.snapshot_frame(&page).await,
    ];
    for frame in opening {
        if sink.send(Message::Text(frame.into())).await.is_err() {
            return;
        }
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
                let Some(reply) = on_text(&library, session, &mut page, text.as_str()).await else {
                    continue;
                };
                if sink.send(Message::Text(reply.into())).await.is_err() {
                    break;
                }
            },
            event = events.recv() => match event {
                Ok(frame) => {
                    let wanted = frame.page.as_deref();
                    if wanted.is_some_and(|wanted| wanted != page) {
                        continue;
                    }
                    if sink.send(Message::Text(frame.text.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "client lagged; resending page");
                    if sink.send(Message::Text(library.snapshot_frame(&page).await.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    library.fan_out(None, live_end(session));
}

/// Applies one client frame; `page` is the session's followed page and changes on `select`.
async fn on_text(library: &Library, session: u64, page: &mut String, text: &str) -> Option<String> {
    let frame: Value = match serde_json::from_str(text) {
        Ok(frame) => frame,
        Err(error) => {
            tracing::warn!(%error, "ignoring malformed frame");
            return None;
        }
    };
    match frame.get("t").and_then(Value::as_str) {
        Some("hello") => Some(library.snapshot_frame(page).await),
        Some("select") => {
            let Some(target) = frame.get("page").and_then(Value::as_str) else {
                return Some(error_frame("missing page"));
            };
            if !library.select(target).await {
                return Some(error_frame("unknown page"));
            }
            target.clone_into(page);
            Some(library.snapshot_frame(target).await)
        }
        Some("live") => {
            // Relay only: the preview must not touch the page state.
            let target = match frame_page(library, &frame, page).await {
                Ok(target) => target,
                Err(reply) => return Some(reply),
            };
            let scoped = with_page(&frame, &target);
            library.fan_out(Some(target), with_session(&scoped, session));
            None
        }
        Some("stroke") => {
            let target = match frame_page(library, &frame, page).await {
                Ok(target) => target,
                Err(reply) => return Some(reply),
            };
            let reply = match frame.get("stroke").cloned() {
                Some(stroke) => match serde_json::from_value::<StrokeMsg>(stroke) {
                    Ok(stroke) => {
                        if library.append(&target, stroke).await {
                            None
                        } else {
                            Some(error_frame("malformed stroke"))
                        }
                    }
                    Err(error) => Some(error_frame(&error.to_string())),
                },
                None => Some(error_frame("missing stroke")),
            };
            // The stroke itself arrives as a normal frame; viewers drop the preview now.
            library.fan_out(None, live_end(session));
            reply
        }
        Some("op") => {
            let target = match frame_page(library, &frame, page).await {
                Ok(target) => target,
                Err(reply) => return Some(reply),
            };
            Some(apply_op(library, &target, &frame).await)
        }
        Some("page") => {
            let target = match frame_page(library, &frame, page).await {
                Ok(target) => target,
                Err(reply) => return Some(reply),
            };
            let strokes = frame
                .get("strokes")
                .and_then(|s| serde_json::from_value::<Vec<StrokeMsg>>(s.clone()).ok())
                .unwrap_or_default();
            let count = library.replace(&target, strokes).await;
            library.fan_out(None, live_end(session));
            Some(json!({ "t": "ack", "strokes": count }).to_string())
        }
        other => {
            tracing::warn!(kind = ?other, "ignoring unknown frame");
            None
        }
    }
}

/// Applies one `{"t":"op","op":{"k":…}}` frame; returns the reply for the client.
async fn apply_op(library: &Library, page: &str, frame: &Value) -> String {
    let Some(op) = frame.get("op") else {
        return error_frame("missing op");
    };
    let seeds = || -> Vec<String> {
        op.get("seeds")
            .and_then(|value| serde_json::from_value::<Vec<String>>(value.clone()).ok())
            .unwrap_or_default()
    };
    match op.get("k").and_then(Value::as_str) {
        Some("remove") => {
            let removed = library.remove_strokes(page, &seeds()).await;
            json!({ "t": "ack", "removed": removed }).to_string()
        }
        Some("clear") => {
            let removed = library.clear_page(page).await;
            json!({ "t": "ack", "removed": removed }).to_string()
        }
        Some("replace") => {
            let strokes = op
                .get("strokes")
                .and_then(|value| serde_json::from_value::<Vec<StrokeMsg>>(value.clone()).ok())
                .unwrap_or_default();
            let total = library.replace_strokes(page, &seeds(), strokes).await;
            json!({ "t": "ack", "strokes": total }).to_string()
        }
        Some("add") => {
            let Some(stroke) = op.get("stroke").cloned() else {
                return error_frame("missing stroke");
            };
            match serde_json::from_value::<StrokeMsg>(stroke) {
                Ok(stroke) => {
                    if library.append(page, stroke).await {
                        json!({ "t": "ack" }).to_string()
                    } else {
                        error_frame("malformed stroke")
                    }
                }
                Err(error) => error_frame(&error.to_string()),
            }
        }
        other => error_frame(&format!("unknown op {other:?}")),
    }
}

/// The page a frame is about: its explicit `page` when present (and known), else the session's.
/// `Err` carries the reply for an unknown page, so a stale client cannot rewrite the active one.
async fn frame_page(
    library: &Library,
    frame: &Value,
    session_page: &str,
) -> Result<String, String> {
    let Some(explicit) = frame.get("page").and_then(Value::as_str) else {
        return Ok(session_page.to_owned());
    };
    if explicit == session_page || library.page(explicit).await.is_some() {
        return Ok(explicit.to_owned());
    }
    Err(error_frame("unknown page"))
}

/// Adds `page` when the client left it out (older app builds), leaving an explicit one alone.
fn with_page(frame: &Value, page: &str) -> Value {
    let mut out = frame.clone();
    if let Some(object) = out.as_object_mut() {
        object.entry("page").or_insert_with(|| Value::from(page));
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_without_a_page_inherits_the_session_page() {
        let frame = json!({ "t": "live", "samples": [] });
        let scoped = with_page(&frame, "pg_1");
        assert_eq!(Some("pg_1"), scoped.get("page").and_then(Value::as_str));
    }

    #[test]
    fn an_explicit_page_is_kept() {
        let frame = json!({ "t": "live", "page": "pg_2" });
        let scoped = with_page(&frame, "pg_1");
        assert_eq!(Some("pg_2"), scoped.get("page").and_then(Value::as_str));
    }

    #[test]
    fn live_frames_carry_the_session() {
        let frame = json!({ "t": "live", "page": "pg_1" });
        let text = with_session(&frame, 7);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(7, parsed["session"]);
    }
}
