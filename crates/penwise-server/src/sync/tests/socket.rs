//! The `/sync` route over a real WebSocket: upgrade, binary frames both ways, text frame → `Error`, close codes.

use futures_util::{SinkExt, StreamExt};
use penwise_proto::{
    ClientKind, ClientMsg, ErrorCode, MAX_FRAME_BYTES, ServerMsg, decode_server, encode_client,
};
use tokio_tungstenite::tungstenite::Message;

use super::fake::{device, notebook_op};
use super::hello;
use crate::serve::{ServerConfig, serve};

async fn recv(
    socket: &mut (
             impl futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
             + Unpin
         ),
) -> Message {
    tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
        .await
        .unwrap_or_else(|_| panic!("timeout"))
        .unwrap_or_else(|| panic!("socket ended"))
        .unwrap_or_else(|e| panic!("{e}"))
}

#[tokio::test]
async fn sync_route_speaks_binary_frames_and_rejects_text() {
    let dir = std::env::temp_dir().join(format!("penwise-sync-socket-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = serve(listener, ServerConfig::new(&dir, super::TOKEN))
        .await
        .unwrap();
    let url = format!("ws://{}/sync", server.addr);

    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let hello_frame = encode_client(&hello(device(1), ClientKind::Tablet, 0)).unwrap();
    socket
        .send(Message::Binary(hello_frame.into()))
        .await
        .unwrap();
    let Message::Binary(frame) = recv(&mut socket).await else {
        panic!("binary Welcome")
    };
    assert!(matches!(
        decode_server(&frame),
        Ok(ServerMsg::Welcome { head_seq: 0, .. })
    ));
    let Message::Binary(frame) = recv(&mut socket).await else {
        panic!("binary catch-up")
    };
    assert!(matches!(
        decode_server(&frame),
        Ok(ServerMsg::OpsSince { more: false, .. })
    ));

    let push = encode_client(&ClientMsg::PushOps {
        ops: vec![notebook_op(1, 1)],
    })
    .unwrap();
    socket.send(Message::Binary(push.into())).await.unwrap();
    let Message::Binary(frame) = recv(&mut socket).await else {
        panic!("Ack")
    };
    assert!(matches!(decode_server(&frame), Ok(ServerMsg::Ack { seqs, .. }) if seqs == vec![1]));
    let Message::Binary(_) = recv(&mut socket).await else {
        panic!("echo")
    };

    // A frame slightly over the protocol limit still reaches the session, which answers TooLarge.
    socket
        .send(Message::Binary(vec![1; MAX_FRAME_BYTES + 1].into()))
        .await
        .unwrap();
    let Message::Binary(frame) = recv(&mut socket).await else {
        panic!("Error")
    };
    assert!(matches!(
        decode_server(&frame),
        Ok(ServerMsg::Error {
            code: ErrorCode::TooLarge,
            ..
        })
    ));
    let Message::Close(Some(close)) = recv(&mut socket).await else {
        panic!("close")
    };
    assert_eq!(u16::from(close.code), 4002);

    server.shutdown().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn text_frames_never_reach_the_log() {
    let dir = std::env::temp_dir().join(format!("penwise-sync-text-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = serve(listener, ServerConfig::new(&dir, super::TOKEN))
        .await
        .unwrap();
    let url = format!("ws://{}/sync", server.addr);
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let hello_frame = encode_client(&hello(device(2), ClientKind::Tablet, 0)).unwrap();
    socket
        .send(Message::Binary(hello_frame.into()))
        .await
        .unwrap();
    recv(&mut socket).await;
    recv(&mut socket).await;
    socket
        .send(Message::Text("{\"op\":\"clear\"}".into()))
        .await
        .unwrap();
    let Message::Binary(frame) = recv(&mut socket).await else {
        panic!("Error")
    };
    assert!(matches!(
        decode_server(&frame),
        Ok(ServerMsg::Error {
            code: ErrorCode::Malformed,
            ..
        })
    ));
    assert_eq!(server.storage.status().await.unwrap().head_seq, 0);
    server.shutdown().await;
    std::fs::remove_dir_all(&dir).ok();
}
