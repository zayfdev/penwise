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
use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const SERVER_NAME: &str = "penwise";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_PROTOCOL: &str = "2024-11-05";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 7420;

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
struct Endpoint {
    host: String,
    port: u16,
    token: String,
}

/// `PENWISE_TOKEN`, else `$XDG_DATA_HOME/penwise/token` (where the server writes it). Empty = no auth.
fn resolve_token() -> String {
    if let Ok(token) = std::env::var("PENWISE_TOKEN")
        && !token.is_empty()
    {
        return token;
    }
    let base = std::env::var("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME").map(|home| std::path::PathBuf::from(home).join(".local/share"))
        })
        .unwrap_or_default();
    std::fs::read_to_string(base.join("penwise/token"))
        .map(|token| token.trim().to_owned())
        .unwrap_or_default()
}

fn handle(request: &serde_json::Value, endpoint: &Endpoint) -> Option<serde_json::Value> {
    let id = request.get("id")?.clone();
    let method = request
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let result = match method {
        "initialize" => Ok(initialize(request)),
        "tools/list" => Ok(tools_list()),
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

fn tools_list() -> serde_json::Value {
    serde_json::json!({
        "tools": [
            {
                "name": "penwise_page_png",
                "description": "Render the current Penwise page (the user's handwriting/drawing) as a PNG image.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "scale": {
                            "type": "number",
                            "description": "Pixels per page pixel, 0.05–4 (default 1; use 0.5 for a smaller image)."
                        }
                    }
                }
            },
            {
                "name": "penwise_page_status",
                "description": "Number of strokes currently on the Penwise page.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "penwise_annotate",
                "description": "Draw polylines on the user's page (marks appear as a separate agent layer and can be cleared).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "strokes": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "points": {
                                        "type": "array",
                                        "description": "Page-pixel points [[x, y], …]; two or more make a line.",
                                        "items": { "type": "array", "items": { "type": "number" } }
                                    },
                                    "color": { "type": "string", "description": "#RRGGBB or #AARRGGBB (default red)" },
                                    "width": { "type": "number", "description": "Stroke width in page px (default 3)" }
                                },
                                "required": ["points"]
                            }
                        }
                    },
                    "required": ["strokes"]
                }
            },
            {
                "name": "penwise_clear_agent_marks",
                "description": "Remove every mark this or another agent drew on the page (the user's own ink is untouched).",
                "inputSchema": { "type": "object", "properties": {} }
            }
        ]
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
    match name {
        "penwise_page_png" => {
            let scale = request
                .pointer("/params/arguments/scale")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0);
            let png = endpoint
                .get(&format!("/page.png?scale={scale}"))
                .map_err(|failure| error(-32603, &failure))?;
            Ok(serde_json::json!({
                "content": [
                    { "type": "text", "text": format!("Penwise page, {} PNG bytes at scale {scale}.", png.len()) },
                    { "type": "image", "mimeType": "image/png", "data": base64(&png) }
                ]
            }))
        }
        "penwise_page_status" => {
            let body = endpoint
                .get("/health")
                .map_err(|failure| error(-32603, &failure))?;
            let text = String::from_utf8_lossy(&body).to_string();
            Ok(serde_json::json!({ "content": [{ "type": "text", "text": text }] }))
        }
        "penwise_annotate" => {
            let body = request.pointer("/params/arguments").map_or_else(
                || "{\"strokes\":[]}".to_owned(),
                serde_json::Value::to_string,
            );
            let reply = endpoint
                .post("/annotate", &body)
                .map_err(|failure| error(-32603, &failure))?;
            Ok(text_content(&String::from_utf8_lossy(&reply)))
        }
        "penwise_clear_agent_marks" => {
            let reply = endpoint
                .post("/annotate/clear", "{}")
                .map_err(|failure| error(-32603, &failure))?;
            Ok(text_content(&String::from_utf8_lossy(&reply)))
        }
        other => Err(error(-32602, &format!("unknown tool {other}"))),
    }
}

impl Endpoint {
    /// Minimal HTTP/1.1 GET with `Connection: close`; returns the body.
    fn get(&self, path: &str) -> Result<Vec<u8>, String> {
        self.request("GET", path, None)
    }

    /// Minimal HTTP/1.1 POST with a JSON body; returns the response body.
    fn post(&self, path: &str, body: &str) -> Result<Vec<u8>, String> {
        self.request("POST", path, Some(body))
    }

    fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<Vec<u8>, String> {
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|e| format!("connect: {e}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| e.to_string())?;
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
            self.host
        );
        if !self.token.is_empty() {
            request.push_str("X-Penwise-Token: ");
            request.push_str(&self.token);
            request.push_str("\r\n");
        }
        if let Some(body) = body {
            let _ = write!(
                request,
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            );
        }
        request.push_str("\r\n");
        if let Some(body) = body {
            request.push_str(body);
        }
        stream
            .write_all(request.as_bytes())
            .map_err(|e| e.to_string())?;
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).map_err(|e| e.to_string())?;
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or("malformed HTTP response")?;
        let head = String::from_utf8_lossy(&raw[..split]);
        if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
            return Err(format!(
                "server replied: {}",
                head.lines().next().unwrap_or("")
            ));
        }
        Ok(raw[split + 4..].to_vec())
    }
}

fn text_content(text: &str) -> serde_json::Value {
    serde_json::json!({ "content": [{ "type": "text", "text": text }] })
}

fn error(code: i32, message: &str) -> serde_json::Value {
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
