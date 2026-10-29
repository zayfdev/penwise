//! Design-harness tools (spec §7.1–7.2): linked projects, frames, mark bundles and mark status.
//!
//! They forward to the server's design routes and inline the bundle's crop PNGs as MCP image content blocks
//! (`penwise_marks`) or the frame snapshot (`penwise_frame_snapshot`). The bundle already carries the meaning,
//! element and source lines, so nothing is interpreted here.

use serde_json::{Value, json};

use crate::endpoint::Endpoint;
use crate::{
    arguments, base64, encode_segment, error, get_text, post_arguments, post_text, text_content,
};

/// Checkpoint name when a call does not name its agent.
const DEFAULT_AGENT: &str = "agent";

/// Which bundle crops become image content blocks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Images {
    None,
    WithInk,
    All,
}

/// `<name>` when it names a design tool, else `None`.
pub(crate) fn tool(
    name: &str,
    request: &Value,
    endpoint: &Endpoint,
) -> Option<Result<Value, Value>> {
    Some(match name {
        "penwise_projects" => get_text(endpoint, "/projects"),
        "penwise_frame_open" => post_arguments(request, endpoint, "/frames/open"),
        "penwise_frame_snapshot" => snapshot(request, endpoint),
        "penwise_marks" => marks(request, endpoint),
        "penwise_mark_status" => status(request, endpoint),
        "penwise_ghost" => ghost(request, endpoint),
        "penwise_variants" => variants(request, endpoint),
        "penwise_choice" => choice(request, endpoint),
        "penwise_variants_clear" => variants_clear(request, endpoint),
        _ => return None,
    })
}

/// `POST /frames/<id>/ghost` — `{patch}` sets a ghost, `{clear: true}` removes it.
fn ghost(request: &Value, endpoint: &Endpoint) -> Result<Value, Value> {
    let args = arguments(request);
    let frame = args
        .get("frame")
        .and_then(Value::as_str)
        .ok_or_else(|| error(-32602, "missing frame"))?;
    let body = if args.get("clear").and_then(Value::as_bool).unwrap_or(false) {
        json!({ "patch": Value::Null })
    } else {
        let patch = args
            .get("patch")
            .filter(|patch| !patch.is_null())
            .ok_or_else(|| error(-32602, "missing patch or clear"))?;
        json!({ "patch": patch })
    };
    let path = format!("/frames/{}/ghost", encode_segment(frame));
    post_text(endpoint, &path, &body.to_string())
}

/// `POST /frames/<id>/variants` — `{variants: [{label, patch}]}`, 2 to 4.
fn variants(request: &Value, endpoint: &Endpoint) -> Result<Value, Value> {
    let args = arguments(request);
    let frame = args
        .get("frame")
        .and_then(Value::as_str)
        .ok_or_else(|| error(-32602, "missing frame"))?;
    let variants = args
        .get("variants")
        .filter(|variants| variants.is_array())
        .ok_or_else(|| error(-32602, "missing variants (2 to 4 of {label, patch})"))?;
    let body = json!({ "variants": variants });
    let path = format!("/frames/{}/variants", encode_segment(frame));
    post_text(endpoint, &path, &body.to_string())
}

/// `GET /variants/<group>` — the pick as one text block, then the variants' mark crops.
fn choice(request: &Value, endpoint: &Endpoint) -> Result<Value, Value> {
    let args = arguments(request);
    let group = args
        .get("group")
        .and_then(Value::as_str)
        .ok_or_else(|| error(-32602, "missing group"))?;
    let path = format!("/variants/{}", encode_segment(group));
    let body = endpoint
        .get(&path)
        .map_err(|failure| error(-32603, &failure))?;
    let text = String::from_utf8_lossy(&body).into_owned();
    let reply: Value = match serde_json::from_str(&text) {
        Ok(reply) => reply,
        Err(_) => return Ok(text_content(&text)),
    };
    let mut content = vec![json!({ "type": "text", "text": text })];
    for mark in reply
        .get("marks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        content.extend(crops(mark, Images::All, endpoint)?);
    }
    Ok(json!({ "content": content }))
}

/// `DELETE /variants/<group>` — the frames go, the marks stay as done.
fn variants_clear(request: &Value, endpoint: &Endpoint) -> Result<Value, Value> {
    let args = arguments(request);
    let group = args
        .get("group")
        .and_then(Value::as_str)
        .ok_or_else(|| error(-32602, "missing group"))?;
    let path = format!("/variants/{}", encode_segment(group));
    let reply = endpoint
        .delete(&path, "{}")
        .map_err(|failure| error(-32603, &failure))?;
    Ok(text_content(&String::from_utf8_lossy(&reply)))
}

/// `GET /frames/<id>/snapshot.png?ink=…` as a text summary plus the PNG.
fn snapshot(request: &Value, endpoint: &Endpoint) -> Result<Value, Value> {
    let args = arguments(request);
    let frame = args
        .get("frame")
        .and_then(Value::as_str)
        .ok_or_else(|| error(-32602, "missing frame"))?;
    let ink = args.get("ink").and_then(Value::as_bool).unwrap_or(true);
    let path = format!("/frames/{}/snapshot.png?ink={ink}", encode_segment(frame));
    let png = endpoint
        .get(&path)
        .map_err(|failure| error(-32603, &failure))?;
    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": format!("Frame {frame} snapshot, {} PNG bytes (ink: {ink}).", png.len())
            },
            image_block(&png)
        ]
    }))
}

/// `GET /marks` — the bundle JSON as one text block, then the requested crops of every mark, in bundle order.
fn marks(request: &Value, endpoint: &Endpoint) -> Result<Value, Value> {
    let args = arguments(request);
    let images = images_option(&args)?;
    let body = endpoint
        .get(&marks_path(&args))
        .map_err(|failure| error(-32603, &failure))?;
    let text = String::from_utf8_lossy(&body).into_owned();
    let bundle: Value = match serde_json::from_str(&text) {
        Ok(bundle) => bundle,
        // A reply that is not JSON is still worth showing (a proxy error page, say).
        Err(_) => return Ok(text_content(&text)),
    };
    let mut content = vec![json!({ "type": "text", "text": text })];
    let marks = bundle.get("marks").and_then(Value::as_array);
    for mark in marks.into_iter().flatten() {
        content.extend(crops(mark, images, endpoint)?);
    }
    Ok(json!({ "content": content }))
}

/// `POST /marks/<id>/status` — the status edit; `note` is forwarded for servers that keep it.
fn status(request: &Value, endpoint: &Endpoint) -> Result<Value, Value> {
    let args = arguments(request);
    let id = args
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| error(-32602, "missing id"))?;
    let status = args
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| error(-32602, "missing status"))?;
    let mut body = json!({ "status": status });
    if let Some(note) = args.get("note").and_then(Value::as_str) {
        body["note"] = Value::String(note.to_owned());
    }
    let path = format!("/marks/{}/status", encode_segment(id));
    post_text(endpoint, &path, &body.to_string())
}

/// `?project=…&page=…&frame=…&status=…&since=…&agent=…` for `GET /marks`.
///
/// `since` defaults to `last` (the agent's previous checkpoint); `"none"` omits it, so the call sees every
/// matching mark. The checkpoint is recorded under `agent` (`"agent"` unless the call names one).
fn marks_path(args: &Value) -> String {
    let mut params = Vec::new();
    for key in ["project", "page", "frame", "status"] {
        if let Some(value) = args
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            params.push(format!("{key}={}", encode_segment(value)));
        }
    }
    let since = args.get("since").and_then(Value::as_str).unwrap_or("last");
    if !matches!(since, "" | "none") {
        params.push(format!("since={}", encode_segment(since)));
    }
    let agent = args
        .get("agent")
        .and_then(Value::as_str)
        .filter(|agent| !agent.is_empty())
        .unwrap_or(DEFAULT_AGENT);
    params.push(format!("agent={}", encode_segment(agent)));
    format!("/marks?{}", params.join("&"))
}

fn images_option(args: &Value) -> Result<Images, Value> {
    match args
        .get("images")
        .and_then(Value::as_str)
        .unwrap_or("with_ink")
    {
        "none" => Ok(Images::None),
        "with_ink" => Ok(Images::WithInk),
        "all" => Ok(Images::All),
        other => Err(error(
            -32602,
            &format!("images is one of none, with_ink, all (got {other:?})"),
        )),
    }
}

/// The crops a bundle names, fetched and base64-encoded. A missing URL is skipped (the mark may not resolve).
fn crops(mark: &Value, images: Images, endpoint: &Endpoint) -> Result<Vec<Value>, Value> {
    let wanted: &[&str] = match images {
        Images::None => return Ok(Vec::new()),
        Images::WithInk => &["with_ink"],
        Images::All => &["with_ink", "context"],
    };
    let Some(urls) = mark.get("images").filter(|urls| urls.is_object()) else {
        return Ok(Vec::new());
    };
    let id = mark.get("id").and_then(Value::as_str).unwrap_or("mark");
    let mut blocks = Vec::with_capacity(wanted.len());
    for kind in wanted {
        let Some(url) = urls.get(*kind).and_then(Value::as_str) else {
            continue;
        };
        let png = endpoint
            .get(url)
            .map_err(|failure| error(-32603, &format!("{id} {kind}: {failure}")))?;
        blocks.push(image_block(&png));
    }
    Ok(blocks)
}

fn image_block(png: &[u8]) -> Value {
    json!({ "type": "image", "mimeType": "image/png", "data": base64::encode(png) })
}

#[cfg(test)]
mod tests;
