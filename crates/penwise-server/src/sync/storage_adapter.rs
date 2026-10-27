//! `Storage` as a [`SyncStore`].

use std::sync::Arc;

use penwise_proto::{AnyOp, ClientKind, DeviceId, PageId, Scope, SeqOp};
use tokio::sync::{broadcast, mpsc};

use super::store::{
    AppendFailure, LogStatus, OpFilter, OpsPage, OpsRead, SnapshotRead, StoreError, SyncStore,
};
use crate::storage::{self, AppendError, DeviceInfo, Storage, StorageError};

fn store_error(error: &StorageError) -> StoreError {
    StoreError(error.to_string())
}

impl SyncStore for Storage {
    async fn status(&self) -> Result<LogStatus, StoreError> {
        let status = Storage::status(self).await.map_err(|e| store_error(&e))?;
        Ok(LogStatus {
            head_seq: status.head_seq,
            log_floor: status.log_floor,
        })
    }

    fn server_id(&self) -> penwise_proto::ServerId {
        Storage::server_id(self)
    }

    async fn append_ops(
        &self,
        device: DeviceId,
        ops: Vec<AnyOp>,
    ) -> Result<Vec<u64>, AppendFailure> {
        match Storage::append_ops(self, device, ops).await {
            Ok(appended) => Ok(appended.into_iter().map(|a| a.seq).collect()),
            Err(AppendError::Rejected(list)) => Err(AppendFailure::Rejected(
                list.into_iter().map(|r| (r.op_id, r.reason)).collect(),
            )),
            Err(AppendError::Storage(error)) => Err(AppendFailure::Store(store_error(&error))),
        }
    }

    async fn ops_since(
        &self,
        after: u64,
        filter: OpFilter,
        limit: usize,
    ) -> Result<OpsRead, StoreError> {
        let filter = match filter {
            OpFilter::All => storage::OpFilter::All,
            OpFilter::Catalog { page, .. } => storage::OpFilter::CatalogAnd(page),
        };
        match Storage::ops_since(self, after, filter, limit).await {
            Ok(page) => Ok(OpsRead::Page(OpsPage {
                ops: page.ops,
                upto_seq: page.upto_seq,
                head_seq: page.head_seq,
            })),
            Err(StorageError::BelowFloor { .. }) => Ok(OpsRead::BelowFloor),
            Err(error) => Err(store_error(&error)),
        }
    }

    async fn snapshot(&self, scope: Scope) -> Result<SnapshotRead, StoreError> {
        let stream = Storage::snapshot(self, scope)
            .await
            .map_err(|e| store_error(&e))?;
        let (tx, rx) = mpsc::channel(2);
        let mut chunks = stream.chunks;
        tokio::spawn(async move {
            while let Some(chunk) = chunks.recv().await {
                if tx.send(chunk.map_err(|e| store_error(&e))).await.is_err() {
                    return;
                }
            }
        });
        Ok(SnapshotRead {
            head_seq: stream.head_seq,
            chunks: rx,
        })
    }

    fn subscribe(&self) -> broadcast::Receiver<Arc<[SeqOp]>> {
        Storage::subscribe(self)
    }

    async fn note_device(
        &self,
        device: DeviceId,
        kind: ClientKind,
        acked_seq: u64,
    ) -> Result<(), StoreError> {
        let info = DeviceInfo {
            id: device,
            name: format!("{kind:?}").to_lowercase(),
            kind,
        };
        self.touch_device(info, acked_seq)
            .await
            .map_err(|e| store_error(&e))
    }

    async fn current_page(&self) -> Result<Option<PageId>, StoreError> {
        Storage::current_page(self)
            .await
            .map_err(|e| store_error(&e))
    }

    async fn set_current_page(&self, page: PageId) -> Result<(), StoreError> {
        Storage::set_current_page(self, page)
            .await
            .map_err(|e| store_error(&e))
    }
}
