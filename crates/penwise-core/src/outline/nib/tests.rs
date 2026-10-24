use super::*;

fn nib(angle: f32, aspect: f32, thickness: f32, fixation: f32) -> NibParams {
    NibParams {
        angle,
        aspect,
        thickness,
        fixation,
    }
}

fn unit(angle: f32) -> [f32; 2] {
    let (s, c) = libm::sincosf(angle);
    [c, s]
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]) * (a[0] - b[0]) + (a[1] - b[1]) * (a[1] - b[1])).sqrt()
}

#[test]
fn fountain_scale_is_widest_across_the_nib_axis() {
    let n = nib(std::f32::consts::FRAC_PI_4, 0.55, 0.0, 1.0);
    let norm = f32::midpoint(1.0, 0.55);
    let along = fountain_scale(unit(n.angle), &n);
    let across = fountain_scale(unit(n.angle + FRAC_PI_2), &n);
    assert!((along - 0.55 / norm).abs() < 1e-5, "along {along}");
    assert!((across - 1.0 / norm).abs() < 1e-5, "across {across}");
    for k in 0..32_u8 {
        let s = fountain_scale(unit(f32::from(k) * 0.2), &n);
        assert!(s >= along - 1e-5 && s <= across + 1e-5);
    }
}

#[test]
fn fountain_scale_degenerate_aspect_is_neutral() {
    assert!((fountain_scale([1.0, 0.0], &nib(0.3, -1.0, 0.0, 1.0)) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn nib_width_lerps_and_is_constant_for_equal_sizes() {
    assert!((nib_width(10.0, 22.0, 0.0) - 10.0).abs() < f32::EPSILON);
    assert!((nib_width(10.0, 22.0, 1.0) - 22.0).abs() < f32::EPSILON);
    assert!((nib_width(10.0, 22.0, 0.5) - 16.0).abs() < f32::EPSILON);
    for p in [0.0, 0.3, 1.0] {
        assert!((nib_width(26.0, 26.0, p) - 26.0).abs() < f32::EPSILON);
    }
}

#[test]
fn effective_angle_fixed_nib_is_exact() {
    let n = nib(0.698_131_7, 1.0, 1.5, 1.0);
    for d in [[1.0, 0.0], [0.0, 1.0], [-0.6, 0.8]] {
        assert_eq!(effective_angle(d, &n).to_bits(), n.angle.to_bits());
    }
}

#[test]
fn effective_angle_free_nib_is_perpendicular_to_motion() {
    let n = nib(0.698_131_7, 1.0, 1.5, 0.0);
    let d = unit(0.3);
    let phi = effective_angle(d, &n);
    let axis = unit(phi);
    assert!((axis[0] * d[0] + axis[1] * d[1]).abs() < 1e-5);
}

#[test]
fn effective_angle_takes_the_short_arc() {
    // θ + π/2 ≈ 4.555 rad and φ ≈ −1.471 rad are 0.257 rad apart the short way (2π − 0.257 the long way).
    let n = nib(-FRAC_PI_2 + 0.1, 1.0, 1.0, 0.5);
    let d = unit(PI * 0.95);
    let free = libm::atan2f(d[1], d[0]) + FRAC_PI_2;
    let phi = effective_angle(d, &n);
    let gap_free = dist(unit(phi), unit(free));
    let gap_nib = dist(unit(phi), unit(n.angle));
    assert!((gap_free - gap_nib).abs() < 1e-4, "{gap_free} vs {gap_nib}");
    assert!(gap_free < 0.2, "long arc taken: {gap_free}");
}

#[test]
fn flat_offsets_are_symmetric_and_span_the_nib() {
    let n = nib(0.698_131_7, 1.0, 1.5, 1.0);
    let p = [10.0, 20.0];
    let (l, r) = flat_offsets(p, [1.0, 0.0], 22.0, n.angle, &n);
    assert!((l[0] + r[0] - 2.0 * p[0]).abs() < 1e-4 && (l[1] + r[1] - 2.0 * p[1]).abs() < 1e-4);
    let span = dist(l, r);
    let expected = (22.0_f32 * 22.0 + 1.5 * 1.5).sqrt();
    assert!((span - expected).abs() < 1e-3, "span {span}");
}

#[test]
fn nib_rect_is_the_nib_footprint() {
    let n = nib(FRAC_PI_2, 1.0, 8.0, 1.0);
    let mut out = Vec::new();
    nib_rect([0.0, 0.0], 26.0, &n, &mut out);
    assert_eq!(out.len(), 4);
    let ys: Vec<f32> = out.iter().map(|p| p[1]).collect();
    let xs: Vec<f32> = out.iter().map(|p| p[0]).collect();
    let span = |v: &[f32]| {
        v.iter().copied().fold(f32::NEG_INFINITY, crate::float::max)
            - v.iter().copied().fold(f32::INFINITY, crate::float::min)
    };
    assert!((span(&ys) - 26.0).abs() < 1e-4);
    assert!((span(&xs) - 8.0).abs() < 1e-4);
}

#[test]
fn direction_smoother_first_call_passes_through_and_stays_unit() {
    let mut s = DirectionSmoother::new();
    let first = s.update([0.0, 1.0], 0.0, 5.0);
    assert_eq!(first.map(f32::to_bits), [0.0_f32, 1.0].map(f32::to_bits));
    let next = s.update([1.0, 0.0], 1.0, 5.0);
    let len = (next[0] * next[0] + next[1] * next[1]).sqrt();
    assert!((len - 1.0).abs() < 1e-5);
    assert!(
        next[0] > 0.0 && next[1] > next[0],
        "small step moves a little: {next:?}"
    );
    let snapped = s.update([1.0, 0.0], 1.0, 0.0);
    assert_eq!(snapped.map(f32::to_bits), [1.0_f32, 0.0].map(f32::to_bits));
}

#[test]
fn direction_smoother_keeps_direction_on_exact_cancellation() {
    let mut s = DirectionSmoother::new();
    let _ = s.update([1.0, 0.0], 0.0, 1.0);
    // w = 0.5 exactly cancels (1,0) against (−1,0): keep the previous direction.
    let ln2 = core::f32::consts::LN_2;
    let kept = s.update([-1.0, 0.0], ln2, 1.0);
    assert!(kept[0].is_finite() && kept[1].is_finite());
    let len = (kept[0] * kept[0] + kept[1] * kept[1]).sqrt();
    assert!((len - 1.0).abs() < 1e-5);
}

#[test]
fn flat_offsets_sweep_the_outer_nib_corners() {
    let n = nib(0.698_131_7, 1.0, 1.5, 1.0);
    for k in 0..24_u8 {
        let theta = f32::from(k) * 0.27 + 0.01;
        let d = unit(theta);
        let (left, right) = flat_offsets([0.0, 0.0], d, 22.0, n.angle, &n);
        let normal = per(d);
        let across = ((left[0] - right[0]) * normal[0] + (left[1] - right[1]) * normal[1]).abs();
        let rel = theta - n.angle;
        let swept = 22.0 * libm::sinf(rel).abs() + 1.5 * libm::cosf(rel).abs();
        assert!(
            (across - swept).abs() < 1e-3,
            "θ {theta}: {across} vs {swept}"
        );
    }
}
