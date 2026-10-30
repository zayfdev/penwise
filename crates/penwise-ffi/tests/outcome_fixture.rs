//! The `Outcome` blobs the tablet parses, written by the Rust encoder and checked in (`tests/fixtures/outcome_*.bin`).
//! The Kotlin side (`OutcomeFocusTest`) reads the same files, so the two layouts cannot drift apart unnoticed.
//! `PENWISE_UPDATE_FIXTURES=1 cargo test -p penwise-ffi --test outcome_fixture` rewrites them after a deliberate
//! format change (bump `OUTCOME_VERSION` too).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use penwise_core::item::ItemId;
use penwise_ffi::sync_core::{Change, Outcome};
use penwise_proto::PageId;

fn bytes(base: u8) -> [u8; 16] {
    std::array::from_fn(|i| base + u8::try_from(i).unwrap())
}

fn page(base: u8) -> PageId {
    PageId::from_slice(&bytes(base)).unwrap()
}

fn item(base: u8) -> ItemId {
    ItemId(bytes(base))
}

fn check(name: &str, outcome: &Outcome) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let blob = outcome.encode();
    if std::env::var_os("PENWISE_UPDATE_FIXTURES").is_some() || !path.exists() {
        std::fs::write(&path, &blob).unwrap();
    }
    assert_eq!(
        std::fs::read(&path).unwrap(),
        blob,
        "{name} drifted from the encoder"
    );
}

/// A `Focus` that names a frame, after two scene changes and two replies, with every byte distinct.
#[test]
fn a_focus_outcome_with_changes_and_replies() {
    check(
        "outcome_focus_item.bin",
        &Outcome {
            kind: 14,
            head_seq: 0x0102_0304_0506_0708,
            applied: 3,
            page: Some(page(0x10)),
            replies: vec![vec![0xA1, 0xA2, 0xA3], vec![0xB1]],
            changes: vec![
                Change {
                    page: page(0x30),
                    item: Some(item(0x50)),
                },
                Change {
                    page: page(0x90),
                    item: None,
                },
            ],
            focus_item: Some(item(0x70)),
            ..Outcome::default()
        },
    );
}

/// A `Focus` for the page's content: no item.
#[test]
fn a_focus_outcome_for_the_pages_content() {
    check(
        "outcome_focus_page.bin",
        &Outcome {
            kind: 14,
            page: Some(page(0x10)),
            ..Outcome::default()
        },
    );
}

/// A protocol-2 `Welcome`.
#[test]
fn a_welcome_that_allows_focus() {
    check(
        "outcome_welcome_focus.bin",
        &Outcome {
            kind: 0,
            head_seq: 9,
            page: Some(page(0x10)),
            focus_supported: true,
            ..Outcome::default()
        },
    );
}
