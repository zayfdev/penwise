use std::time::Duration;

use penwise_proto::{
    AuthToken, ClientKind, ClientMsg, MAX_FRAME_BYTES, PROTOCOL_VERSION, RejectReason, ServerMsg,
};

use super::fake::{FakeStore, device, notebook_op};
use super::{Rig, TOKEN, config};
use crate::sync::Hub;
use crate::sync::outbound::CLOSE_HANDSHAKE;
use crate::sync::transport::WsIn;

#[tokio::test]
async fn welcome_carries_identity_head_floor_clock_and_current_page() {
    let store = FakeStore::new(16);
    store.commit(vec![notebook_op(1, 1), notebook_op(1, 2)]);
    let page = super::fake::page_id(9);
    store.current_page_set(page);
    let mut rig = Rig::new(&store);
    let ServerMsg::Welcome {
        protocol_version,
        server,
        device: dev,
        head_seq,
        log_floor,
        server_now_ms,
        current_page,
    } = rig.connect(device(5), ClientKind::Tablet, 2).await
    else {
        panic!("expected Welcome");
    };
    assert_eq!(protocol_version, PROTOCOL_VERSION);
    assert_eq!(
        server,
        penwise_proto::ServerId::from_parts(1_700_000_000_000, 7)
    );
    assert_eq!((dev, head_seq, log_floor), (device(5), 2, 0));
    assert_eq!(server_now_ms, 1_700_000_123_456);
    assert_eq!(current_page, Some(page));
    assert_eq!(store.noted(), vec![(device(5), ClientKind::Tablet, 2)]);
}

#[tokio::test]
async fn bad_token_is_rejected_and_closed_with_4001() {
    let store = FakeStore::new(16);
    let mut rig = Rig::new(&store);
    rig.send(&ClientMsg::Hello {
        protocol_version: PROTOCOL_VERSION,
        device: device(1),
        token: AuthToken("nope".to_owned()),
        last_seen_seq: 0,
        client: ClientKind::Tablet,
    })
    .await;
    assert_eq!(
        rig.next().await,
        ServerMsg::Reject {
            reason: RejectReason::BadToken
        }
    );
    assert_eq!(rig.next_close().await, CLOSE_HANDSHAKE);
    assert!(store.noted().is_empty());
}

#[tokio::test]
async fn unsupported_protocol_reports_the_range() {
    let store = FakeStore::new(16);
    let mut rig = Rig::new(&store);
    rig.send(&ClientMsg::Hello {
        protocol_version: PROTOCOL_VERSION + 1,
        device: device(1),
        token: AuthToken(TOKEN.to_owned()),
        last_seen_seq: 0,
        client: ClientKind::Tablet,
    })
    .await;
    let ServerMsg::Reject {
        reason: RejectReason::UnsupportedProtocol { min, max },
    } = rig.next().await
    else {
        panic!("expected UnsupportedProtocol");
    };
    assert!(min <= max && max == PROTOCOL_VERSION);
    assert_eq!(rig.next_close().await, CLOSE_HANDSHAKE);
}

#[tokio::test]
async fn non_hello_garbage_text_and_oversized_first_frames_are_malformed() {
    let store = FakeStore::new(16);
    let firsts: Vec<WsIn> = vec![
        WsIn::Binary(
            penwise_proto::encode_client(&ClientMsg::Ping { nonce: 1 })
                .unwrap_or_default()
                .into(),
        ),
        WsIn::Binary(vec![1, 0xff, 0xff, 0xff].into()),
        WsIn::Binary(Vec::new().into()),
        WsIn::Binary(vec![9; 20].into()),
        WsIn::Text,
        WsIn::Binary(vec![1; MAX_FRAME_BYTES + 1].into()),
    ];
    for first in firsts {
        let mut rig = Rig::new(&store);
        rig.tx.send(first).await.ok();
        assert_eq!(
            rig.next().await,
            ServerMsg::Reject {
                reason: RejectReason::Malformed
            }
        );
        assert_eq!(rig.next_close().await, CLOSE_HANDSHAKE);
    }
    assert_eq!(store.head(), 0);
}

#[tokio::test]
async fn silent_peer_is_dropped_after_the_handshake_timeout() {
    let store = FakeStore::new(16);
    let mut cfg = config();
    cfg.handshake_timeout = Duration::from_millis(40);
    let mut rig = Rig::on(Hub::new(store.clone(), cfg));
    assert_eq!(
        rig.next().await,
        ServerMsg::Reject {
            reason: RejectReason::Malformed
        }
    );
    assert_eq!(rig.next_close().await, CLOSE_HANDSHAKE);
}

#[tokio::test]
async fn repeated_failures_trip_rate_limited() {
    let store = FakeStore::new(16);
    let hub = Hub::new(store.clone(), config());
    for _ in 0..10 {
        let mut rig = Rig::on(hub.clone());
        rig.tx.send(WsIn::Text).await.ok();
        assert!(matches!(
            rig.next().await,
            ServerMsg::Reject {
                reason: RejectReason::Malformed
            }
        ));
    }
    let mut rig = Rig::on(hub.clone());
    assert_eq!(
        rig.next().await,
        ServerMsg::Reject {
            reason: RejectReason::RateLimited
        }
    );
    assert_eq!(rig.next_close().await, CLOSE_HANDSHAKE);
}
