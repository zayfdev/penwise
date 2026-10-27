//! Penwise sync protocol: identities, hybrid logical clock, ops, last-writer-wins materialization and the
//! versioned client ↔ server wire messages (PLAN "Sync protocol").
//!
//! The contract of record is `docs/rewrite-design.md` (§2 ops, §4 wire protocol, §5 proto).
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
pub mod catalog;
pub mod envelope;
pub mod error;
pub mod hlc;
pub mod ids;
pub mod legacy;
pub mod live;
pub mod lww;
pub mod message;
pub mod notebook;
pub mod op;
pub mod outbox;
pub mod replica;
pub mod snapshot;

pub use catalog::Catalog;
pub use envelope::{
    FRAME_VERSION, MAX_FRAME_BYTES, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION, decode_client,
    decode_op, decode_server, encode_client, encode_op, encode_server,
};
pub use error::{ProtoError, ReplicaError};
pub use hlc::{ClockOffset, Hlc, HlcClock, HlcError, MAX_SKEW_MS};
pub use ids::{DeviceId, IdParseError, ItemRef, NotebookId, OpId, PageId, ServerId};
pub use live::{LiveError, LiveSample, LiveStroke, LiveStrokes, StrokeCancel, StrokeDelta};
pub use lww::{FieldStamps, LwwError, MaterializedItem, Register, merge_patch};
pub use message::{
    AuthToken, ClientKind, ClientMsg, ErrorCode, OpRejected, RejectReason, ServerMsg,
};
pub use notebook::{
    Background, Notebook, NotebookChange, NotebookPatch, Page, PageChange, PageKind, PagePatch,
};
pub use op::{AnyOp, NotebookOp, Op, OpRejection, PageOp, SeqOp};
pub use outbox::{DeadOp, Outbox};
pub use replica::{Applied, PageReplica, Replica, SnapshotApplied};
pub use snapshot::{ItemState, NotebookState, PageState, Scope, SnapshotChunk};
