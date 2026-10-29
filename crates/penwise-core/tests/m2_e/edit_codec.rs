//! `PWED` edit blob: round trips for every edit, item kind and `Field`; canonical bytes; crafted errors.

use super::fixtures::{BLUE, RED, every_kind, id, shape_kinds, square};
use super::item_codec::reseal;
use penwise_core::codec::edit::{
    EDIT_MAGIC, EDIT_VERSION, EDIT_VERSION_V1, decode_edit, decode_edit_body, encode_edit,
    encode_edit_body,
};
use penwise_core::codec::item::{ITEM_MAGIC, encode_item};
use penwise_core::codec::varint::{Reader, write_uv32};
use penwise_core::edit::{Edit, Field, ItemPatch};
use penwise_core::item::{Affine, FramePreset, FrameSize, Layer, Paint, Polygon, ShapeKind};
use penwise_core::{CodecError, Preset};

fn blob(edit: &Edit) -> Vec<u8> {
    let mut out = Vec::new();
    encode_edit(edit, &mut out);
    out
}

/// The version [`encode_edit`] must pick for `edit` (minimal: v2 only for a frame field).
fn expected_version(edit: &Edit) -> u16 {
    match edit {
        Edit::Update { patch, .. } if patch.fields().iter().any(|field| field.code() >= 13) => {
            EDIT_VERSION
        }
        _ => EDIT_VERSION_V1,
    }
}

fn assert_round_trip(edit: &Edit) {
    let bytes = blob(edit);
    assert_eq!(&bytes[..4], &EDIT_MAGIC);
    assert_eq!(bytes[4..6], expected_version(edit).to_le_bytes());
    assert_eq!(decode_edit(&bytes).as_ref(), Ok(edit));
    assert_eq!(blob(&decode_edit(&bytes).unwrap()), bytes, "canonical");
    let mut body = Vec::new();
    encode_edit_body(edit, &mut body);
    assert_eq!(&bytes[6..bytes.len() - 8], body.as_slice());
    let mut r = Reader::new(&body);
    assert_eq!(decode_edit_body(&mut r).as_ref(), Ok(edit));
    assert_eq!(r.remaining(), 0);
}

fn update(patch: ItemPatch) -> Edit {
    Edit::Update { id: id(42), patch }
}

/// A patch setting only `field` (`variant` picks among a field's alternative values).
fn single(field: Field, variant: usize) -> ItemPatch {
    let mut p = ItemPatch::default();
    match field {
        Field::Deleted => p.deleted = Some(variant.is_multiple_of(2)),
        Field::Layer => p.layer = Some([Layer::Ink, Layer::Agent, Layer::Text][variant % 3]),
        Field::Z => p.z = Some([i64::MIN, -1, 0, i64::MAX][variant % 4]),
        Field::Transform => {
            p.transform = Some(Affine {
                a: 0.5,
                b: -0.25,
                c: 2.0,
                d: 1.0e-3,
                e: -123.5,
                f: 9.0e5,
            });
        }
        Field::Brush => p.brush = Some(Preset::ALL[variant % Preset::ALL.len()].params()),
        Field::ShapeFill => p.shape_fill = Some([None, Some(RED)][variant % 2]),
        Field::Paint => {
            p.paint = Some(Paint {
                color: BLUE,
                opacity: 0.5,
            });
        }
        Field::Polygons => {
            p.polygons = Some(vec![Polygon {
                outer: square(-8.0, 4.0, 32.0),
                holes: vec![square(0.0, 8.0, 2.0)],
            }]);
        }
        Field::ShapeKind => {
            p.shape_kind = Some(shape_kinds()[variant % shape_kinds().len()].clone());
        }
        Field::ShapeSize => p.shape_size = Some([0.0, 77.25]),
        Field::Text => p.text = Some(["", "hello ✍"][variant % 2].to_owned()),
        Field::TextBox => p.text_box = Some([300.0, 0.5]),
        Field::FontSize => p.font_size = Some(11.0),
        Field::FramePreset => {
            p.frame_preset = Some(FrameSize {
                preset: FramePreset::ALL[variant % FramePreset::ALL.len()],
                css_w: 390 + u32::try_from(variant).unwrap(),
                css_h: 800,
            });
        }
        Field::FrameRoute => p.frame_route = Some(["", "/a?b=c"][variant % 2].to_owned()),
    }
    p
}

#[test]
fn add_of_every_kind_and_delete_round_trip() {
    for item in every_kind() {
        assert_round_trip(&Edit::Add(item));
    }
    assert_round_trip(&Edit::Delete(id(9)));
}

#[test]
fn every_field_round_trips_alone_and_together() {
    let mut all = ItemPatch::default();
    for field in Field::ALL {
        for variant in 0..7 {
            assert_round_trip(&update(single(field, variant)));
        }
        all.merge(&single(field, 1));
    }
    assert_round_trip(&update(all));
    assert_round_trip(&update(ItemPatch::default()));
}

#[test]
fn mask_bit_is_the_field_code() {
    for field in Field::ALL {
        let bytes = blob(&update(single(field, 0)));
        // header 6, tag 1, id 16, then the mask.
        let mut r = Reader::new(&bytes[6 + 1 + 16..]);
        assert_eq!(r.uv32().unwrap(), 1 << field.code(), "{field:?}");
    }
}

#[test]
fn patches_are_decoded_without_applicability_checks() {
    let mut p = single(Field::FontSize, 0);
    p.font_size = Some(-4.0);
    p.shape_kind = Some(ShapeKind::Polygon { points: Vec::new() });
    assert_round_trip(&update(p));
}

#[test]
fn every_truncation_is_an_error() {
    let mut all = ItemPatch::default();
    for field in Field::ALL {
        all.merge(&single(field, 3));
    }
    for edit in [
        update(all),
        Edit::Delete(id(1)),
        Edit::Add(every_kind().remove(2)),
    ] {
        let bytes = blob(&edit);
        for len in 0..bytes.len() {
            assert!(decode_edit(&bytes[..len]).is_err(), "len {len}");
        }
    }
}

#[test]
fn header_trailer_and_checksum_errors() {
    let bytes = blob(&Edit::Delete(id(3)));
    let mut item_blob = Vec::new();
    encode_item(&every_kind().remove(0), &mut item_blob);
    assert_eq!(
        decode_edit(&item_blob),
        Err(CodecError::BadMagic { found: ITEM_MAGIC })
    );
    let mut bad = bytes.clone();
    bad[4..6].copy_from_slice(&0_u16.to_le_bytes());
    assert_eq!(decode_edit(&bad), Err(CodecError::UnsupportedVersion(0)));
    let mut bad = bytes.clone();
    bad.extend_from_slice(&[1, 2]);
    assert_eq!(
        decode_edit(&bad),
        Err(CodecError::TrailingBytes { count: 2 })
    );
    let mut bad = bytes.clone();
    bad[10] ^= 0x80;
    assert!(matches!(
        decode_edit(&bad),
        Err(CodecError::ChecksumMismatch { .. })
    ));
}

#[test]
fn unknown_tag_and_mask_bits_are_invalid() {
    let mut bad = blob(&Edit::Delete(id(3)));
    bad[6] = 3;
    assert_eq!(
        decode_edit(&reseal(bad)),
        Err(CodecError::InvalidValue { field: "edit tag" })
    );

    let bytes = blob(&update(ItemPatch::default()));
    let mask_at = 6 + 1 + 16;
    assert_eq!(bytes[mask_at], 0);
    let mut bad = bytes[..mask_at].to_vec();
    bad.extend_from_slice(&[0x80, 0x80, 0x02]); // bit 15 (two bits past the last field)
    bad.extend_from_slice(&bytes[mask_at + 1..]);
    assert_eq!(
        decode_edit(&reseal(bad)),
        Err(CodecError::InvalidValue {
            field: "field mask"
        })
    );
}

#[test]
fn blobs_use_the_minimal_version_and_older_blobs_still_decode() {
    let plain = update(ItemPatch {
        font_size: Some(12.0),
        ..ItemPatch::default()
    });
    let bytes = blob(&plain);
    assert_eq!(
        bytes[4..6],
        EDIT_VERSION_V1.to_le_bytes(),
        "an edit without frame fields stays v1, byte-compatible with frozen fixtures"
    );
    assert_eq!(decode_edit(&bytes), Ok(plain));
    assert_eq!(
        blob(&Edit::Add(every_kind().remove(0)))[4..6],
        EDIT_VERSION_V1.to_le_bytes()
    );

    let framed = update(single(Field::FrameRoute, 1));
    let bytes = blob(&framed);
    assert_eq!(bytes[4..6], EDIT_VERSION.to_le_bytes());
    assert_eq!(decode_edit(&bytes), Ok(framed));

    let mut v1_frame = bytes;
    v1_frame[4..6].copy_from_slice(&EDIT_VERSION_V1.to_le_bytes());
    assert_eq!(
        decode_edit(&reseal(v1_frame)),
        Err(CodecError::InvalidValue {
            field: "field mask"
        }),
        "mask bits 13/14 did not exist in v1"
    );

    let mut unknown = blob(&Edit::Delete(id(1)));
    unknown[4..6].copy_from_slice(&3_u16.to_le_bytes());
    assert_eq!(
        decode_edit(&reseal(unknown)),
        Err(CodecError::UnsupportedVersion(3))
    );
}

#[test]
fn invalid_values_inside_patches() {
    let bytes = blob(&update(single(Field::Deleted, 0)));
    let value_at = 6 + 1 + 16 + 1;
    assert_eq!(bytes[value_at], 1);
    let mut bad = bytes.clone();
    bad[value_at] = 2;
    assert_eq!(
        decode_edit(&reseal(bad)),
        Err(CodecError::InvalidValue { field: "deleted" })
    );

    let bytes = blob(&update(single(Field::Layer, 0)));
    let mut bad = bytes.clone();
    bad[value_at] = 200;
    assert_eq!(
        decode_edit(&reseal(bad)),
        Err(CodecError::InvalidValue { field: "layer" })
    );

    let bytes = blob(&update(single(Field::FontSize, 0)));
    let mut bad = bytes.clone();
    let float_at = value_at + 1; // bit 12 needs a 2-byte mask varint
    bad[float_at..float_at + 4].copy_from_slice(&f32::INFINITY.to_bits().to_le_bytes());
    assert_eq!(
        decode_edit(&reseal(bad)),
        Err(CodecError::InvalidValue {
            field: "text.font_size"
        })
    );
}

#[test]
fn hostile_polygon_count_is_an_eof_error() {
    let mut body = Vec::new();
    encode_edit_body(&update(ItemPatch::default()), &mut body);
    let mask_at = 1 + 16;
    body.truncate(mask_at);
    write_uv32(&mut body, 1 << Field::Polygons.code());
    write_uv32(&mut body, u32::MAX);
    let err = decode_edit_body(&mut Reader::new(&body)).unwrap_err();
    assert!(matches!(err, CodecError::UnexpectedEof { .. }), "{err:?}");
}
