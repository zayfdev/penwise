//! Frame codec: round trips for every variant, v1 fixtures, rejection of bad frames, decode totality.
//!
//! Fixtures live in `tests/fixtures/v1/*.bin` (one per variant). They are frozen: a change that alters their
//! bytes breaks compatibility with shipped clients. Regenerate deliberately with
//! `PENWISE_BLESS=1 cargo test -p penwise-proto --test wire`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "test harness: failures are reported by panicking"
)]

mod common;
mod samples;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use common::Rng;
use penwise_proto::{
    AnyOp, ClientMsg, ErrorCode, FRAME_VERSION, MAX_FRAME_BYTES, ProtoError, ServerMsg,
    decode_client, decode_op, decode_server, encode_client, encode_op, encode_server,
};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1")
}

fn blessing() -> bool {
    std::env::var_os("PENWISE_BLESS").is_some()
}

/// Asserts `frame` equals the stored fixture `name` (or rewrites it when blessing).
fn check_fixture(name: &str, frame: &[u8]) -> Vec<u8> {
    let path = fixture_dir().join(format!("{name}.bin"));
    if blessing() {
        fs::create_dir_all(fixture_dir()).unwrap();
        fs::write(&path, frame).unwrap();
    }
    let stored = fs::read(&path).unwrap_or_else(|e| panic!("missing fixture {name}: {e}"));
    assert_eq!(stored, frame, "fixture {name} drifted from the encoder");
    stored
}

fn client_index(msg: &ClientMsg) -> usize {
    match msg {
        ClientMsg::Hello { .. } => 0,
        ClientMsg::PushOps { .. } => 1,
        ClientMsg::StrokeDelta(_) => 2,
        ClientMsg::StrokeCancel(_) => 3,
        ClientMsg::SwitchPage { .. } => 4,
        ClientMsg::Watch { .. } => 5,
        ClientMsg::Resync { .. } => 6,
        ClientMsg::Ping { .. } => 7,
        ClientMsg::Pong { .. } => 8,
    }
}

fn server_index(msg: &ServerMsg) -> usize {
    match msg {
        ServerMsg::Welcome { .. } => 0,
        ServerMsg::Reject { .. } => 1,
        ServerMsg::Ack { .. } => 2,
        ServerMsg::OpsSince { .. } => 3,
        ServerMsg::SnapshotBegin { .. } => 4,
        ServerMsg::SnapshotChunk(_) => 5,
        ServerMsg::SnapshotEnd { .. } => 6,
        ServerMsg::StrokeDelta { .. } => 7,
        ServerMsg::StrokeCancel { .. } => 8,
        ServerMsg::SwitchPage { .. } => 9,
        ServerMsg::Resync { .. } => 10,
        ServerMsg::Error { .. } => 11,
        ServerMsg::Ping { .. } => 12,
        ServerMsg::Pong { .. } => 13,
    }
}

fn op_index(op: &AnyOp) -> usize {
    match op {
        AnyOp::Item(_) => 0,
        AnyOp::Page(_) => 1,
        AnyOp::Notebook(_) => 2,
    }
}

#[test]
fn every_client_variant_round_trips_and_matches_its_fixture() {
    let mut covered = BTreeSet::new();
    for (name, msg) in samples::client_msgs() {
        covered.insert(client_index(&msg));
        let frame = encode_client(&msg).unwrap();
        assert_eq!(frame[0], FRAME_VERSION);
        assert_eq!(
            usize::from(frame[1]),
            client_index(&msg),
            "{name}: postcard discriminant"
        );
        assert_eq!(decode_client(&frame).unwrap(), msg, "{name}");
        let stored = check_fixture(name, &frame);
        assert_eq!(
            decode_client(&stored).unwrap(),
            msg,
            "{name}: fixture decodes"
        );
    }
    assert_eq!(covered.len(), 9, "a client variant has no sample");
}

#[test]
fn every_server_variant_round_trips_and_matches_its_fixture() {
    let mut covered = BTreeSet::new();
    for (name, msg) in samples::server_msgs() {
        covered.insert(server_index(&msg));
        let frame = encode_server(&msg).unwrap();
        assert_eq!(frame[0], FRAME_VERSION);
        assert_eq!(
            usize::from(frame[1]),
            server_index(&msg),
            "{name}: postcard discriminant"
        );
        assert_eq!(decode_server(&frame).unwrap(), msg, "{name}");
        let stored = check_fixture(name, &frame);
        assert_eq!(
            decode_server(&stored).unwrap(),
            msg,
            "{name}: fixture decodes"
        );
    }
    assert_eq!(covered.len(), 14, "a server variant has no sample");
}

#[test]
fn every_op_kind_round_trips_and_matches_its_fixture() {
    let mut covered = BTreeSet::new();
    for (name, op) in samples::ops() {
        covered.insert(op_index(&op));
        assert_eq!(usize::from(op.kind_code()), op_index(&op));
        let frame = encode_op(&op).unwrap();
        assert_eq!(usize::from(frame[1]), op_index(&op));
        assert_eq!(decode_op(&frame).unwrap(), op, "{name}");
        let stored = check_fixture(name, &frame);
        assert_eq!(decode_op(&stored).unwrap(), op, "{name}: fixture decodes");
    }
    assert_eq!(covered.len(), 3);
}

#[test]
fn fixture_directory_holds_exactly_the_sampled_frames() {
    let expected: BTreeSet<String> = samples::client_msgs()
        .iter()
        .map(|(n, _)| (*n).to_owned())
        .chain(samples::server_msgs().iter().map(|(n, _)| (*n).to_owned()))
        .chain(samples::ops().iter().map(|(n, _)| (*n).to_owned()))
        .map(|n| format!("{n}.bin"))
        .collect();
    let found: BTreeSet<String> = fs::read_dir(fixture_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(found, expected);
}

#[test]
fn bad_frames_are_rejected_with_typed_errors() {
    assert_eq!(decode_client(&[]), Err(ProtoError::Empty));
    assert_eq!(decode_server(&[]), Err(ProtoError::Empty));
    let ping = encode_client(&ClientMsg::Ping { nonce: 5 }).unwrap();
    for version in [0_u8, 2, 255] {
        let mut frame = ping.clone();
        frame[0] = version;
        assert_eq!(
            decode_client(&frame),
            Err(ProtoError::UnsupportedVersion(version))
        );
    }
    let mut trailing = ping.clone();
    trailing.extend_from_slice(&[0, 0, 0]);
    assert_eq!(
        decode_client(&trailing),
        Err(ProtoError::TrailingBytes { count: 3 })
    );
    // Unknown variant discriminant.
    assert!(matches!(
        decode_client(&[FRAME_VERSION, 0x7F]),
        Err(ProtoError::Decode(_))
    ));
    // Oversized input is refused before decoding.
    let big = vec![FRAME_VERSION; MAX_FRAME_BYTES + 1];
    assert_eq!(
        decode_client(&big),
        Err(ProtoError::TooLarge {
            len: MAX_FRAME_BYTES + 1
        })
    );
    // Oversized output is refused after encoding.
    let huge = ServerMsg::Error {
        code: ErrorCode::Internal,
        detail: "x".repeat(MAX_FRAME_BYTES),
    };
    assert!(matches!(
        encode_server(&huge),
        Err(ProtoError::TooLarge { .. })
    ));
}

#[test]
fn every_strict_prefix_of_every_frame_is_rejected() {
    // A truncated frame of a decoder's own type never decodes.
    for (name, msg) in samples::client_msgs() {
        let frame = encode_client(&msg).unwrap();
        for cut in 0..frame.len() {
            assert!(decode_client(&frame[..cut]).is_err(), "{name} cut at {cut}");
        }
    }
    for (name, msg) in samples::server_msgs() {
        let frame = encode_server(&msg).unwrap();
        for cut in 0..frame.len() {
            assert!(decode_server(&frame[..cut]).is_err(), "{name} cut at {cut}");
        }
    }
    for (name, op) in samples::ops() {
        let frame = encode_op(&op).unwrap();
        for cut in 0..frame.len() {
            assert!(decode_op(&frame[..cut]).is_err(), "{name} cut at {cut}");
        }
    }
}

/// Totality: decoding arbitrary bytes returns `Ok` or `Err`, never panics, and an `Ok` value re-encodes to a
/// frame that decodes to the same value.
fn probe(bytes: &[u8]) {
    if let Ok(msg) = decode_client(bytes) {
        assert_eq!(decode_client(&encode_client(&msg).unwrap()).unwrap(), msg);
    }
    if let Ok(msg) = decode_server(bytes) {
        assert_eq!(decode_server(&encode_server(&msg).unwrap()).unwrap(), msg);
    }
    if let Ok(op) = decode_op(bytes) {
        assert_eq!(decode_op(&encode_op(&op).unwrap()).unwrap(), op);
    }
}

#[test]
fn decoders_are_total_on_random_bytes() {
    let mut rng = Rng(0x00DE_C0DE);
    for _ in 0..30_000 {
        let len = rng.below(300);
        let mut bytes: Vec<u8> = (0..len).map(|_| rng.next_u64().to_le_bytes()[0]).collect();
        if !bytes.is_empty() && rng.chance(80) {
            bytes[0] = FRAME_VERSION;
        }
        if bytes.len() > 1 && rng.chance(60) {
            bytes[1] %= 16;
        }
        probe(&bytes);
    }
}

#[test]
fn decoders_are_total_on_mutated_valid_frames() {
    let mut frames: Vec<Vec<u8>> = Vec::new();
    for (_, msg) in samples::client_msgs() {
        frames.push(encode_client(&msg).unwrap());
    }
    for (_, msg) in samples::server_msgs() {
        frames.push(encode_server(&msg).unwrap());
    }
    for (_, op) in samples::ops() {
        frames.push(encode_op(&op).unwrap());
    }
    let mut rng = Rng(0xF1ED);
    for round in 0..40_000 {
        let mut frame = frames[round % frames.len()].clone();
        for _ in 0..=rng.below(3) {
            let at = rng.below(frame.len());
            match rng.below(3) {
                0 => frame[at] ^= 1 << rng.below(8),
                1 => frame[at] = rng.next_u64().to_le_bytes()[0],
                _ => {
                    frame.truncate(at.max(1));
                }
            }
        }
        probe(&frame);
    }
}

#[test]
fn a_corrupted_edit_blob_inside_an_op_is_a_decode_error() {
    let frame = encode_op(&samples::op_item_add()).unwrap();
    // The PWED blob starts with its magic; flip a byte inside it (after the magic, before the checksum).
    let at = frame.windows(4).position(|w| w == b"PWED").unwrap() + 10;
    let mut bad = frame.clone();
    bad[at] ^= 0xFF;
    assert!(matches!(decode_op(&bad), Err(ProtoError::Decode(_))));
}

#[test]
fn push_and_delta_limits_are_checked_by_within_limits() {
    assert!(samples::full_push().within_limits());
    let ClientMsg::PushOps { mut ops } = samples::full_push() else {
        unreachable!()
    };
    ops.push(samples::op_item_delete());
    assert!(!ClientMsg::PushOps { ops }.within_limits());
    assert!(!ClientMsg::PushOps { ops: Vec::new() }.within_limits());
    assert!(ClientMsg::StrokeDelta(samples::full_delta()).within_limits());
    let mut over = samples::full_delta();
    over.samples.push(over.samples[0]);
    assert!(!ClientMsg::StrokeDelta(over).within_limits());
    let mut empty = samples::full_delta();
    empty.samples.clear();
    assert!(!ClientMsg::StrokeDelta(empty).within_limits());
    // A full push still fits a frame.
    assert!(encode_client(&samples::full_push()).is_ok());
}

#[test]
fn the_token_never_appears_in_debug_output() {
    let hello = encode_client(&samples::client_msgs()[0].1).unwrap();
    let msg = decode_client(&hello).unwrap();
    let text = format!("{msg:?}");
    assert!(!text.contains("tttt"));
    assert!(text.contains("redacted"));
}
