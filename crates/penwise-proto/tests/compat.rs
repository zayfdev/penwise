//! Envelope compatibility: every frame fixture of the previous version (`tests/fixtures/v1/`, checked in with
//! the first release and never regenerated) still decodes under the current envelope, keeps its version byte,
//! and re-encodes byte for byte, because the writer picks the smallest envelope version that carries a value.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test harness: failures are reported by panicking"
)]

use std::fs;
use std::path::PathBuf;

use penwise_proto::{
    ClientMsg, FRAME_VERSION, FRAME_VERSION_V1, ProtoError, ServerMsg, decode_client, decode_op,
    decode_server, encode_client, encode_op, encode_server,
};

fn fixtures(version: &str) -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(version);
    let mut out: Vec<(String, Vec<u8>)> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            (name, fs::read(&path).unwrap())
        })
        .collect();
    out.sort();
    out
}

#[test]
fn every_v1_fixture_decodes_and_re_encodes_unchanged() {
    let frames = fixtures("v1");
    assert!(frames.len() >= 20, "the v1 fixture set is checked in");
    for (name, bytes) in frames {
        assert_eq!(bytes[0], FRAME_VERSION_V1, "{name}: written by envelope v1");
        let re_encoded = if name.starts_with("client_") {
            encode_client(&decode_client(&bytes).unwrap()).unwrap()
        } else if name.starts_with("server_") {
            encode_server(&decode_server(&bytes).unwrap()).unwrap()
        } else {
            encode_op(&decode_op(&bytes).unwrap()).unwrap()
        };
        assert_eq!(re_encoded, bytes, "{name}: still the same bytes");
    }
}

#[test]
fn v1_variant_indices_are_unchanged_by_focus() {
    // Focus is appended last: the discriminants the v1 fixtures pin keep their meaning.
    let switch = fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1/client_switch_page.bin"),
    )
    .unwrap();
    assert!(matches!(
        decode_client(&switch).unwrap(),
        ClientMsg::SwitchPage { .. }
    ));
    let pong = fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1/server_pong.bin"),
    )
    .unwrap();
    assert!(matches!(
        decode_server(&pong).unwrap(),
        ServerMsg::Pong { .. }
    ));
}

#[test]
fn v2_fixtures_carry_the_current_version_and_older_ones_do_not_leak_in() {
    for (name, bytes) in fixtures("v2") {
        assert_eq!(bytes[0], FRAME_VERSION, "{name}");
    }
    // A version this build does not know is still refused.
    let mut frame = fixtures("v1").remove(0).1;
    frame[0] = FRAME_VERSION + 1;
    assert_eq!(
        decode_client(&frame),
        Err(ProtoError::UnsupportedVersion(FRAME_VERSION + 1))
    );
}
