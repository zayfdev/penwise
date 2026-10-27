//! Genesis op emitter: stamps from the server device clock at the migration time and applies each op through
//! the live writer's materialization.

use penwise_proto::{AnyOp, DeviceId, HlcClock, OpId};
use rusqlite::{Connection, Transaction};

use crate::storage::apply::{self, Done, Fail};
use crate::storage::error::StorageError;
use crate::storage::random::random_u128;

pub(super) fn fail(message: impl Into<String>) -> StorageError {
    StorageError::Migration(message.into())
}

pub(super) struct Emitter<'a> {
    pub(super) tx: &'a Transaction<'a>,
    pub(super) clock: HlcClock,
    pub(super) device: DeviceId,
    pub(super) t0: u64,
    pub(super) batch: u64,
    pub(super) seq: u64,
    pub(super) items: usize,
    pub(super) fault_after_items: Option<usize>,
}

impl Emitter<'_> {
    pub(super) fn stamp(&mut self) -> penwise_proto::Hlc {
        self.clock.send(self.t0)
    }

    pub(super) fn op_id(&self) -> OpId {
        OpId::from_parts(self.t0, random_u128())
    }

    pub(super) fn emit(&mut self, op: &AnyOp) -> Result<(), StorageError> {
        op.validate()
            .map_err(|e| fail(format!("genesis op invalid: {e}")))?;
        match apply::materialize(self.tx, op, self.seq) {
            Ok(Done::Applied) => {}
            Ok(Done::Existing(_)) => return Err(fail("genesis op collided with an existing item")),
            Err(Fail::Rejected(reason)) => {
                return Err(fail(format!("genesis op refused: {reason}")));
            }
            Err(Fail::Db(error)) => return Err(error),
        }
        apply::log_op(self.tx, op, self.seq, self.batch, self.t0).map_err(|f| match f {
            Fail::Db(e) => e,
            Fail::Rejected(r) => fail(r.to_string()),
        })?;
        self.seq += 1;
        Ok(())
    }

    pub(super) fn after_item(&mut self) -> Result<(), StorageError> {
        self.items += 1;
        if self.fault_after_items == Some(self.items) {
            return Err(StorageError::Fault);
        }
        Ok(())
    }
}

pub(super) fn count(conn: &Connection, table: &str) -> Result<u64, StorageError> {
    let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
    Ok(u64::try_from(n).unwrap_or(0))
}
