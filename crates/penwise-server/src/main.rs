//! Penwise PC server binary: the op-log store, `/sync`, the JSON API and the static viewer on one port.
//!
//! `penwise-server migrate --db <path>` converts a pre-rewrite (v3) library; the server itself refuses one.

mod cli;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use penwise_server::pairing::CODE_TTL;
use penwise_server::serve::{DB_FILE, ServeError, ServerConfig, serve};
use penwise_server::storage::{Storage, StorageError};
use penwise_server::{auth, urls};

/// Port from the contract; sync, viewer and the API share it.
const DEFAULT_PORT: u16 = 7420;
/// Retention runs once at startup and then this often.
const COMPACT_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("migrate") => return cli::run_migrate(&args[1..]),
        Some("--help" | "-h" | "help") => {
            use std::io::Write as _;
            writeln!(std::io::stdout(), "{USAGE}")?;
            return Ok(());
        }
        Some(other) => return Err(format!("unknown argument {other:?}\n{USAGE}").into()),
        None => {}
    }
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    let host = std::env::var("PENWISE_HOST").unwrap_or_else(|_| "0.0.0.0".to_owned());
    let port = std::env::var("PENWISE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let web = PathBuf::from(std::env::var("PENWISE_WEB").unwrap_or_else(|_| "web/dist".to_owned()));
    let data =
        PathBuf::from(std::env::var("PENWISE_DATA").unwrap_or_else(|_| "penwise-data".to_owned()));
    if !web.join("index.html").is_file() {
        tracing::warn!(dir = %web.display(), "no index.html (set PENWISE_WEB)");
    }

    let token = auth::load_or_create(&data);
    let mut config = ServerConfig::new(&data, token.clone());
    config.web_dir = web;
    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let handle = match serve(listener, config).await {
        Ok(handle) => handle,
        Err(ServeError::Storage(StorageError::NeedsMigration { version })) => {
            return Err(format!(
                "{} is a schema v{version} library from before the rewrite. Convert it first:\n  \
                 penwise-server migrate --db {}",
                data.join(DB_FILE).display(),
                data.join(DB_FILE).display()
            )
            .into());
        }
        Err(error) => return Err(error.into()),
    };
    tracing::info!(addr = %handle.addr, "penwise-server listening");
    // Design harness M0 (§6.1): per-project ports are optional and must never take the main server down.
    let projects = match std::env::var("PENWISE_PROJECTS") {
        Ok(json) if !json.trim().is_empty() => {
            match penwise_server::frames::start_all(&json, &token).await {
                Ok(projects) => projects,
                Err(error) => {
                    tracing::error!(%error, "PENWISE_PROJECTS ignored");
                    Vec::new()
                }
            }
        }
        _ => Vec::new(),
    };
    // Discovery runs helper programs; log from a task so listening never waits for them.
    let logged_token = token.clone();
    tokio::spawn(async move {
        for url in urls::platform_urls(port).await {
            tracing::info!("viewer: {url}/?token={logged_token}");
        }
    });
    tracing::info!("token: {token}  (also in {}/token)", data.display());
    tracing::info!(
        "pairing code: {}  (single use, {} minutes; show it in the viewer)",
        handle.pairing.current(),
        CODE_TTL.as_secs() / 60
    );
    tokio::spawn(compact_forever(handle.storage.clone()));
    tokio::select! {
        () = handle.wait() => {}
        () = shutdown_signal() => tracing::info!("shutting down"),
    }
    for project in projects {
        project.shutdown().await;
    }
    Ok(())
}

const USAGE: &str = "usage: penwise-server            serve (env: PENWISE_PORT, PENWISE_HOST, PENWISE_DATA, PENWISE_WEB,\n                                              PENWISE_PROJECTS, PENWISE_PROJECT_PORT)\n       \
                     penwise-server migrate --db <path> [--dry-run]";

/// Ctrl-c or SIGTERM (systemd stop). Either way `ServerHandle::drop` runs: the listener stops and Chromium is
/// killed synchronously, so no headless browser outlives the server.
async fn shutdown_signal() {
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => signal.recv().await,
            Err(_) => std::future::pending::<Option<()>>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<Option<()>>();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate => {}
    }
}

/// Retention (design §3.3): at startup, then daily.
async fn compact_forever(storage: Storage) {
    loop {
        match storage.compact().await {
            Ok(report) => tracing::info!(?report, "compacted"),
            Err(StorageError::Closed) => return,
            Err(error) => tracing::warn!(%error, "compaction failed"),
        }
        tokio::time::sleep(COMPACT_EVERY).await;
    }
}
