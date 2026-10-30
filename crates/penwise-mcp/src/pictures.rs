//! Picture tools: place a screenshot or component cut-out on the page, list and remove them.

use crate::endpoint::Endpoint;
use crate::{arguments, encode_segment, error, get_text, text_content};

/// The three picture tools, kept out of [`call_tool`] so each stays readable.
pub(crate) fn picture_tool(
    name: &str,
    request: &serde_json::Value,
    endpoint: &Endpoint,
) -> Result<serde_json::Value, serde_json::Value> {
    match name {
        "penwise_place_image" => place_image(request, endpoint),
        "penwise_list_images" => {
            let page = resolve_page(endpoint, request)?;
            get_text(endpoint, &format!("/pages/{page}/images"))
        }
        "penwise_remove_image" => {
            let page = resolve_page(endpoint, request)?;
            let id = arguments(request)
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| error(-32602, "missing id"))?
                .to_owned();
            post_text_typed(
                endpoint,
                &format!("/pages/{page}/images"),
                &serde_json::json!({ "ids": [id] }).to_string(),
                "DELETE",
            )
        }
        other => Err(error(-32601, &format!("unknown tool {other}"))),
    }
}

/// Uploads `path` (or uses `asset`) and places the picture at `x, y` on the page.
fn place_image(
    request: &serde_json::Value,
    endpoint: &Endpoint,
) -> Result<serde_json::Value, serde_json::Value> {
    let arguments = arguments(request);
    let number = |name: &str| arguments.get(name).and_then(serde_json::Value::as_f64);
    let x = number("x").ok_or_else(|| error(-32602, "missing x"))?;
    let y = number("y").ok_or_else(|| error(-32602, "missing y"))?;
    let (asset, natural) =
        if let Some(asset) = arguments.get("asset").and_then(serde_json::Value::as_str) {
            (asset.to_owned(), None)
        } else {
            let path = arguments
                .get("path")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| error(-32602, "give path or asset"))?;
            let bytes = std::fs::read(path)
                .map_err(|failure| error(-32602, &format!("cannot read {path}: {failure}")))?;
            let reply = endpoint
                .post_bytes("/assets", &bytes, "image/png")
                .map_err(|failure| error(-32603, &failure))?;
            let stored: serde_json::Value = serde_json::from_slice(&reply)
                .map_err(|failure| error(-32603, &failure.to_string()))?;
            let asset = stored
                .get("asset")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| error(-32603, "the server did not name the asset"))?
                .to_owned();
            let size = stored
                .get("width")
                .and_then(serde_json::Value::as_f64)
                .zip(stored.get("height").and_then(serde_json::Value::as_f64));
            (asset, size)
        };
    let width = number("w")
        .or_else(|| natural.map(|(width, _)| width))
        .ok_or_else(|| error(-32602, "missing w (no path to size it from)"))?;
    let height = number("h")
        .or_else(|| natural.map(|(_, height)| height))
        .ok_or_else(|| error(-32602, "missing h (no path to size it from)"))?;
    let page = resolve_page(endpoint, request)?;
    let body = serde_json::json!({
        "asset": asset,
        "x": x,
        "y": y,
        "w": width,
        "h": height,
        "layer": "agent",
        "label": arguments.get("label").and_then(serde_json::Value::as_str),
    });
    post_text_typed(
        endpoint,
        &format!("/pages/{page}/images"),
        &body.to_string(),
        "POST",
    )
}

/// The page named in the arguments, else the library's active page, percent-encoded for use in a request path.
fn resolve_page(
    endpoint: &Endpoint,
    request: &serde_json::Value,
) -> Result<String, serde_json::Value> {
    if let Some(page) = arguments(request)
        .get("page")
        .and_then(serde_json::Value::as_str)
    {
        return Ok(encode_segment(page));
    }
    let body = endpoint
        .get("/pages")
        .map_err(|failure| error(-32603, &failure))?;
    let meta: serde_json::Value =
        serde_json::from_slice(&body).map_err(|failure| error(-32603, &failure.to_string()))?;
    meta.get("active")
        .and_then(serde_json::Value::as_str)
        .filter(|active| !active.is_empty())
        .map(encode_segment)
        .ok_or_else(|| error(-32603, "the server has no pages yet"))
}

/// `POST`/`DELETE path` with an explicit body, returned as a text content block.
fn post_text_typed(
    endpoint: &Endpoint,
    path: &str,
    body: &str,
    method: &str,
) -> Result<serde_json::Value, serde_json::Value> {
    let reply = match method {
        "DELETE" => endpoint.delete(path, body),
        _ => endpoint.post(path, body),
    }
    .map_err(|failure| error(-32603, &failure))?;
    Ok(text_content(&String::from_utf8_lossy(&reply)))
}
