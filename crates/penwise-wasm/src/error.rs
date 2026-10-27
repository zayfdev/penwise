//! Failures of the binding layer. Plain Rust, so error paths are testable natively; the `wasm_bindgen`
//! wrappers map them to `JsError` with `?` (`JsError::new` only works on wasm).

use penwise_core::edit::EditError;
use penwise_proto::{IdParseError, ProtoError, ReplicaError};

/// Why a binding call failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
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
    /// A drawing call before [`crate::viewer::Viewer::identify`].
    #[error("viewer has no device identity; call identify first")]
    NotIdentified,
    /// The page is not in the replica (catalog/snapshot not received yet).
    #[error("unknown page")]
    UnknownPage,
    /// A stroke call without `stroke_begin`, or `stroke_begin` while a stroke is open.
    #[error("{0}")]
    Stroke(&'static str),
    /// A malformed drawing argument.
    #[error("invalid argument: {0}")]
    BadArgument(&'static str),
    /// The replica refused a local op.
    #[error(transparent)]
    Replica(#[from] ReplicaError),
    /// The scene refused a local edit.
    #[error(transparent)]
    Edit(#[from] EditError),
}
