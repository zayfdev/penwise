//! Malformed, oversized, text, mis-shaped frames end the session with `Error` and never change state.

use penwise_proto::{ClientKind, ClientMsg, ErrorCode, MAX_FRAME_BYTES, ServerMsg, encode_client};

use super::fake::{FakeStore, device, item_op, notebook_op, page_id};
use super::{Rig, hello};
use crate::sync::outbound::{CLOSE_ERROR, WsOut};
use crate::sync::transport::WsIn;

async fn connected(store: &FakeStore) -> Rig {
    let mut rig = Rig::new(store);
    assert!(matches!(
        rig.connect(device(1), ClientKind::Tablet, 0).await,
        ServerMsg::Welcome { .. }
    ));
    assert!(matches!(
        rig.next().await,
        ServerMsg::OpsSince { more: false, .. }
    ));
    rig
}

async fn expect_error(rig: &mut Rig, code: ErrorCode) {
    let ServerMsg::Error { code: got, .. } = rig.next().await else {
        panic!("expected Error");
    };
    assert_eq!(got, code);
    assert_eq!(rig.next_close().await, CLOSE_ERROR);
}

#[tokio::test]
async fn bad_frames_get_error_and_close_and_log_is_untouched() {
    let store = FakeStore::new(16);
    store.commit(vec![notebook_op(1, 1)]);
    let good_push = encode_client(&ClientMsg::PushOps {
        ops: vec![notebook_op(1, 2)],
    })
    .unwrap_or_default();
    let mut truncated = good_push.clone();
    truncated.truncate(truncated.len() - 3);
    let mut trailing = good_push.clone();
    trailing.push(0);
    let mut wrong_version = good_push.clone();
    wrong_version[0] = 2;
    let cases: Vec<(WsIn, ErrorCode)> = vec![
        (WsIn::Text, ErrorCode::Malformed),
        (WsIn::Binary(Vec::new().into()), ErrorCode::Malformed),
        (WsIn::Binary(vec![1, 200, 200].into()), ErrorCode::Malformed),
        (WsIn::Binary(truncated.into()), ErrorCode::Malformed),
        (WsIn::Binary(trailing.into()), ErrorCode::Malformed),
        (WsIn::Binary(wrong_version.into()), ErrorCode::Malformed),
        (
            WsIn::Binary(vec![1; MAX_FRAME_BYTES + 1].into()),
            ErrorCode::TooLarge,
        ),
    ];
    for (frame, code) in cases {
        let mut rig = connected(&store).await;
        rig.tx.send(frame).await.ok();
        expect_error(&mut rig, code).await;
        assert_eq!(store.head(), 1, "state must not change");
    }
}

#[tokio::test]
async fn shape_violations_are_protocol_errors() {
    let store = FakeStore::new(16);
    let page = page_id(1);
    let too_many: Vec<_> = (1..=513).map(|n| item_op(1, n, page, "x")).collect();
    for msg in [
        ClientMsg::PushOps { ops: Vec::new() },
        ClientMsg::PushOps { ops: too_many },
        hello(device(1), ClientKind::Tablet, 0),
    ] {
        let mut rig = connected(&store).await;
        rig.send(&msg).await;
        expect_error(&mut rig, ErrorCode::Malformed).await;
    }
    assert_eq!(store.head(), 0);
}

#[tokio::test]
async fn ping_is_answered_with_the_same_nonce() {
    let store = FakeStore::new(16);
    let mut rig = connected(&store).await;
    rig.send(&ClientMsg::Ping { nonce: 77 }).await;
    assert_eq!(rig.next().await, ServerMsg::Pong { nonce: 77 });
    rig.send(&ClientMsg::Pong { nonce: 1 }).await;
    assert!(rig.quiet().await);
}

#[tokio::test]
async fn silent_client_gets_pings_then_is_closed_at_the_idle_limit() {
    let store = FakeStore::new(16);
    let mut cfg = super::config();
    cfg.ping_interval = std::time::Duration::from_millis(30);
    cfg.idle_timeout = std::time::Duration::from_millis(100);
    let mut rig = Rig::on(crate::sync::Hub::new(store.clone(), cfg));
    rig.connect(device(1), ClientKind::Tablet, 0).await;
    let mut pings = 0;
    loop {
        match rig.next_out().await {
            Some(WsOut::Frame(frame)) => {
                if matches!(
                    penwise_proto::decode_server(&frame),
                    Ok(ServerMsg::Ping { .. })
                ) {
                    pings += 1;
                }
            }
            Some(WsOut::Close { code, .. }) => {
                assert_eq!(code, crate::sync::outbound::CLOSE_NORMAL);
                break;
            }
            None => break,
        }
    }
    assert!(pings >= 1);
}

#[tokio::test]
async fn shutdown_sends_error_and_closes() {
    let store = FakeStore::new(16);
    let mut rig = connected(&store).await;
    rig.hub.shutdown();
    expect_error(&mut rig, ErrorCode::ShuttingDown).await;
}

#[tokio::test]
async fn storage_failure_is_internal_error_not_a_hang() {
    let store = FakeStore::new(16);
    store.set_fail_status(true);
    let mut rig = Rig::new(&store);
    rig.send(&hello(device(1), ClientKind::Tablet, 0)).await;
    expect_error(&mut rig, ErrorCode::Internal).await;
}
