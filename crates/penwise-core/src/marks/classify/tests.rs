//! Classifier tests: synthetic ink for every vocabulary entry (including sloppy variants) and one real
//! recording from `fixtures/pwrec`.
#![allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::panic,
    reason = "tests: exact synthetic coordinates, small counts, and panics report failures"
)]

use super::*;
use crate::brush::Preset;
use crate::codec::decode_pwrec;
use crate::codec::pwrec::RunKind;
use crate::item::StrokeItem;
use crate::sample::Sample;

fn ink(points: &[[f32; 2]]) -> StrokeItem {
    let samples = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            Sample {
                x: p[0],
                y: p[1],
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: u32::try_from(i).unwrap_or(0) * 2000,
            }
            .quantize()
        })
        .collect();
    StrokeItem {
        params: Preset::Ballpoint.params(),
        seed: 7,
        samples,
        trim: None,
        binding: None,
    }
}

/// Interpolate a vertex list at `step` px, as a real pen would deliver samples.
fn densify(vertices: &[[f32; 2]], step: f32) -> Vec<[f32; 2]> {
    let mut out = vec![vertices[0]];
    for pair in vertices.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let d = libm::hypotf(b[0] - a[0], b[1] - a[1]);
        let n = crate::float::max((d / step).ceil(), 1.0) as usize;
        for k in 1..=n {
            let t = k as f32 / n as f32;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

fn path(vertices: &[[f32; 2]]) -> StrokeItem {
    ink(&densify(vertices, 4.0))
}

fn arc(cx: f32, cy: f32, r: f32, n: usize, sweep_deg: f32) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32 * sweep_deg.to_radians();
            [cx + r * libm::cosf(t), cy + r * libm::sinf(t)]
        })
        .collect()
}

fn assert_kind(shape: &MarkShape, kind: MarkKind) {
    assert_eq!(shape.guess, kind, "expected {kind:?}, got {shape:?}");
    assert!(
        (0.0..=1.0).contains(&shape.confidence),
        "confidence out of range: {shape:?}"
    );
    assert!(shape.confidence >= 0.4, "unexpectedly unsure: {shape:?}");
}

fn classify_one(stroke: &StrokeItem) -> MarkShape {
    classify(&[stroke])
}

fn classify_one_with_rect(stroke: &StrokeItem, rect: [f32; 4]) -> MarkShape {
    classify_with_rect(&[stroke], Some(rect))
}

#[test]
fn closed_circle_is_a_circle() {
    let points = arc(220.0, 220.0, 80.0, 64, 359.0);
    let shape = classify_one(&ink(&points));
    assert_kind(&shape, MarkKind::Circle);
    assert!(shape.confidence > 0.8, "{shape:?}");
    assert!(shape.from.is_none() && shape.to.is_none());
}

#[test]
fn sloppy_open_circle_is_still_a_circle() {
    let points = arc(220.0, 220.0, 80.0, 56, 320.0);
    let shape = classify_one(&ink(&points));
    assert_kind(&shape, MarkKind::Circle);
}

#[test]
fn jittery_circle_is_a_circle() {
    let points: Vec<[f32; 2]> = arc(220.0, 220.0, 80.0, 64, 355.0)
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let wobble = (i % 5) as f32 - 2.0;
            [p[0] + wobble, p[1] - wobble * 0.5]
        })
        .collect();
    let shape = classify_one(&ink(&points));
    assert_kind(&shape, MarkKind::Circle);
}

#[test]
fn one_stroke_arrow_points_from_tail_to_tip() {
    let shape = classify_one(&path(&[[20.0, 100.0], [160.0, 100.0], [140.0, 85.0]]));
    assert_kind(&shape, MarkKind::Arrow);
    let from = shape.from.unwrap();
    let to = shape.to.unwrap();
    assert!(
        libm::hypotf(from[0] - 20.0, from[1] - 100.0) < 2.0,
        "{shape:?}"
    );
    assert!(
        libm::hypotf(to[0] - 160.0, to[1] - 100.0) < 2.0,
        "{shape:?}"
    );
}

#[test]
fn vertical_arrow_points_down() {
    let shape = classify_one(&path(&[[100.0, 20.0], [100.0, 160.0], [115.0, 140.0]]));
    assert_kind(&shape, MarkKind::Arrow);
    assert!(
        libm::hypotf(
            shape.from.unwrap()[0] - 100.0,
            shape.from.unwrap()[1] - 20.0
        ) < 2.0
    );
    assert!(libm::hypotf(shape.to.unwrap()[0] - 100.0, shape.to.unwrap()[1] - 160.0) < 2.0);
}

#[test]
fn two_stroke_arrow_with_a_v_head() {
    let shaft = path(&[[20.0, 100.0], [160.0, 100.0]]);
    let head = path(&[[140.0, 85.0], [160.0, 100.0], [140.0, 115.0]]);
    let shape = classify(&[&shaft, &head]);
    assert_kind(&shape, MarkKind::Arrow);
    assert!(
        libm::hypotf(
            shape.from.unwrap()[0] - 20.0,
            shape.from.unwrap()[1] - 100.0
        ) < 2.0
    );
    assert!(libm::hypotf(shape.to.unwrap()[0] - 160.0, shape.to.unwrap()[1] - 100.0) < 2.0);
}

#[test]
fn arrow_drawn_head_first_still_points_the_right_way() {
    let shape = classify_one(&path(&[[140.0, 85.0], [160.0, 100.0], [20.0, 100.0]]));
    assert_kind(&shape, MarkKind::Arrow);
    assert!(
        libm::hypotf(
            shape.from.unwrap()[0] - 20.0,
            shape.from.unwrap()[1] - 100.0
        ) < 2.0
    );
    assert!(libm::hypotf(shape.to.unwrap()[0] - 160.0, shape.to.unwrap()[1] - 100.0) < 2.0);
}

#[test]
fn two_crossing_strokes_are_a_cross() {
    let a = path(&[[40.0, 40.0], [160.0, 160.0]]);
    let b = path(&[[160.0, 40.0], [40.0, 160.0]]);
    let shape = classify(&[&a, &b]);
    assert_kind(&shape, MarkKind::Cross);
    assert!(shape.confidence > 0.75, "{shape:?}");
    assert!(shape.from.is_none() && shape.to.is_none());
}

#[test]
fn sloppy_cross_is_still_a_cross() {
    let a = path(&[[40.0, 45.0], [158.0, 158.0]]);
    let b = path(&[[165.0, 42.0], [44.0, 162.0]]);
    assert_kind(&classify(&[&a, &b]), MarkKind::Cross);
}

#[test]
fn wavy_line_is_wavy() {
    let points: Vec<[f32; 2]> = (0..=240)
        .map(|i| {
            let x = 20.0 + i as f32;
            let y = 100.0 + 12.0 * libm::sinf(x * core::f32::consts::TAU / 60.0);
            [x, y]
        })
        .collect();
    let shape = classify_one(&ink(&points));
    assert_kind(&shape, MarkKind::Wavy);
    assert!(shape.confidence > 0.7, "{shape:?}");
}

#[test]
fn small_wavy_line_is_wavy() {
    let points: Vec<[f32; 2]> = (0..=120)
        .map(|i| {
            let x = 10.0 + i as f32 * 0.8;
            let y = 40.0 + 4.0 * libm::sinf(x * core::f32::consts::TAU / 24.0);
            [x, y]
        })
        .collect();
    assert_kind(&classify_one(&ink(&points)), MarkKind::Wavy);
}

#[test]
fn rectangle_in_empty_space_is_a_new_box() {
    let shape = classify_one(&path(&[
        [40.0, 40.0],
        [200.0, 40.0],
        [200.0, 160.0],
        [40.0, 160.0],
        [40.0, 40.0],
    ]));
    assert_kind(&shape, MarkKind::BoxNew);
    assert!(shape.confidence > 0.7, "{shape:?}");
}

#[test]
fn sloppy_two_stroke_rectangle_is_a_new_box() {
    let a = path(&[[40.0, 40.0], [200.0, 40.0], [200.0, 160.0]]);
    let b = path(&[[200.0, 160.0], [40.0, 160.0], [40.0, 40.0]]);
    assert_kind(&classify(&[&a, &b]), MarkKind::BoxNew);
}

#[test]
fn horizontal_line_through_a_text_box_is_a_strike() {
    let line = path(&[[20.0, 50.0], [200.0, 52.0]]);
    let shape = classify_one_with_rect(&line, [0.0, 0.0, 220.0, 100.0]);
    assert_kind(&shape, MarkKind::Strike);
}

#[test]
fn horizontal_line_below_a_text_box_is_an_underline() {
    let line = path(&[[20.0, 110.0], [200.0, 108.0]]);
    let shape = classify_one_with_rect(&line, [0.0, 0.0, 220.0, 100.0]);
    assert_kind(&shape, MarkKind::Underline);

    // Without the rect the fixed vocabulary has no plain underline: strike, with low confidence.
    let blind = classify_one(&line);
    assert_kind(&blind, MarkKind::Strike);
    assert!(blind.confidence < 0.6, "{blind:?}");
}

#[test]
fn scribble_is_free() {
    let strokes = [
        ink(&densify(&[[30.0, 40.0], [90.0, 130.0], [40.0, 90.0]], 3.0)),
        ink(&densify(
            &[[100.0, 30.0], [60.0, 120.0], [120.0, 70.0]],
            3.0,
        )),
        ink(&densify(&[[20.0, 140.0], [60.0, 50.0], [90.0, 150.0]], 3.0)),
        ink(&densify(&[[130.0, 40.0], [100.0, 140.0]], 3.0)),
    ];
    let refs: Vec<&StrokeItem> = strokes.iter().collect();
    assert_kind(&classify(&refs), MarkKind::Free);
}

#[test]
fn handwriting_is_free() {
    let strokes = [
        path(&[[20.0, 20.0], [24.0, 34.0], [30.0, 22.0]]),
        path(&[[40.0, 20.0], [42.0, 34.0]]),
        path(&[[55.0, 20.0], [58.0, 34.0], [64.0, 20.0]]),
        path(&[[75.0, 22.0], [80.0, 30.0], [72.0, 34.0]]),
    ];
    let refs: Vec<&StrokeItem> = strokes.iter().collect();
    assert_kind(&classify(&refs), MarkKind::Free);
}

#[test]
fn empty_or_dot_mark_is_unsure_free() {
    assert_eq!(classify(&[]).guess, MarkKind::Free);
    assert!(classify(&[]).confidence < 0.2);
    let dot = ink(&[[10.0, 10.0]]);
    let shape = classify_one(&dot);
    assert_eq!(shape.guess, MarkKind::Free);
    assert!(shape.confidence < 0.2, "{shape:?}");
}

#[test]
fn shape_json_uses_snake_case_and_omits_empty_ends() {
    let shape = MarkShape::guessed(MarkKind::BoxNew, 0.75);
    let json = serde_json::to_string(&shape).unwrap();
    assert_eq!(
        json, r#"{"guess":"box_new","confidence":0.75}"#,
        "got {json}"
    );
    let arrow = MarkShape::arrow(0.8, [1.0, 2.0], [3.0, 4.0]);
    let json = serde_json::to_string(&arrow).unwrap();
    assert!(json.contains(r#""guess":"arrow""#), "{json}");
    assert!(json.contains(r#""from":[1.0,2.0]"#), "{json}");
    assert!(json.contains(r#""to":[3.0,4.0]"#), "{json}");
}

#[test]
fn kind_json_names_and_confidence_clamp() {
    for (kind, name) in [
        (MarkKind::Circle, "circle"),
        (MarkKind::Arrow, "arrow"),
        (MarkKind::Cross, "cross"),
        (MarkKind::Wavy, "wavy"),
        (MarkKind::BoxNew, "box_new"),
        (MarkKind::Strike, "strike"),
        (MarkKind::Underline, "underline"),
        (MarkKind::Free, "free"),
    ] {
        assert_eq!(
            serde_json::to_string(&kind).unwrap(),
            format!("\"{name}\""),
            "{kind:?}"
        );
    }
    assert_eq!(MarkShape::guessed(MarkKind::Free, 2.0).confidence, 1.0);
    assert_eq!(MarkShape::guessed(MarkKind::Free, -1.0).confidence, 0.0);
}

/// Real ink from the Tab S9 (`10-shapes`): rectangle, diagonal line, triangle, two taps, circle, hooked
/// vertical line, V. Pins that real hand tremor still reads as the right shapes, and that a V or a triangle
/// edge is not mistaken for an arrow.
#[test]
fn real_recording_shapes_classify() {
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/pwrec/10-shapes.pwrec"
    ));
    let recording = decode_pwrec(bytes).unwrap();
    let ranges = recording.digitizer_ranges().unwrap();
    let shapes: Vec<MarkShape> = recording
        .strokes
        .iter()
        .filter(|run| run.kind == RunKind::Contact)
        .map(|run| {
            let mut samples = Vec::new();
            run.to_samples(&ranges, &mut samples);
            let stroke = StrokeItem {
                params: Preset::Ballpoint.params(),
                seed: 1,
                samples,
                trim: None,
                binding: None,
            };
            classify_one(&stroke)
        })
        .collect();
    assert_eq!(shapes.len(), 8, "{shapes:?}");
    let guessed: Vec<MarkKind> = shapes.iter().map(|shape| shape.guess).collect();
    assert_eq!(shapes[0].guess, MarkKind::BoxNew, "rectangle: {shapes:?}");
    assert!(shapes[0].confidence > 0.8, "rectangle: {shapes:?}");
    assert_eq!(shapes[2].guess, MarkKind::BoxNew, "triangle: {shapes:?}");
    assert_eq!(shapes[4].guess, MarkKind::Circle, "circle: {shapes:?}");
    assert!(shapes[4].confidence > 0.8, "circle: {shapes:?}");
    for tap in [3, 5] {
        assert_eq!(shapes[tap].guess, MarkKind::Free, "tap {tap}: {shapes:?}");
        assert!(shapes[tap].confidence < 0.2, "tap {tap}: {shapes:?}");
    }
    for (index, shape) in shapes.iter().enumerate() {
        assert_ne!(shape.guess, MarkKind::Arrow, "stroke {index}: {shapes:?}");
    }
    assert!(
        guessed
            .iter()
            .filter(|kind| **kind == MarkKind::Free)
            .count()
            >= 4,
        "{guessed:?}"
    );
}
