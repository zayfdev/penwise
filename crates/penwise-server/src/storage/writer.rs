//! The writer thread: the only owner of the write connection. Commands arrive over an mpsc channel and are
//! executed one transaction at a time; replies go back through oneshots, and committed ops are broadcast
//! after the commit.

use std::sync::Arc;

use penwise_proto::{AnyOp, DeviceId, Hlc, HlcClock, PageId, SeqOp, ServerId};
use rusqlite::{Connection, TransactionBehavior, params};
use tokio::sync::{broadcast, oneshot};

use super::apply::{self, FrameEnv};
use super::compact;
use super::error::{AppendError, RestoreError, StorageError};
use super::random::random_u128;
use super::read::ReadPool;
use super::rows::{meta_get, meta_put, meta_put_u64, parse_hlc, to_i64};
use super::{
    Appended, DeviceInfo, FaultPlan, Inner, NowMs, RETENTION_DAYS, Restored, Storage,
    StorageConfig, restore, schema,
};

pub(crate) enum Command {
    Append {
        device: DeviceId,
        ops: Vec<AnyOp>,
        reply: oneshot::Sender<Result<Vec<Appended>, AppendError>>,
    },
    ServerStamp {
        reply: oneshot::Sender<Hlc>,
    },
    Touch {
        device: DeviceInfo,
        acked_seq: u64,
        reply: oneshot::Sender<Result<(), StorageError>>,
    },
    SetCurrentPage {
        page: PageId,
        reply: oneshot::Sender<Result<(), StorageError>>,
    },
    Restore {
        page: PageId,
        to_seq: u64,
        reply: oneshot::Sender<Result<Restored, RestoreError>>,
    },
    Compact {
        reply: oneshot::Sender<Result<compact::Step, StorageError>>,
    },
}

/// State of the writer thread.
pub(crate) struct Writer {
    pub(crate) conn: Connection,
    pub(crate) now_ms: NowMs,
    pub(crate) clock: HlcClock,
    pub(crate) server_device: DeviceId,
    ops: broadcast::Sender<Arc<[SeqOp]>>,
    fault: Option<FaultPlan>,
}

pub(crate) fn start(config: StorageConfig) -> Result<Storage, StorageError> {
    let mut conn = schema::open_writer(&config.path)?;
    schema::ensure_current(&mut conn)?;
    let (server_id, server_device) = load_identity(&mut conn, &config.now_ms)?;
    let clock = match meta_get(&conn, "clock_hlc")? {
        Some(bytes) => HlcClock::resume(parse_hlc(&bytes)?),
        None => HlcClock::new(server_device),
    };
    let (ops, _) = broadcast::channel(config.broadcast_capacity.max(1));
    let (commands, inbox) = std::sync::mpsc::channel();
    let mut writer = Writer {
        conn,
        now_ms: Arc::clone(&config.now_ms),
        clock,
        server_device,
        ops: ops.clone(),
        fault: config.fault,
    };
    std::thread::Builder::new()
        .name("penwise-storage-writer".into())
        .spawn(move || {
            while let Ok(command) = inbox.recv() {
                writer.handle(command);
            }
        })
        .map_err(StorageError::Io)?;
    Ok(Storage {
        inner: Arc::new(Inner {
            commands,
            readers: ReadPool::new(config.path),
            ops,
            server_id,
            server_device,
            now_ms: config.now_ms,
        }),
    })
}

/// Reads or creates `server_id` / `server_device` and the other bootstrap `server_meta` keys.
fn load_identity(
    conn: &mut Connection,
    now_ms: &NowMs,
) -> Result<(ServerId, DeviceId), StorageError> {
    if let (Some(id), Some(device)) = (
        meta_get(conn, "server_id")?,
        meta_get(conn, "server_device")?,
    ) {
        let id =
            ServerId::from_slice(&id).ok_or_else(|| StorageError::Corrupt("server_id".into()))?;
        let device = DeviceId::from_slice(&device)
            .ok_or_else(|| StorageError::Corrupt("server_device".into()))?;
        return Ok((id, device));
    }
    let now = now_ms();
    let id = ServerId::from_parts(now, random_u128());
    let device = DeviceId::from_parts(now, random_u128());
    // One transaction: a crash in between must not leave a `server_id` without its device (or the reverse).
    let tx = conn.transaction()?;
    seed_meta(&tx, id, device, now)?;
    tx.commit()?;
    Ok((id, device))
}

/// Writes the bootstrap `server_meta` rows (also used by the v3 migration).
pub(crate) fn seed_meta(
    conn: &Connection,
    id: ServerId,
    device: DeviceId,
    now: u64,
) -> Result<(), StorageError> {
    meta_put(conn, "server_id", &id.0)?;
    meta_put(conn, "server_device", &device.0)?;
    meta_put_u64(conn, "created_ms", now)?;
    meta_put_u64(conn, "log_floor", 0)?;
    meta_put_u64(conn, "retention_days", RETENTION_DAYS)?;
    Ok(())
}

impl Writer {
    fn handle(&mut self, command: Command) {
        match command {
            Command::Append { device, ops, reply } => {
                let _ = reply.send(self.append(device, &ops));
            }
            Command::ServerStamp { reply } => {
                let stamp = self.clock.send((self.now_ms)());
                let _ = reply.send(stamp);
            }
            Command::Touch {
                device,
                acked_seq,
                reply,
            } => {
                let _ = reply.send(self.touch(&device, acked_seq));
            }
            Command::SetCurrentPage { page, reply } => {
                let _ = reply.send(meta_put(&self.conn, "current_page", &page.0));
            }
            Command::Restore {
                page,
                to_seq,
                reply,
            } => {
                let _ = reply.send(restore::run(self, page, to_seq));
            }
            Command::Compact { reply } => {
                let _ = reply.send(compact::step(&mut self.conn, (self.now_ms)()));
            }
        }
    }

    /// One transaction: apply `ops`, persist the server clock, commit; broadcast only after the commit.
    pub(crate) fn append(
        &mut self,
        device: DeviceId,
        ops: &[AnyOp],
    ) -> Result<Vec<Appended>, AppendError> {
        let now_ms = (self.now_ms)();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let env = FrameEnv {
            device,
            now_ms,
            fault: self.fault.as_ref(),
        };
        let result = apply::append_frame(&tx, &env, ops)?;
        let mut clock = self.clock;
        if let Some(max) = result.max_hlc {
            // Remote stamps were skew-checked, so `recv` only fails for a stamp we already refused.
            let _ = clock.recv(max, now_ms);
            super::rows::meta_put(&tx, "clock_hlc", &clock.last().to_bytes())?;
        }
        tx.commit()?;
        self.clock = clock;
        if !result.new_ops.is_empty() {
            // No receivers is fine: sessions subscribe on connect.
            let _ = self.ops.send(result.new_ops.into());
        }
        Ok(result.appended)
    }

    fn touch(&self, device: &DeviceInfo, acked_seq: u64) -> Result<(), StorageError> {
        let now = to_i64((self.now_ms)());
        let kind = match device.kind {
            penwise_proto::ClientKind::Tablet => 0_i64,
            penwise_proto::ClientKind::Browser => 1,
            penwise_proto::ClientKind::Agent => 2,
        };
        self.conn.execute(
            "INSERT INTO devices (id, name, kind, created_ms, last_seen_ms, acked_seq)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, kind = excluded.kind,
                 last_seen_ms = excluded.last_seen_ms,
                 acked_seq = MAX(devices.acked_seq, excluded.acked_seq)",
            params![&device.id.0[..], device.name, kind, now, to_i64(acked_seq)],
        )?;
        Ok(())
    }
}
