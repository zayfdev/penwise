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
    config.urls = urls::platform_urls(port);
    let urls = config.urls.clone();
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
    for url in &urls {
        tracing::info!("viewer: {url}/?token={token}");
    }
    tracing::info!("token: {token}  (also in {}/token)", data.display());
    tracing::info!(
        "pairing code: {}  (single use, {} minutes; show it in the viewer)",
        handle.pairing.current(),
        CODE_TTL.as_secs() / 60
    );
    tokio::spawn(compact_forever(handle.storage.clone()));
    tokio::select! {
        () = handle.wait() => {}
        _ = tokio::signal::ctrl_c() => tracing::info!("shutting down"),
    }
    Ok(())
}

const USAGE: &str = "usage: penwise-server            serve (env: PENWISE_PORT, PENWISE_HOST, PENWISE_DATA, PENWISE_WEB)\n       \
                     penwise-server migrate --db <path> [--dry-run]";

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
