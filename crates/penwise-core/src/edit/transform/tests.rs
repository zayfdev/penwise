#![allow(
    clippy::float_cmp,
    reason = "exact outputs are the contract under test"
)]

use super::{SNAP_STEP, drag_angle, snap_angle};
use core::f32::consts::PI;

fn deg(d: f32) -> f32 {
    d * PI / 180.0
}

#[test]
fn snaps_within_three_degrees_of_fifteen() {
    assert_eq!(snap_angle(deg(14.0)), SNAP_STEP);
    assert_eq!(snap_angle(deg(15.0)), SNAP_STEP);
    // Exactly 3° away is still "within" (inclusive boundary).
    assert_eq!(snap_angle(deg(18.0)), SNAP_STEP);
    assert_eq!(snap_angle(deg(18.5)), deg(18.5));
    assert_eq!(snap_angle(deg(-14.0)), -SNAP_STEP);
    assert_eq!(snap_angle(deg(172.0)), deg(172.0));
    assert_eq!(snap_angle(deg(1.0)), 0.0);
    assert_eq!(snap_angle(f32::NAN), 0.0);
    assert_eq!(snap_angle(f32::INFINITY), 0.0);
}

#[test]
fn drag_angle_is_clockwise_and_wrapped() {
    let pivot = [10.0, 10.0];
    // Screen y points down: right → down is a clockwise quarter turn, positive.
    let quarter = drag_angle(pivot, [20.0, 10.0], [10.0, 20.0], false);
    assert!((quarter - PI / 2.0).abs() < 1e-6);
    let back = drag_angle(pivot, [10.0, 20.0], [20.0, 10.0], false);
    assert!((back + PI / 2.0).abs() < 1e-6);
    // From just above the negative x axis to just below it: a small turn, not ~2π.
    let small = drag_angle(pivot, [0.0, 9.9], [0.0, 10.1], false);
    assert!(small.abs() < 0.05 && small > -PI && small <= PI);
    let half = drag_angle(pivot, [20.0, 10.0], [0.0, 10.0], false);
    assert!(
        half > 0.0 && (half - PI).abs() < 1e-6,
        "wrapped into (−π, π]"
    );
    assert_eq!(drag_angle(pivot, pivot, [0.0, 0.0], false), 0.0);
    assert_eq!(drag_angle(pivot, [0.0, 0.0], pivot, true), 0.0);
    let snapped = drag_angle(pivot, [20.0, 10.0], [20.0, 12.5], true);
    assert_eq!(snapped, SNAP_STEP);
}
