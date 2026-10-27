//! Reload survival and dead letters: the outbox (and the device identity it belongs to) as a blob the page
//! keeps in storage, plus the list of ops the server refused.

use penwise_proto::{AnyOp, OpId, decode_op};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use super::{Viewer, author::Author, parse_id};
use crate::error::WasmError;

/// One op the server refused, for the UI.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadLetter {
    pub op_id: String,
    /// Why it was refused (proto `OpRejection` text).
    pub reason: String,
    /// What it tried to do, e.g. `item on page 01H…`.
    pub summary: String,
}

#[wasm_bindgen]
impl Viewer {
    /// Replaces the identity with the one saved by [`Viewer::export_state`]: same device, same unsent ops and
    /// dead letters. `entropy` reseeds the id stream. Call before `begin_session`, instead of `identify`.
    ///
    /// # Errors
    /// `JsError` when the blob is not a state this version wrote.
    pub fn restore_state(&mut self, state: &[u8], entropy: u64) -> Result<(), JsError> {
        self.author = Some(Author::from_state(state, entropy)?);
        Ok(())
    }

    /// The bytes to keep across a reload (device + outbox); changes exactly when `state_revision` moves.
    ///
    /// # Errors
    /// `JsError` before `identify`.
    pub fn export_state(&self) -> Result<Vec<u8>, JsError> {
        let author = self.author.as_ref().ok_or(WasmError::NotIdentified)?;
        Ok(author.state_bytes()?)
    }

    /// Counter that moves whenever `export_state` would return different bytes (0 before `identify`).
    #[must_use]
    pub fn state_revision(&self) -> f64 {
        #[allow(
            clippy::cast_precision_loss,
            reason = "a counter of local edits stays far below 2^53"
        )]
        self.author.as_ref().map_or(0.0, |a| a.revision() as f64)
    }

    /// Ops the server refused, oldest first.
    #[must_use]
    pub fn dead_letters(&self) -> Vec<DeadLetter> {
        self.author.as_ref().map_or_else(Vec::new, |a| {
            a.dead()
                .iter()
                .map(|d| DeadLetter {
                    op_id: d.op_id.to_string(),
                    reason: d.reason.to_string(),
                    summary: summarise(&d.bytes),
                })
                .collect()
        })
    }

    /// Stored envelope bytes of one dead letter (for an export file).
    ///
    /// # Errors
    /// `JsError` for a malformed id; `None` for an unknown one.
    pub fn dead_letter_bytes(&self, op_id: &str) -> Result<Option<Vec<u8>>, JsError> {
        let op_id: OpId = parse_id(op_id)?;
        Ok(self
            .author
            .as_ref()
            .and_then(|a| a.dead().iter().find(|d| d.op_id == op_id))
            .map(|d| d.bytes.clone()))
    }

    /// Forgets one dead letter; true when it existed.
    ///
    /// # Errors
    /// `JsError` for a malformed id.
    pub fn dismiss_dead_letter(&mut self, op_id: &str) -> Result<bool, JsError> {
        let op_id: OpId = parse_id(op_id)?;
        Ok(self
            .author
            .as_mut()
            .is_some_and(|a| a.remove_dead(op_id).is_some()))
    }
}

fn summarise(bytes: &[u8]) -> String {
    match decode_op(bytes) {
        Ok(AnyOp::Item(op)) => format!("item edit on page {}", op.page),
        Ok(AnyOp::Page(op)) => format!("page change {}", op.page),
        Ok(AnyOp::Notebook(op)) => format!("notebook change {}", op.notebook),
        Err(_) => "undecodable op".to_owned(),
    }
}
