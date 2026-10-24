#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    reason = "test coordinates are small integers; bytes are rounded unit values"
)]

use super::*;

/// Repeat length of the paper in px at `scale`.
fn period_px(scale: f32) -> f32 {
    PAPER_PERIOD as f32 * PAPER_CELL_PX * scale
}

#[test]
fn height_is_in_unit_range_and_varies() {
    let mut lo = 1.0f32;
    let mut hi = 0.0f32;
    for i in 0..2000 {
        let x = i as f32 * 0.73 - 300.0;
        let y = i as f32 * 1.37 + 11.0;
        let h = paper_height(x, y, 1.0);
        assert!((0.0..=1.0).contains(&h), "h {h} at ({x}, {y})");
        lo = crate::float::min(lo, h);
        hi = crate::float::max(hi, h);
    }
    assert!(hi - lo > 0.5, "noise too flat: {lo}..{hi}");
}

#[test]
fn height_is_periodic_exactly_on_power_of_two_scales() {
    for scale in [0.25f32, 1.0, 2.0] {
        let p = period_px(scale);
        for i in 0..200 {
            let x = i as f32 * 1.25 + 0.375;
            let y = i as f32 * 0.5 - 40.0;
            let h = paper_height(x, y, scale);
            assert_eq!(h.to_bits(), paper_height(x + p, y, scale).to_bits());
            assert_eq!(h.to_bits(), paper_height(x, y + p, scale).to_bits());
            assert_eq!(
                h.to_bits(),
                paper_height(x - 3.0 * p, y + 2.0 * p, scale).to_bits()
            );
        }
    }
}

#[test]
fn height_is_periodic_on_other_scales() {
    let scale = 3.0;
    let p = period_px(scale);
    for i in 0..200 {
        let x = i as f32 * 2.5 + 0.25;
        let y = i as f32 * 1.75;
        let d = (paper_height(x, y, scale) - paper_height(x + p, y + p, scale)).abs();
        assert!(d < 1e-3, "difference {d}");
    }
}

#[test]
fn height_is_continuous() {
    for i in 0..500 {
        let x = i as f32 * 0.37;
        let a = paper_height(x, 5.0, 1.0);
        let b = paper_height(x + 0.01, 5.0, 1.0);
        assert!((a - b).abs() < 0.05, "jump {a} → {b} at x {x}");
    }
}

#[test]
fn degenerate_inputs_give_mid_gray() {
    assert_eq!(paper_height(1.0, 1.0, 0.0), 0.5);
    assert_eq!(paper_height(1.0, 1.0, -2.0), 0.5);
    assert_eq!(paper_height(1.0, 1.0, f32::NAN), 0.5);
    assert_eq!(paper_height(f32::INFINITY, 1.0, 1.0), 0.5);
    assert_eq!(paper_height(1.0, f32::NAN, 1.0), 0.5);
    assert_eq!(paper_height(1.0e30, 1.0, 1.0e-30), 0.5);
    let h = paper_height(1.0e6, -1.0e6, 1.0e-6);
    assert!((0.0..=1.0).contains(&h));
}

#[test]
fn grain_coverage_tooth_model() {
    // No strength: untouched.
    assert_eq!(grain_coverage(0.3, 0.5, 0.1, 0.0), 1.0);
    // Light pressure: only peaks catch.
    let level = 0.15;
    assert_eq!(grain_coverage(0.95, level, 0.05, 1.0), 1.0);
    assert_eq!(grain_coverage(0.2, level, 0.05, 1.0), 0.0);
    // Full pressure fills valleys.
    assert_eq!(grain_coverage(0.2, 1.0, 0.05, 1.0), 1.0);
    // Half strength halves the modulation.
    assert!((grain_coverage(0.2, level, 0.05, 0.5) - 0.5).abs() < 1e-6);
    // Monotone in height and level.
    let mut prev = 0.0;
    for i in 0..=100 {
        let c = grain_coverage(i as f32 / 100.0, 0.5, 0.15, 1.0);
        assert!(c >= prev);
        prev = c;
    }
    assert!(grain_coverage(0.4, 0.7, 0.15, 1.0) >= grain_coverage(0.4, 0.3, 0.15, 1.0));
}

#[test]
fn bake_tile_matches_paper_height_and_is_deterministic() {
    let mut a = vec![7u8; 3];
    bake_tile(1.0, 64, &mut a);
    assert_eq!(a.len(), 64 * 64);
    let mut b = Vec::new();
    bake_tile(1.0, 64, &mut b);
    assert_eq!(a, b);
    for (y, x) in [(0u32, 0u32), (5, 17), (63, 63)] {
        let h = paper_height(x as f32 + 0.5, y as f32 + 0.5, 1.0);
        let byte = (h * 255.0).round() as u8;
        assert_eq!(a[(y * 64 + x) as usize], byte);
    }
    bake_tile(1.0, 0, &mut a);
    assert!(a.is_empty());
}

#[test]
fn bake_tile_rejects_oversized_tiles_without_panicking() {
    let mut out = vec![1u8; 4];
    bake_tile(1.0, u32::MAX, &mut out);
    assert!(out.is_empty());
    out.push(9);
    bake_tile(1.0, MAX_TILE_SIZE + 1, &mut out);
    assert!(out.is_empty());
}

#[test]
fn baked_period_tile_wraps_at_edges() {
    // At scale 1 the paper repeats every 512 px, so a 512 px tile wraps seamlessly: the pixel past the right
    // (bottom) edge equals the first column (row), and the seam step is no larger than interior steps.
    let scale = 1.0;
    let size = 512u32;
    assert_eq!(period_px(scale), size as f32);
    let mut tile = Vec::new();
    bake_tile(scale, size, &mut tile);
    let at = |x: u32, y: u32| tile[(y * size + x) as usize];
    let mut max_interior = 0i32;
    let mut max_seam = 0i32;
    for k in 0..size {
        let past_right = paper_height(size as f32 + 0.5, k as f32 + 0.5, scale);
        assert_eq!((past_right * 255.0).round() as u8, at(0, k));
        let past_bottom = paper_height(k as f32 + 0.5, size as f32 + 0.5, scale);
        assert_eq!((past_bottom * 255.0).round() as u8, at(k, 0));
        max_seam = max_seam.max((i32::from(at(size - 1, k)) - i32::from(at(0, k))).abs());
        max_seam = max_seam.max((i32::from(at(k, size - 1)) - i32::from(at(k, 0))).abs());
        for x in 1..size {
            max_interior = max_interior.max((i32::from(at(x, k)) - i32::from(at(x - 1, k))).abs());
        }
    }
    assert!(
        max_seam <= 2 * max_interior,
        "seam {max_seam} > interior {max_interior}"
    );
}
