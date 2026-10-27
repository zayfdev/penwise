//! This client's own undo/redo and the local-edit commit path (stamp → replica → outbox).

use penwise_core::edit::undo::DEFAULT_DEPTH;
use penwise_core::edit::{Edit, UndoStack};
use penwise_proto::{AnyOp, Op, PageId};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use super::{Viewer, parse_id, whole_ms};
use crate::error::WasmError;

#[wasm_bindgen]
impl Viewer {
    /// Undoes this client's newest still-valid step on `page`; returns how many edits were emitted (0 = nothing
    /// to undo). The edits are applied and queued like any op (send `poll_push()` next).
    ///
    /// # Errors
    /// `JsError` before `identify` or for a malformed/unknown page.
    pub fn undo(&mut self, page: &str, now_ms: f64) -> Result<u32, JsError> {
        Ok(self.step_undo(parse_id(page)?, false, whole_ms(now_ms))?)
    }

    /// Mirror of [`Viewer::undo`].
    ///
    /// # Errors
    /// As [`Viewer::undo`].
    pub fn redo(&mut self, page: &str, now_ms: f64) -> Result<u32, JsError> {
        Ok(self.step_undo(parse_id(page)?, true, whole_ms(now_ms))?)
    }

    /// True when an undo step is stored for `page`.
    #[must_use]
    pub fn can_undo(&self, page: &str) -> bool {
        self.stack(page).is_some_and(UndoStack::can_undo)
    }

    /// True when a redo step is stored for `page`.
    #[must_use]
    pub fn can_redo(&self, page: &str) -> bool {
        self.stack(page).is_some_and(UndoStack::can_redo)
    }
}

impl Viewer {
    fn stack(&self, page: &str) -> Option<&UndoStack> {
        let page: PageId = parse_id(page).ok()?;
        self.author.as_ref()?.undo.get(&page)
    }

    /// One undo/redo step on `page`; the emitted edits are committed without being recorded.
    ///
    /// # Errors
    /// [`WasmError::NotIdentified`], [`WasmError::UnknownPage`] or a replica/scene refusal.
    pub fn step_undo(&mut self, page: PageId, redo: bool, now_ms: u64) -> Result<u32, WasmError> {
        let author = self.author.as_mut().ok_or(WasmError::NotIdentified)?;
        let scene = self
            .replica
            .page(page)
            .ok_or(WasmError::UnknownPage)?
            .scene();
        let stack = author
            .undo
            .entry(page)
            .or_insert_with(|| UndoStack::new(DEFAULT_DEPTH));
        let edits = if redo {
            stack.redo(scene)
        } else {
            stack.undo(scene)
        };
        let count = u32::try_from(edits.len()).unwrap_or(u32::MAX);
        self.commit_edits(page, &edits, now_ms)?;
        Ok(count)
    }

    /// Records `edits` as one undo step (on a scratch copy of the scene: the replica applies them below),
    /// then commits them.
    pub(super) fn record_local(
        &mut self,
        page: PageId,
        edits: &[Edit],
        now_ms: u64,
    ) -> Result<(), WasmError> {
        let author = self.author.as_mut().ok_or(WasmError::NotIdentified)?;
        let mut scratch = self
            .replica
            .page(page)
            .ok_or(WasmError::UnknownPage)?
            .scene()
            .clone();
        let stack = author
            .undo
            .entry(page)
            .or_insert_with(|| UndoStack::new(DEFAULT_DEPTH));
        stack.begin_gesture();
        let recorded = stack.apply_local(&mut scratch, edits);
        stack.end_gesture();
        recorded?;
        self.commit_edits(page, edits, now_ms)
    }

    /// Stamps, applies (replica = LWW authority) and queues each edit as one op.
    fn commit_edits(&mut self, page: PageId, edits: &[Edit], now_ms: u64) -> Result<(), WasmError> {
        let author = self.author.as_mut().ok_or(WasmError::NotIdentified)?;
        author.mint.set_now(now_ms);
        for edit in edits {
            let op = AnyOp::Item(Box::new(Op {
                op_id: author.mint.op(),
                device: author.device,
                hlc: author.stamp(now_ms),
                page,
                edit: edit.clone(),
            }));
            op.validate()
                .map_err(|_| WasmError::BadArgument("op failed validation"))?;
            self.replica.apply(&op)?;
            author.queue(&op)?;
        }
        Ok(())
    }
}
