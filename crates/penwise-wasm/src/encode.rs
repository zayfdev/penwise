//! Client frames the viewer sends, and device-id generation from JS-supplied time + randomness.
//!
//! Each export is a thin `JsError` wrapper over a plain function returning [`WasmError`] (`*_frame`), so error
//! paths run natively too. The viewer never pushes ops or live ink.

use penwise_proto::{
    AuthToken, ClientKind, ClientMsg, DeviceId, PROTOCOL_VERSION, PageId, encode_client,
};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use crate::error::WasmError;
use crate::viewer::parse_id;

/// Random bytes `new_device_id` consumes (the 80 random bits of a ULID).
pub const DEVICE_RANDOM_BYTES: usize = 10;

/// proto `PROTOCOL_VERSION`.
#[wasm_bindgen]
#[must_use]
pub fn protocol_version() -> u16 {
    PROTOCOL_VERSION
}

/// `DeviceId::from_parts(now_ms, random)` as text. `random` must be exactly 10 bytes
/// (`crypto.getRandomValues`).
///
/// # Errors
/// `JsError` for a wrong length or non-finite/negative `now_ms`.
#[wasm_bindgen]
pub fn new_device_id(now_ms: f64, random: &[u8]) -> Result<String, JsError> {
    Ok(device_id(now_ms, random)?.to_string())
}

/// `Hello { protocol_version: PROTOCOL_VERSION, device, token, last_seen_seq, client: Browser }`.
///
/// # Errors
/// `JsError` for a malformed device id or an encode failure.
#[wasm_bindgen]
pub fn encode_hello(device: &str, token: &str, last_seen_seq: u64) -> Result<Vec<u8>, JsError> {
    Ok(hello_frame(device, token, last_seen_seq)?)
}

/// `Watch { page }` (`None` = stop watching).
///
/// # Errors
/// `JsError` for a malformed page id.
#[wasm_bindgen]
#[allow(
    clippy::needless_pass_by_value,
    reason = "wasm_bindgen passes optional strings by value"
)]
pub fn encode_watch(page: Option<String>) -> Result<Vec<u8>, JsError> {
    Ok(watch_frame(page.as_deref())?)
}

/// `SwitchPage { page }` (only in "lead" mode: the browser moves the tablet).
///
/// # Errors
/// `JsError` for a malformed page id.
#[wasm_bindgen]
pub fn encode_switch_page(page: &str) -> Result<Vec<u8>, JsError> {
    Ok(switch_page_frame(page)?)
}

/// `Resync { from_seq }`.
///
/// # Errors
/// Encode failure.
#[wasm_bindgen]
pub fn encode_resync(from_seq: u64) -> Result<Vec<u8>, JsError> {
    Ok(frame(&ClientMsg::Resync { from_seq })?)
}

/// `Ping { nonce }`.
///
/// # Errors
/// Encode failure.
#[wasm_bindgen]
pub fn encode_ping(nonce: u64) -> Result<Vec<u8>, JsError> {
    Ok(frame(&ClientMsg::Ping { nonce })?)
}

/// `Pong { nonce }`.
///
/// # Errors
/// Encode failure.
#[wasm_bindgen]
pub fn encode_pong(nonce: u64) -> Result<Vec<u8>, JsError> {
    Ok(frame(&ClientMsg::Pong { nonce })?)
}

/// `DeviceId::from_parts` of the millisecond clock (fraction dropped) and 80 big-endian random bits.
///
/// # Errors
/// [`WasmError::RandomLength`] unless `random` is 10 bytes; [`WasmError::BadClock`] for a NaN, infinite or
/// negative clock.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "checked finite and non-negative; from_parts clamps to the 48-bit timestamp range"
)]
pub fn device_id(now_ms: f64, random: &[u8]) -> Result<DeviceId, WasmError> {
    if random.len() != DEVICE_RANDOM_BYTES {
        return Err(WasmError::RandomLength(random.len()));
    }
    if !now_ms.is_finite() || now_ms < 0.0 {
        return Err(WasmError::BadClock);
    }
    let bits = random
        .iter()
        .fold(0_u128, |acc, &byte| (acc << 8) | u128::from(byte));
    Ok(DeviceId::from_parts(now_ms as u64, bits))
}

/// Encoded `Hello` of a browser.
///
/// # Errors
/// [`WasmError::Id`] for a malformed device id; [`WasmError::Proto`] for an encode failure.
pub fn hello_frame(device: &str, token: &str, last_seen_seq: u64) -> Result<Vec<u8>, WasmError> {
    frame(&ClientMsg::Hello {
        protocol_version: PROTOCOL_VERSION,
        device: parse_id(device)?,
        token: AuthToken(token.to_owned()),
        last_seen_seq,
        client: ClientKind::Browser,
    })
}

/// Encoded `Watch`.
///
/// # Errors
/// [`WasmError::Id`] for a malformed page id.
pub fn watch_frame(page: Option<&str>) -> Result<Vec<u8>, WasmError> {
    let page = page.map(parse_id::<PageId>).transpose()?;
    frame(&ClientMsg::Watch { page })
}

/// Encoded `SwitchPage`.
///
/// # Errors
/// [`WasmError::Id`] for a malformed page id.
pub fn switch_page_frame(page: &str) -> Result<Vec<u8>, WasmError> {
    frame(&ClientMsg::SwitchPage {
        page: parse_id(page)?,
    })
}

fn frame(msg: &ClientMsg) -> Result<Vec<u8>, WasmError> {
    Ok(encode_client(msg)?)
}
