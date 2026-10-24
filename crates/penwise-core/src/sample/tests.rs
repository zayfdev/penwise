#![allow(
    clippy::float_cmp,
    reason = "exact outputs are the contract under test"
)]

use core::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use super::{
    ANGLE_STEP, AZIMUTH_GRID_HALF, AxisRange, DigitizerRanges, GridSample, POSITION_LIMIT_PX,
    PxScale, RawSample, Sample, TILT_GRID_MAX,
};

const R: DigitizerRanges = DigitizerRanges::SM_X710;

fn raw(x: i32, y: i32, pressure: i32, tilt_x_deg: i32, tilt_y_deg: i32) -> RawSample {
    RawSample {
        x,
        y,
        pressure,
        tilt_x_deg,
        tilt_y_deg,
        t_us: 1234,
    }
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn digitizer_corners_map_to_page_corners() {
    // (raw x, raw y) → expected page corner (px); the last device count lands < 1 px before the page edge.
    let corners = [
        ((0, 0), (0.0, 0.0)),
        ((14752, 0), (1600.0, 0.0)),
        ((0, 23603), (0.0, 2560.0)),
        ((14752, 23603), (1600.0, 2560.0)),
    ];
    for ((rx, ry), (px, py)) in corners {
        let s = R.map(&raw(rx, ry, 0, 0, 0));
        assert!(close(s.x, px, 0.2), "x {rx} → {} (want ≈{px})", s.x);
        assert!(close(s.y, py, 0.2), "y {ry} → {} (want ≈{py})", s.y);
        assert!(s.x >= 0.0 && s.x < 1600.0 && s.y >= 0.0 && s.y < 2560.0);
    }
    let origin = R.map(&raw(0, 0, 0, 0, 0));
    assert_eq!(origin.x.to_bits(), 0.0f32.to_bits());
    assert_eq!(origin.y.to_bits(), 0.0f32.to_bits());
}

#[test]
fn pressure_normalizes_and_clamps() {
    assert_eq!(R.map(&raw(0, 0, 0, 0, 0)).pressure, 0.0);
    assert_eq!(R.map(&raw(0, 0, 4095, 0, 0)).pressure, 1.0);
    assert!(close(R.map(&raw(0, 0, 2048, 0, 0)).pressure, 0.5, 1e-3));
    assert_eq!(R.map(&raw(0, 0, -10, 0, 0)).pressure, 0.0);
    assert_eq!(R.map(&raw(0, 0, 9000, 0, 0)).pressure, 1.0);

    let mut degenerate = R;
    degenerate.pressure = AxisRange { min: 5, max: 5 };
    assert_eq!(degenerate.map(&raw(0, 0, 5, 0, 0)).pressure, 0.0);
}

#[test]
fn zero_den_maps_position_to_zero() {
    let mut r = R;
    r.x_scale = PxScale { num: 1600, den: 0 };
    assert_eq!(r.map(&raw(5000, 0, 0, 0, 0)).x, 0.0);
}

#[test]
fn upright_pen_has_zero_tilt_and_azimuth() {
    let s = R.map(&raw(100, 100, 1000, 0, 0));
    assert_eq!(s.tilt.to_bits(), 0.0f32.to_bits());
    assert_eq!(s.azimuth.to_bits(), 0.0f32.to_bits());
    assert_eq!(s.t_us, 1234);
}

#[test]
fn lean_direction_follows_orientation_convention() {
    // Device +X/+Y are page +x/+y (y down); azimuth is clockwise from screen-up.
    let h = core::f32::consts::FRAC_1_SQRT_2;
    let cases = [
        ((30, 0), FRAC_PI_2, [1.0, 0.0]),    // leans right
        ((-30, 0), -FRAC_PI_2, [-1.0, 0.0]), // leans left
        ((0, -30), 0.0, [0.0, -1.0]),        // leans up
        ((0, 30), PI, [0.0, 1.0]),           // leans down
        ((30, -30), FRAC_PI_4, [h, -h]),
    ];
    for ((tx, ty), want, lean) in cases {
        let s = R.map(&raw(0, 0, 0, tx, ty));
        assert!(
            close(s.azimuth, want, 1e-5),
            "({tx},{ty}) → az {} want {want}",
            s.azimuth
        );
        // lean_vector points the same way in page axes.
        let [lx, ly] = s.lean_vector();
        assert!(
            close(lx, lean[0], 1e-5) && close(ly, lean[1], 1e-5),
            "({tx},{ty}) → lean ({lx},{ly})"
        );
    }
}

#[test]
fn tilt_magnitude_matches_single_axis_angle() {
    let s = R.map(&raw(0, 0, 0, 30, 0));
    assert!(close(s.tilt, 30.0 * PI / 180.0, 1e-5));
    let s = R.map(&raw(0, 0, 0, 0, -63));
    assert!(close(s.tilt, 63.0 * PI / 180.0, 1e-5));
    // Out-of-range degrees clamp to 89° per axis; tilt never exceeds π/2.
    let s = R.map(&raw(0, 0, 0, 500, -500));
    assert!(s.tilt.is_finite() && s.tilt <= FRAC_PI_2 && s.tilt > 1.5);
    assert!(s.azimuth.is_finite());
}

#[test]
fn mapped_samples_survive_quantize() {
    let s: Sample = R.map(&raw(14752, 23603, 4095, -63, 63)).quantize();
    assert!(s.x.is_finite() && s.y.is_finite());
    assert_eq!(s, s.quantize());
}

fn cell(x: i32, y: i32, pressure: u16, tilt: u16, azimuth: i16) -> GridSample {
    GridSample {
        x,
        y,
        pressure,
        tilt,
        azimuth,
        t_us: 17,
    }
}

fn upright(x: f32, y: f32, pressure: f32, tilt: f32, azimuth: f32) -> Sample {
    Sample {
        x,
        y,
        pressure,
        tilt,
        azimuth,
        t_us: 17,
    }
}

#[test]
fn every_grid_value_round_trips_through_sample() {
    let lim = 1_i32 << 24;
    let mut positions: Vec<i32> = (-lim..=lim).step_by(65_537).collect();
    positions.extend([-lim, -lim + 1, -1, 0, 1, lim - 1, lim]);
    for &p in &positions {
        let g = cell(p, -p, 0, 0, 0);
        assert_eq!(g.to_sample().to_grid(), g, "position {p}");
    }
    for pressure in 0..=4096 {
        let g = cell(0, 0, pressure, 0, 0);
        assert_eq!(g.to_sample().to_grid(), g, "pressure {pressure}");
    }
    for tilt in 0..=TILT_GRID_MAX {
        let g = cell(0, 0, 0, tilt, 0);
        assert_eq!(g.to_sample().to_grid(), g, "tilt {tilt}");
    }
    for azimuth in (1 - AZIMUTH_GRID_HALF)..=AZIMUTH_GRID_HALF {
        let g = cell(0, 0, 0, 0, azimuth);
        assert_eq!(g.to_sample().to_grid(), g, "azimuth {azimuth}");
    }
}

#[test]
fn nan_components_map_to_zero() {
    let nan = f32::NAN;
    assert_eq!(
        upright(nan, nan, nan, nan, nan).to_grid(),
        cell(0, 0, 0, 0, 0)
    );
}

#[test]
fn negative_zero_quantizes_to_positive_zero() {
    let q = upright(-0.0, -0.0, -0.0, -0.0, -0.0).quantize();
    for v in [q.x, q.y, q.pressure, q.tilt, q.azimuth] {
        assert_eq!(v.to_bits(), 0.0_f32.to_bits());
    }
}

#[test]
fn out_of_range_values_clamp_to_the_grid_edges() {
    let big = 2.0 * POSITION_LIMIT_PX;
    let g = upright(big, -big, 1.5, 2.0, 0.0).to_grid();
    assert_eq!(g, cell(1 << 24, -(1 << 24), 4096, TILT_GRID_MAX, 0));
    let g = upright(f32::INFINITY, f32::NEG_INFINITY, -1.0, -0.5, 0.0).to_grid();
    assert_eq!(g, cell(1 << 24, -(1 << 24), 0, 0, 0));
}

#[test]
fn azimuth_wraps_into_half_open_pi_interval() {
    let half = AZIMUTH_GRID_HALF;
    let cases = [
        (PI, half),
        (-PI, half),
        (3.0 * PI, half),
        (-3.0 * PI, half),
        (PI + ANGLE_STEP, 1 - half),
        (-PI + ANGLE_STEP, 1 - half),
        (2.0 * PI, 0),
        (-FRAC_PI_2, -half / 2),
        (FRAC_PI_2 + 2.0 * PI, half / 2),
    ];
    for (azimuth, want) in cases {
        let g = upright(0.0, 0.0, 0.0, 0.0, azimuth).to_grid();
        assert_eq!(g.azimuth, want, "azimuth {azimuth}");
    }
    // Wrapped values stay on the grid: re-quantizing π and −π gives the same, inclusive π.
    let pi = upright(0.0, 0.0, 0.0, 0.0, -PI).quantize();
    assert_eq!(pi.azimuth, PI);
    assert_eq!(pi, pi.quantize());
}
