//! Connection lifecycle of the browser author: identity, `Hello`, pushes.

use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use super::{Viewer, author::Author, parse_id, whole_ms};
use crate::error::WasmError;

#[wasm_bindgen]
impl Viewer {
    /// Gives this viewer its device identity (`new_device_id`) and the entropy seeding its id stream
    /// (`crypto.getRandomValues`). Required before `begin_session` and any drawing call; replaces earlier state.
    ///
    /// # Errors
    /// `JsError` for a malformed device id.
    pub fn identify(&mut self, device: &str, entropy: u64) -> Result<(), JsError> {
        self.author = Some(Author::new(parse_id(device)?, entropy));
        Ok(())
    }

    /// Starts a connection: the `Hello` frame (cursor = `head_seq`) at local clock `now_ms`.
    ///
    /// # Errors
    /// `JsError` before `identify`, or an encode failure.
    pub fn begin_session(&mut self, token: &str, now_ms: f64) -> Result<Vec<u8>, JsError> {
        let head = self.replica.head_seq();
        let author = self.author.as_mut().ok_or(WasmError::NotIdentified)?;
        author.begin_session(whole_ms(now_ms));
        self.server_protocol = 0;
        let device = author.device.to_string();
        Ok(crate::encode::hello_frame(&device, token, head)?)
    }

    /// True when this connection's `Welcome` reported protocol 2 or newer (`Focus` may be sent).
    #[must_use]
    pub fn supports_focus(&self) -> bool {
        self.server_protocol >= penwise_proto::FOCUS_PROTOCOL_VERSION
    }

    /// The `Focus` frame (see `encode_focus`), or `None` when the server did not report protocol 2 (an older
    /// server would drop it).
    ///
    /// # Errors
    /// `JsError` for a malformed page or item id.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "wasm_bindgen passes optional strings by value"
    )]
    pub fn focus(&self, page: &str, item: Option<String>) -> Result<Option<Vec<u8>>, JsError> {
        if !self.supports_focus() {
            return Ok(None);
        }
        Ok(Some(crate::encode::focus_frame(page, item.as_deref())?))
    }

    /// The socket closed: pushes pause, previews of other devices are dropped (the outbox is kept).
    pub fn end_session(&mut self) {
        if let Some(author) = self.author.as_mut() {
            author.go_offline();
        }
        self.live.clear();
    }

    /// The next `PushOps` frame, if the session may send one (welcomed, no snapshot, nothing in flight).
    ///
    /// # Errors
    /// `JsError` on an encode failure (never expected).
    pub fn poll_push(&mut self) -> Result<Option<Vec<u8>>, JsError> {
        match self.author.as_mut() {
            Some(author) => Ok(author.poll_push()?),
            None => Ok(None),
        }
    }

    /// Local ops not yet acknowledged.
    #[must_use]
    pub fn pending_ops(&self) -> u32 {
        self.author
            .as_ref()
            .map_or(0, |a| u32::try_from(a.pending()).unwrap_or(u32::MAX))
    }

    /// How far (ms) the newest stamp from the server log was ahead of this clock at the last local stamp, then
    /// resets to 0. Non-zero means new ops may be refused as `ClockSkew` until the clock catches up.
    pub fn take_clock_skew_ms(&mut self) -> f64 {
        #[allow(
            clippy::cast_precision_loss,
            reason = "a skew in ms stays far below 2^53"
        )]
        self.author
            .as_mut()
            .map_or(0.0, |a| a.take_clock_skew() as f64)
    }
}
