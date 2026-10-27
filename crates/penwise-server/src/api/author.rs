//! Builds server-authored ops: every op carries the server device and a fresh, increasing stamp, and a batch
//! is appended in one transaction ([`Storage::append_ops`]).

use penwise_core::edit::Edit;
use penwise_proto::{
    AnyOp, NotebookChange, NotebookId, NotebookOp, Op, OpId, PageChange, PageId, PageOp,
};

use super::error::ApiError;
use crate::storage::random::random_u128;
use crate::storage::{Appended, Storage};

/// A batch under construction.
pub(crate) struct Author<'a> {
    storage: &'a Storage,
    ops: Vec<AnyOp>,
}

impl<'a> Author<'a> {
    pub(crate) fn new(storage: &'a Storage) -> Self {
        Self {
            storage,
            ops: Vec::new(),
        }
    }

    fn op_id(&self) -> OpId {
        OpId::from_parts(self.storage.now_ms(), random_u128())
    }

    pub(crate) async fn item(&mut self, page: PageId, edit: Edit) -> Result<(), ApiError> {
        let hlc = self.storage.server_stamp().await?;
        self.ops.push(AnyOp::Item(Box::new(Op {
            op_id: self.op_id(),
            device: self.storage.server_device(),
            hlc,
            page,
            edit,
        })));
        Ok(())
    }

    pub(crate) async fn page(&mut self, page: PageId, change: PageChange) -> Result<(), ApiError> {
        let hlc = self.storage.server_stamp().await?;
        self.ops.push(AnyOp::Page(PageOp {
            op_id: self.op_id(),
            device: self.storage.server_device(),
            hlc,
            page,
            change,
        }));
        Ok(())
    }

    pub(crate) async fn notebook(
        &mut self,
        notebook: NotebookId,
        change: NotebookChange,
    ) -> Result<(), ApiError> {
        let hlc = self.storage.server_stamp().await?;
        self.ops.push(AnyOp::Notebook(NotebookOp {
            op_id: self.op_id(),
            device: self.storage.server_device(),
            hlc,
            notebook,
            change,
        }));
        Ok(())
    }

    /// Appends the whole batch (nothing when it is empty).
    pub(crate) async fn commit(self) -> Result<Vec<Appended>, ApiError> {
        if self.ops.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .storage
            .append_ops(self.storage.server_device(), self.ops)
            .await?)
    }
}
