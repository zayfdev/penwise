//! Session tests: a real [`Hub`] and session task over channels, an in-memory store, no sockets.

mod adapter;
mod cursor;
mod delivery;
mod fake;
mod frames;
mod handshake;
mod live;
mod socket;
mod watch;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use penwise_proto::{
    AuthToken, ClientKind, ClientMsg, DeviceId, PROTOCOL_VERSION, ServerMsg, decode_server,
    encode_client,
};
use tokio::sync::mpsc;

use super::hub::{Hub, SyncConfig};
use super::outbound::{Outbound, WsOut};
use super::session;
use super::store::SyncStore;
use super::transport::WsIn;
use fake::FakeStore;

pub const TOKEN: &str = "secret";
const WAIT: Duration = Duration::from_secs(5);

pub fn config() -> SyncConfig {
    let mut config = SyncConfig::new(TOKEN.to_owned());
    config.now_ms = Arc::new(|| 1_700_000_123_456);
    config
}

pub fn hello(device: DeviceId, kind: ClientKind, last_seen_seq: u64) -> ClientMsg {
    ClientMsg::Hello {
        protocol_version: PROTOCOL_VERSION,
        device,
        token: AuthToken(TOKEN.to_owned()),
        last_seen_seq,
        client: kind,
    }
}

/// One connected client as the test sees it.
pub struct Rig<S: SyncStore = FakeStore> {
    pub hub: Arc<Hub<S>>,
    pub tx: mpsc::Sender<WsIn>,
    pub rx: mpsc::Receiver<WsOut>,
}

impl Rig {
    /// New session on a new hub.
    pub fn new(store: &FakeStore) -> Self {
        Self::on(Hub::new(store.clone(), config()))
    }
}

impl<S: SyncStore> Rig<S> {
    /// New session on `hub` (several rigs on one hub share the live channel).
    pub fn on(hub: Arc<Hub<S>>) -> Self {
        let (tx, in_rx) = mpsc::channel(16);
        let (out_tx, rx) = mpsc::channel(hub.config.outbound_capacity);
        tokio::spawn(session::run(
            Arc::clone(&hub),
            None,
            in_rx,
            Outbound::new(out_tx),
        ));
        Self { hub, tx, rx }
    }

    pub async fn send(&self, msg: &ClientMsg) {
        self.send_raw(encode_client(msg).unwrap_or_default()).await;
    }

    pub async fn send_raw(&self, bytes: Vec<u8>) {
        self.tx.send(WsIn::Binary(Bytes::from(bytes))).await.ok();
    }

    /// Next frame or close from the session.
    pub async fn next_out(&mut self) -> Option<WsOut> {
        tokio::time::timeout(WAIT, self.rx.recv())
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for the session"))
    }

    /// Next server message (panics on close).
    pub async fn next(&mut self) -> ServerMsg {
        match self.next_out().await {
            Some(WsOut::Frame(frame)) => decode_server(&frame).unwrap_or_else(|e| panic!("{e}")),
            other => panic!("expected a frame, got {other:?}"),
        }
    }

    /// Expects the close and returns its code.
    pub async fn next_close(&mut self) -> u16 {
        match self.next_out().await {
            Some(WsOut::Close { code, .. }) => code,
            other => panic!("expected a close, got {other:?}"),
        }
    }

    /// Hello, then the `Welcome` (returned).
    pub async fn connect(
        &mut self,
        device: DeviceId,
        kind: ClientKind,
        last_seen: u64,
    ) -> ServerMsg {
        self.send(&hello(device, kind, last_seen)).await;
        self.next().await
    }

    /// True when nothing is queued towards the client right now (after letting the session run).
    pub async fn quiet(&mut self) -> bool {
        tokio::time::sleep(Duration::from_millis(60)).await;
        self.rx.try_recv().is_err()
    }
}

/// Every seq delivered by `OpsSince` frames until `more == false`, plus the frames.
pub async fn collect_catch_up<S: SyncStore>(rig: &mut Rig<S>) -> Vec<ServerMsg> {
    let mut frames = Vec::new();
    loop {
        let msg = rig.next().await;
        let done = matches!(msg, ServerMsg::OpsSince { more: false, .. });
        frames.push(msg);
        if done {
            return frames;
        }
    }
}

pub fn seqs(frames: &[ServerMsg]) -> Vec<u64> {
    frames
        .iter()
        .flat_map(|m| match m {
            ServerMsg::OpsSince { ops, .. } => ops.iter().map(|o| o.seq).collect(),
            _ => Vec::new(),
        })
        .collect()
}
