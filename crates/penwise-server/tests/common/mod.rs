//! Shared harness for the `/sync` scenarios: an in-process server (127.0.0.1:0, temp data dir, injectable clock,
//! optional fault injection) plus convergence assertions. Devices come from `penwise-testkit`.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use penwise_proto::ClientKind;
use penwise_server::serve::{ServerConfig, ServerHandle, serve};
use penwise_server::storage::{FaultPlan, Storage};
use penwise_server::sync::SyncConfig;
use penwise_testkit::check::{self, Fingerprint};
use penwise_testkit::{Clock, DeviceConfig, TempDir, TestDevice};

pub const TOKEN: &str = "test-token";

/// Knobs a scenario may turn.
#[derive(Clone, Default)]
pub struct Opts {
    pub broadcast_capacity: Option<usize>,
    pub live_capacity: Option<usize>,
    pub outbound_capacity: Option<usize>,
    pub fault: Option<FaultPlan>,
    /// Reuse this data dir (restart / swapped DB scenarios).
    pub data: Option<Arc<TempDir>>,
}

/// An in-process server.
pub struct TestServer {
    pub addr: SocketAddr,
    pub data: Arc<TempDir>,
    pub clock: Arc<AtomicU64>,
    pub storage: Storage,
    handle: Option<ServerHandle>,
    opts: Opts,
}

impl TestServer {
    pub async fn start() -> Self {
        Self::start_with(Opts::default()).await
    }

    pub async fn start_with(opts: Opts) -> Self {
        Self::bind(
            "127.0.0.1:0".parse().expect("addr"),
            opts,
            Arc::new(AtomicU64::new(real_now())),
        )
        .await
    }

    async fn bind(addr: SocketAddr, opts: Opts, clock: Arc<AtomicU64>) -> Self {
        let data = opts
            .data
            .clone()
            .unwrap_or_else(|| Arc::new(TempDir::new("srv").expect("tempdir")));
        let now: Clock = {
            let clock = Arc::clone(&clock);
            Arc::new(move || clock.load(Ordering::SeqCst))
        };
        let mut cfg = ServerConfig::new(data.path(), TOKEN);
        cfg.clock = now;
        cfg.broadcast_capacity = opts.broadcast_capacity;
        cfg.fault.clone_from(&opts.fault);
        let mut sync = SyncConfig::new(TOKEN.to_owned());
        if let Some(cap) = opts.live_capacity {
            sync.live_capacity = cap;
        }
        if let Some(cap) = opts.outbound_capacity {
            sync.outbound_capacity = cap;
        }
        cfg.sync = Some(sync);
        let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
        let handle = serve(listener, cfg).await.expect("server starts");
        Self {
            addr: handle.addr,
            data,
            clock,
            storage: handle.storage.clone(),
            handle: Some(handle),
            opts,
        }
    }

    pub fn url(&self) -> String {
        format!("ws://{}/sync", self.addr)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data.path().join("penwise.db")
    }

    /// Server wall clock now (ms).
    pub fn now(&self) -> u64 {
        self.clock.load(Ordering::SeqCst)
    }

    pub fn advance(&self, ms: u64) {
        self.clock.fetch_add(ms, Ordering::SeqCst);
    }

    /// Device config whose local clock is the server's (`skew_ms` added; may be negative via wrapping math).
    pub fn config(&self, kind: ClientKind, skew_ms: i64) -> DeviceConfig {
        let clock = Arc::clone(&self.clock);
        DeviceConfig {
            url: self.url(),
            token: TOKEN.to_owned(),
            kind,
            clock: Arc::new(move || clock.load(Ordering::SeqCst).saturating_add_signed(skew_ms)),
        }
    }

    pub fn tablet(&self, seed: u64) -> TestDevice {
        TestDevice::new(self.config(ClientKind::Tablet, 0), seed)
    }

    /// Stops sessions and storage, then reopens the same data dir on the same port.
    pub async fn restart(&mut self) {
        self.restart_with(self.opts.clone()).await;
    }

    pub async fn restart_with(&mut self, opts: Opts) {
        let addr = self.addr;
        let clock = Arc::clone(&self.clock);
        let data = Arc::clone(&self.data);
        self.shutdown().await;
        let mut opts = opts;
        opts.data = Some(data);
        let fresh = Self::bind(addr, opts, clock).await;
        *self = fresh;
    }

    /// Restarts on the same port with an empty data dir (a different server identity).
    pub async fn restart_on_fresh_db(&mut self) {
        let addr = self.addr;
        let clock = Arc::clone(&self.clock);
        self.shutdown().await;
        let mut opts = self.opts.clone();
        opts.data = None;
        *self = Self::bind(addr, opts, clock).await;
    }

    /// Ends every session and the listener, and gives the writer thread time to release the DB.
    pub async fn shutdown(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.shutdown().await;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    /// Current head of the log (read straight from the DB file).
    pub fn head(&self) -> u64 {
        check::db_head(&self.db_path()).expect("db head")
    }

    /// Asserts tables == log fold and returns the server's fingerprint.
    pub fn fingerprint(&self) -> Fingerprint {
        check::tables_equal_fold(&self.db_path()).unwrap_or_else(|e| panic!("{e}"))
    }
}

pub fn real_now() -> u64 {
    penwise_testkit::system_clock()()
}

/// Brings every device online and waits until each replica reached the server head, then asserts that all
/// replicas and the server tables hold the identical state. Returns that state.
pub async fn converge(server: &TestServer, devices: &mut [&mut TestDevice]) -> Fingerprint {
    for d in devices.iter_mut() {
        if !d.is_online() {
            d.online().await.unwrap_or_else(|e| panic!("online: {e}"));
        }
    }
    for d in devices.iter_mut() {
        d.settle(Duration::from_secs(10))
            .await
            .unwrap_or_else(|e| panic!("settle: {e}"));
    }
    let head = server.head();
    for d in devices.iter_mut() {
        assert!(
            d.pump_until(Duration::from_secs(10), |d| d.replica.head_seq() >= head)
                .await,
            "device {:?} stuck at {} of {head}",
            d.id,
            d.replica.head_seq()
        );
        assert!(
            d.events.apply_errors.is_empty(),
            "apply errors: {:?}",
            d.events.apply_errors
        );
    }
    let expected = server.fingerprint();
    for d in devices.iter() {
        assert_eq!(
            check::fingerprint(&d.replica),
            expected,
            "device {:?} diverged from the server",
            d.id
        );
    }
    expected
}

/// A short wavy polyline: `n` points from `(x, y)` rightwards.
pub fn line(x: f32, y: f32, n: u16) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| [x + f32::from(i) * 6.0, y + (f32::from(i) * 0.5).sin() * 8.0])
        .collect()
}
