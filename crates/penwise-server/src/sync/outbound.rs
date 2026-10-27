//! A session's bounded way out: reliable frames wait for room, live frames are dropped when the client is slow.

use std::sync::Arc;

use penwise_proto::{ErrorCode, ServerMsg, encode_server};
use tokio::sync::mpsc;

/// Frames the transport pump hands to the socket.
#[derive(Debug)]
pub enum WsOut {
    Frame(Arc<[u8]>),
    Close { code: u16, reason: &'static str },
}

/// Close code after a refused handshake.
pub const CLOSE_HANDSHAKE: u16 = 4001;
/// Close code after a connection-level `Error` frame.
pub const CLOSE_ERROR: u16 = 4002;
/// Normal close.
pub const CLOSE_NORMAL: u16 = 1000;

/// Why a session stops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// The socket went away (or the idle limit hit): nothing more to say.
    Peer,
    /// Connection-level failure: `Error{code, detail}`, then close.
    Protocol(ErrorCode, String),
}

impl Stop {
    pub fn malformed(detail: &str) -> Self {
        Self::Protocol(ErrorCode::Malformed, detail.to_owned())
    }

    pub fn internal(error: &impl std::fmt::Display) -> Self {
        tracing::warn!(%error, "sync session storage failure");
        Self::Protocol(ErrorCode::Internal, "storage failure; retry".to_owned())
    }
}

#[derive(Debug, Clone)]
pub struct Outbound {
    tx: mpsc::Sender<WsOut>,
}

impl Outbound {
    #[must_use]
    pub fn new(tx: mpsc::Sender<WsOut>) -> Self {
        Self { tx }
    }

    /// Encodes and queues one reliable frame, waiting for room.
    ///
    /// # Errors
    /// [`Stop::Peer`] when the socket is gone; an encode failure is an internal error.
    pub async fn send(&self, msg: &ServerMsg) -> Result<(), Stop> {
        let frame = encode_server(msg).map_err(|error| Stop::internal(&error))?;
        self.send_frame(frame).await
    }

    /// Queues one already-encoded reliable frame, waiting for room.
    ///
    /// # Errors
    /// [`Stop::Peer`] when the socket is gone.
    pub async fn send_frame(&self, frame: Vec<u8>) -> Result<(), Stop> {
        self.tx
            .send(WsOut::Frame(frame.into()))
            .await
            .map_err(|_| Stop::Peer)
    }

    /// Queues a pre-encoded live frame unless the queue is full (then it is dropped). False when dropped.
    #[must_use]
    pub fn try_send_live(&self, frame: Arc<[u8]>) -> bool {
        self.tx.try_send(WsOut::Frame(frame)).is_ok()
    }

    /// Queues the close (best effort).
    pub async fn close(&self, code: u16, reason: &'static str) {
        let _ = self.tx.send(WsOut::Close { code, reason }).await;
    }
}
