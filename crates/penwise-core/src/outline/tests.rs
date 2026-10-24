use super::*;
use crate::brush::{BrushFamily, Preset};
use crate::float::{max, min};

const DEG: f32 = core::f32::consts::PI / 180.0;

fn ink(x: f32, y: f32, p: f32) -> InkPoint {
    InkPoint {
        x,
        y,
        t_us: 0,
        pressure: p,
        size_pressure: p,
        alpha_factor: 1.0,
        speed: 0.0,
        tilt: 0.0,
        azimuth: 0.0,
    }
}

fn kind(params: &BrushParams) -> OutlineKind {
    match params.family {
        BrushFamily::Outline(k) => k,
        BrushFamily::Stamp(_) => panic!("not an outline preset"),
    }
}

fn outline_presets() -> Vec<BrushParams> {
    Preset::ALL
        .iter()
        .map(|p| p.params())
        .filter(|p| matches!(p.family, BrushFamily::Outline(_)))
        .collect()
}

/// Feeds `pts` like `StrokeBuilder`: each chunk commits everything but its newest point (the pending tail).
fn run(params: &BrushParams, pts: &[InkPoint], chunk: usize) -> Outline {
    let mut e = OutlineEngine::new(params, kind(params));
    let mut tail: Option<InkPoint> = None;
    let mut committed = Vec::new();
    for c in pts.chunks(chunk.max(1)) {
        committed.clear();
        committed.extend(tail);
        committed.extend_from_slice(&c[..c.len() - 1]);
        tail = c.last().copied();
        e.extend(&committed);
        e.update_tail(tail.as_ref());
    }
    e.finish(tail.as_ref())
}

fn build(params: &BrushParams, pts: &[InkPoint]) -> Outline {
    run(params, pts, pts.len())
}

fn wavy(n: u16) -> Vec<InkPoint> {
    (0..n)
        .map(|i| {
            let t = f32::from(i);
            let s = libm::sinf(t * 0.03);
            ink(t * 1.5, 20.0 * libm::sinf(t * 0.05), 0.2 + 0.7 * s * s)
        })
        .collect()
}

/// Sharp reversals (exercise corner fans) with an occasional duplicate sample.
fn zigzag() -> Vec<InkPoint> {
    let mut out = Vec::new();
    for leg in 0..6_u16 {
        for k in 0..30_u16 {
            let x = f32::from(leg) * 8.0 + f32::from(k) * 0.25;
            let y = if leg % 2 == 0 {
                f32::from(k) * 2.0
            } else {
                60.0 - f32::from(k) * 2.0
            };
            out.push(ink(x, y, 0.6));
            if k == 7 {
                out.push(ink(x, y, 0.6));
            }
        }
    }
    out
}

fn line(from: [f32; 2], angle: f32, count: u16, step: f32, pressure: f32) -> Vec<InkPoint> {
    let (sin, cos) = libm::sincosf(angle);
    (0..count)
        .map(|i| {
            let d = f32::from(i) * step;
            ink(from[0] + cos * d, from[1] + sin * d, pressure)
        })
        .collect()
}

fn bits(o: &Outline) -> Vec<(u32, u32)> {
    o.points
        .iter()
        .map(|p| (p[0].to_bits(), p[1].to_bits()))
        .collect()
}

fn assert_closed_finite(o: &Outline) {
    assert!(o.points.len() >= 3, "too few points: {}", o.points.len());
    assert_eq!(o.contour_ends, vec![u32::try_from(o.points.len()).unwrap()]);
    assert!(
        o.points
            .iter()
            .all(|p| p[0].is_finite() && p[1].is_finite())
    );
    assert!(area(&o.points).abs() > 0.0);
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]) * (a[0] - b[0]) + (a[1] - b[1]) * (a[1] - b[1])).sqrt()
}

fn area(pts: &[[f32; 2]]) -> f32 {
    let mut a = 0.0;
    for (i, p) in pts.iter().enumerate() {
        let q = pts[(i + 1) % pts.len()];
        a += p[0] * q[1] - q[0] * p[1];
    }
    a * 0.5
}

/// Extent of the polygon along unit `n` among points whose coordinate along `t` is within `±band` of `at`.
fn width_at(outline: &Outline, along: [f32; 2], across: [f32; 2], at: f32, band: f32) -> f32 {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for p in &outline.points {
        if (p[0] * along[0] + p[1] * along[1] - at).abs() <= band {
            let v = p[0] * across[0] + p[1] * across[1];
            lo = min(lo, v);
            hi = max(hi, v);
        }
    }
    hi - lo
}

fn smooth_free(mut p: BrushParams) -> BrushParams {
    p.outline.smoothing = 0.0;
    p
}

#[test]
fn every_outline_preset_yields_a_closed_finite_polygon() {
    for params in outline_presets() {
        for pts in [wavy(400), zigzag(), line([5.0, 5.0], 0.4, 3, 0.5, 0.5)] {
            let o = build(&params, &pts);
            assert_closed_finite(&o);
            let first = o.points[0];
            let last = o.points[o.points.len() - 1];
            let gap = dist(first, last);
            assert!(
                gap < 0.5 * params.size_max,
                "{:?}: contour does not close ({gap})",
                kind(&params)
            );
        }
    }
}

#[test]
fn no_points_yield_an_empty_outline() {
    for params in outline_presets() {
        let o = OutlineEngine::new(&params, kind(&params)).finish(None);
        assert!(o.points.is_empty() && o.contour_ends.is_empty());
    }
}

#[test]
fn a_tap_yields_a_dot() {
    for params in outline_presets() {
        for pts in [vec![ink(30.0, 40.0, 0.5)], vec![ink(30.0, 40.0, 0.5); 3]] {
            let o = build(&params, &pts);
            assert_closed_finite(&o);
            let flat = matches!(
                kind(&params),
                OutlineKind::Calligraphy | OutlineKind::Highlighter
            );
            assert_eq!(o.points.len(), if flat { 4 } else { 13 });
            for p in &o.points {
                let d = dist(*p, [30.0, 40.0]);
                assert!(d > 0.0 && d <= params.size_max, "{:?}: {d}", kind(&params));
            }
        }
    }
}

#[test]
fn chunked_extend_matches_batch_bit_for_bit() {
    for params in outline_presets() {
        for pts in [wavy(500), zigzag()] {
            let batch = bits(&build(&params, &pts));
            for chunk in [1, 2, 3, 7, 16, 61] {
                let chunked = bits(&run(&params, &pts, chunk));
                assert_eq!(chunked, batch, "{:?} chunk {chunk}", kind(&params));
            }
        }
    }
}

#[test]
fn update_tail_does_not_change_finish() {
    let params = Preset::Fountain.params();
    let pts = wavy(200);
    let (last, rest) = pts.split_last().unwrap();
    let mut a = OutlineEngine::new(&params, kind(&params));
    a.extend(rest);
    let mut b = a.clone();
    for p in pts.iter().step_by(17) {
        b.update_tail(Some(p));
    }
    b.update_tail(None);
    assert_eq!(bits(&a.finish(Some(last))), bits(&b.finish(Some(last))));
}

#[test]
fn stable_prefix_never_changes() {
    for params in outline_presets() {
        let pts = wavy(600);
        let mut e = OutlineEngine::new(&params, kind(&params));
        let mut seen_left: Vec<[f32; 2]> = Vec::new();
        let mut seen_right: Vec<[f32; 2]> = Vec::new();
        let mut tail = None;
        for c in pts.chunks(5) {
            let mut committed: Vec<InkPoint> = tail.into_iter().collect();
            committed.extend_from_slice(&c[..c.len() - 1]);
            tail = c.last().copied();
            e.extend(&committed);
            e.update_tail(tail.as_ref());
            let v = e.view();
            assert!(
                v.stable_left.starts_with(&seen_left) && v.stable_right.starts_with(&seen_right)
            );
            seen_left = v.stable_left.to_vec();
            seen_right = v.stable_right.to_vec();
        }
        assert!(
            seen_left.len() > 20,
            "{:?}: nothing became stable",
            kind(&params)
        );
        let o = e.finish(tail.as_ref());
        assert!(o.points.starts_with(&seen_left));
    }
}

#[test]
fn round_width_at_mid_stroke_is_within_preset_sizes() {
    for preset in [Preset::Ballpoint, Preset::Fineliner, Preset::Marker] {
        let params = smooth_free(preset.params());
        for p in [0.0, 0.5, 1.0] {
            let o = build(&params, &line([0.0, 50.0], 0.0, 200, 1.0, p));
            let w = width_at(&o, [1.0, 0.0], [0.0, 1.0], 100.0, 0.6);
            assert!(
                w >= params.size_min - 1e-3 && w <= params.size_max + 1e-3,
                "{preset:?} p {p}: {w}"
            );
        }
        let thin = build(&params, &line([0.0, 50.0], 0.0, 200, 1.0, 0.0));
        let fat = build(&params, &line([0.0, 50.0], 0.0, 200, 1.0, 1.0));
        let w0 = width_at(&thin, [1.0, 0.0], [0.0, 1.0], 100.0, 0.6);
        let w1 = width_at(&fat, [1.0, 0.0], [0.0, 1.0], 100.0, 0.6);
        assert!((w0 - params.size_min).abs() < 1e-3 && (w1 - params.size_max).abs() < 1e-3);
    }
}

#[test]
fn fountain_width_follows_the_nib_ellipse() {
    let params = smooth_free(Preset::Fountain.params());
    let nib = params.outline.nib;
    let norm = f32::midpoint(1.0, nib.aspect);
    let mut widths = Vec::new();
    for angle in [nib.angle, nib.angle + 90.0 * DEG] {
        let o = build(&params, &line([100.0, 100.0], angle, 160, 1.0, 1.0));
        let (s, c) = libm::sincosf(angle);
        let w = width_at(&o, [c, s], [-s, c], c * 100.0 + s * 100.0 + 80.0, 0.6);
        assert!(
            w >= params.size_min * nib.aspect / norm - 1e-2 && w <= params.size_max / norm + 1e-2
        );
        widths.push(w);
    }
    assert!(
        widths[0] < widths[1] * 0.7,
        "along nib {} vs across {}",
        widths[0],
        widths[1]
    );
}

#[test]
fn calligraphy_width_depends_on_stroke_direction() {
    let params = smooth_free(Preset::Calligraphy.params());
    let nib = params.outline.nib;
    let mut widths = Vec::new();
    for angle in [nib.angle, nib.angle + 90.0 * DEG, 0.0] {
        let o = build(&params, &line([100.0, 100.0], angle, 120, 1.0, 1.0));
        let (s, c) = libm::sincosf(angle);
        widths.push(width_at(
            &o,
            [c, s],
            [-s, c],
            c * 100.0 + s * 100.0 + 60.0,
            2.0,
        ));
    }
    assert!(
        widths[0] < 2.0 * nib.thickness,
        "along the nib: {}",
        widths[0]
    );
    assert!(
        (widths[1] - params.size_max).abs() < 0.1,
        "across the nib: {}",
        widths[1]
    );
    // W·|sin(θ − φ)| + t·|cos(θ − φ)| at θ = 0.
    let swept = params.size_max * libm::sinf(nib.angle) + nib.thickness * libm::cosf(nib.angle);
    assert!(
        (widths[2] - swept).abs() < 0.05,
        "horizontal: {} vs {swept}",
        widths[2]
    );
}

#[test]
fn highlighter_width_ignores_pressure() {
    let params = smooth_free(Preset::Highlighter.params());
    let pts: Vec<InkPoint> = (0..200_u16)
        .map(|i| ink(f32::from(i), 30.0, f32::from(i % 50) / 49.0))
        .collect();
    let o = build(&params, &pts);
    for x in [40.0, 75.0, 110.0, 160.0] {
        let w = width_at(&o, [1.0, 0.0], [0.0, 1.0], x, 0.6);
        assert!((w - 26.0).abs() < 1e-3, "x {x}: {w}");
    }
}

#[test]
fn tapers_thin_both_ends() {
    let mut params = smooth_free(Preset::Ballpoint.params());
    params.outline.taper_start = 30.0;
    params.outline.taper_end = 30.0;
    let o = build(&params, &line([0.0, 10.0], 0.0, 101, 1.0, 0.8));
    assert_closed_finite(&o);
    let mid = width_at(&o, [1.0, 0.0], [0.0, 1.0], 50.0, 0.6);
    // x = 97 is the last point outside the 3 px end-noise skip.
    let start = width_at(&o, [1.0, 0.0], [0.0, 1.0], 3.0, 0.4);
    let end = width_at(&o, [1.0, 0.0], [0.0, 1.0], 97.0, 0.4);
    assert!(start > 0.0 && end > 0.0, "start {start} end {end}");
    assert!(
        start < mid * 0.5 && end < mid * 0.5,
        "start {start} mid {mid} end {end}"
    );
}

#[test]
fn soft_first_contact_is_lifted_by_the_pressure_window() {
    let params = smooth_free(Preset::Ballpoint.params());
    let mut pts = line([0.0, 10.0], 0.0, 100, 1.0, 0.8);
    pts[0].size_pressure = 0.0;
    pts[1].size_pressure = 0.02;
    let o = build(&params, &pts);
    let near_start = width_at(&o, [1.0, 0.0], [0.0, 1.0], 1.0, 0.4);
    assert!(
        near_start > params.size_min + 0.1,
        "start width {near_start}"
    );
}
