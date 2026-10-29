//! The loopback HTTP client the MCP server talks to the Penwise server with.

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub(crate) const DEFAULT_HOST: &str = "127.0.0.1";
pub(crate) const DEFAULT_PORT: u16 = 7420;

pub(crate) struct Endpoint {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) token: String,
}

/// `PENWISE_TOKEN`, else `$XDG_DATA_HOME/penwise/token` (where the server writes it). Empty = no auth.
pub(crate) fn resolve_token() -> String {
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

impl Endpoint {
    /// Minimal HTTP/1.1 GET with `Connection: close`; returns the body.
    pub(crate) fn get(&self, path: &str) -> Result<Vec<u8>, String> {
        self.request("GET", path, None, "application/json")
    }

    /// Minimal HTTP/1.1 POST with a JSON body; returns the response body.
    pub(crate) fn post(&self, path: &str, body: &str) -> Result<Vec<u8>, String> {
        self.request("POST", path, Some(body.as_bytes()), "application/json")
    }

    /// Minimal HTTP/1.1 DELETE with a JSON body.
    pub(crate) fn delete(&self, path: &str, body: &str) -> Result<Vec<u8>, String> {
        self.request("DELETE", path, Some(body.as_bytes()), "application/json")
    }

    /// `POST` raw bytes (a PNG), used to upload pictures.
    pub(crate) fn post_bytes(
        &self,
        path: &str,
        body: &[u8],
        content_type: &str,
    ) -> Result<Vec<u8>, String> {
        self.request("POST", path, Some(body), content_type)
    }

    pub(crate) fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        content_type: &str,
    ) -> Result<Vec<u8>, String> {
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|e| format!("connect: {e}"))?;
        // A mark bundle opens a headless page and may read handwriting (the server caps OCR at ~20 s), so the
        // client waits longer than a plain page render.
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
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
                "Content-Type: {content_type}\r\nContent-Length: {}\r\n",
                body.len()
            );
        }
        request.push_str("\r\n");
        stream
            .write_all(request.as_bytes())
            .map_err(|e| e.to_string())?;
        if let Some(body) = body {
            stream.write_all(body).map_err(|e| e.to_string())?;
        }
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
