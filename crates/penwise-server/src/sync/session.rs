//! One `/sync` connection: handshake, catch-up, then the select loop over socket, committed ops, live
//! deltas, pings and shutdown. State changes only through decoded, limit-checked client messages.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;

use penwise_proto::{
    ClientKind, ClientMsg, DeviceId, ErrorCode, ItemRef, MAX_FRAME_BYTES, PROTOCOL_VERSION, PageId,
    SeqOp, ServerMsg, decode_client,
};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{Instant, MissedTickBehavior};

use super::handshake::{self, Accepted};
use super::hub::{Hub, LiveEvent};
use super::outbound::{CLOSE_ERROR, CLOSE_NORMAL, Outbound, Stop};
use super::store::{OpFilter, SyncStore};
use super::transport::WsIn;

pub(super) struct Session<S: SyncStore> {
    pub(super) hub: Arc<Hub<S>>,
    pub(super) id: u64,
    pub(super) device: DeviceId,
    pub(super) kind: ClientKind,
    pub(super) out: Outbound,
    /// Last seq delivered (or covered by a snapshot).
    pub(super) cursor: u64,
    pub(super) watch: Option<PageId>,
    /// Browser scope: item ops of the watched page at or below this seq are inside its snapshot.
    pub(super) page_from_seq: u64,
    /// Live strokes this session opened and has not committed or cancelled.
    pub(super) open: BTreeMap<ItemRef, PageId>,
    last_inbound: Instant,
    ping_nonce: u64,
    /// Cursor value last recorded as the device ack, and when.
    acked: (u64, Instant),
}

/// Runs a session on an already-upgraded socket's channels until it ends.
pub async fn run<S: SyncStore>(
    hub: Arc<Hub<S>>,
    peer: Option<IpAddr>,
    mut inbound: mpsc::Receiver<WsIn>,
    out: Outbound,
) {
    let Ok(accepted) = handshake::accept(&hub, peer, &mut inbound, &out).await else {
        return;
    };
    // Subscribe before reading anything: every commit after our reads is then queued for us.
    let mut ops_rx = hub.store.subscribe();
    let mut live_rx = hub.subscribe_live();
    let mut shutdown = hub.shutdown_signal();
    let mut session = Session::new(hub, accepted, out.clone());
    let result = match session.start(accepted).await {
        Ok(()) => {
            session
                .drive(&mut inbound, &mut ops_rx, &mut live_rx, &mut shutdown)
                .await
        }
        Err(stop) => Err(stop),
    };
    session.cancel_open_strokes();
    session.record_disconnect().await;
    match result {
        Err(Stop::Protocol(code, detail)) => {
            let _ = out.send(&ServerMsg::Error { code, detail }).await;
            out.close(CLOSE_ERROR, "protocol error").await;
        }
        Err(Stop::Peer) | Ok(()) => out.close(CLOSE_NORMAL, "bye").await,
    }
}

impl<S: SyncStore> Session<S> {
    fn new(hub: Arc<Hub<S>>, accepted: Accepted, out: Outbound) -> Self {
        let id = hub.next_session_id();
        Self {
            hub,
            id,
            device: accepted.device,
            kind: accepted.kind,
            out,
            cursor: 0,
            watch: None,
            page_from_seq: 0,
            open: BTreeMap::new(),
            last_inbound: Instant::now(),
            ping_nonce: 0,
            acked: (0, Instant::now()),
        }
    }

    /// Records the delivery cursor as the device's ack (design: acks update at `Hello`, periodically while connected, and at disconnect). A
    /// tablet or agent that missed frames in flight reconnects behind the cursor, which at worst turns into a
    /// snapshot. Browsers hold no cursor.
    async fn record_disconnect(&self) {
        self.record_ack("disconnect").await;
    }

    /// Records the delivery cursor as the device's ack (never for browsers or an empty cursor).
    async fn record_ack(&self, when: &str) {
        if self.kind == ClientKind::Browser || self.cursor == 0 {
            return;
        }
        if let Err(error) = self
            .hub
            .store
            .note_device(self.device, self.kind, self.cursor)
            .await
        {
            tracing::warn!(%error, session = self.id, "device ack not recorded at {when}");
        }
    }

    pub(super) fn filter(&self) -> OpFilter {
        super::cursor::filter_for(self.kind, self.watch, self.page_from_seq)
    }

    /// `Welcome`, then the snapshot or log catch-up the client's cursor calls for.
    async fn start(&mut self, hello: Accepted) -> Result<(), Stop> {
        let store = &self.hub.store;
        let status = store.status().await.map_err(|e| Stop::internal(&e))?;
        // Browsers hold no cursor, so they never hold the retention floor back.
        let acked = if hello.kind == ClientKind::Browser {
            status.head_seq
        } else {
            hello.last_seen_seq.min(status.head_seq)
        };
        store
            .note_device(hello.device, hello.kind, acked)
            .await
            .map_err(|e| Stop::internal(&e))?;
        let current_page = store.current_page().await.map_err(|e| Stop::internal(&e))?;
        self.out
            .send(&ServerMsg::Welcome {
                protocol_version: PROTOCOL_VERSION,
                server: store.server_id(),
                device: hello.device,
                head_seq: status.head_seq,
                log_floor: status.log_floor,
                server_now_ms: self.hub.now_ms(),
                current_page,
            })
            .await?;
        self.resync_from(hello.last_seen_seq, self.kind == ClientKind::Browser)
            .await
    }

    async fn drive(
        &mut self,
        inbound: &mut mpsc::Receiver<WsIn>,
        ops_rx: &mut broadcast::Receiver<Arc<[SeqOp]>>,
        live_rx: &mut broadcast::Receiver<LiveEvent>,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<(), Stop> {
        let period = self.hub.config.ping_interval;
        let mut ping = tokio::time::interval_at(Instant::now() + period, period);
        ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                frame = inbound.recv() => match frame {
                    None | Some(WsIn::Closed) => return Ok(()),
                    Some(frame) => {
                        self.last_inbound = Instant::now();
                        self.on_frame(frame).await?;
                    }
                },
                batch = ops_rx.recv() => match batch {
                    Ok(batch) => self.on_batch(&batch).await?,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, session = self.id, "sync session lagged; resyncing from cursor");
                        let from = self.cursor;
                        self.out.send(&ServerMsg::Resync { from_seq: from }).await?;
                        self.resync_from(from, false).await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(Stop::Protocol(ErrorCode::ShuttingDown, "storage closed".to_owned()));
                    }
                },
                event = live_rx.recv() => match event {
                    Ok(event) => self.on_live_event(&event),
                    // Previews are droppable: a lagged receiver just misses some.
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                },
                _ = ping.tick() => self.on_tick().await?,
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Err(Stop::Protocol(ErrorCode::ShuttingDown, "server stopping".to_owned()));
                    }
                }
            }
        }
    }

    async fn on_tick(&mut self) -> Result<(), Stop> {
        if self.last_inbound.elapsed() >= self.hub.config.idle_timeout {
            return Err(Stop::Peer);
        }
        if self.cursor > self.acked.0 && self.acked.1.elapsed() >= self.hub.config.ack_interval {
            self.record_ack("tick").await;
            self.acked = (self.cursor, Instant::now());
        }
        self.ping_nonce = self.ping_nonce.wrapping_add(1);
        self.out
            .send(&ServerMsg::Ping {
                nonce: self.ping_nonce,
            })
            .await
    }

    /// Decodes and applies one frame. Anything undecodable, oversized, textual or out of place ends the
    /// session with `Error` before touching state.
    pub(super) async fn on_frame(&mut self, frame: WsIn) -> Result<(), Stop> {
        let WsIn::Binary(bytes) = frame else {
            return Err(Stop::malformed("text frames are not part of the protocol"));
        };
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(Stop::Protocol(
                ErrorCode::TooLarge,
                format!("frame of {} bytes exceeds {MAX_FRAME_BYTES}", bytes.len()),
            ));
        }
        let msg = decode_client(&bytes).map_err(|e| Stop::malformed(&e.to_string()))?;
        if !msg.within_limits() && !matches!(msg, ClientMsg::StrokeDelta(_)) {
            return Err(Stop::malformed("frame violates a size limit"));
        }
        self.on_msg(msg).await
    }

    async fn on_msg(&mut self, msg: ClientMsg) -> Result<(), Stop> {
        match msg {
            ClientMsg::Hello { .. } => Err(Stop::malformed("Hello after handshake")),
            ClientMsg::PushOps { ops } => self.on_push(ops).await,
            ClientMsg::StrokeDelta(delta) => {
                self.on_delta(delta);
                Ok(())
            }
            ClientMsg::StrokeCancel(cancel) => {
                self.on_cancel(cancel);
                Ok(())
            }
            ClientMsg::SwitchPage { page } => {
                if let Err(error) = self.hub.store.set_current_page(page).await {
                    tracing::warn!(%error, "current page not saved");
                }
                self.publish(
                    None,
                    &ServerMsg::SwitchPage {
                        page,
                        by: self.device,
                    },
                );
                Ok(())
            }
            ClientMsg::Watch { page } => self.on_watch(page).await,
            ClientMsg::Resync { from_seq } => {
                self.out.send(&ServerMsg::Resync { from_seq }).await?;
                self.resync_from(from_seq, self.kind == ClientKind::Browser)
                    .await
            }
            ClientMsg::Ping { nonce } => self.out.send(&ServerMsg::Pong { nonce }).await,
            ClientMsg::Pong { .. } => Ok(()),
        }
    }

    pub(super) fn publish(&self, page: Option<PageId>, msg: &ServerMsg) {
        match penwise_proto::encode_server(msg) {
            Ok(frame) => self.hub.publish_live(LiveEvent {
                origin: self.id,
                page,
                frame: frame.into(),
            }),
            Err(error) => tracing::debug!(%error, "live frame not encodable; dropped"),
        }
    }
}
