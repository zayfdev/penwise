//! `notes`: the owner's plain notes through the existing page routes.

use serde_json::{Value, json};

use crate::endpoint::Endpoint;
use crate::{arguments, base64, encode_query, encode_segment, error, get_text};

/// `notes {query?, page?, png?}`: a search, a page as PNG, a page as text, or the page list.
pub(crate) fn notes(request: &Value, endpoint: &Endpoint) -> Result<Value, Value> {
    let args = arguments(request);
    let page = args
        .get("page")
        .and_then(Value::as_str)
        .filter(|page| !page.is_empty())
        .map(|page| format!("&page={}", encode_segment(page)))
        .unwrap_or_default();
    if let Some(query) = args
        .get("query")
        .and_then(Value::as_str)
        .filter(|query| !query.is_empty())
    {
        return get_text(endpoint, &format!("/search?q={}", encode_query(query)));
    }
    if args.get("png").and_then(Value::as_bool).unwrap_or(false) {
        let png = endpoint
            .get(&format!("/page.png?scale=1{page}"))
            .map_err(|failure| error(-32603, &failure))?;
        return Ok(json!({
            "content": [
                { "type": "text", "text": format!("Penwise page, {} PNG bytes.", png.len()) },
                { "type": "image", "mimeType": "image/png", "data": base64::encode(&png) }
            ]
        }));
    }
    if args.get("page").is_some() {
        return get_text(endpoint, &format!("/text?x=1{page}"));
    }
    get_text(endpoint, "/pages")
}
