#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::many_single_char_names,
    reason = "test geometry on small integer counts"
)]

use core::f32::consts::{FRAC_PI_2, PI};

use super::*;
use crate::brush::{Jitter, Preset, SprayDistribution, StampParams};
use crate::dynamics::{TiltResponse, VelocityResponse};

const STAMP_PRESETS: [(Preset, StampKind); 7] = [
    (Preset::Pencil, StampKind::Pencil),
    (Preset::Crayon, StampKind::Crayon),
    (Preset::Airbrush, StampKind::Airbrush),
    (Preset::Spray, StampKind::Spray),
    (Preset::Soft, StampKind::Soft),
    (Preset::Solid, StampKind::Solid),
    (Preset::Shading, StampKind::Shading),
];

/// Full-pressure, upright point.
fn pt(x: f32, y: f32, t_us: u32) -> InkPoint {
    InkPoint {
        x,
        y,
        t_us,
        pressure: 1.0,
        size_pressure: 1.0,
        alpha_factor: 1.0,
        speed: 0.0,
        tilt: 0.0,
        azimuth: 0.0,
    }
}

/// Constant-size round brush without randomness, tilt, velocity, grain or spray.
fn plain(d: f32, spacing: f32, flow: f32) -> BrushParams {
    let mut p = Preset::Solid.params();
    p.size_min = d;
    p.size_max = d;
    p.size_gamma = 1.0;
    p.dynamics.tilt = TiltResponse::NONE;
    p.dynamics.velocity = VelocityResponse::NONE;
    p.stamp = StampParams {
        flow,
        flow_pressure: 0.0,
        hardness: 1.0,
        spacing,
        scatter: 0.0,
        jitter: Jitter::NONE,
        dabs_per_second: 0.0,
        grain: None,
        spray: None,
    };
    p
}

/// Horizontal line `0..=len` sampled every `step` px, 2 ms apart.
fn line(len: f32, step: f32) -> Vec<InkPoint> {
    let n = (len / step).round() as u32;
    (0..=n)
        .map(|i| pt(len * i as f32 / n as f32, 10.0, i * 2_000))
        .collect()
}

/// Wiggly stroke with varying pressure, tilt and azimuth (exercises every lerped input).
fn wiggle(n: u32) -> Vec<InkPoint> {
    (0..n)
        .map(|i| {
            let s = i as f32;
            let phase = s * 0.13;
            InkPoint {
                x: 20.0 + s * 1.7,
                y: 50.0 + 15.0 * libm::sinf(phase),
                t_us: i * 2_075,
                pressure: 0.5 + 0.45 * libm::sinf(phase * 0.7),
                size_pressure: 0.5 + 0.4 * libm::cosf(phase * 0.5),
                alpha_factor: 0.9,
                speed: 300.0,
                tilt: (s * 0.02) % 1.3,
                azimuth: ((s * 0.11) % (2.0 * PI)) - PI,
            }
        })
        .collect()
}

fn run(params: &BrushParams, seed: u64, points: &[InkPoint]) -> Vec<Dab> {
    let mut e = StampEngine::new(params, StampKind::Solid, seed);
    e.extend(points);
    e.finish(None)
}

fn bits(dabs: &[Dab]) -> Vec<[u32; 9]> {
    dabs.iter()
        .map(|d| {
            [
                d.x,
                d.y,
                d.radius_x,
                d.radius_y,
                d.angle,
                d.opacity,
                d.hardness,
                d.grain_strength,
                d.grain_level,
            ]
            .map(f32::to_bits)
        })
        .collect()
}

/// Union coverage `1 − Π(1 − α)` at `(x, y)` from hard round dabs covering it.
fn coverage_at(dabs: &[Dab], x: f32, y: f32) -> f32 {
    let mut clear = 1.0f32;
    for d in dabs {
        let (dx, dy) = (x - d.x, y - d.y);
        if dx * dx + dy * dy <= d.radius_y * d.radius_y {
            clear *= 1.0 - d.opacity;
        }
    }
    1.0 - clear
}

#[test]
fn no_points_no_dabs() {
    let mut e = StampEngine::new(&plain(10.0, 0.1, 1.0), StampKind::Solid, 0);
    e.update_tail(None);
    assert!(e.stable().is_empty());
    assert!(e.tentative().is_empty());
    assert!(e.finish(None).is_empty());
}

#[test]
fn single_point_gives_one_dab_at_s0() {
    let e = StampEngine::new(&plain(10.0, 0.1, 1.0), StampKind::Solid, 0);
    let dabs = e.finish(Some(&pt(3.0, 4.0, 0)));
    assert_eq!(dabs.len(), 1);
    let d = dabs[0];
    assert_eq!((d.x, d.y, d.radius_x, d.radius_y), (3.0, 4.0, 5.0, 5.0));
    assert_eq!(d.opacity, 1.0);
}

#[test]
fn straight_line_dabs_are_evenly_spaced() {
    // D = 10, spacing 0.1 → one dab per px.
    let dabs = run(&plain(10.0, 0.1, 0.5), 0, &line(100.0, 2.0));
    assert!((100..=101).contains(&dabs.len()), "{} dabs", dabs.len());
    for (k, d) in dabs.iter().enumerate() {
        assert!((d.x - k as f32).abs() < 1e-3, "dab {k} at {}", d.x);
        assert_eq!(d.y, 10.0);
    }
}

#[test]
fn dab_count_scales_linearly_with_length() {
    let p = plain(8.0, 0.25, 0.5);
    let counts: Vec<usize> = [200.0, 400.0, 800.0]
        .iter()
        .map(|&len| run(&p, 0, &line(len, 3.0)).len())
        .collect();
    // Step 2 px: len/2 + 1 dabs.
    for (&len, &c) in [200.0f32, 400.0, 800.0].iter().zip(&counts) {
        let expected = len / 2.0 + 1.0;
        assert!((c as f32 - expected).abs() <= 1.0, "len {len}: {c} dabs");
    }
    let ratio = (counts[2] - 1) as f32 / (counts[0] - 1) as f32;
    assert!((ratio - 4.0).abs() < 0.05, "ratio {ratio}");
}

#[test]
fn placement_follows_arc_length_not_sample_density() {
    let p = plain(10.0, 0.3, 0.5);
    let coarse = run(&p, 0, &line(90.0, 90.0));
    let fine = run(&p, 0, &line(90.0, 0.7));
    assert!(coarse.len().abs_diff(fine.len()) <= 1);
    for (a, b) in coarse.iter().zip(&fine) {
        assert!((a.x - b.x).abs() < 1e-2, "{} vs {}", a.x, b.x);
    }
}

#[test]
fn spatial_step_is_clamped_to_half_pixel() {
    // spacing·D = 0.1 px, clamped to 0.5 px.
    let dabs = run(&plain(1.0, 0.1, 0.5), 0, &line(100.0, 2.0));
    assert!((200..=201).contains(&dabs.len()), "{} dabs", dabs.len());
}

#[test]
fn flow_compensation_keeps_coverage_constant_across_spacing() {
    let flow = 0.5;
    for (d, spacing) in [(10.0, 0.05), (10.0, 0.1), (10.0, 0.2), (2.0, 0.05)] {
        let dabs = run(&plain(d, spacing, flow), 0, &line(100.0, 2.0));
        for x in [50.3f32, 61.7, 70.05] {
            let c = coverage_at(&dabs, x, 10.0);
            assert!(
                (c - flow).abs() < 0.07,
                "D {d} spacing {spacing}: coverage {c} at {x}"
            );
        }
    }
}

#[test]
fn airbrush_emits_over_time_while_stationary() {
    let mut p = plain(30.0, 0.1, 0.1);
    p.stamp.dabs_per_second = 60.0;
    // One second still, sampled every 5 ms.
    let still: Vec<InkPoint> = (0..=200).map(|i| pt(40.0, 40.0, i * 5_000)).collect();
    let dabs = run(&p, 0, &still);
    assert!((60..=62).contains(&dabs.len()), "{} dabs", dabs.len());
    assert!(dabs.iter().all(|d| d.x == 40.0 && d.y == 40.0));
    // Same samples, timestamps doubled → twice the dabs: emission depends on timestamps only.
    let slow: Vec<InkPoint> = (0..=200).map(|i| pt(40.0, 40.0, i * 10_000)).collect();
    let slow_count = run(&p, 0, &slow).len();
    assert!(
        slow_count.abs_diff(2 * dabs.len()) <= 2,
        "{slow_count} vs {}",
        dabs.len()
    );
    // No temporal rate → only the s = 0 dab.
    assert_eq!(run(&plain(30.0, 0.1, 0.1), 0, &still).len(), 1);
}

#[test]
fn airbrush_gap_is_clamped() {
    let mut p = plain(30.0, 0.1, 0.1);
    p.stamp.dabs_per_second = 60.0;
    // A 1 s gap counts as 50 ms: ~3 dabs, not 60.
    let dabs = run(&p, 0, &[pt(5.0, 5.0, 0), pt(5.0, 5.0, 1_000_000)]);
    assert!((3..=4).contains(&dabs.len()), "{} dabs", dabs.len());
    // Time running backwards adds nothing.
    assert_eq!(run(&p, 0, &[pt(5.0, 5.0, 9_000), pt(5.0, 5.0, 0)]).len(), 1);
}

/// Builder-style streaming: push chunks of committed points, update the tail with the next pending point.
fn streamed(params: &BrushParams, seed: u64, points: &[InkPoint], chunk: usize) -> Vec<Dab> {
    let (last, committed) = points.split_last().unwrap();
    let mut e = StampEngine::new(params, StampKind::Solid, seed);
    let mut start = 0;
    while start < committed.len() {
        let end = (start + chunk).min(committed.len());
        e.extend(&committed[start..end]);
        e.update_tail(points.get(end));
        start = end;
    }
    e.finish(Some(last))
}

#[test]
fn chunked_extend_matches_batch_for_every_preset() {
    let points = wiggle(240);
    for (preset, _) in STAMP_PRESETS {
        let params = preset.params();
        let batch = bits(&streamed(&params, 7, &points, points.len()));
        assert!(!batch.is_empty());
        for chunk in [1, 2, 3, 7, 13, 64] {
            assert_eq!(
                batch,
                bits(&streamed(&params, 7, &points, chunk)),
                "{} chunk {chunk}",
                preset.name()
            );
        }
    }
}

#[test]
fn stable_never_changes_and_tentative_predicts_commit() {
    for (preset, kind) in STAMP_PRESETS {
        let params = preset.params();
        let points = wiggle(120);
        let mut e = StampEngine::new(&params, kind, 3);
        let mut seen: Vec<[u32; 9]> = Vec::new();
        for (i, p) in points.iter().enumerate() {
            e.update_tail(Some(p));
            let predicted = bits(e.tentative());
            assert_eq!(
                bits(e.stable()),
                seen,
                "{} tail must not touch stable",
                preset.name()
            );
            e.extend(core::slice::from_ref(p));
            let now = bits(e.stable());
            assert_eq!(
                &now[..seen.len()],
                &seen[..],
                "{} stable prefix changed",
                preset.name()
            );
            assert_eq!(
                &now[seen.len()..],
                &predicted[..],
                "{} point {i}",
                preset.name()
            );
            seen = now;
        }
    }
}

#[test]
fn same_seed_same_dabs_different_seed_different_scatter() {
    let mut p = plain(12.0, 0.2, 0.5);
    p.stamp.scatter = 0.5;
    p.stamp.jitter = Jitter {
        size: 0.3,
        opacity: 0.3,
        angle: 1.0,
    };
    let points = line(200.0, 2.5);
    let a = run(&p, 42, &points);
    assert_eq!(bits(&a), bits(&run(&p, 42, &points)));
    let b = run(&p, 43, &points);
    assert_eq!(a.len(), b.len());
    let moved = a
        .iter()
        .zip(&b)
        .filter(|(da, db)| da.x != db.x || da.y != db.y)
        .count();
    assert!(
        moved * 10 >= a.len() * 9,
        "only {moved}/{} dabs differ",
        a.len()
    );
    // Scatter stays within scatter·D of the path.
    for d in &a {
        assert!((d.y - 10.0).abs() <= 6.0 + 1e-3);
        assert!(d.radius_y <= 6.0 && d.radius_y >= 6.0 * 0.7 - 1e-3);
    }
}

#[test]
fn dab_cap_stops_emission() {
    let cap = MAX_DABS_PER_STROKE as usize;
    // Step 0.5 px over 120 000 px → 240 001 dabs uncapped.
    let p = plain(1.0, 0.01, 0.5);
    let far = [pt(0.0, 0.0, 0), pt(120_000.0, 0.0, 2_000)];
    let mut e = StampEngine::new(&p, StampKind::Solid, 0);
    e.extend(&far[..1]);
    e.update_tail(Some(&far[1]));
    assert_eq!(e.stable().len() + e.tentative().len(), cap);
    e.extend(&far[1..]);
    e.update_tail(Some(&pt(0.0, 0.0, 4_000)));
    assert!(e.tentative().is_empty());
    assert_eq!(e.finish(None).len(), cap);
}

#[test]
fn zero_particle_spray_with_huge_rate_terminates_empty() {
    let mut p = Preset::Spray.params();
    let Some(mut spray) = p.stamp.spray else {
        panic!("spray preset has spray params")
    };
    spray.particles_max = 0;
    p.stamp.spray = Some(spray);
    p.stamp.dabs_per_second = 1e9;
    let points = [
        pt(0.0, 0.0, 0),
        pt(30.0, 0.0, 1_000_000),
        pt(60.0, 0.0, 2_000_000),
    ];
    assert!(run(&p, 3, &points).is_empty());
    assert!(streamed(&p, 3, &points, 1).is_empty());
    // Zero-pressure samples burst no particles either, even with particles.
    let mut p = Preset::Spray.params();
    p.stamp.dabs_per_second = 1e9;
    let mut dry = points;
    for q in &mut dry {
        q.pressure = 0.0;
    }
    assert!(run(&p, 3, &dry).is_empty());
}

#[test]
fn huge_temporal_rate_stops_at_the_cap_identically_when_streamed() {
    let mut p = plain(10.0, 0.1, 0.5);
    p.stamp.dabs_per_second = 1e12;
    let points = [pt(0.0, 0.0, 0), pt(5.0, 0.0, 10_000), pt(9.0, 0.0, 20_000)];
    let batch = run(&p, 0, &points);
    assert_eq!(batch.len(), MAX_DABS_PER_STROKE as usize);
    assert_eq!(bits(&batch), bits(&streamed(&p, 0, &points, 1)));
}

#[test]
fn side_hardness_alone_still_shades_tilted_dabs() {
    let mut p = plain(10.0, 0.1, 0.5);
    p.stamp.hardness = 0.8;
    p.dynamics.tilt = TiltResponse {
        side_hardness: 0.2,
        ..TiltResponse::NONE
    };
    let mut tilted = pt(50.0, 50.0, 0);
    tilted.tilt = 70.0 * crate::dynamics::DEG;
    let dab =
        |p: &BrushParams, q: &InkPoint| StampEngine::new(p, StampKind::Solid, 0).finish(Some(q))[0];
    assert!((dab(&p, &tilted).hardness - 0.2).abs() < 1e-6);
    assert!((dab(&p, &pt(50.0, 50.0, 0)).hardness - 0.8).abs() < 1e-6);
    // A fully neutral response leaves tilted dabs untouched (no hardening, no lean shift).
    p.dynamics.tilt = TiltResponse::NONE;
    let d = dab(&p, &tilted);
    assert!((d.hardness - 0.8).abs() < 1e-6);
    assert_eq!((d.x, d.y), (50.0, 50.0));
}

#[test]
fn spray_bursts_hard_particles_inside_the_area() {
    for distribution in [SprayDistribution::Uniform, SprayDistribution::Gaussian] {
        let mut p = Preset::Spray.params();
        let Some(mut spray) = p.stamp.spray else {
            panic!("spray preset has spray params")
        };
        spray.distribution = distribution;
        p.stamp.spray = Some(spray);
        let e = StampEngine::new(&p, StampKind::Spray, 5);
        let burst = e.finish(Some(&pt(100.0, 100.0, 0)));
        assert_eq!(burst.len(), spray.particles_max as usize);
        let area_r = 0.5 * p.size_max;
        for d in &burst {
            let (dx, dy) = (d.x - 100.0, d.y - 100.0);
            let dist = (dx * dx + dy * dy).sqrt();
            assert!(dist <= area_r + 1e-3, "particle at {dist} > {area_r}");
            assert_eq!(d.hardness, 1.0);
            assert!(d.radius_x * 2.0 >= spray.particle_size_min - 1e-4);
            assert!(d.radius_x * 2.0 <= spray.particle_size_max + 1e-4);
        }
        let mut half = pt(0.0, 0.0, 0);
        half.pressure = 0.5;
        let e = StampEngine::new(&p, StampKind::Spray, 5);
        assert_eq!(
            e.finish(Some(&half)).len(),
            (spray.particles_max / 2) as usize
        );
    }
}

#[test]
fn pencil_tilt_makes_elliptical_side_dabs() {
    let params = Preset::Pencil.params();
    let mut tilted = pt(50.0, 50.0, 0);
    tilted.tilt = 70.0 * crate::dynamics::DEG;
    tilted.azimuth = FRAC_PI_2; // leaning toward +x
    let d = StampEngine::new(&params, StampKind::Pencil, 0).finish(Some(&tilted))[0];
    let r = 0.5 * params.size_max;
    let t = params.dynamics.tilt;
    assert!((d.radius_y - r * t.max_radius_scale).abs() < 1e-4, "{d:?}");
    assert!(
        (d.radius_x - d.radius_y * t.max_aspect).abs() < 1e-3,
        "{d:?}"
    );
    assert!((d.angle - FRAC_PI_2).abs() < 1e-5, "{d:?}");
    assert!(d.x > 50.0 && (d.y - 50.0).abs() < 1e-4, "{d:?}");
    // Writing posture stays round.
    let mut upright = tilted;
    upright.tilt = 30.0 * crate::dynamics::DEG;
    let d = StampEngine::new(&params, StampKind::Pencil, 0).finish(Some(&upright))[0];
    assert_eq!(d.radius_x, d.radius_y);
    assert_eq!((d.x, d.y), (50.0, 50.0));
}

#[test]
fn dabs_are_finite_and_in_range_for_every_preset() {
    let points = wiggle(200);
    for (preset, kind) in STAMP_PRESETS {
        let mut e = StampEngine::new(&preset.params(), kind, 11);
        e.extend(&points);
        for d in e.finish(None) {
            let all = [
                d.x,
                d.y,
                d.radius_x,
                d.radius_y,
                d.angle,
                d.opacity,
                d.hardness,
                d.grain_strength,
                d.grain_level,
            ];
            assert!(all.iter().all(|v| v.is_finite()), "{} {d:?}", preset.name());
            assert!(
                d.radius_y > 0.0 && d.radius_x >= d.radius_y,
                "{} {d:?}",
                preset.name()
            );
            assert!(d.hardness > 0.0 && d.hardness <= 1.0);
            for v in [d.opacity, d.grain_strength, d.grain_level] {
                assert!((0.0..=1.0).contains(&v));
            }
        }
    }
}

#[test]
fn zero_length_zero_time_segments_emit_nothing() {
    let p = plain(10.0, 0.1, 0.5);
    let dup = [pt(1.0, 1.0, 0), pt(1.0, 1.0, 0), pt(1.0, 1.0, 0)];
    assert_eq!(run(&p, 0, &dup).len(), 1);
}
