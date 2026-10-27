//! `TestDevice`: a real `/sync` client (tablet-like) assembled from `penwise-proto` parts.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use penwise_proto::{
    AuthToken, ClientKind, ClientMsg, ClockOffset, DeviceId, ErrorCode, HlcClock, OpId, OpRejected,
    Outbox, PROTOCOL_VERSION, PageId, RejectReason, Replica, ServerId, ServerMsg, StrokeCancel,
    StrokeDelta,
};

use crate::socket::{RawSocket, Recv};
use crate::{Clock, IdGen};

/// Connection settings.
#[derive(Clone)]
pub struct DeviceConfig {
    /// `ws://127.0.0.1:port/sync`.
    pub url: String,
    /// Shared token.
    pub token: String,
    /// Client kind sent in `Hello`.
    pub kind: ClientKind,
    /// The device's local wall clock (tests skew it on purpose).
    pub clock: Clock,
}

impl std::fmt::Debug for DeviceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceConfig")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

/// Contents of the last `Welcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Welcome {
    pub server: ServerId,
    pub head_seq: u64,
    pub log_floor: u64,
    pub server_now_ms: u64,
    pub current_page: Option<PageId>,
}

/// Everything the device observed from the server, for assertions.
#[derive(Debug, Default)]
pub struct Events {
    /// Every `seq` delivered through `OpsSince`, in arrival order (including duplicates).
    pub delivered: Vec<u64>,
    /// `(op_ids, seqs)` of every `Ack`.
    pub acks: Vec<(Vec<OpId>, Vec<u64>)>,
    /// Every per-op rejection received.
    pub rejected: Vec<OpRejected>,
    /// Handshake rejections.
    pub rejects: Vec<RejectReason>,
    /// Connection-level errors.
    pub errors: Vec<(ErrorCode, String)>,
    /// Live deltas from other devices.
    pub deltas: Vec<(DeviceId, StrokeDelta)>,
    /// Live cancels from other devices.
    pub cancels: Vec<(DeviceId, StrokeCancel)>,
    /// `Resync` requests from the server.
    pub resyncs: Vec<u64>,
    /// Snapshot sequences completed.
    pub snapshots: usize,
    /// Close code of the last close.
    pub closed: Option<Option<u16>>,
    /// Replica apply failures (logged, never fatal).
    pub apply_errors: Vec<String>,
}

/// A tablet-like device.
#[derive(Debug)]
pub struct TestDevice {
    pub id: DeviceId,
    pub(crate) cfg: DeviceConfig,
    pub hlc: HlcClock,
    pub offset: ClockOffset,
    pub replica: Replica,
    pub outbox: Outbox,
    pub ids: IdGen,
    /// The server this device's replica came from.
    pub pinned: Option<ServerId>,
    pub welcome: Option<Welcome>,
    pub events: Events,
    pub(crate) conn: Option<RawSocket>,
    pub(crate) inflight: BTreeSet<OpId>,
    pub(crate) snapshotting: bool,
    /// Scope of the snapshot in progress (a page snapshot does not move the cursor).
    pub(crate) snapshot_scope: penwise_proto::Scope,
}

impl TestDevice {
    /// New device with a fresh id derived from `seed`.
    #[must_use]
    pub fn new(cfg: DeviceConfig, seed: u64) -> Self {
        let id = IdGen::new(seed).device();
        Self::with_id(cfg, id)
    }

    /// New device with a given id.
    #[must_use]
    pub fn with_id(cfg: DeviceConfig, id: DeviceId) -> Self {
        Self {
            id,
            cfg,
            hlc: HlcClock::new(id),
            offset: ClockOffset::ZERO,
            replica: Replica::new(),
            outbox: Outbox::new(),
            ids: IdGen::for_device(id),
            pinned: None,
            welcome: None,
            events: Events::default(),
            conn: None,
            inflight: BTreeSet::new(),
            snapshotting: false,
            snapshot_scope: penwise_proto::Scope::All,
        }
    }

    /// True while a socket is held.
    #[must_use]
    pub fn is_online(&self) -> bool {
        self.conn.is_some()
    }

    /// Drops the socket; local state is kept.
    pub fn offline(&mut self) {
        self.conn = None;
        self.inflight.clear();
        self.snapshotting = false;
    }

    /// Simulates a process crash and restart. `persist = true` keeps what the real client stores in its local
    /// DB (replica, outbox bytes, clock, offset, server pin); `false` loses everything (fresh install, same id).
    ///
    /// # Panics
    /// If the outbox does not survive its own serialisation (a proto bug the harness must surface).
    #[allow(
        clippy::expect_used,
        reason = "test harness: a broken outbox round trip must fail the test"
    )]
    pub fn crash_restart(&mut self, persist: bool) {
        self.offline();
        self.events = Events::default();
        self.welcome = None;
        if persist {
            let bytes = self.outbox.to_bytes().expect("outbox encodes");
            self.outbox = Outbox::from_bytes(&bytes).expect("outbox decodes");
            self.hlc = HlcClock::resume(self.hlc.last());
        } else {
            *self = Self::with_id(self.cfg.clone(), self.id);
        }
    }

    pub(crate) fn local_now(&self) -> u64 {
        self.offset.adjust((self.cfg.clock)())
    }

    /// Connects, handshakes and settles: catch-up applied, outbox flushed and acked.
    ///
    /// # Errors
    /// Transport failure, a handshake `Reject`, or a settle timeout.
    pub async fn online(&mut self) -> Result<(), String> {
        self.connect().await?;
        self.settle(Duration::from_secs(10)).await
    }

    /// Connect + `Hello` + wait for `Welcome`. A different `ServerId` than the pinned one resets the replica
    /// (outbox kept) and reconnects with cursor 0.
    ///
    /// # Errors
    /// Transport failure or handshake rejection.
    pub async fn connect(&mut self) -> Result<(), String> {
        for _ in 0..2 {
            let mut socket = RawSocket::connect(&self.cfg.url).await?;
            socket
                .send(&ClientMsg::Hello {
                    protocol_version: PROTOCOL_VERSION,
                    device: self.id,
                    token: AuthToken(self.cfg.token.clone()),
                    last_seen_seq: self.replica.head_seq(),
                    client: self.cfg.kind,
                })
                .await?;
            let local_before = (self.cfg.clock)();
            match socket.recv(Duration::from_secs(5)).await {
                Recv::Msg(ServerMsg::Welcome {
                    server,
                    head_seq,
                    log_floor,
                    server_now_ms,
                    current_page,
                    ..
                }) => {
                    self.offset = ClockOffset::from_welcome(server_now_ms, local_before);
                    self.welcome = Some(Welcome {
                        server,
                        head_seq,
                        log_floor,
                        server_now_ms,
                        current_page,
                    });
                    if self.pinned.is_some_and(|p| p != server) {
                        self.replica = Replica::new();
                        for op in self.outbox.pending_ops() {
                            let _ = self.replica.apply(&op);
                        }
                        self.pinned = Some(server);
                        continue;
                    }
                    self.pinned = Some(server);
                    self.conn = Some(socket);
                    self.inflight.clear();
                    return Ok(());
                }
                Recv::Msg(ServerMsg::Reject { reason }) => {
                    self.events.rejects.push(reason);
                    return Err(format!("rejected: {reason:?}"));
                }
                other => return Err(format!("no Welcome: {other:?}")),
            }
        }
        Err("server id kept changing".to_owned())
    }

    /// Pumps until caught up with the `Welcome` head, nothing is pending or in flight, or `timeout`.
    ///
    /// # Errors
    /// On timeout, naming what is still outstanding.
    pub async fn settle(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            self.flush().await;
            let head = self.welcome.map_or(0, |w| w.head_seq);
            let done = self.conn.is_some()
                && !self.snapshotting
                && self.replica.head_seq() >= head
                && self.outbox.is_empty()
                && self.inflight.is_empty();
            if done {
                return Ok(());
            }
            if self.conn.is_none() || Instant::now() >= deadline {
                return Err(format!(
                    "settle failed: online={} head={}/{head} pending={} inflight={} snapshotting={}",
                    self.conn.is_some(),
                    self.replica.head_seq(),
                    self.outbox.len(),
                    self.inflight.len(),
                    self.snapshotting
                ));
            }
            self.pump_one(Duration::from_millis(100)).await;
        }
    }

    /// Pumps until `cond` holds or `timeout`; returns whether it held.
    pub async fn pump_until(&mut self, timeout: Duration, cond: impl Fn(&Self) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while !cond(self) {
            if Instant::now() >= deadline {
                return false;
            }
            self.flush().await;
            self.pump_one(Duration::from_millis(50)).await;
        }
        true
    }

    /// Reads whatever arrives for `quiet` of silence (no flushing, no condition).
    pub async fn pump_quiet(&mut self, quiet: Duration) {
        while self.conn.is_some() && self.pump_one(quiet).await {}
    }
}
