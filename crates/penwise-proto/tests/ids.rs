//! ULID identities: layout, text form, parsing.
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

use common::Rng;
use penwise_core::item::ItemId;
use penwise_proto::ids::{ID_TEXT_LEN, MAX_ID_TIMESTAMP_MS};
use penwise_proto::{DeviceId, ItemRef, OpId, PageId};

#[test]
fn layout_is_timestamp_then_random_big_endian() {
    let id = OpId::from_parts(0x0102_0304_0506, 0x0A0B_0C0D_0E0F_1011_1213);
    assert_eq!(
        id.to_bytes(),
        [
            1, 2, 3, 4, 5, 6, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13
        ]
    );
    assert_eq!(id.timestamp_ms(), 0x0102_0304_0506);
}

#[test]
fn timestamp_saturates_and_random_is_masked_to_80_bits() {
    let id = DeviceId::from_parts(u64::MAX, u128::MAX);
    assert_eq!(id.timestamp_ms(), MAX_ID_TIMESTAMP_MS);
    assert_eq!(id.to_bytes()[6..], [0xFF; 10]);
}

#[test]
fn order_is_creation_time_order() {
    let early = PageId::from_parts(1_000, u128::MAX);
    let late = PageId::from_parts(1_001, 0);
    assert!(early < late);
}

#[test]
fn text_form_round_trips_for_random_ids() {
    let mut rng = Rng(11);
    for _ in 0..500 {
        let id = ItemRef::from_parts(rng.next_u64() >> 16, rng.u128());
        let text = id.to_string();
        assert_eq!(text.len(), ID_TEXT_LEN);
        assert_eq!(text.parse::<ItemRef>(), Ok(id));
        assert_eq!(text.to_lowercase().parse::<ItemRef>(), Ok(id));
    }
}

#[test]
fn known_ulid_text() {
    let zero = OpId::from_parts(0, 0);
    assert_eq!(zero.to_string(), "00000000000000000000000000");
    let max = OpId([0xFF; 16]);
    assert_eq!(max.to_string(), "7ZZZZZZZZZZZZZZZZZZZZZZZZZ");
    assert_eq!("7ZZZZZZZZZZZZZZZZZZZZZZZZZ".parse::<OpId>(), Ok(max));
    // Crockford aliases: I, L → 1; O → 0.
    assert_eq!("0000000000000000000000000O".parse::<OpId>(), Ok(zero));
    assert_eq!(
        "0000000000000000000000000I".parse::<OpId>(),
        "00000000000000000000000001".parse::<OpId>()
    );
    assert_eq!(
        "0000000000000000000000000l".parse::<OpId>(),
        "00000000000000000000000001".parse::<OpId>()
    );
}

#[test]
fn parse_rejects_bad_text() {
    for bad in [
        "",
        "0000000000000000000000000",   // 25 chars
        "000000000000000000000000000", // 27 chars
        "80000000000000000000000000",  // overflows 128 bits
        "0000000000000000000000000U",  // U is not in the alphabet
        "0000000000000000000000000!",
        "０0000000000000000000000000", // non-ASCII
    ] {
        assert!(bad.parse::<OpId>().is_err(), "{bad:?}");
    }
}

#[test]
fn slices_and_core_item_ids_convert() {
    let id = DeviceId::from_parts(5, 6);
    assert_eq!(DeviceId::from_slice(&id.to_bytes()), Some(id));
    assert_eq!(DeviceId::from_slice(&[0; 15]), None);
    assert_eq!(DeviceId::from_slice(&[0; 17]), None);
    let item = ItemId([9; 16]);
    assert_eq!(ItemId::from(ItemRef::from(item)), item);
    assert!(format!("{id:?}").starts_with("DeviceId("));
}
