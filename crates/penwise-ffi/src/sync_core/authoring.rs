//! Local authoring: every method stamps ops with the calibrated clock, applies them to the replica (the LWW
//! authority) and queues them in the outbox. Nothing here touches the network.

use penwise_core::edit::Edit;
use penwise_proto::{
    AnyOp, Background, NotebookChange, NotebookId, NotebookOp, NotebookPatch, Op, OpId, PageChange,
    PageId, PageKind, PageOp, PagePatch,
};

use super::SyncCore;
use crate::error::SyncError;

/// Everything a page `Create` needs besides its id.
#[derive(Debug, Clone, PartialEq)]
pub struct NewPage {
    pub notebook: NotebookId,
    pub title: String,
    pub position: i64,
    pub kind: PageKind,
    pub background: Background,
}

impl SyncCore {
    /// Validates, applies (replica) and queues an already-stamped op.
    pub(super) fn commit(&mut self, op: &AnyOp) -> Result<(), SyncError> {
        op.validate()
            .map_err(|_| SyncError::BadArgument("op failed validation"))?;
        self.replica.apply(op)?;
        self.outbox.insert(op)?;
        Ok(())
    }

    /// Wraps `edits` (already applied to the page's scene by the caller) into ops in order and queues them.
    /// Returns the op ids. Callers order Adds before Deletes inside a gesture.
    ///
    /// # Errors
    /// `UnknownPage` when the page is not in the replica; replica/validation errors. Edits before a failing
    /// one stay committed.
    pub fn commit_edits(
        &mut self,
        page: PageId,
        edits: &[Edit],
        now_ms: u64,
    ) -> Result<Vec<OpId>, SyncError> {
        if self.replica.page(page).is_none() {
            return Err(SyncError::UnknownPage);
        }
        self.ids.set_now(now_ms);
        let mut ids = Vec::with_capacity(edits.len());
        for edit in edits {
            let op_id = self.ids.op();
            let hlc = self.stamp(now_ms);
            let op = AnyOp::Item(Box::new(Op {
                op_id,
                device: self.device,
                hlc,
                page,
                edit: edit.clone(),
            }));
            self.commit(&op)?;
            ids.push(op_id);
        }
        Ok(ids)
    }

    /// New notebook (`id` pins a deterministic id, e.g. a legacy one; `None` mints one).
    ///
    /// # Errors
    /// Validation/replica errors.
    pub fn create_notebook(
        &mut self,
        id: Option<NotebookId>,
        title: &str,
        position: i64,
        now_ms: u64,
    ) -> Result<NotebookId, SyncError> {
        self.ids.set_now(now_ms);
        let id = id.unwrap_or_else(|| self.ids.notebook());
        let op = AnyOp::Notebook(NotebookOp {
            op_id: self.ids.op(),
            device: self.device,
            hlc: self.stamp(now_ms),
            notebook: id,
            change: NotebookChange::Create {
                title: title.to_owned(),
                position,
            },
        });
        self.commit(&op)?;
        Ok(id)
    }

    /// Notebook registers (`None` = untouched).
    ///
    /// # Errors
    /// Validation/replica errors (`UnknownNotebook` for an id the replica never saw).
    pub fn update_notebook(
        &mut self,
        id: NotebookId,
        patch: NotebookPatch,
        now_ms: u64,
    ) -> Result<(), SyncError> {
        self.ids.set_now(now_ms);
        let op = AnyOp::Notebook(NotebookOp {
            op_id: self.ids.op(),
            device: self.device,
            hlc: self.stamp(now_ms),
            notebook: id,
            change: NotebookChange::Update(patch),
        });
        self.commit(&op)
    }

    /// New page (`id` pins a deterministic id; `None` mints one).
    ///
    /// # Errors
    /// Validation/replica errors.
    pub fn create_page(
        &mut self,
        id: Option<PageId>,
        page: NewPage,
        now_ms: u64,
    ) -> Result<PageId, SyncError> {
        self.ids.set_now(now_ms);
        let id = id.unwrap_or_else(|| self.ids.page());
        let op = AnyOp::Page(PageOp {
            op_id: self.ids.op(),
            device: self.device,
            hlc: self.stamp(now_ms),
            page: id,
            change: PageChange::Create {
                notebook: page.notebook,
                title: page.title,
                position: page.position,
                kind: page.kind,
                background: page.background,
            },
        });
        self.commit(&op)?;
        Ok(id)
    }

    /// Page registers (`None` = untouched).
    ///
    /// # Errors
    /// Validation/replica errors.
    pub fn update_page(
        &mut self,
        id: PageId,
        patch: PagePatch,
        now_ms: u64,
    ) -> Result<(), SyncError> {
        self.ids.set_now(now_ms);
        let op = AnyOp::Page(PageOp {
            op_id: self.ids.op(),
            device: self.device,
            hlc: self.stamp(now_ms),
            page: id,
            change: PageChange::Update(patch),
        });
        self.commit(&op)
    }
}
