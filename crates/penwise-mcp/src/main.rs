//! Penwise MCP server (stdio): lets an agent read the tablet's page.
//!
//! JSON-RPC 2.0 over stdin/stdout, MCP `initialize` / `tools/list` / `tools/call`. It talks to the Penwise server
//! over plain HTTP on the loopback interface, so it needs no TLS stack and no async runtime.
//!
//! Tools: `penwise_page_png` (render the page), `penwise_page_status` (stroke count).
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "stdout is the MCP transport; diagnostics go to stderr"
)]

use std::fmt::Write as _;
use std::io::{BufRead, Write};

const SERVER_NAME: &str = "penwise";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_PROTOCOL: &str = "2024-11-05";
mod endpoint;
mod pictures;
mod tools;

use endpoint::resolve_token;
use endpoint::{DEFAULT_HOST, DEFAULT_PORT, Endpoint};

fn main() {
    let host = std::env::var("PENWISE_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_owned());
    let port = std::env::var("PENWISE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let endpoint = Endpoint {
        host,
        port,
        token: resolve_token(),
    };
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    while let Some(Ok(line)) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let request: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("penwise-mcp: bad JSON: {error}");
                continue;
            }
        };
        if let Some(response) = handle(&request, &endpoint) {
            let mut out = std::io::stdout();
            if writeln!(out, "{response}").is_err() || out.flush().is_err() {
                break;
            }
        }
    }
}

/// Where the Penwise server lives.
fn handle(request: &serde_json::Value, endpoint: &Endpoint) -> Option<serde_json::Value> {
    let id = request.get("id")?.clone();
    let method = request
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let result = match method {
        "initialize" => Ok(initialize(request)),
        "tools/list" => Ok(tools::tools_list()),
        "tools/call" => call_tool(request, endpoint),
        "ping" => Ok(serde_json::json!({})),
        other => Err(error(-32601, &format!("unknown method {other}"))),
    };
    Some(match result {
        Ok(result) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(failure) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "error": failure }),
    })
}

fn initialize(request: &serde_json::Value) -> serde_json::Value {
    let version = request
        .pointer("/params/protocolVersion")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(DEFAULT_PROTOCOL);
    serde_json::json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
        "instructions": "Penwise: the user's pen tablet page. Use penwise_page_png to look at what they drew.",
    })
}

fn call_tool(
    request: &serde_json::Value,
    endpoint: &Endpoint,
) -> Result<serde_json::Value, serde_json::Value> {
    let name = request
        .pointer("/params/name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if matches!(
        name,
        "penwise_place_image"
            | "penwise_list_images"
            | "penwise_remove_image"
            | "penwise_page_annotations"
    ) {
        return pictures::picture_tool(name, request, endpoint);
    }
    match name {
        "penwise_page_png" => {
            let scale = request
                .pointer("/params/arguments/scale")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0);
            let arguments = arguments(request);
            let page = page_query(&arguments);
            let png = endpoint
                .get(&format!("/page.png?scale={scale}{page}"))
                .map_err(|failure| error(-32603, &failure))?;
            Ok(serde_json::json!({
                "content": [
                    { "type": "text", "text": format!("Penwise page, {} PNG bytes at scale {scale}.", png.len()) },
                    { "type": "image", "mimeType": "image/png", "data": base64(&png) }
                ]
            }))
        }
        "penwise_page_status" => get_text(endpoint, &page_path("/health", request)),
        "penwise_annotate" => post_arguments(request, endpoint, "/annotate"),
        "penwise_clear_agent_marks" => {
            post_text(endpoint, &page_path("/annotate/clear", request), "{}")
        }
        "penwise_list_pages" => get_text(endpoint, "/pages"),
        "penwise_search" => {
            let query = arguments(request)
                .get("query")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| error(-32602, "missing query"))?
                .to_owned();
            get_text(endpoint, &format!("/search?q={}", encode_query(&query)))
        }
        "penwise_page_text" => {
            let arguments = arguments(request);
            let page = page_query(&arguments);
            let refresh = if arguments
                .get("refresh")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                "&refresh=true"
            } else {
                ""
            };
            get_text(endpoint, &format!("/text?x=1{page}{refresh}"))
        }
        "penwise_create_page" => post_arguments(request, endpoint, "/pages"),
        "penwise_create_notebook" => post_arguments(request, endpoint, "/notebooks"),
        "penwise_switch_page" => {
            let arguments = arguments(request);
            let page = arguments
                .get("page")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| error(-32602, "missing page"))?;
            post_text(
                endpoint,
                &format!("/pages/{}/select", encode_segment(page)),
                "{}",
            )
        }
        other => Err(error(-32602, &format!("unknown tool {other}"))),
    }
}

/// `&page=<id>` (encoded) when the arguments name a page, else nothing.
fn page_query(arguments: &serde_json::Value) -> String {
    arguments
        .get("page")
        .and_then(serde_json::Value::as_str)
        .map(|page| format!("&page={}", encode_segment(page)))
        .unwrap_or_default()
}

/// Percent-encodes everything but unreserved bytes, spaces as `+` (a query value).
fn encode_query(value: &str) -> String {
    percent_encode(value, "+")
}

/// Percent-encodes everything but unreserved bytes (a path segment or `page=` value). A page id comes from
/// the agent's tool arguments and is spliced into the HTTP request line, so a CR/LF/space in it would end the
/// request early and smuggle in another one.
pub(crate) fn encode_segment(value: &str) -> String {
    percent_encode(value, "%20")
}

fn percent_encode(value: &str, space: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => space.to_owned(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// `GET path`, returned as a text content block.
pub(crate) fn get_text(
    endpoint: &Endpoint,
    path: &str,
) -> Result<serde_json::Value, serde_json::Value> {
    let body = endpoint
        .get(path)
        .map_err(|failure| error(-32603, &failure))?;
    Ok(text_content(&String::from_utf8_lossy(&body)))
}

/// `POST path` with the call's `arguments` as JSON, returned as a text content block.
fn post_arguments(
    request: &serde_json::Value,
    endpoint: &Endpoint,
    path: &str,
) -> Result<serde_json::Value, serde_json::Value> {
    post_text(endpoint, path, &arguments(request).to_string())
}

/// `POST path` with an explicit body, returned as a text content block.
fn post_text(
    endpoint: &Endpoint,
    path: &str,
    body: &str,
) -> Result<serde_json::Value, serde_json::Value> {
    let reply = endpoint
        .post(path, body)
        .map_err(|failure| error(-32603, &failure))?;
    Ok(text_content(&String::from_utf8_lossy(&reply)))
}

/// The tool call's `arguments` object, empty when the client sent none.
pub(crate) fn arguments(request: &serde_json::Value) -> serde_json::Value {
    request
        .pointer("/params/arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}))
}

/// `path`, plus `?page=<id>` when the arguments name one.
fn page_path(path: &str, request: &serde_json::Value) -> String {
    match arguments(request)
        .get("page")
        .and_then(serde_json::Value::as_str)
    {
        Some(page) => format!("{path}?page={}", encode_segment(page)),
        None => path.to_owned(),
    }
}

pub(crate) fn text_content(text: &str) -> serde_json::Value {
    serde_json::json!({ "content": [{ "type": "text", "text": text }] })
}

pub(crate) fn error(code: i32, message: &str) -> serde_json::Value {
    serde_json::json!({ "code": code, "message": message })
}

/// Standard base64 (RFC 4648, with padding) for the image content block.
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for shift in [18, 12, 6, 0] {
            let _ = write!(out, "{}", ALPHABET[((n >> shift) & 0x3F) as usize] as char);
        }
        if chunk.len() < 3 {
            let pad = 3 - chunk.len();
            out.truncate(out.len() - pad);
            for _ in 0..pad {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_id_with_a_request_smuggle_is_one_inert_path_segment() {
        let hostile = "pg_1 HTTP/1.1\r\n\r\nGET /pages HTTP/1.1\r\nX: ";
        let encoded = encode_segment(hostile);
        assert!(
            encoded
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.~%".contains(&b)),
            "only unreserved bytes and escapes survive: {encoded}"
        );
        assert!(encoded.contains("%0D%0A%0D%0A"));
        assert_eq!("pg_0123456789abcdef", encode_segment("pg_0123456789abcdef"));
    }

    #[test]
    fn the_page_query_encodes_the_id() {
        let request = serde_json::json!({ "params": { "arguments": { "page": "a b&x=1" } } });
        assert_eq!("/p?page=a%20b%26x%3D1", page_path("/p", &request));
    }
}
