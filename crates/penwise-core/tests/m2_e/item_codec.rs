//! `PWIT` item blob: round trips, canonical bytes, and every error from crafted input.

use super::fixtures::{every_kind, fill, id, image, piece, shape, text};
use penwise_core::CodecError;
use penwise_core::codec::item::{
    ITEM_MAGIC, ITEM_VERSION, decode_item, decode_item_body, decode_points, encode_item,
    encode_item_body, encode_points,
};
use penwise_core::codec::varint::{Reader, write_sv32, write_uv32};
use penwise_core::hash::fnv1a64;
use penwise_core::item::{Item, ItemKind, ShapeKind};
use penwise_core::smoothing::{HISTORY_LEN, SmootherState};

pub fn blob(item: &Item) -> Vec<u8> {
    let mut out = Vec::new();
    encode_item(item, &mut out);
    out
}

/// Replaces the trailer with the checksum of the (modified) content, so field validation is reached.
pub fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
    let content = bytes.len() - 8;
    bytes.truncate(content);
    let hash = fnv1a64(&bytes);
    bytes.extend_from_slice(&hash.to_le_bytes());
    bytes
}

/// Offset of the first occurrence of `needle` in `hay`.
fn find(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len())
        .position(|w| w == needle)
        .expect("needle present")
}

/// Byte offsets inside the blob: header 6, id 16, kind 1, layer 1, z 8, transform 24.
const KIND_AT: usize = 6 + 16;
const LAYER_AT: usize = KIND_AT + 1;
const TRANSFORM_AT: usize = LAYER_AT + 1 + 8;

#[test]
fn every_kind_round_trips_bit_exactly_and_canonically() {
    for item in every_kind() {
        let bytes = blob(&item);
        assert_eq!(&bytes[..4], &ITEM_MAGIC);
        assert_eq!(bytes[4..6], ITEM_VERSION.to_le_bytes());
        let decoded = decode_item(&bytes).unwrap_or_else(|e| panic!("{:?}: {e}", item.id));
        assert_eq!(decoded, item);
        assert_eq!(blob(&decoded), bytes, "encoding is canonical");
    }
}

#[test]
fn body_round_trips_and_is_the_blob_interior() {
    for item in every_kind() {
        let mut body = Vec::new();
        encode_item_body(&item, &mut body);
        let bytes = blob(&item);
        assert_eq!(&bytes[6..bytes.len() - 8], body.as_slice());
        let mut r = Reader::new(&body);
        assert_eq!(decode_item_body(&mut r).unwrap(), item);
        assert_eq!(r.remaining(), 0);
    }
}

#[test]
fn encode_appends_after_existing_bytes() {
    let item = fill(id(5), 3);
    let mut out = vec![0xAA, 0xBB];
    encode_item(&item, &mut out);
    assert_eq!(&out[..2], &[0xAA, 0xBB]);
    assert_eq!(decode_item(&out[2..]).unwrap(), item);
}

#[test]
fn negative_zero_encodes_as_positive_zero() {
    let item = text(id(6), 1, Vec::new());
    let mut negative = item.clone();
    negative.transform.b = -0.0;
    negative.transform.c = -0.0;
    assert_eq!(blob(&negative), blob(&item));
}

#[test]
fn points_round_trip_on_the_grid_and_snap_off_it() {
    let on_grid = vec![
        [0.0, 0.0],
        [1.0625, -3.5],
        [-1_048_576.0, 1_048_576.0],
        [7.0, 7.0],
    ];
    let mut out = Vec::new();
    encode_points(&on_grid, &mut out);
    let mut decoded = Vec::new();
    decode_points(&mut Reader::new(&out), &mut decoded).unwrap();
    assert_eq!(decoded, on_grid);

    let off_grid = [[0.04, -0.04], [f32::NAN, 2.0e7], [f32::NEG_INFINITY, 0.09]];
    let mut out = Vec::new();
    encode_points(&off_grid, &mut out);
    let mut decoded = vec![[9.0, 9.0]];
    decode_points(&mut Reader::new(&out), &mut decoded).unwrap();
    let expected = vec![
        [9.0, 9.0],
        [0.0625, -0.0625],
        [0.0, 1_048_576.0],
        [-1_048_576.0, 0.0625],
    ];
    assert_eq!(decoded, expected, "appends; NaN → 0, clamped, rounded");
}

#[test]
fn points_beyond_the_position_limit_are_out_of_range() {
    let mut bytes = Vec::new();
    write_uv32(&mut bytes, 2);
    write_sv32(&mut bytes, 1 << 24);
    write_sv32(&mut bytes, 0);
    write_sv32(&mut bytes, 1);
    write_sv32(&mut bytes, 0);
    let err = decode_points(&mut Reader::new(&bytes), &mut Vec::new()).unwrap_err();
    assert_eq!(err, CodecError::OutOfRange { field: "point.x" });
}

#[test]
fn overlong_count_varint_overflows() {
    let bytes = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01];
    let err = decode_points(&mut Reader::new(&bytes), &mut Vec::new()).unwrap_err();
    assert!(matches!(err, CodecError::VarintOverflow { .. }), "{err:?}");
}

#[test]
fn hostile_point_count_does_not_preallocate() {
    let mut bytes = Vec::new();
    write_uv32(&mut bytes, u32::MAX);
    let mut out = Vec::new();
    let err = decode_points(&mut Reader::new(&bytes), &mut out).unwrap_err();
    assert!(matches!(err, CodecError::UnexpectedEof { .. }), "{err:?}");
    assert!(out.capacity() < 16, "capacity {}", out.capacity());
}

#[test]
fn hostile_counts_inside_items_are_eof_errors() {
    // Text without sources ends in `count 0`; fill ends in its polygon list.
    let bytes = blob(&text(id(7), 1, Vec::new()));
    let at = bytes.len() - 9;
    assert_eq!(bytes[at], 0);
    let mut hostile = bytes[..at].to_vec();
    hostile.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
    hostile.extend_from_slice(&bytes[at + 1..]);
    let bytes = hostile;
    let err = decode_item(&reseal(bytes)).unwrap_err();
    assert!(matches!(err, CodecError::UnexpectedEof { .. }), "{err:?}");

    let item = fill(id(5), 0);
    let mut body = Vec::new();
    encode_item_body(&item, &mut body);
    // id, kind, layer, z, transform, paint (4 + 4), then the polygon count.
    let count_at = 16 + 1 + 1 + 8 + 24 + 8;
    assert_eq!(body[count_at], 2);
    body.truncate(count_at);
    write_uv32(&mut body, u32::MAX);
    let err = decode_item_body(&mut Reader::new(&body)).unwrap_err();
    assert!(matches!(err, CodecError::UnexpectedEof { .. }), "{err:?}");
}

#[test]
fn every_truncation_is_an_error() {
    for item in every_kind() {
        let bytes = blob(&item);
        for len in 0..bytes.len() {
            assert!(
                decode_item(&bytes[..len]).is_err(),
                "{:?} len {len}",
                item.id
            );
        }
    }
}

#[test]
fn every_flipped_byte_is_an_error() {
    let bytes = blob(&shape(id(20), 4, ShapeKind::Ellipse));
    for at in 0..bytes.len() {
        let mut bad = bytes.clone();
        bad[at] ^= 0x10;
        assert!(decode_item(&bad).is_err(), "byte {at}");
    }
    let mut bad = bytes.clone();
    bad[8] ^= 1; // inside the id: structurally fine, caught by the trailer
    assert!(matches!(
        decode_item(&bad),
        Err(CodecError::ChecksumMismatch { .. })
    ));
}

#[test]
fn header_and_trailer_errors() {
    let bytes = blob(&fill(id(5), 0));
    let mut bad = bytes.clone();
    bad[..4].copy_from_slice(b"PWSK");
    assert_eq!(
        decode_item(&bad),
        Err(CodecError::BadMagic { found: *b"PWSK" })
    );
    let mut bad = bytes.clone();
    bad[4..6].copy_from_slice(&2_u16.to_le_bytes());
    assert_eq!(decode_item(&bad), Err(CodecError::UnsupportedVersion(2)));
    let mut bad = bytes.clone();
    bad.push(0);
    assert_eq!(
        decode_item(&bad),
        Err(CodecError::TrailingBytes { count: 1 })
    );
    assert!(matches!(
        decode_item(&[]),
        Err(CodecError::UnexpectedEof { offset: 0 })
    ));
}

#[test]
fn invalid_tags_and_values() {
    let bytes = blob(&fill(id(5), 0));
    let mut bad = bytes.clone();
    bad[KIND_AT] = 9;
    assert_eq!(
        decode_item(&reseal(bad.clone())),
        Err(CodecError::InvalidValue { field: "kind" })
    );
    assert!(
        matches!(decode_item(&bad), Err(CodecError::ChecksumMismatch { .. })),
        "the checksum is verified before field validation"
    );
    let mut bad = bytes.clone();
    bad[LAYER_AT] = 3;
    assert_eq!(
        decode_item(&reseal(bad)),
        Err(CodecError::InvalidValue { field: "layer" })
    );
    let mut bad = bytes.clone();
    bad[TRANSFORM_AT..TRANSFORM_AT + 4].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
    assert_eq!(
        decode_item(&reseal(bad)),
        Err(CodecError::InvalidValue { field: "transform" })
    );
}

#[test]
fn bad_utf8_flags_and_shape_tags_are_invalid() {
    let bytes = blob(&text(id(6), 1, Vec::new()));
    let at = find(&bytes, "Grüße".as_bytes());
    let mut bad = bytes.clone();
    bad[at + 2] = 0xFF;
    assert_eq!(
        decode_item(&reseal(bad)),
        Err(CodecError::InvalidValue { field: "text" })
    );

    // Filled shape: blob ends in `fill flag 1, r g b a`, then the trailer.
    let bytes = blob(&shape(id(21), 4, ShapeKind::Rect));
    let flag_at = bytes.len() - 8 - 5;
    assert_eq!(bytes[flag_at], 1);
    let mut bad = bytes.clone();
    bad[flag_at] = 2;
    assert_eq!(
        decode_item(&reseal(bad)),
        Err(CodecError::InvalidValue { field: "fill flag" })
    );
    // Shape kind tag follows the transform.
    let kind_at = TRANSFORM_AT + 24;
    assert_eq!(bytes[kind_at], 0);
    let mut bad = bytes.clone();
    bad[kind_at] = 6;
    assert_eq!(
        decode_item(&reseal(bad)),
        Err(CodecError::InvalidValue {
            field: "shape.kind"
        })
    );
}

#[test]
fn items_failing_validation_are_invalid() {
    let mut item = text(id(6), 1, Vec::new());
    if let penwise_core::item::ItemKind::Text(t) = &mut item.kind {
        t.font_size = 0.0;
    }
    let err = decode_item(&blob(&item)).unwrap_err();
    assert!(matches!(err, CodecError::InvalidValue { .. }), "{err:?}");

    let mut item = fill(id(5), 0);
    if let penwise_core::item::ItemKind::Fill(f) = &mut item.kind {
        f.polygons.clear();
    }
    let err = decode_item(&blob(&item)).unwrap_err();
    assert!(matches!(err, CodecError::InvalidValue { .. }), "{err:?}");
}

#[test]
fn image_blob_carries_the_asset_hash_and_size_verbatim() {
    let item = image(id(9), -5);
    let bytes = blob(&item);
    let ItemKind::Image(data) = &item.kind else {
        panic!("image fixture")
    };
    // Kind data is the last thing before the 8-byte trailer: 32 hash bytes + two f32.
    let at = bytes.len() - 8 - 40;
    assert_eq!(&bytes[at..at + 32], &data.asset);
    assert_eq!(bytes[KIND_AT], 5, "Image kind tag");
}

#[test]
fn degenerate_pictures_are_invalid() {
    for (w, h) in [(0.0, 10.0), (10.0, 0.0), (-1.0, 10.0), (2.0e6, 10.0)] {
        let mut item = image(id(9), 0);
        if let ItemKind::Image(data) = &mut item.kind {
            data.w = w;
            data.h = h;
        }
        let err = decode_item(&blob(&item)).unwrap_err();
        assert_eq!(
            err,
            CodecError::InvalidValue {
                field: "image_size"
            },
            "{w}x{h}"
        );
    }
}

#[test]
fn hostile_warm_state_is_rejected_on_decode() {
    let good = piece(id(3), id(1), 1, 10.0);
    let tweak = |f: &dyn Fn(&mut SmootherState)| {
        let mut item = good.clone();
        if let ItemKind::Stroke(s) = &mut item.kind
            && let Some(warm) = s.trim.as_mut().and_then(|t| t.warm.as_mut())
        {
            f(warm);
        }
        decode_item(&blob(&item))
    };
    assert!(tweak(&|_| {}).is_ok());
    let bad = Err(CodecError::InvalidValue { field: "trim" });
    assert_eq!(tweak(&|w| w.position.dt_s = 0.0), bad);
    assert_eq!(tweak(&|w| w.pressure.dt_s = 1.0e9), bad);
    assert_eq!(tweak(&|w| w.position.dx = 1.0e12), bad);
    assert_eq!(tweak(&|w| w.history.clear()), bad);
    assert_eq!(
        tweak(&|w| w.history.reverse()),
        bad,
        "timestamps must not decrease"
    );
    assert!(
        matches!(
            tweak(&|w| w.history = vec![w.history[0]; HISTORY_LEN + 1]),
            Err(CodecError::InvalidValue { .. })
        ),
        "more history than the speed window holds"
    );
}
