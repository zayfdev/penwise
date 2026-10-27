//! Frame inspection for tests and dev tooling: which message/op a byte string holds.

use penwise_proto::{AnyOp, ClientMsg, decode_client, decode_op, decode_server};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use crate::error::WasmError;
use crate::received::MessageKind;

/// Kind of a server frame.
///
/// # Errors
/// `JsError` with the `ProtoError` text when the frame does not decode.
#[wasm_bindgen]
pub fn decode_server_kind(frame: &[u8]) -> Result<MessageKind, JsError> {
    Ok(server_kind(frame)?)
}

/// Variant name of a client frame (`Hello`, `PushOps`, `StrokeDelta`, …).
///
/// # Errors
/// `JsError` with the `ProtoError` text when the frame does not decode.
#[wasm_bindgen]
pub fn decode_client_kind(frame: &[u8]) -> Result<String, JsError> {
    Ok(client_kind(frame)?.to_owned())
}

/// Variant name of a stored op blob: `Item`, `Page` or `Notebook`.
///
/// # Errors
/// `JsError` with the `ProtoError` text when the blob does not decode.
#[wasm_bindgen]
pub fn decode_op_kind(bytes: &[u8]) -> Result<String, JsError> {
    Ok(op_kind(bytes)?.to_owned())
}

/// Plain-Rust [`decode_server_kind`].
///
/// # Errors
/// [`WasmError::Proto`].
pub fn server_kind(frame: &[u8]) -> Result<MessageKind, WasmError> {
    Ok(MessageKind::of(&decode_server(frame)?))
}

/// Plain-Rust [`decode_client_kind`].
///
/// # Errors
/// [`WasmError::Proto`].
pub fn client_kind(frame: &[u8]) -> Result<&'static str, WasmError> {
    Ok(match decode_client(frame)? {
        ClientMsg::Hello { .. } => "Hello",
        ClientMsg::PushOps { .. } => "PushOps",
        ClientMsg::StrokeDelta(_) => "StrokeDelta",
        ClientMsg::StrokeCancel(_) => "StrokeCancel",
        ClientMsg::SwitchPage { .. } => "SwitchPage",
        ClientMsg::Watch { .. } => "Watch",
        ClientMsg::Resync { .. } => "Resync",
        ClientMsg::Ping { .. } => "Ping",
        ClientMsg::Pong { .. } => "Pong",
    })
}

/// Plain-Rust [`decode_op_kind`].
///
/// # Errors
/// [`WasmError::Proto`].
pub fn op_kind(bytes: &[u8]) -> Result<&'static str, WasmError> {
    Ok(match decode_op(bytes)? {
        AnyOp::Item(_) => "Item",
        AnyOp::Page(_) => "Page",
        AnyOp::Notebook(_) => "Notebook",
    })
}
