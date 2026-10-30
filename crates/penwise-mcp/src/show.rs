//! `show` and `resolve`: forward to `POST /agent/show` and `POST /agent/resolve`. The only work done here is
//! the one thing the server cannot do: reading a local PNG for `image {path}` and uploading it to the asset
//! store.

use serde_json::{Value, json};

use crate::endpoint::Endpoint;
use crate::{arguments, error, post_text};

/// The largest picture `show image` reads, in bytes (the server's own limit is 24 MiB).
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// `show {frame | ghost | variants | sketch | image | focus}`.
pub(crate) fn show(request: &Value, endpoint: &Endpoint, project: &str) -> Result<Value, Value> {
    let mut body = arguments(request);
    let Some(fields) = body.as_object_mut() else {
        return Err(error(
            -32602,
            "show takes exactly one of frame, ghost, variants, sketch, image, focus",
        ));
    };
    if let Some(image) = fields.get("image").cloned() {
        fields.insert("image".to_owned(), upload(&image, endpoint)?);
    }
    fields.insert("project".to_owned(), json!(project));
    post_text(endpoint, "/agent/show", &body.to_string())
}

/// `resolve {marks?, round?}`.
pub(crate) fn resolve(request: &Value, endpoint: &Endpoint, project: &str) -> Result<Value, Value> {
    let mut body = arguments(request);
    let Some(fields) = body.as_object_mut() else {
        return Err(error(-32602, "resolve takes marks and/or round"));
    };
    fields.insert("project".to_owned(), json!(project));
    post_text(endpoint, "/agent/resolve", &body.to_string())
}

/// `{path, x?, y?, label?}` → `{asset, w, h, x?, y?, label?}`: the file is uploaded (the same file twice costs
/// one asset) and sized from the picture itself.
fn upload(image: &Value, endpoint: &Endpoint) -> Result<Value, Value> {
    let path = image
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| error(-32602, "image needs a path"))?;
    let meta = std::fs::metadata(path)
        .map_err(|failure| error(-32602, &format!("cannot read {path}: {failure}")))?;
    if !meta.is_file() {
        return Err(error(-32602, &format!("{path} is not a regular file")));
    }
    if meta.len() > MAX_IMAGE_BYTES {
        return Err(error(
            -32602,
            &format!(
                "{path} is {} bytes; the limit is {MAX_IMAGE_BYTES}",
                meta.len()
            ),
        ));
    }
    let bytes = std::fs::read(path)
        .map_err(|failure| error(-32602, &format!("cannot read {path}: {failure}")))?;
    let reply = endpoint
        .post_bytes("/assets", &bytes, "image/png")
        .map_err(|failure| error(-32603, &failure))?;
    let stored: Value =
        serde_json::from_slice(&reply).map_err(|failure| error(-32603, &failure.to_string()))?;
    let field = |name: &str| {
        stored
            .get(name)
            .cloned()
            .ok_or_else(|| error(-32603, "the server did not describe the asset"))
    };
    let mut out = json!({ "asset": field("asset")?, "w": field("width")?, "h": field("height")? });
    for key in ["x", "y", "label"] {
        if let Some(value) = image.get(key).filter(|value| !value.is_null()) {
            out[key] = value.clone();
        }
    }
    Ok(out)
}
