//! First frame: `Hello` → validated `Accepted`, or `Reject` + close 4001.

use std::net::IpAddr;

use penwise_proto::message::AuthToken;
use penwise_proto::{
    ClientKind, ClientMsg, DeviceId, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION, RejectReason,
    ServerMsg, decode_client,
};
use tokio::sync::mpsc;

use super::hub::Hub;
use super::outbound::{CLOSE_HANDSHAKE, Outbound, Stop};
use super::store::SyncStore;
use super::transport::WsIn;

/// A `Hello` that passed every check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accepted {
    pub device: DeviceId,
    pub kind: ClientKind,
    pub last_seen_seq: u64,
}

/// Why a first frame is refused, if it is.
fn judge(frame: &WsIn, token_ok: impl Fn(&AuthToken) -> bool) -> Result<Accepted, RejectReason> {
    let WsIn::Binary(bytes) = frame else {
        return Err(RejectReason::Malformed);
    };
    let Ok(ClientMsg::Hello {
        protocol_version,
        device,
        token: offered,
        last_seen_seq,
        client,
    }) = decode_client(bytes)
    else {
        return Err(RejectReason::Malformed);
    };
    if !(MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION).contains(&protocol_version) {
        return Err(RejectReason::UnsupportedProtocol {
            min: MIN_PROTOCOL_VERSION,
            max: PROTOCOL_VERSION,
        });
    }
    if !token_ok(&offered) {
        return Err(RejectReason::BadToken);
    }
    Ok(Accepted {
        device,
        kind: client,
        last_seen_seq,
    })
}

/// Waits for `Hello` (within the handshake timeout) and answers a refusal itself.
///
/// # Errors
/// [`Stop::Peer`] after a refusal (the `Reject` and close are already queued) or when the socket is gone.
pub async fn accept<S: SyncStore>(
    hub: &Hub<S>,
    peer: Option<IpAddr>,
    inbound: &mut mpsc::Receiver<WsIn>,
    out: &Outbound,
) -> Result<Accepted, Stop> {
    if !hub.handshake_allowed(peer) {
        return refuse(out, RejectReason::RateLimited).await;
    }
    let first = tokio::time::timeout(hub.config.handshake_timeout, inbound.recv()).await;
    let frame = match first {
        Ok(Some(frame)) => frame,
        Ok(None) => return Err(Stop::Peer),
        Err(_) => {
            hub.handshake_failed(peer);
            return refuse(out, RejectReason::Malformed).await;
        }
    };
    let expected = hub.config.token.as_str();
    let verdict = judge(&frame, |offered| {
        crate::auth::authorized(expected, Some(&offered.0), &axum::http::HeaderMap::new())
    });
    match verdict {
        Ok(accepted) => {
            hub.handshake_succeeded(peer);
            Ok(accepted)
        }
        Err(reason) => {
            hub.handshake_failed(peer);
            refuse(out, reason).await
        }
    }
}

async fn refuse(out: &Outbound, reason: RejectReason) -> Result<Accepted, Stop> {
    out.send(&ServerMsg::Reject { reason }).await?;
    out.close(CLOSE_HANDSHAKE, "handshake refused").await;
    Err(Stop::Peer)
}
