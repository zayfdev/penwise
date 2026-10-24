//! Penwise sync protocol: identities, hybrid logical clock, ops, last-writer-wins materialization and the
//! versioned client ↔ server wire messages (PLAN "Sync protocol").
//!
//! Owner: contract. Only the contract owner edits this file; the M3 contract lives in
//! `docs/contracts/m3-server.md`.
//!
//! Pure like `penwise-core`: no I/O, async, clocks, randomness or logging. Callers inject wall-clock time
//! (`now_ms`) and random bits (id generation), so every function is deterministic and runs unchanged on the
//! server, in WebAssembly and on Android.
//!
//! Data flow:
//! ```text
//! core tool → Vec<Edit> → Op { op_id, device, hlc (HlcClock::send), page, edit } → ClientMsg::PushOps
//!   → server log (seq) → ServerMsg::OpsSince / broadcast → Replica::apply (LWW per (item, field) by HLC)
//!   → PageReplica scene → renderer
//! ```
#![allow(
    clippy::todo,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::unused_self,
    unused_variables,
    dead_code,
    reason = "M3 contract stubs"
)]

pub mod catalog;
pub mod envelope;
pub mod error;
pub mod hlc;
pub mod ids;
pub mod live;
pub mod lww;
pub mod message;
pub mod notebook;
pub mod op;
pub mod replica;

pub use catalog::Catalog;
pub use envelope::{
    FRAME_VERSION, MAX_FRAME_BYTES, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION, decode_client,
    decode_op, decode_server, encode_client, encode_op, encode_server,
};
pub use error::{ProtoError, ReplicaError};
pub use hlc::{Hlc, HlcClock, HlcError, MAX_SKEW_MS};
pub use ids::{DeviceId, IdParseError, ItemRef, NotebookId, OpId, PageId, ServerId};
pub use live::{LiveError, LiveSample, LiveStroke, LiveStrokes, StrokeCancel, StrokeDelta};
pub use lww::{FieldStamps, LwwError, MaterializedItem, Register};
pub use message::{
    AuthToken, ClientKind, ClientMsg, NotebookInfo, OpRejected, PageInfo, PairingCode,
    RejectReason, ServerMsg,
};
pub use notebook::{
    Background, Notebook, NotebookChange, NotebookPatch, Page, PageChange, PageKind, PagePatch,
};
pub use op::{AnyOp, NotebookOp, Op, OpRejection, PageOp, SeqOp};
pub use replica::{Applied, PageReplica, Replica};
