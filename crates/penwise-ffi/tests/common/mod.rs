//! Headless tablet: `SyncCore` + `SceneHandle` driven over a real WebSocket against an in-process server,
//! exactly the glue the Kotlin `SyncService` will have (frames between socket and core, nothing else).
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use penwise_core::item::{ItemId, LayerSet};
use penwise_ffi::scene_handle::{SAMPLE_FLOATS, SceneHandle, StrokeSpec};
use penwise_ffi::sync_core::{SyncCore, decode_changes};
use penwise_proto::{ClientKind, DeviceId, PageId, ServerMsg, decode_server};
use penwise_server::serve::{ServerConfig, ServerHandle, serve};
use penwise_testkit::check::{self, Fingerprint};
use penwise_testkit::{TempDir, system_clock};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub const TOKEN: &str = "test-token";

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct TestServer {
    pub addr: SocketAddr,
    pub data: TempDir,
    handle: Option<ServerHandle>,
}

impl TestServer {
    pub async fn start() -> Self {
        let data = TempDir::new("ffi-srv").expect("tempdir");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let handle = serve(listener, ServerConfig::new(data.path(), TOKEN))
            .await
            .expect("server starts");
        Self {
            addr: handle.addr,
            data,
            handle: Some(handle),
        }
    }

    pub fn url(&self) -> String {
        format!("ws://{}/sync", self.addr)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data.path().join("penwise.db")
    }

    pub fn head(&self) -> u64 {
        check::db_head(&self.db_path()).expect("db head")
    }

    pub fn fingerprint(&self) -> Fingerprint {
        check::tables_equal_fold(&self.db_path()).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Replaces the server by an empty one (new identity) on the same port.
    pub async fn restart_fresh(&mut self) {
        let addr = self.addr;
        if let Some(handle) = self.handle.take() {
            handle.shutdown().await;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        let data = TempDir::new("ffi-srv2").expect("tempdir");
        let listener = tokio::net::TcpListener::bind(addr).await.expect("rebind");
        let handle = serve(listener, ServerConfig::new(data.path(), TOKEN))
            .await
            .expect("server starts");
        self.data = data;
        self.handle = Some(handle);
    }

    pub async fn shutdown(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.shutdown().await;
        }
    }
}

pub struct Tablet {
    pub core: SyncCore,
    pub scene: Option<SceneHandle>,
    pub page: Option<PageId>,
    ws: Option<Ws>,
    url: String,
    welcome_head: u64,
    pub apply_errors: u32,
    pub reconnects: u32,
}

pub fn now() -> u64 {
    system_clock()()
}

pub fn device(seed: u64) -> DeviceId {
    DeviceId::from_parts(now(), u128::from(seed) << 20 | 0x5eed)
}

impl Tablet {
    pub fn new(server: &TestServer, seed: u64) -> Self {
        Self::with_core(server, SyncCore::new(device(seed), seed))
    }

    pub fn with_core(server: &TestServer, core: SyncCore) -> Self {
        Self {
            core,
            scene: None,
            page: None,
            ws: None,
            url: server.url(),
            welcome_head: 0,
            apply_errors: 0,
            reconnects: 0,
        }
    }

    pub fn is_online(&self) -> bool {
        self.ws.is_some()
    }

    pub fn offline(&mut self) {
        self.ws = None;
        self.core.go_offline();
    }

    pub fn open_page(&mut self, page: PageId) {
        self.scene = Some(SceneHandle::open(&self.core, page, 7).expect("page exists"));
        self.page = Some(page);
    }

    fn scene_mut(&mut self) -> &mut SceneHandle {
        self.scene.as_mut().expect("page open")
    }

    async fn send(&mut self, frame: Vec<u8>) {
        if let Some(ws) = self.ws.as_mut() {
            let _ = ws.send(Message::Binary(frame.into())).await;
        }
    }

    /// Pushes whatever the outbox may send now.
    pub async fn flush(&mut self) {
        while let Some(frame) = self.core.poll_push().expect("poll") {
            self.send(frame).await;
        }
    }

    /// Connects, handshakes and settles (catch-up applied, outbox flushed and acked).
    pub async fn online(&mut self) {
        for _ in 0..3 {
            let (ws, _) = tokio_tungstenite::connect_async(&self.url)
                .await
                .expect("connect");
            self.ws = Some(ws);
            let hello = self
                .core
                .hello(TOKEN, ClientKind::Tablet, now())
                .expect("hello");
            self.send(hello).await;
            self.welcome_head = 0;
            let reconnects = self.reconnects;
            self.settle().await;
            if self.reconnects == reconnects {
                return;
            }
        }
        panic!("server identity kept changing");
    }

    /// Reads one frame (or times out); returns whether one arrived.
    pub async fn pump_one(&mut self, timeout: Duration) -> bool {
        let Some(ws) = self.ws.as_mut() else {
            return false;
        };
        let Ok(next) = tokio::time::timeout(timeout, ws.next()).await else {
            return false;
        };
        let frame = match next {
            Some(Ok(Message::Binary(bytes))) => bytes.to_vec(),
            Some(Ok(_)) => return true,
            _ => {
                self.offline();
                return false;
            }
        };
        if let Ok(ServerMsg::Welcome { head_seq, .. }) = decode_server(&frame) {
            self.welcome_head = head_seq;
        }
        let outcome = self.core.receive(&frame, now()).expect("receive");
        self.apply_errors += outcome.apply_errors;
        if outcome.reconnect {
            self.reconnects += 1;
            self.ws = None;
        }
        for reply in &outcome.replies {
            self.send(reply.clone()).await;
        }
        let changes = decode_changes(&outcome.encode()).expect("outcome blob decodes");
        assert_eq!(changes, outcome.changes, "blob round trip");
        if let Some(scene) = self.scene.as_mut() {
            scene.refresh(&self.core, &changes).expect("refresh");
        }
        self.flush().await;
        true
    }

    /// Pumps until the session is settled at the `Welcome` head (or fails after 10 s).
    pub async fn settle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.flush().await;
            if self.ws.is_none() {
                return;
            }
            if self.core.is_settled() && self.core.head_seq() >= self.welcome_head {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "settle timeout: head {}/{} pending {}",
                self.core.head_seq(),
                self.welcome_head,
                self.core.outbox().len()
            );
            self.pump_one(Duration::from_millis(100)).await;
        }
    }

    /// Pumps until the replica reaches `head` (remote changes arriving).
    pub async fn catch_up_to(&mut self, head: u64) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.core.head_seq() < head {
            assert!(
                Instant::now() < deadline,
                "stuck at {} of {head}",
                self.core.head_seq()
            );
            self.pump_one(Duration::from_millis(50)).await;
        }
    }

    /// Draws a stroke through `points`, commits it and returns its id.
    pub async fn draw(&mut self, points: &[[f32; 2]]) -> ItemId {
        let page = self.page.expect("page open");
        let t = now();
        let scene = self.scene_mut();
        let id = scene.new_item_id(t);
        let (mut samples, mut times) = (Vec::new(), Vec::new());
        for (i, p) in (0_i32..).zip(points) {
            samples.extend([p[0], p[1], 0.5, 0.0, 0.0]);
            times.push(i * 8_333);
        }
        assert_eq!(samples.len(), times.len() * SAMPLE_FLOATS);
        let edits = scene
            .add_stroke(&StrokeSpec {
                id,
                preset: 0,
                argb: 0xFF00_0000,
                scale: 1.0,
                seed: u64::from(id.0[15]) << 8 | 1,
                samples: &samples,
                times: &times,
            })
            .expect("add_stroke");
        self.core.commit_edits(page, &edits, t).expect("commit");
        self.flush().await;
        id
    }

    /// One eraser gesture over `path`; returns the number of edits.
    pub async fn erase(&mut self, path: &[[f32; 2]], radius: f32, precise: bool) -> usize {
        let page = self.page.expect("page open");
        let scene = self.scene_mut();
        scene.begin_gesture();
        let edits = scene
            .erase(path, radius, precise, LayerSet::ALL)
            .expect("erase");
        scene.end_gesture();
        self.core.commit_edits(page, &edits, now()).expect("commit");
        self.flush().await;
        edits.len()
    }

    pub async fn undo(&mut self) -> usize {
        let page = self.page.expect("page open");
        let edits = self.scene_mut().undo().expect("undo");
        self.core.commit_edits(page, &edits, now()).expect("commit");
        self.flush().await;
        edits.len()
    }

    pub async fn redo(&mut self) -> usize {
        let page = self.page.expect("page open");
        let edits = self.scene_mut().redo().expect("redo");
        self.core.commit_edits(page, &edits, now()).expect("commit");
        self.flush().await;
        edits.len()
    }

    /// Live item count of the handle's scene.
    pub fn live(&self) -> usize {
        self.scene
            .as_ref()
            .expect("page open")
            .render_order(LayerSet::ALL)
            .len()
    }

    /// The handle's scene must equal the replica's page scene (the mirror invariant).
    pub fn assert_mirror(&self) {
        let page = self.page.expect("page open");
        let handle = self.scene.as_ref().expect("page open").scene();
        let replica = self.core.replica().page(page).expect("page").scene();
        let ids = replica.render_order(LayerSet::ALL);
        assert_eq!(handle.render_order(LayerSet::ALL), ids, "paint order");
        for id in ids {
            assert_eq!(
                penwise_ffi::render_floats::item_floats(handle, id),
                penwise_ffi::render_floats::item_floats(replica, id),
                "geometry of {id:?}"
            );
        }
    }
}

/// A short wavy polyline from `(x, y)` rightwards.
pub fn line(x: f32, y: f32, n: u16) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| [x + f32::from(i) * 6.0, y + (f32::from(i) * 0.5).sin() * 8.0])
        .collect()
}

/// Brings every tablet online, settles, then asserts replicas, scenes and the server tables are identical.
pub async fn converge(server: &TestServer, tablets: &mut [&mut Tablet]) -> Fingerprint {
    for t in tablets.iter_mut() {
        if !t.is_online() {
            t.online().await;
        }
    }
    for t in tablets.iter_mut() {
        t.settle().await;
    }
    let head = server.head();
    for t in tablets.iter_mut() {
        t.catch_up_to(head).await;
        assert_eq!(t.apply_errors, 0, "apply errors");
    }
    let expected = server.fingerprint();
    for t in tablets.iter() {
        assert_eq!(
            check::fingerprint(t.core.replica()),
            expected,
            "replica diverged"
        );
        if t.scene.is_some() {
            t.assert_mirror();
        }
    }
    expected
}
