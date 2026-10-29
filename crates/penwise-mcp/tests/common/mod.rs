//! Shared MCP child harness: `penwise-mcp` over stdio JSON-RPC, one request at a time.
#![allow(dead_code, reason = "each test binary uses part of the harness")]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
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

/// The MCP child; calls are blocking, so they run on the blocking pool while the server keeps serving.
#[derive(Clone)]
pub struct Mcp(Arc<Mutex<Session>>);

impl Mcp {
    pub fn start(port: u16) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_penwise-mcp"))
            .env("PENWISE_PORT", port.to_string())
            .env("PENWISE_HOST", "127.0.0.1")
            .env("PENWISE_TOKEN", TOKEN)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn penwise-mcp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self(Arc::new(Mutex::new(Session {
            child,
            stdin,
            stdout,
            next_id: 0,
        })))
    }

    pub async fn rpc(&self, method: &'static str, params: Value) -> Value {
        let session = Arc::clone(&self.0);
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
        let reply = self
            .rpc(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await;
        assert!(reply.get("error").is_none(), "{name}: {reply}");
        reply
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        if Arc::strong_count(&self.0) == 1
            && let Ok(mut session) = self.0.lock()
        {
            let _ = session.child.kill();
            let _ = session.child.wait();
        }
    }
}
