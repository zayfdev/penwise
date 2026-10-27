//! Legacy converters: deterministic ids, signed-seed recovery, brush rule, quarantine of bad records.
//! Golden hashes were computed independently with Python's `hashlib` from the rules in `legacy.rs`.
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

use std::fmt::Write;

use common::{device, hlc, page_id};
use penwise_core::brush::BrushParams;
use penwise_core::codec::item::{decode_item, encode_item};
use penwise_core::item::{Affine, ItemKind, Layer, Z_STEP};
use penwise_core::sample::Sample;
use penwise_proto::legacy::{
    LegacyError, LegacyImage, LegacyStroke, image_to_item, legacy_add_op, legacy_item_id,
    legacy_notebook_id, legacy_op_id, legacy_page_id, legacy_seed, stroke_to_item,
};
use penwise_proto::{Replica, decode_op, encode_op};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn stroke_json(seed: &str) -> String {
    format!(
        r#"{{"preset":0,"argb":-16777216,"scale":1.5,"seed":"{seed}",
           "samples":[10.0,20.0,0.5,0.1,0.2, 11.3,21.7,0.6,0.1,0.2, 12.9,23.1,0.7,0.1,0.2],
           "times":[0,8000,16000]}}"#
    )
}

fn stroke(seed: &str) -> LegacyStroke {
    serde_json::from_str(&stroke_json(seed)).unwrap()
}

#[test]
fn json_stroke_defaults_to_the_ink_layer() {
    let s = stroke("5");
    assert_eq!(s.layer, "ink");
    assert_eq!((s.preset, s.argb, s.seed.as_str()), (0, -16_777_216, "5"));
    assert_eq!(s.times, vec![0, 8000, 16000]);
}

#[test]
fn ids_match_independently_computed_goldens() {
    assert_eq!(
        hex(&legacy_notebook_id("nb_1").to_bytes()),
        "000000000000ee111a9fe7f12183f718"
    );
    assert_eq!(
        hex(&legacy_page_id("pg_1").to_bytes()),
        "00000000000024d55c1657ace34b2205"
    );
    let item = legacy_item_id("pg_1", "42");
    assert_eq!(hex(&item.0), "0000000000006c80f9fedc7319e4ffc9");
    assert_eq!(
        hex(&legacy_op_id(item).to_bytes()),
        "a10ed64b549a5aede33d611dac27a701"
    );
}

#[test]
fn ids_are_deterministic_and_scoped() {
    assert_eq!(legacy_item_id("pg_1", "42"), legacy_item_id("pg_1", "42"));
    assert_ne!(legacy_item_id("pg_1", "42"), legacy_item_id("pg_2", "42"));
    assert_ne!(legacy_item_id("pg_1", "42"), legacy_item_id("pg_1", "43"));
    // The separator keeps ("pg_1", "2") distinct from ("pg_12", "") style splits.
    assert_ne!(legacy_item_id("pg_1", "2x"), legacy_item_id("pg_12", "x"));
    // Notebook, page and item domains never collide on the same text.
    assert_ne!(
        legacy_notebook_id("x").to_bytes(),
        legacy_page_id("x").to_bytes()
    );
    // ULID shape: timestamp 0.
    assert_eq!(legacy_page_id("pg_1").timestamp_ms(), 0);
}

#[test]
fn seeds_are_recovered_from_the_row_id() {
    assert_eq!(legacy_seed("0"), 0);
    assert_eq!(legacy_seed("7"), 7);
    // Negative: two's complement, exactly the tablet FFI's `Long → u64` cast (the old blob wrote 0).
    assert_eq!(legacy_seed("-1"), u64::MAX);
    assert_eq!(legacy_seed("-5"), 0xFFFF_FFFF_FFFF_FFFB);
    assert_eq!(legacy_seed("-9223372036854775808"), 1 << 63);
    assert_eq!(legacy_seed("9223372036854775807"), i64::MAX.cast_unsigned());
    // Above i64::MAX: plain u64.
    assert_eq!(legacy_seed("9223372036854775808"), 1 << 63);
    assert_eq!(legacy_seed("18446744073709551615"), u64::MAX);
    // Not a number: first 8 hash bytes (little endian).
    assert_eq!(legacy_seed("abc"), 10_549_905_418_062_144_916);
    assert_eq!(legacy_seed("abc"), legacy_seed("abc"));
    assert_ne!(legacy_seed("abc"), legacy_seed("abd"));
    assert_eq!(legacy_seed(""), legacy_seed(""));
}

#[test]
fn stroke_conversion_keeps_identity_seed_layer_z_and_quantizes_samples() {
    let mut s = stroke("-5");
    s.layer = "agent".into();
    s.times[0] = -3;
    let item = stroke_to_item("pg_1", &s, 3 * Z_STEP).unwrap();
    assert_eq!(item.id, legacy_item_id("pg_1", "-5"));
    assert_eq!(item.layer, Layer::Agent);
    assert_eq!(item.z, 3 * Z_STEP);
    assert_eq!(item.transform, Affine::IDENTITY);
    let ItemKind::Stroke(stroke) = &item.kind else {
        panic!("not a stroke")
    };
    assert_eq!(stroke.seed, 0xFFFF_FFFF_FFFF_FFFB);
    assert!(stroke.trim.is_none());
    assert_eq!(stroke.params, BrushParams::for_preset(0, 0xFF00_0000, 1.5));
    assert_eq!(stroke.samples.len(), 3);
    let first = Sample {
        x: 10.0,
        y: 20.0,
        pressure: 0.5,
        tilt: 0.1,
        azimuth: 0.2,
        t_us: 0,
    }
    .quantize();
    assert_eq!(
        stroke.samples[0], first,
        "negative time clamps to 0, then quantized"
    );
    assert_eq!(stroke.samples[2].t_us, 16_000);
    // Survives the core codec bit-exactly (seed included).
    let mut blob = Vec::new();
    encode_item(&item, &mut blob);
    assert_eq!(decode_item(&blob).unwrap(), item);
}

#[test]
fn seed_zero_and_non_numeric_ids_convert() {
    let zero = stroke_to_item("pg_1", &stroke("0"), Z_STEP).unwrap();
    let ItemKind::Stroke(z) = &zero.kind else {
        panic!()
    };
    assert_eq!(z.seed, 0);
    let named = stroke_to_item("pg_1", &stroke("stroke-a"), Z_STEP).unwrap();
    let ItemKind::Stroke(n) = &named.kind else {
        panic!()
    };
    assert_eq!(n.seed, legacy_seed("stroke-a"));
    assert_eq!(named.id, legacy_item_id("pg_1", "stroke-a"));
    assert_ne!(zero.id, named.id);
}

#[test]
fn conversion_is_deterministic_so_a_reimport_dedupes_by_op_id() {
    let a = stroke_to_item("pg_1", &stroke("123"), Z_STEP).unwrap();
    let b = stroke_to_item("pg_1", &stroke("123"), Z_STEP).unwrap();
    assert_eq!(a, b);
    let server = legacy_add_op(page_id(1), device(1), hlc(1_000, 0, 1), a);
    let tablet = legacy_add_op(page_id(1), device(2), hlc(5_000, 3, 2), b);
    assert_eq!(
        server.op_id(),
        tablet.op_id(),
        "same op id: the server logs it once"
    );
    assert_eq!(server.item(), tablet.item());
    assert_eq!(server.op_id(), legacy_op_id(legacy_item_id("pg_1", "123")));
    assert!(server.validate().is_ok());
    // The op travels the storage envelope unchanged.
    assert_eq!(decode_op(&encode_op(&server).unwrap()).unwrap(), server);
}

#[test]
fn imported_ops_apply_to_a_replica() {
    let mut replica = Replica::new();
    let mut clock = penwise_proto::HlcClock::new(device(1));
    let setup = [
        common::notebook_op(
            &mut clock,
            1_000,
            1,
            legacy_notebook_id("nb_1"),
            penwise_proto::NotebookChange::Create {
                title: "Old".into(),
                position: 0,
            },
        ),
        common::page_op(
            &mut clock,
            1_000,
            2,
            legacy_page_id("pg_1"),
            penwise_proto::PageChange::Create {
                notebook: legacy_notebook_id("nb_1"),
                title: String::new(),
                position: 0,
                kind: penwise_proto::PageKind::Infinite,
                background: penwise_proto::Background::Blank,
            },
        ),
    ];
    for op in &setup {
        replica.apply(op).unwrap();
    }
    for seed in ["-9", "0", "77"] {
        let item = stroke_to_item("pg_1", &stroke(seed), Z_STEP).unwrap();
        let op = legacy_add_op(legacy_page_id("pg_1"), device(1), clock.send(2_000), item);
        replica.apply(&op).unwrap();
    }
    assert_eq!(
        replica.page(legacy_page_id("pg_1")).unwrap().scene().len(),
        3
    );
}

#[test]
fn bad_strokes_are_quarantined_not_guessed() {
    let mut empty = stroke("1");
    empty.samples.clear();
    empty.times.clear();
    assert_eq!(stroke_to_item("p", &empty, 0), Err(LegacyError::NoSamples));

    let mut short = stroke("1");
    short.samples.pop();
    assert_eq!(
        stroke_to_item("p", &short, 0),
        Err(LegacyError::LengthMismatch)
    );

    let mut nan = stroke("1");
    nan.samples[3] = f32::NAN;
    assert_eq!(stroke_to_item("p", &nan, 0), Err(LegacyError::NonFinite));
    let mut inf = stroke("1");
    inf.samples[0] = f32::INFINITY;
    assert_eq!(stroke_to_item("p", &inf, 0), Err(LegacyError::NonFinite));

    let huge = LegacyStroke {
        times: vec![0; (1 << 20) + 1],
        samples: Vec::new(),
        ..stroke("1")
    };
    assert_eq!(
        stroke_to_item("p", &huge, 0),
        Err(LegacyError::TooManySamples)
    );
}

fn image(asset: &str) -> LegacyImage {
    serde_json::from_str(&format!(
        r#"{{"id":"img1","asset":"{asset}","x":100.5,"y":-20.0,"w":640.0,"h":480.0}}"#
    ))
    .unwrap()
}

#[test]
fn image_conversion_places_the_picture_by_translation() {
    let asset = "ab".repeat(32);
    let item = image_to_item("pg_1", &image(&asset), -Z_STEP).unwrap();
    assert_eq!(item.id, legacy_item_id("pg_1", "img1"));
    assert_eq!(item.z, -Z_STEP, "pictures keep their (possibly negative) z");
    assert_eq!(item.transform, Affine::translate(100.5, -20.0));
    let ItemKind::Image(img) = &item.kind else {
        panic!()
    };
    assert_eq!(img.asset, [0xAB; 32]);
    assert_eq!((img.w, img.h), (640.0, 480.0));
    let mut blob = Vec::new();
    encode_item(&item, &mut blob);
    assert_eq!(decode_item(&blob).unwrap(), item);
}

#[test]
fn bad_images_are_rejected() {
    for bad in [
        "",
        "ab",
        &"zz".repeat(32),
        &"ab".repeat(33),
        &"é".repeat(32),
    ] {
        assert_eq!(
            image_to_item("p", &image(bad), 0),
            Err(LegacyError::BadAsset),
            "{bad:?}"
        );
    }
    let asset = "00".repeat(32);
    let mut flat = image(&asset);
    flat.w = 0.0;
    assert!(matches!(
        image_to_item("p", &flat, 0),
        Err(LegacyError::Invalid(_))
    ));
    let mut nan = image(&asset);
    nan.x = f32::NAN;
    assert_eq!(image_to_item("p", &nan, 0), Err(LegacyError::NonFinite));
}
