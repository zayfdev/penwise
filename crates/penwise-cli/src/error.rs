//! The CLI's errors: every variant carries what a person needs to fix it.

use std::path::PathBuf;

/// A `penwise` command failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Usage(String),
    #[error("no .penwise.json in {} — run `penwise link` first", .0.display())]
    NotLinked(PathBuf),
    #[error("{0}")]
    Undetected(String),
    #[error(
        "cannot reach the Penwise server at {base}: {detail}\n  \
             start it with `systemctl --user start penwise.service`, or run `penwise`"
    )]
    Unreachable { base: String, detail: String },
    #[error("{request} failed: {base} answered {status} ({detail})")]
    Refused {
        request: String,
        base: String,
        status: u16,
        detail: String,
    },
    #[error("cannot build the HTTP client: {0}")]
    Client(String),
    #[error("{0}")]
    Launch(String),
}
