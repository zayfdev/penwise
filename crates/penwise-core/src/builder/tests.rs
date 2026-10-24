#![allow(
    clippy::cast_precision_loss,
    reason = "small sample indices converted to f32 for synthetic strokes"
)]

use super::StrokeBuilder;
use crate::brush::{BrushFamily, Preset};
use crate::geometry::{GeometryView, StrokeGeometry};
use crate::sample::Sample;

const N: u32 = 150;
/// ≈240 Hz digitizer period, µs.
const PERIOD_US: u32 = 4_167;

fn sample(i: u32, x: f32, y: f32, pressure: f32) -> Sample {
    Sample {
        x,
        y,
        pressure,
        tilt: 0.0,
        azimuth: 0.0,
        t_us: i * PERIOD_US,
    }
}

fn line() -> Vec<Sample> {
    (0..N)
        .map(|i| {
            let f = i as f32;
            sample(i, 100.0 + 2.3 * f, 200.0 + 1.1 * f, 0.5)
        })
        .collect()
}

/// Spiral-ish curve with varying tilt and azimuth, off-grid inputs (exercises quantize-before-build).
fn curve() -> Vec<Sample> {
    (0..N)
        .map(|i| {
            let a = i as f32 * 0.05;
            let r = 120.0 + 0.4 * i as f32;
            Sample {
                tilt: 0.3 + 0.006 * i as f32,
                azimuth: -2.0 + 0.03 * i as f32,
                ..sample(
                    i,
                    400.0 + r * libm::cosf(a),
                    500.0 + r * libm::sinf(a),
                    0.45,
                )
            }
        })
        .collect()
}

fn pressure_ramp() -> Vec<Sample> {
    (0..N)
        .map(|i| {
            let f = i as f32;
            sample(i, 50.0 + 3.0 * f, 300.0, 0.05 + 0.95 * f / (N - 1) as f32)
        })
        .collect()
}

fn bytes(g: &StrokeGeometry) -> Vec<u8> {
    g.canonical_bytes().expect("finite geometry")
}

fn build_chunked(preset: Preset, samples: &[Sample], chunks: &[usize]) -> Vec<u8> {
    let mut b = StrokeBuilder::new(&preset.params(), 7);
    let mut rest = samples;
    let mut k = 0;
    while !rest.is_empty() {
        let n = chunks[k % chunks.len()].clamp(1, rest.len());
        b.push(&rest[..n]);
        // Live views must not perturb the result.
        let _ = b.geometry();
        rest = &rest[n..];
        k += 1;
    }
    bytes(&b.finish())
}

#[test]
fn finish_is_chunk_invariant() {
    let strokes = [
        ("line", line()),
        ("curve", curve()),
        ("pressure ramp", pressure_ramp()),
    ];
    for preset in Preset::ALL {
        for (name, samples) in &strokes {
            let batch = bytes(&StrokeBuilder::build(&preset.params(), 7, samples));
            assert!(!batch.is_empty());
            for size in [1, 3, 7, 64] {
                let chunked = build_chunked(preset, samples, &[size]);
                assert!(
                    chunked == batch,
                    "{} / {name}: chunk size {size} differs",
                    preset.name()
                );
            }
        }
    }
}

#[test]
fn finish_is_invariant_under_irregular_chunkings() {
    // Deterministic pseudo-random chunk sizes in 1..=20.
    let mut state: u32 = 0x9E37_79B9;
    let mut sizes = [0usize; 32];
    for s in &mut sizes {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *s = (state >> 27) as usize % 20 + 1;
    }
    let samples = curve();
    for preset in [
        Preset::Ballpoint,
        Preset::Fountain,
        Preset::Pencil,
        Preset::Spray,
    ] {
        let batch = bytes(&StrokeBuilder::build(&preset.params(), 7, &samples));
        for offset in 0..4 {
            let chunks: Vec<usize> = sizes
                .iter()
                .cycle()
                .skip(offset * 5)
                .take(32)
                .copied()
                .collect();
            assert!(
                build_chunked(preset, &samples, &chunks) == batch,
                "{} offset {offset}",
                preset.name()
            );
        }
    }
}

/// Repeated, backwards and gapped timestamps (speed window state crosses pushes) stay chunk-invariant.
#[test]
fn finish_is_chunk_invariant_with_ragged_timestamps() {
    let samples: Vec<Sample> = line()
        .into_iter()
        .enumerate()
        .map(|(i, s)| Sample {
            t_us: match i % 7 {
                2 => s.t_us - PERIOD_US,
                4 => s.t_us - 3 * PERIOD_US,
                6 => s.t_us + 30_000,
                _ => s.t_us,
            },
            ..s
        })
        .collect();
    for preset in Preset::ALL {
        let batch = bytes(&StrokeBuilder::build(&preset.params(), 7, &samples));
        for size in [1, 2, 5, 17] {
            assert!(
                build_chunked(preset, &samples, &[size]) == batch,
                "{} chunk size {size}",
                preset.name()
            );
        }
    }
}

#[test]
fn quantize_happens_before_build() {
    let raw = curve();
    let quantized: Vec<Sample> = raw.iter().map(|s| s.quantize()).collect();
    for preset in [Preset::Ballpoint, Preset::Pencil] {
        let a = bytes(&StrokeBuilder::build(&preset.params(), 3, &raw));
        let b = bytes(&StrokeBuilder::build(&preset.params(), 3, &quantized));
        assert!(a == b, "{}", preset.name());
    }
}

#[test]
fn build_is_repeatable() {
    let samples = pressure_ramp();
    for preset in Preset::ALL {
        let a = bytes(&StrokeBuilder::build(&preset.params(), 11, &samples));
        let b = bytes(&StrokeBuilder::build(&preset.params(), 11, &samples));
        assert!(a == b, "{}", preset.name());
    }
}

#[test]
fn empty_stroke_has_style_and_no_geometry() {
    for preset in Preset::ALL {
        let params = preset.params();
        let mut b = StrokeBuilder::new(&params, 1);
        b.push(&[]);
        assert_eq!(b.params(), &params);
        assert_eq!(b.seed(), 1);
        match (params.family, b.geometry()) {
            (BrushFamily::Outline(_), GeometryView::Outline { outline, .. }) => {
                let mut poly = Vec::new();
                outline.write_polygon(&mut poly);
                assert!(poly.is_empty(), "{}", preset.name());
            }
            (
                BrushFamily::Stamp(_),
                GeometryView::Stamps {
                    stable, tentative, ..
                },
            ) => {
                assert!(
                    stable.is_empty() && tentative.is_empty(),
                    "{}",
                    preset.name()
                );
            }
            _ => panic!("{}: live view has the wrong family", preset.name()),
        }
        let g = b.finish();
        assert_eq!(g.style().color, params.color);
        assert_eq!(g.style().opacity.to_bits(), params.opacity.to_bits());
        assert_eq!(g.style().blend, params.blend);
        match g {
            StrokeGeometry::Outline { outline, .. } => {
                assert!(outline.points.is_empty() && outline.contour_ends.is_empty());
            }
            StrokeGeometry::Stamps { dabs, grain, .. } => {
                assert!(dabs.is_empty());
                assert_eq!(grain, params.stamp.grain);
            }
        }
    }
}

#[test]
fn single_sample_yields_dot_or_dab() {
    let s = [sample(0, 321.0, 123.0, 0.6)];
    for preset in Preset::ALL {
        match StrokeBuilder::build(&preset.params(), 5, &s) {
            StrokeGeometry::Outline { outline, .. } => {
                assert!(!outline.points.is_empty(), "{}", preset.name());
                let b = outline.bounds();
                assert!(
                    b.min_x <= 321.0 && b.max_x >= 321.0 && b.min_y <= 123.0 && b.max_y >= 123.0
                );
            }
            StrokeGeometry::Stamps { dabs, .. } => {
                assert!(!dabs.is_empty(), "{}", preset.name());
            }
        }
    }
}

#[test]
fn stroke_ends_at_raw_pen_up_position() {
    // A fast line: the filtered tail lags the pen, but the finished stroke reaches the last raw sample.
    let samples: Vec<Sample> = (0..40)
        .map(|i| sample(i, 10.0 + 25.0 * i as f32, 50.0, 0.7))
        .collect();
    let last_x = samples[samples.len() - 1].x;
    let g = StrokeBuilder::build(&Preset::Fineliner.params(), 0, &samples);
    assert!(
        g.bounds().max_x >= last_x,
        "max_x {} < pen-up {last_x}",
        g.bounds().max_x
    );
}
