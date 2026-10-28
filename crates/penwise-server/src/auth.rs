//! Access token for the LAN socket.
//!
//! One token per data directory, created on first run from `/dev/urandom`. Every route that reveals or changes the
//! page requires it (`?token=…` or `X-Penwise-Token`); the viewer's static assets do not, since they are public code.

use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use axum::extract::{Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

/// `PENWISE_TOKEN` wins (useful in tests); otherwise `${data}/token`, created when missing.
pub fn load_or_create(data_dir: &Path) -> String {
    if let Ok(existing) = std::env::var("PENWISE_TOKEN")
        && !existing.is_empty()
    {
        return existing;
    }
    let path = data_dir.join("token");
    if let Ok(token) = std::fs::read_to_string(&path) {
        let token = token.trim().to_owned();
        if !token.is_empty() {
            // Files written by older builds followed the umask: tighten them.
            restrict(&path);
            return token;
        }
    }
    let token = random_hex();
    if let Err(error) =
        std::fs::create_dir_all(data_dir).and_then(|()| write_private(&path, &token))
    {
        tracing::warn!(%error, "could not persist the access token; it will change on restart");
    }
    token
}

/// Writes `text` to a file only the owner can read (the token is the credential).
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(text.as_bytes())?;
    restrict(path);
    Ok(())
}

/// Best effort: mode `0600` on Unix (a file that already existed keeps its old mode through `open`).
fn restrict(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// 16 random bytes as hex. Falls back to a time/pid mix only if `/dev/urandom` is unreadable.
fn random_hex() -> String {
    let mut bytes = [0_u8; 16];
    let filled = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok();
    if !filled {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
            .unwrap_or(0)
            ^ (u64::from(std::process::id()) << 32);
        bytes[..8].copy_from_slice(&seed.to_le_bytes());
        bytes[8..].copy_from_slice(&seed.rotate_left(17).to_le_bytes());
    }
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"));
    }
    out
}

/// `true` when the request carries the expected token in the query or the `X-Penwise-Token` header.
#[must_use]
pub fn authorized(expected: &str, query_token: Option<&str>, headers: &HeaderMap) -> bool {
    if expected.is_empty() {
        return true;
    }
    if query_token.is_some_and(|t| constant_time_eq(t, expected)) {
        return true;
    }
    headers
        .get("x-penwise-token")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| constant_time_eq(t, expected))
}

/// Length-independent-enough comparison: no early exit on content, and length alone leaks only the size.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 401 with a plain-text reason (the caller answers the request directly).
#[must_use]
pub fn deny() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "PenwiseToken")],
        "missing or wrong token (append ?token=… or set X-Penwise-Token)",
    )
        .into_response()
}

/// The `?token=` query parameter (`/page.png?scale=1&token=…` is how an `<img>` tag authenticates).
#[derive(Debug, Deserialize)]
struct AuthQuery {
    token: Option<String>,
}

/// Middleware: lets the request through only with the token (query or `X-Penwise-Token`), else 401.
pub async fn require(State(token): State<Arc<String>>, request: Request, next: Next) -> Response {
    let query_token = Query::<AuthQuery>::try_from_uri(request.uri())
        .ok()
        .and_then(|Query(q)| q.token);
    if authorized(&token, query_token.as_deref(), request.headers()) {
        next.run(request).await
    } else {
        deny()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    #[test]
    fn the_token_file_is_private_even_when_an_older_one_was_not() {
        let dir = penwise_testkit::TempDir::new("token").expect("tempdir");
        let token = load_or_create(dir.path());
        let path = dir.path().join("token");
        let mode = |p: &Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
        if std::env::var_os("PENWISE_TOKEN").is_none() {
            assert_eq!(0o600, mode(&path));
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
            assert_eq!(token, load_or_create(dir.path()));
            assert_eq!(0o600, mode(&path));
        }
    }
}
