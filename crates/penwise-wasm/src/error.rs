//! Failures of the binding layer. Plain Rust, so error paths are testable natively; the `wasm_bindgen`
//! wrappers map them to `JsError` with `?` (`JsError::new` only works on wasm).

use penwise_proto::{IdParseError, ProtoError};

/// Why a binding call failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WasmError {
    /// A frame did not decode or encode.
    #[error(transparent)]
    Proto(#[from] ProtoError),
    /// An id argument is not a 26-char ULID text.
    #[error("{0}")]
    Id(#[from] IdParseError),
    /// `new_device_id` needs exactly 10 random bytes.
    #[error("device id needs 10 random bytes, got {0}")]
    RandomLength(usize),
    /// `new_device_id` needs a finite, non-negative millisecond clock.
    #[error("clock must be finite and non-negative")]
    BadClock,
}
