//! Spotting dev-server HMR traffic inside the proxy's WebSocket tunnels.
//!
//! The proxy copies an upgraded connection byte for byte. [`Tap`] watches the server → browser half of an *HMR
//! socket* ([`should_tap`]) for the messages that mean "the page just changed": Vite's `update` and
//! `full-reload`, Next's `built`, `reloadPage` and `serverComponentChanges`. Server frames are never masked and
//! these dev servers do not compress them, so a substring scan with a carried tail is enough; no frame parser is
//! needed. Binary or compressed traffic never matches, so a change it carries is missed until the next event or
//! reload; a missed event costs freshness, never correctness.
//!
//! Only HMR sockets are tapped (Vite's carries the `vite-hmr` subprotocol, Next's lives under `/_next/` with
//! `hmr` in the path), and never one opened by the server's own capture page ([`crate::inspect::CAPTURE_HEADER`]):
//! a capture is not a reader of the page, and a dev server that talks to every new connection (Next compiling a
//! route on first load) would otherwise make each capture trigger the next.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::broadcast;

/// What a message must contain to count as a page change. Messages a dev server sends when a page *connects*
/// (Vite's `connected`, Next's `sync`) are deliberately absent: opening a page is not a change.
const MARKERS: [&[u8]; 5] = [
    br#""type":"update""#,
    br#""type":"full-reload""#,
    br#""action":"built""#,
    br#""action":"reloadPage""#,
    br#""action":"serverComponentChanges""#,
];

/// Whether the upgrade request `path`/`headers` opens an HMR socket that a person's page holds, not a capture's.
pub(super) fn should_tap(path: &str, headers: &axum::http::HeaderMap) -> bool {
    if headers.contains_key(crate::inspect::CAPTURE_HEADER) {
        return false;
    }
    let vite = headers
        .get_all(axum::http::header::SEC_WEBSOCKET_PROTOCOL)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|protocol| protocol.trim() == "vite-hmr");
    vite || (path.starts_with("/_next/") && path.contains("hmr"))
}

/// Longest marker, so a marker split across two reads is still seen.
const OVERLAP: usize = 32;

/// Sees every chunk of one tunnel direction and reports page changes to a feed.
#[derive(Debug)]
pub(super) struct Tap {
    feed: broadcast::Sender<()>,
    tail: Vec<u8>,
}

impl Tap {
    pub(super) fn new(feed: broadcast::Sender<()>) -> Self {
        Self {
            feed,
            tail: Vec::with_capacity(OVERLAP),
        }
    }

    /// Scans `chunk` (after the carried tail) and sends one feed event when it holds a marker.
    pub(super) fn see(&mut self, chunk: &[u8]) {
        self.tail.extend_from_slice(chunk);
        if MARKERS.iter().any(|marker| contains(&self.tail, marker)) {
            // No receivers is fine: the stills service may not be listening yet.
            let _ = self.feed.send(());
            self.tail.clear();
            return;
        }
        let keep = self.tail.len().saturating_sub(OVERLAP);
        self.tail.drain(..keep);
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// `copy` from `reader` to `writer` that also feeds every chunk to `tap`; shuts the writer down at EOF.
///
/// # Errors
/// Any read or write error of the tunnel.
pub(super) async fn copy_tapped<R, W>(
    reader: &mut R,
    writer: &mut W,
    mut tap: Option<Tap>,
) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0_u8; 16 * 1024];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            writer.shutdown().await?;
            return Ok(());
        }
        let chunk = &buffer[..read];
        if let Some(tap) = tap.as_mut() {
            tap.see(chunk);
        }
        writer.write_all(chunk).await?;
    }
}

#[cfg(test)]
mod tests;
