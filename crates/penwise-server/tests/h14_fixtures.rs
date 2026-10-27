//! H-14 (proto-fixture part): the frozen `penwise-proto/tests/fixtures/v1/*.bin` frames decode, re-encode to the
//! same bytes, and a real server understands the recorded client frames. (JVM/wasm decoding is S5.)
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

use std::path::PathBuf;
use std::time::Duration;

use common::TestServer;
use penwise_proto::{
    ClientMsg, ServerMsg, decode_client, decode_op, decode_server, encode_client, encode_op,
    encode_server,
};
use penwise_testkit::{RawSocket, Recv};

fn fixtures() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../penwise-proto/tests/fixtures/v1");
    let mut out: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "bin"))
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read(e.path()).expect("read fixture"),
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn every_v1_fixture_round_trips_byte_for_byte() {
    let all = fixtures();
    assert!(all.len() >= 20, "fixtures missing: {}", all.len());
    for (name, bytes) in all {
        let again = if name.starts_with("client_") {
            encode_client(&decode_client(&bytes).unwrap_or_else(|e| panic!("{name}: {e}")))
        } else if name.starts_with("server_") {
            encode_server(&decode_server(&bytes).unwrap_or_else(|e| panic!("{name}: {e}")))
        } else {
            encode_op(&decode_op(&bytes).unwrap_or_else(|e| panic!("{name}: {e}")))
        };
        assert_eq!(
            again.unwrap_or_else(|e| panic!("{name}: {e}")),
            bytes,
            "{name} re-encodes identically"
        );
    }
}

#[tokio::test]
async fn a_recorded_hello_gets_a_protocol_answer_from_a_live_server() {
    let server = TestServer::start().await;
    let (_, hello) = fixtures()
        .into_iter()
        .find(|(n, _)| n == "client_hello.bin")
        .expect("hello fixture");
    assert!(matches!(decode_client(&hello), Ok(ClientMsg::Hello { .. })));
    let mut socket = RawSocket::connect(&server.url()).await.expect("connect");
    socket.send_bytes(hello).await.expect("send");
    match socket.recv(Duration::from_secs(5)).await {
        Recv::Msg(ServerMsg::Welcome { .. } | ServerMsg::Reject { .. }) => {}
        other => {
            panic!("a valid recorded Hello must be answered with Welcome/Reject, got {other:?}")
        }
    }
}
