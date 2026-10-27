//! Replacing local creations by the server's version, and rebuilding a replica from pending ops.

use penwise_core::edit::Edit;

use super::{Applied, Replica};
use crate::error::ReplicaError;
use crate::notebook::{NotebookChange, PageChange};
use crate::op::AnyOp;

impl Replica {
    /// Applies a *create* (`Add`, page or notebook `Create`) of an entity this replica already holds under
    /// another creation stamp: the existing entity is dropped first, so the incoming (server) version wins
    /// wholesale. For the tablet's legacy import, whose locally stamped `Add`s share ids with the server's own
    /// migration of the same rows. A page keeps its items; an item loses its local stamps and values (ops
    /// still pending are re-applied by the caller). Any other op is applied as [`Replica::apply`].
    ///
    /// # Errors
    /// As [`Replica::apply`].
    pub fn adopt_create(&mut self, op: &AnyOp) -> Result<Applied, ReplicaError> {
        match op {
            AnyOp::Notebook(n) if matches!(n.change, NotebookChange::Create { .. }) => {
                self.catalog.forget_notebook(n.notebook);
            }
            AnyOp::Page(p) if matches!(p.change, PageChange::Create { .. }) => {
                self.catalog.forget_page(p.page);
            }
            AnyOp::Item(i) => {
                if let Edit::Add(item) = &i.edit
                    && let Some(page) = self.pages.get_mut(&i.page)
                {
                    page.forget(item.id)?;
                }
            }
            _ => {}
        }
        self.apply(op)
    }

    /// Resets to an empty replica (cursor 0) and applies `ops` in order; ops that fail are skipped. The way to
    /// drop the effect of refused ops: pass the ops the server has not refused, then resync.
    pub fn rebuild_from(&mut self, ops: &[AnyOp]) {
        *self = Self::new();
        for op in ops {
            let _ = self.apply(op);
        }
    }
}
