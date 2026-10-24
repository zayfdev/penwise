//! Client frames the viewer sends, and device-id generation from JS-supplied time + randomness.
//!
//! Owner: slice d.
//!
//! The viewer is read-only in M3: it never pushes ops or live ink.

use wasm_bindgen::prelude::{JsError, wasm_bindgen};

/// proto `PROTOCOL_VERSION`.
#[wasm_bindgen]
#[must_use]
pub fn protocol_version() -> u16 {
    todo!()
}

/// `DeviceId::from_parts(now_ms, random)` as text. `random` must be exactly 10 bytes
/// (`crypto.getRandomValues`).
///
/// # Errors
/// `JsError` for a wrong length or non-finite/negative `now_ms`.
#[wasm_bindgen]
pub fn new_device_id(now_ms: f64, random: &[u8]) -> Result<String, JsError> {
    todo!()
}

/// `Hello { protocol_version: PROTOCOL_VERSION, device, token, last_seen_seq, client: Browser }`.
///
/// # Errors
/// `JsError` for a malformed device id or an encode failure.
#[wasm_bindgen]
pub fn encode_hello(device: &str, token: &str, last_seen_seq: u64) -> Result<Vec<u8>, JsError> {
    todo!()
}

/// `Pair { protocol_version, code, device, name, client: Browser }`.
///
/// # Errors
/// As [`encode_hello`].
#[wasm_bindgen]
pub fn encode_pair(code: &str, device: &str, name: &str) -> Result<Vec<u8>, JsError> {
    todo!()
}

/// `SwitchPage { page }` (only in "lead" mode: the browser moves the tablet).
///
/// # Errors
/// As [`encode_hello`].
#[wasm_bindgen]
pub fn encode_switch_page(page: &str) -> Result<Vec<u8>, JsError> {
    todo!()
}

/// `ListNotebooks`.
///
/// # Errors
/// Encode failure.
#[wasm_bindgen]
pub fn encode_list_notebooks() -> Result<Vec<u8>, JsError> {
    todo!()
}

/// `ListPages { notebook }`.
///
/// # Errors
/// As [`encode_hello`].
#[wasm_bindgen]
pub fn encode_list_pages(notebook: &str) -> Result<Vec<u8>, JsError> {
    todo!()
}

/// `Resync { from_seq }`.
///
/// # Errors
/// Encode failure.
#[wasm_bindgen]
pub fn encode_resync(from_seq: u64) -> Result<Vec<u8>, JsError> {
    todo!()
}

/// `Ping { nonce }`.
///
/// # Errors
/// Encode failure.
#[wasm_bindgen]
pub fn encode_ping(nonce: u64) -> Result<Vec<u8>, JsError> {
    todo!()
}

/// `Pong { nonce }`.
///
/// # Errors
/// Encode failure.
#[wasm_bindgen]
pub fn encode_pong(nonce: u64) -> Result<Vec<u8>, JsError> {
    todo!()
}
