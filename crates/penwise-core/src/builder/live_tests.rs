//! Live stroke contract: what the pen shows while writing is what `finish` commits. Samples are fed one at a
//! time (the worst chunking for a builder that re-fits), and every frame is checked against the final stroke.

#![allow(
    clippy::cast_precision_loss,
    reason = "small sample indices converted to f32 for synthetic strokes"
)]

use std::collections::HashSet;

use super::StrokeBuilder;
use crate::brush::{BrushFamily, Preset};
use crate::geometry::{Dab, GeometryView, StrokeGeometry};
use crate::sample::Sample;

/// ≈482 Hz digitizer period, µs.
const PERIOD_US: u32 = 2_075;
/// Outline brushes with the round tips the live ink is judged on.
const ROUND: [Preset; 3] = [Preset::Ballpoint, Preset::Fineliner, Preset::Fountain];

fn sample(i: usize, x: f32, y: f32, pressure: f32) -> Sample {
    Sample {
        x,
        y,
        pressure,
        tilt: 0.4,
        azimuth: 0.0,
        t_us: u32::try_from(i).expect("small index") * PERIOD_US,
    }
}

/// Straight runs between `corners`, `per_leg` samples each, pressure `p`.
fn polyline(corners: &[[f32; 2]], per_leg: usize, p: f32) -> Vec<Sample> {
    let mut out = Vec::new();
    for w in corners.windows(2) {
        for k in 0..per_leg {
            let t = k as f32 / per_leg as f32;
            out.push(sample(
                out.len(),
                w[0][0] + t * (w[1][0] - w[0][0]),
                w[0][1] + t * (w[1][1] - w[0][1]),
                p,
            ));
        }
    }
    let end = corners[corners.len() - 1];
    out.push(sample(out.len(), end[0], end[1], p));
    out
}

/// Four 80 px legs at ≈1000 px/s.
fn fast_zigzag() -> Vec<Sample> {
    let pts = [
        [100.0, 100.0],
        [160.0, 180.0],
        [220.0, 100.0],
        [280.0, 180.0],
        [340.0, 100.0],
    ];
    polyline(&pts, 40, 0.5)
}

/// Right angles at ≈500 px/s.
fn corners() -> Vec<Sample> {
    polyline(
        &[[50.0, 50.0], [50.0, 150.0], [150.0, 150.0], [150.0, 50.0]],
        100,
        0.6,
    )
}

/// A lowercase "e"-sized loop (28 px) at ≈150 px/s with pressure building up.
fn small_letter() -> Vec<Sample> {
    (0..260)
        .map(|i| {
            let a = 0.4 + i as f32 * 0.024;
            let p = 0.2 + 0.4 * (i as f32 / 260.0);
            sample(
                i,
                300.0 + 14.0 * libm::cosf(a),
                400.0 - 10.0 * libm::sinf(a),
                p,
            )
        })
        .collect()
}

/// A 6 px tick: three samples, shorter than the end taper.
fn tick() -> Vec<Sample> {
    polyline(&[[200.0, 200.0], [206.0, 203.0]], 2, 0.5)
}

fn strokes() -> Vec<(&'static str, Vec<Sample>)> {
    vec![
        ("fast zigzag", fast_zigzag()),
        ("corners", corners()),
        ("small letter", small_letter()),
        ("tick", tick()),
    ]
}

/// One live frame of an outline stroke.
struct Frame {
    stable_left: Vec<[f32; 2]>,
    stable_right: Vec<[f32; 2]>,
    polygon: Vec<[f32; 2]>,
}

/// Feeds `samples` one by one; returns every outline frame and the finished polygon.
fn outline_run(preset: Preset, samples: &[Sample]) -> (Vec<Frame>, Vec<[f32; 2]>) {
    let mut builder = StrokeBuilder::new(&preset.params(), 5);
    let mut frames = Vec::new();
    for s in samples {
        builder.push(std::slice::from_ref(s));
        let GeometryView::Outline { outline, .. } = builder.geometry() else {
            panic!("{} is not an outline brush", preset.name());
        };
        let mut polygon = Vec::new();
        outline.write_polygon(&mut polygon);
        frames.push(Frame {
            stable_left: outline.stable_left.to_vec(),
            stable_right: outline.stable_right.to_vec(),
            polygon,
        });
    }
    let StrokeGeometry::Outline { outline, .. } = builder.finish() else {
        panic!("{} finished as stamps", preset.name());
    };
    (frames, outline.points)
}

fn is_outline(preset: Preset) -> bool {
    matches!(preset.params().family, BrushFamily::Outline(_))
}

fn bits(p: [f32; 2]) -> [u32; 2] {
    [p[0].to_bits(), p[1].to_bits()]
}

/// Every vertex shown as stable stays, bit for bit, in every later frame and in the finished outline.
#[test]
fn outline_geometry_once_stable_never_moves() {
    for preset in Preset::ALL.into_iter().filter(|p| is_outline(*p)) {
        for (name, samples) in strokes() {
            let (frames, fin) = outline_run(preset, &samples);
            let on_final: HashSet<[u32; 2]> = fin.iter().copied().map(bits).collect();
            let what = format!("{} / {name}", preset.name());
            for pair in frames.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                assert!(
                    b.stable_left.starts_with(&a.stable_left),
                    "{what}: left side moved"
                );
                assert!(
                    b.stable_right.starts_with(&a.stable_right),
                    "{what}: right side moved"
                );
            }
            for f in &frames {
                assert!(
                    fin.starts_with(&f.stable_left),
                    "{what}: final lost a shown left vertex"
                );
                for p in &f.stable_right {
                    assert!(
                        on_final.contains(&bits(*p)),
                        "{what}: final lost a shown right vertex"
                    );
                }
            }
        }
    }
}

/// The last live frame is the finished stroke: no jump on pen-up, whatever the brush.
#[test]
fn last_live_frame_is_the_finished_stroke() {
    for preset in Preset::ALL.into_iter().filter(|p| is_outline(*p)) {
        for (name, samples) in strokes() {
            let (frames, fin) = outline_run(preset, &samples);
            let last = frames.last().expect("frames");
            assert!(
                last.polygon == fin,
                "{} / {name}: pen-up changed the ink",
                preset.name()
            );
        }
    }
}

/// Stamp brushes: stable dabs are only ever appended to, the last frame's dabs are the finished dabs.
#[test]
fn stamp_dabs_once_stable_never_move() {
    for preset in Preset::ALL.into_iter().filter(|p| !is_outline(*p)) {
        for (name, samples) in strokes() {
            let mut builder = StrokeBuilder::new(&preset.params(), 5);
            let mut stable: Vec<Dab> = Vec::new();
            let mut last_live: Vec<Dab> = Vec::new();
            for s in &samples {
                builder.push(std::slice::from_ref(s));
                let GeometryView::Stamps {
                    stable: now,
                    tentative,
                    ..
                } = builder.geometry()
                else {
                    panic!("{} is not a stamp brush", preset.name());
                };
                assert!(
                    now.starts_with(&stable),
                    "{} / {name}: a stable dab changed",
                    preset.name()
                );
                stable = now.to_vec();
                last_live = now.iter().chain(tentative).copied().collect();
            }
            let StrokeGeometry::Stamps { dabs, .. } = builder.finish() else {
                panic!("{} finished as an outline", preset.name());
            };
            assert!(
                dabs.starts_with(&stable),
                "{} / {name}: final lost a stable dab",
                preset.name()
            );
            assert!(
                last_live == dabs,
                "{} / {name}: pen-up changed the dabs",
                preset.name()
            );
        }
    }
}

/// Winding number of `p` in `poly` (nonzero fill).
fn winding(p: [f32; 2], poly: &[[f32; 2]]) -> i32 {
    let mut w = 0;
    for (i, a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        let cross = (b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1]);
        if a[1] <= p[1] {
            w += i32::from(b[1] > p[1] && cross > 0.0);
        } else {
            w -= i32::from(b[1] <= p[1] && cross < 0.0);
        }
    }
    w
}

fn dist_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (abx, aby) = (b[0] - a[0], b[1] - a[1]);
    let len2 = abx * abx + aby * aby;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * abx + (p[1] - a[1]) * aby) / len2).clamp(0.0, 1.0)
    };
    let (dx, dy) = (a[0] + t * abx - p[0], a[1] + t * aby - p[1]);
    (dx * dx + dy * dy).sqrt()
}

/// 0 when `p` lies in the ink of `poly`, else its distance to the outline: how far a shown vertex ended up outside
/// the finished stroke.
fn outside(p: [f32; 2], poly: &[[f32; 2]]) -> f32 {
    if winding(p, poly) != 0 {
        return 0.0;
    }
    (0..poly.len())
        .map(|i| dist_to_segment(p, poly[i], poly[(i + 1) % poly.len()]))
        .fold(f32::MAX, crate::float::min)
}

/// Ink shown at any step lies inside the finished stroke to within about half a pixel for letters with round nibs:
/// nothing drawn is taken back, and the pen-up frame adds nothing. Only the tentative tail can overshoot, and only
/// where the pen turns sharply at speed (the filtered path cuts the corner the raw tip reached); those bounds are
/// regression fences, measured on this smoothing.
#[test]
fn shown_ink_stays_inside_the_finished_stroke() {
    for preset in ROUND {
        for (name, samples, bound) in [
            ("small letter", small_letter(), 0.6),
            ("tick", tick(), 1.0),
            ("corners", corners(), 2.5),
            ("fast zigzag", fast_zigzag(), 6.0),
        ] {
            let (frames, fin) = outline_run(preset, &samples);
            let worst = frames
                .iter()
                .flat_map(|f| f.polygon.iter())
                .map(|p| outside(*p, &fin))
                .fold(0.0, crate::float::max);
            assert!(
                worst <= bound,
                "{} / {name}: {worst} px of shown ink was taken back",
                preset.name()
            );
        }
    }
}
