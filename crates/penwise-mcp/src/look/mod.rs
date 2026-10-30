//! `look`: forwards to `POST /agent/look` (looking has effects on the server) and turns the answer into MCP content blocks.

use serde_json::{Value, json};

use crate::endpoint::Endpoint;
use crate::{arguments, error, text_content};

/// `look {mark?, frame?, images?}` (`POST /agent/look`) → the text, then one image block per returned image.
pub(crate) fn look(request: &Value, endpoint: &Endpoint, project: &str) -> Result<Value, Value> {
    let args = arguments(request);
    let mut body = json!({ "project": project });
    for key in ["mark", "frame", "images"] {
        if let Some(value) = args
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            body[key] = json!(value);
        }
    }
    // A POST: looking has effects on the server (marks become seen, the round counts as looked at).
    let reply = endpoint
        .post("/agent/look", &body.to_string())
        .map_err(|failure| error(-32603, &failure))?;
    Ok(content_of(&String::from_utf8_lossy(&reply)))
}

/// `{"text", "images": [{"label", "png"}]}` as content blocks; anything else is shown as it came.
fn content_of(body: &str) -> Value {
    let Ok(reply) = serde_json::from_str::<Value>(body) else {
        return text_content(body);
    };
    let Some(text) = reply.get("text").and_then(Value::as_str) else {
        return text_content(body);
    };
    let mut content = vec![json!({ "type": "text", "text": text })];
    for image in reply
        .get("images")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(png) = image.get("png").and_then(Value::as_str) {
            content.push(json!({ "type": "image", "mimeType": "image/png", "data": png }));
        }
    }
    json!({ "content": content })
}

#[cfg(test)]
mod tests;
