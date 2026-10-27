//! Versioned frames and stored op blobs.
//!
//! Layout (both directions, and `SQLite` `ops.body`):
//! ```text
//! version  u8 = FRAME_VERSION (1)
//! body     postcard(ClientMsg | ServerMsg | AnyOp), nothing after it
//! ```
//! Evolution rules (postcard has no field tags, server-stack.md §4):
//! - `FRAME_VERSION` changes only if postcard itself changes (postcard 2) or the envelope layout changes.
//! - Message and op enums are **append-only**: new variants at the end; existing variants and structs never
//!   change shape. A new shape is a new variant (`AddItemV2`), gated by `protocol_version`.
//! - `ClientMsg::Hello` and `ServerMsg::Welcome`/`Reject` are frozen forever: version negotiation depends on
//!   decoding them.
//! - Item edits inside ops are core `PWED` blobs with their own version (`op::edit_blob`).
//! - Every change to this crate ships a compat test decoding the previous version's frame fixtures
//!   (`crates/penwise-proto/tests/fixtures/v<N>/`), per AGENTS.md "Wire format & storage".

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::ProtoError;
use crate::message::{ClientMsg, ServerMsg};
use crate::op::AnyOp;

/// Envelope layout version (first byte of every frame and stored op).
pub const FRAME_VERSION: u8 = 1;
/// Message-set version this build speaks (`Hello`/`Welcome`).
pub const PROTOCOL_VERSION: u16 = 1;
/// Oldest client protocol the server accepts.
pub const MIN_PROTOCOL_VERSION: u16 = 1;
/// Largest frame either side sends or accepts (also the axum `max_message_size`).
pub const MAX_FRAME_BYTES: usize = 4 << 20;

/// Encodes a client frame.
///
/// # Errors
/// [`ProtoError::Encode`]; [`ProtoError::TooLarge`] when the frame exceeds `MAX_FRAME_BYTES`.
pub fn encode_client(msg: &ClientMsg) -> Result<Vec<u8>, ProtoError> {
    encode(msg)
}

/// Decodes a client frame: length check, version byte, postcard, no trailing bytes.
///
/// # Errors
/// `Empty`, `TooLarge`, `UnsupportedVersion`, `Decode`, `TrailingBytes`.
pub fn decode_client(frame: &[u8]) -> Result<ClientMsg, ProtoError> {
    decode(frame)
}

/// Encodes a server frame (as [`encode_client`]).
///
/// # Errors
/// As [`encode_client`].
pub fn encode_server(msg: &ServerMsg) -> Result<Vec<u8>, ProtoError> {
    encode(msg)
}

/// Decodes a server frame (as [`decode_client`]).
///
/// # Errors
/// As [`decode_client`].
pub fn decode_server(frame: &[u8]) -> Result<ServerMsg, ProtoError> {
    decode(frame)
}

/// Encodes one op for storage (`ops.body`), same envelope.
///
/// # Errors
/// As [`encode_client`].
pub fn encode_op(op: &AnyOp) -> Result<Vec<u8>, ProtoError> {
    encode(op)
}

/// Decodes a stored op.
///
/// # Errors
/// As [`decode_client`].
pub fn decode_op(bytes: &[u8]) -> Result<AnyOp, ProtoError> {
    decode(bytes)
}

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtoError> {
    let frame = postcard::to_extend(value, vec![FRAME_VERSION]).map_err(ProtoError::Encode)?;
    if frame.len() > MAX_FRAME_BYTES {
        return Err(ProtoError::TooLarge { len: frame.len() });
    }
    Ok(frame)
}

fn decode<T: DeserializeOwned>(frame: &[u8]) -> Result<T, ProtoError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(ProtoError::TooLarge { len: frame.len() });
    }
    let (&version, body) = frame.split_first().ok_or(ProtoError::Empty)?;
    if version != FRAME_VERSION {
        return Err(ProtoError::UnsupportedVersion(version));
    }
    let (value, rest) = postcard::take_from_bytes::<T>(body).map_err(ProtoError::Decode)?;
    if rest.is_empty() {
        Ok(value)
    } else {
        Err(ProtoError::TrailingBytes { count: rest.len() })
    }
}
