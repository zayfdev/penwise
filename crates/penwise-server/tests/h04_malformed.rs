//! H-4: malformed, garbage, oversized and wrong-version frames, text frames, bad handshakes: the session gets
//! `Error`/`Reject` and the page is untouched.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::similar_names,
    clippy::items_after_statements
)]

mod common;

use std::time::Duration;

use common::{TestServer, converge, line};
use penwise_proto::{
    AuthToken, ClientKind, ClientMsg, ErrorCode, FRAME_VERSION, PROTOCOL_VERSION, RejectReason,
    ServerMsg,
};
use penwise_testkit::{RawSocket, Recv};

const WAIT: Duration = Duration::from_secs(5);

async fn seeded() -> (TestServer, penwise_testkit::TestDevice) {
    let server = TestServer::start().await;
    let mut a = server.tablet(1);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    for i in 0..4_u16 {
        a.draw(page, &line(10.0, 40.0 + f32::from(i) * 30.0, 30));
    }
    converge(&server, &mut [&mut a]).await;
    (server, a)
}

/// Sends `bytes` after a good handshake; the session must answer with `Error` (or just close) and the log and
/// tables must be unchanged.
async fn inject_and_expect_error(
    server: &TestServer,
    device: &mut penwise_testkit::TestDevice,
    bytes: Vec<u8>,
) {
    let head = server.head();
    let fp = server.fingerprint();
    device.send_raw(bytes).await.expect("send");
    let mut socket_closed = false;
    for _ in 0..20 {
        device.pump_quiet(Duration::from_millis(200)).await;
        if !device.is_online() {
            socket_closed = true;
            break;
        }
    }
    assert!(socket_closed, "a malformed frame must end the session");
    assert!(
        device
            .events
            .errors
            .iter()
            .any(|(code, _)| matches!(code, ErrorCode::Malformed | ErrorCode::TooLarge)),
        "an Error frame must precede the close: {:?}",
        device.events.errors
    );
    assert_eq!(
        device.events.closed,
        Some(Some(4002)),
        "protocol error close code"
    );
    assert_eq!(server.head(), head, "log untouched");
    assert_eq!(server.fingerprint(), fp, "page untouched");
}

#[tokio::test]
async fn garbage_bytes_wrong_version_and_truncation_leave_the_page_alone() {
    let (server, mut a) = seeded().await;
    let valid = penwise_proto::encode_client(&ClientMsg::Ping { nonce: 1 }).expect("encode");
    let cases: Vec<Vec<u8>> = vec![
        vec![0xFF; 64],
        vec![],
        vec![FRAME_VERSION],
        {
            let mut v = valid.clone();
            v[0] = FRAME_VERSION + 41;
            v
        },
        valid[..valid.len() - 1].to_vec(),
        {
            let mut v = valid.clone();
            v.extend_from_slice(&[1, 2, 3]);
            v
        },
    ];
    for bytes in cases {
        a.online().await.expect("online");
        inject_and_expect_error(&server, &mut a, bytes).await;
        a.events.errors.clear();
        a.events.closed = None;
    }
}

#[tokio::test]
async fn text_frames_and_oversized_frames_are_refused() {
    let (server, mut a) = seeded().await;
    let head = server.head();
    let fp = server.fingerprint();
    a.online().await.expect("online");
    a.send_text("{\"op\":\"clear\"}").await.expect("send");
    for _ in 0..20 {
        a.pump_quiet(Duration::from_millis(200)).await;
        if !a.is_online() {
            break;
        }
    }
    assert!(!a.is_online(), "a text frame ends the session");

    a.online().await.expect("online again");
    let _ = a.send_raw(vec![FRAME_VERSION; 5 << 20]).await;
    for _ in 0..30 {
        a.pump_quiet(Duration::from_millis(200)).await;
        if !a.is_online() {
            break;
        }
    }
    assert!(!a.is_online(), "an oversized frame ends the session");
    assert_eq!(server.head(), head);
    assert_eq!(server.fingerprint(), fp);
}

#[tokio::test]
async fn push_ops_outside_the_frame_limits_is_refused_without_logging() {
    let (server, mut a) = seeded().await;
    let page = a
        .replica
        .catalog()
        .notebooks()
        .first()
        .map(|n| a.replica.catalog().pages(n.id)[0].id)
        .expect("page");
    a.online().await.expect("online");
    let head = server.head();
    let empty = penwise_proto::encode_client(&ClientMsg::PushOps { ops: vec![] }).expect("encode");
    a.send_raw(empty).await.expect("send");
    let mut ops = Vec::new();
    for _ in 0..513 {
        let id = a.ids.next_id_item();
        let item = penwise_testkit::stroke_item(
            id,
            65_536,
            penwise_core::item::Layer::Ink,
            1,
            &line(0.0, 0.0, 3),
        );
        ops.push(a.edit_op(page, penwise_core::edit::Edit::Add(item)));
    }
    let too_many = penwise_proto::encode_client(&ClientMsg::PushOps { ops }).expect("encode");
    let _ = a.send_raw(too_many).await;
    a.pump_quiet(Duration::from_millis(500)).await;
    assert_eq!(server.head(), head, "over-limit frames log nothing");
}

#[tokio::test]
async fn bad_handshakes_are_rejected() {
    let server = TestServer::start().await;
    let hello = |token: &str, version: u16| ClientMsg::Hello {
        protocol_version: version,
        device: penwise_testkit::IdGen::new(9).device(),
        token: AuthToken(token.to_owned()),
        last_seen_seq: 0,
        client: ClientKind::Tablet,
    };

    let mut s = RawSocket::connect(&server.url()).await.expect("connect");
    s.send(&hello("wrong-token", PROTOCOL_VERSION))
        .await
        .expect("send");
    assert_eq!(
        s.recv(WAIT).await,
        Recv::Msg(ServerMsg::Reject {
            reason: RejectReason::BadToken
        })
    );
    assert_eq!(s.drain_until_closed(WAIT).await, Some(Some(4001)));

    let mut s = RawSocket::connect(&server.url()).await.expect("connect");
    s.send(&hello(common::TOKEN, PROTOCOL_VERSION + 9))
        .await
        .expect("send");
    assert!(matches!(
        s.recv(WAIT).await,
        Recv::Msg(ServerMsg::Reject {
            reason: RejectReason::UnsupportedProtocol { .. }
        })
    ));

    let mut s = RawSocket::connect(&server.url()).await.expect("connect");
    s.send_bytes(vec![0xAB; 40]).await.expect("send");
    assert!(matches!(
        s.recv(WAIT).await,
        Recv::Msg(ServerMsg::Reject {
            reason: RejectReason::Malformed
        })
    ));
    assert_eq!(s.drain_until_closed(WAIT).await, Some(Some(4001)));

    let mut s = RawSocket::connect(&server.url()).await.expect("connect");
    s.send(&ClientMsg::Ping { nonce: 5 }).await.expect("send");
    assert!(
        matches!(s.recv(WAIT).await, Recv::Msg(ServerMsg::Reject { .. })),
        "first frame must be Hello"
    );

    let mut s = RawSocket::connect(&server.url()).await.expect("connect");
    s.send_text("hello").await.expect("send");
    assert!(
        matches!(
            s.recv(WAIT).await,
            Recv::Msg(ServerMsg::Reject { .. }) | Recv::Closed(_)
        ),
        "a text first frame is refused"
    );
    assert_eq!(server.head(), 0);
}
