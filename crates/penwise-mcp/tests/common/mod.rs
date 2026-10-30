//! Shared MCP child harness: `penwise-mcp` over stdio JSON-RPC, one request at a time.
#![allow(dead_code, reason = "each test binary uses part of the harness")]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

pub const TOKEN: &str = "mcp-test-token";

pub struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Session {
    fn request(&mut self, method: &str, params: &Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(self.stdin, "{line}").expect("write request");
        self.stdin.flush().expect("flush");
        let mut reply = String::new();
        self.stdout.read_line(&mut reply).expect("read reply");
        let reply: Value = serde_json::from_str(&reply).expect("json reply");
        assert_eq!(reply["id"], id, "reply matches the request");
        reply
    }
}

/// The MCP child; calls are blocking, so they run on the blocking pool while the server keeps serving. Every
/// `tools/call` is counted, so a test can say how many calls a round cost the agent.
#[derive(Clone)]
pub struct Mcp {
    session: Arc<Mutex<Session>>,
    calls: Arc<AtomicUsize>,
}

impl Mcp {
    /// The child without a project (`PENWISE_PROJECT` unset), as an agent outside a linked folder would run it.
    pub fn start(port: u16) -> Self {
        Self::spawn(port, None)
    }

    /// The child as `penwise link` configures it: `PENWISE_PROJECT` names the linked project.
    pub fn start_for(port: u16, project: &str) -> Self {
        Self::spawn(port, Some(project))
    }

    fn spawn(port: u16, project: Option<&str>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_penwise-mcp"));
        command
            .env("PENWISE_PORT", port.to_string())
            .env("PENWISE_HOST", "127.0.0.1")
            .env("PENWISE_TOKEN", TOKEN)
            .env_remove("PENWISE_PROJECT")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        if let Some(project) = project {
            command.env("PENWISE_PROJECT", project);
        }
        let mut child = command.spawn().expect("spawn penwise-mcp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            session: Arc::new(Mutex::new(Session {
                child,
                stdin,
                stdout,
                next_id: 0,
            })),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// How many `tools/call`s the agent has made so far.
    pub fn tool_calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub async fn rpc(&self, method: &'static str, params: Value) -> Value {
        if method == "tools/call" {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
        let session = Arc::clone(&self.session);
        tokio::task::spawn_blocking(move || session.lock().expect("lock").request(method, &params))
            .await
            .expect("join")
    }

    /// `tools/call`; the first content block's text parsed as JSON.
    pub async fn json(&self, name: &str, arguments: Value) -> Value {
        let reply = self.call(name, arguments).await;
        let text = reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: no text in {reply}"));
        serde_json::from_str(text).unwrap_or_else(|e| panic!("{name}: {e} in {text}"))
    }

    pub async fn call(&self, name: &str, arguments: Value) -> Value {
        let reply = self.call_raw(name, arguments).await;
        assert!(reply.get("error").is_none(), "{name}: {reply}");
        assert_ne!(reply["result"]["isError"], true, "{name} failed: {reply}");
        reply
    }

    /// `tools/call` that must fail as a tool: an `isError` result, never a JSON-RPC error. Returns its text.
    pub async fn failure(&self, name: &str, arguments: Value) -> String {
        let reply = self.call_raw(name, arguments).await;
        assert!(
            reply.get("error").is_none(),
            "{name}: a tool error is a result: {reply}"
        );
        assert_eq!(
            reply["result"]["isError"], true,
            "{name} should fail: {reply}"
        );
        reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: no text in {reply}"))
            .to_owned()
    }

    /// `tools/call` whose reply may be an error.
    pub async fn call_raw(&self, name: &str, arguments: Value) -> Value {
        self.rpc(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
        .await
    }

    /// `tools/call`; the first content block's text.
    pub async fn text(&self, name: &str, arguments: Value) -> String {
        let reply = self.call(name, arguments).await;
        reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: no text in {reply}"))
            .to_owned()
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        if Arc::strong_count(&self.session) == 1
            && let Ok(mut session) = self.session.lock()
        {
            let _ = session.child.kill();
            let _ = session.child.wait();
        }
    }
}

/// Standard base64 (padding optional) to bytes; enough to look inside the images the tools return.
pub fn base64_decode(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut buffer, mut bits) = (0_u32, 0_u32);
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from(buffer >> bits).expect("byte"));
            buffer &= (1 << bits) - 1;
        }
    }
    out
}

/// `(width, height)` of a base64 PNG, read from its header.
pub fn png_size(data: &str) -> (u32, u32) {
    let bytes = base64_decode(data);
    assert_eq!(&bytes[1..4], b"PNG", "a PNG");
    let side = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
    (side(16), side(20))
}
