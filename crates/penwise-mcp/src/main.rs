//! Penwise MCP server (stdio): four tools between an agent and the owner's design board.
//!
//! JSON-RPC 2.0 over stdin/stdout, MCP `initialize` / `tools/list` / `tools/call`. It talks to the Penwise server
//! over plain HTTP on the loopback interface, so it needs no TLS stack and no async runtime. The tools are
//! `look`, `show`, `resolve` and `notes` ([`tools`]); every one maps onto a server route and the composition
//! lives in the server. The project comes from `PENWISE_PROJECT` (written by `penwise link`).
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "stdout is the MCP transport; diagnostics go to stderr"
)]

use std::io::{BufRead, Write};

const SERVER_NAME: &str = "penwise";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_PROTOCOL: &str = "2024-11-05";
mod base64;
mod endpoint;
mod look;
mod notes;
mod show;
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
    // The project `penwise link` wrote into this config; no tool takes a project argument.
    let project = std::env::var("PENWISE_PROJECT")
        .ok()
        .map(|project| project.trim().to_owned())
        .filter(|project| !project.is_empty());
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
        if let Some(response) = handle(&request, &endpoint, project.as_deref()) {
            let mut out = std::io::stdout();
            if writeln!(out, "{response}").is_err() || out.flush().is_err() {
                break;
            }
        }
    }
}

/// One JSON-RPC request; `None` for notifications (no `id`).
fn handle(
    request: &serde_json::Value,
    endpoint: &Endpoint,
    project: Option<&str>,
) -> Option<serde_json::Value> {
    let id = request.get("id")?.clone();
    let method = request
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let result = match method {
        "initialize" => Ok(initialize(request)),
        "tools/list" => Ok(tools::tools_list()),
        "tools/call" => match call_tool(request, endpoint, project) {
            Err(CallError::Tool(message)) => Ok(tool_error(&message)),
            Err(CallError::Protocol(failure)) => Err(failure),
            Ok(result) => Ok(result),
        },
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
        "instructions": "Penwise: the owner draws marks on a live copy of the linked project. Loop: look (read the marks with file:line), show (a frame, a ghost, 2-4 variants, a sketch), edit the code, look again to read the owner's pick, resolve. notes reads the owner's plain notes.",
    })
}

/// Why a `tools/call` produced no result: a malformed request (a JSON-RPC error) or a tool that could not do
/// its work (reported to the model as an `isError` result, so it can read the message and recover).
enum CallError {
    Protocol(serde_json::Value),
    Tool(String),
}

fn call_tool(
    request: &serde_json::Value,
    endpoint: &Endpoint,
    project: Option<&str>,
) -> Result<serde_json::Value, CallError> {
    let name = request
        .pointer("/params/name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if !matches!(name, "look" | "show" | "resolve" | "notes") {
        return Err(CallError::Protocol(error(
            -32602,
            &format!("unknown tool {name}"),
        )));
    }
    let outcome = if name == "notes" {
        notes::notes(request, endpoint)
    } else {
        match project {
            None => Err(error(
                -32602,
                "PENWISE_PROJECT is not set: run `penwise link` in the project and restart the agent",
            )),
            Some(project) => match name {
                "look" => look::look(request, endpoint, project),
                "show" => show::show(request, endpoint, project),
                _ => show::resolve(request, endpoint, project),
            },
        }
    };
    outcome.map_err(|failure| {
        CallError::Tool(
            failure
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("the tool failed")
                .to_owned(),
        )
    })
}

/// A failed tool call as MCP wants it: a result with `isError`, whose text the model reads.
fn tool_error(message: &str) -> serde_json::Value {
    serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": message }] })
}

/// Percent-encodes everything but unreserved bytes, spaces as `+` (a query value).
pub(crate) fn encode_query(value: &str) -> String {
    percent_encode(value, "+")
}

/// Percent-encodes everything but unreserved bytes (a path segment or `page=` value). A value comes from the
/// agent's tool arguments and is spliced into the HTTP request line, so a CR/LF/space in it would end the
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

/// `POST path` with an explicit body, returned as a text content block.
pub(crate) fn post_text(
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

pub(crate) fn text_content(text: &str) -> serde_json::Value {
    serde_json::json!({ "content": [{ "type": "text", "text": text }] })
}

pub(crate) fn error(code: i32, message: &str) -> serde_json::Value {
    serde_json::json!({ "code": code, "message": message })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_with_a_request_smuggle_is_one_inert_path_segment() {
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
    fn a_query_value_turns_spaces_into_plus() {
        assert_eq!("hello+world%26x", encode_query("hello world&x"));
    }

    #[test]
    fn exactly_four_tools_are_listed() {
        let list = tools::tools_list();
        let names: Vec<&str> = list["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert_eq!(vec!["look", "show", "resolve", "notes"], names);
    }
}
