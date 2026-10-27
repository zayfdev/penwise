//! Stamp coverage parity fixture (`tests/goldens/stamp_coverage.txt`): what core's per-dab renderer leaves in
//! the coverage buffer `S` at sample pixels. The tablet painter (`StampRaster.kt`) and the browser painter
//! (`web/src/render/stamps.ts`) replay the same dabs through their own copy of the formulas and must land within
//! 1e-3 of the recorded `S`; this test pins the file to core, so changing core's formulas fails here until the
//! fixture is blessed (`just bless`, `PENWISE_BLESS=1`) and the clients' tests are run against it.
//!
//! Format (whitespace separated, `#` lines ignored):
//! `case <name> <viewport scale> <origin x> <origin y> <grain scale or 0> <grain softness>`, then
//! `dab x y rx ry angle opacity hardness grain_strength grain_level`, then
//! `at <col> <row> <paper height at the pixel centre> <S>`.
//!
//! `height <x> <y> <scale> <paper_height>` lines pin the clients' port of the procedural paper noise.
#![allow(
    clippy::unwrap_used,
    clippy::print_stderr,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test harness over pixel indices far below 2^24"
)]

use std::fmt::Write as _;
use std::path::PathBuf;

use penwise_core::brush::{BlendMode, Color, GrainParams};
use penwise_core::geometry::{Dab, StrokeStyle};
use penwise_core::raster::Viewport;
use penwise_core::raster::stamps::{CoverageBuffer, render_stamps};
use penwise_core::raster::tiny_skia::Pixmap;
use penwise_core::stamp::texture::paper_height;

const SIDE: u32 = 96;

struct Case {
    name: &'static str,
    viewport: Viewport,
    grain: Option<GrainParams>,
    dabs: Vec<Dab>,
}

fn dab(x: f32, y: f32, rx: f32, ry: f32, angle: f32, opacity: f32, hardness: f32) -> Dab {
    Dab {
        x,
        y,
        radius_x: rx,
        radius_y: ry,
        angle,
        opacity,
        hardness,
        grain_strength: 0.0,
        grain_level: 1.0,
    }
}

fn grained(mut d: Dab, strength: f32, level: f32) -> Dab {
    d.grain_strength = strength;
    d.grain_level = level;
    d
}

fn grain(scale: f32, softness: f32) -> GrainParams {
    GrainParams {
        scale,
        strength: 1.0,
        softness,
    }
}

fn cases() -> Vec<Case> {
    let vp = |scale: f32, ox: f32, oy: f32| Viewport {
        origin_x: ox,
        origin_y: oy,
        scale,
    };
    vec![
        Case {
            name: "hard_and_soft_discs",
            viewport: Viewport::IDENTITY,
            grain: None,
            dabs: vec![
                dab(24.0, 24.0, 9.0, 9.0, 0.0, 1.0, 1.0),
                dab(46.0, 26.0, 12.0, 12.0, 0.0, 0.8, 0.5),
                dab(30.0, 52.0, 14.0, 7.0, 0.7, 0.6, 0.25),
                dab(34.0, 56.0, 14.0, 7.0, -0.4, 0.6, 0.25),
            ],
        },
        Case {
            name: "minimum_fade_for_thin_and_sub_pixel_dabs",
            viewport: Viewport::IDENTITY,
            grain: None,
            dabs: vec![
                dab(10.3, 10.6, 0.4, 0.4, 0.0, 1.0, 0.5),
                dab(20.5, 10.5, 0.8, 0.8, 0.0, 1.0, 0.9),
                dab(30.2, 10.7, 1.2, 1.2, 0.0, 0.9, 1.0),
                dab(14.0, 30.0, 0.5, 4.0, 0.3, 1.0, 0.8),
                dab(30.0, 30.0, 6.0, 0.6, 1.1, 1.0, 0.6),
                dab(50.0, 30.0, 0.3, 0.3, 0.0, 1.0, 0.0),
            ],
        },
        Case {
            name: "grain_is_applied_per_dab_before_combining",
            viewport: Viewport::IDENTITY,
            grain: Some(grain(1.0, 0.15)),
            dabs: vec![
                grained(dab(30.0, 30.0, 14.0, 14.0, 0.0, 0.9, 0.6), 1.0, 0.3),
                grained(dab(42.0, 34.0, 14.0, 14.0, 0.0, 0.9, 0.6), 0.6, 0.7),
                grained(dab(36.0, 48.0, 12.0, 8.0, 0.5, 0.8, 0.4), 0.9, 0.5),
                dab(60.0, 60.0, 8.0, 8.0, 0.0, 0.8, 0.5),
            ],
        },
        Case {
            name: "grain_scale_three_softness_five_hundredths",
            viewport: Viewport::IDENTITY,
            grain: Some(grain(3.0, 0.05)),
            dabs: vec![
                grained(dab(30.0, 30.0, 16.0, 16.0, 0.0, 1.0, 0.7), 1.0, 0.55),
                grained(dab(52.0, 40.0, 16.0, 12.0, 0.9, 1.0, 0.7), 0.7, 0.4),
            ],
        },
        Case {
            name: "zoomed_view_with_origin_and_grain",
            viewport: vp(1.75, 1003.5, 207.25),
            grain: Some(grain(0.5, 0.2)),
            dabs: vec![
                grained(dab(1010.0, 215.0, 6.0, 6.0, 0.0, 1.0, 0.5), 1.0, 0.45),
                grained(dab(1022.0, 220.0, 5.0, 3.0, 0.8, 0.8, 0.3), 0.8, 0.6),
                grained(dab(1016.0, 232.0, 0.3, 0.3, 0.0, 1.0, 0.5), 1.0, 0.5),
            ],
        },
    ]
}

fn render_case(case: &Case) -> String {
    let style = StrokeStyle {
        color: Color::rgb(0, 0, 0),
        opacity: 1.0,
        blend: BlendMode::Normal,
    };
    let mut pm = Pixmap::new(SIDE, SIDE).unwrap();
    let mut scratch = CoverageBuffer::default();
    render_stamps(
        &mut pm,
        &case.dabs,
        case.grain.as_ref(),
        &style,
        &case.viewport,
        &mut scratch,
    )
    .unwrap();
    let vp = case.viewport;
    let (gscale, gsoft) = case.grain.map_or((0.0, 0.0), |g| (g.scale, g.softness));
    let mut out = format!(
        "case {} {:?} {:?} {:?} {:?} {:?}\n",
        case.name, vp.scale, vp.origin_x, vp.origin_y, gscale, gsoft
    );
    for d in &case.dabs {
        let _ = writeln!(
            out,
            "dab {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
            d.x,
            d.y,
            d.radius_x,
            d.radius_y,
            d.angle,
            d.opacity,
            d.hardness,
            d.grain_strength,
            d.grain_level
        );
    }
    for row in 0..SIDE {
        for col in 0..SIDE {
            // Every pixel the coverage buffer touches, plus a thin ring and a sparse grid of misses.
            let s = coverage(&scratch, col, row);
            let sparse = col % 7 == 0 && row % 7 == 0;
            if s <= 0.0 && !sparse {
                continue;
            }
            if s > 0.0 && (col + 2 * row) % 3 != 0 {
                continue;
            }
            let (qx, qy) = (
                (col as f32 + 0.5) / vp.scale + vp.origin_x,
                (row as f32 + 0.5) / vp.scale + vp.origin_y,
            );
            let height = if gscale > 0.0 {
                paper_height(qx, qy, gscale)
            } else {
                0.5
            };
            let _ = writeln!(out, "at {col} {row} {height:?} {s:?}");
        }
    }
    out
}

fn coverage(buf: &CoverageBuffer, col: u32, row: u32) -> f32 {
    let (x, y) = (
        i64::from(col) - i64::from(buf.origin_x),
        i64::from(row) - i64::from(buf.origin_y),
    );
    if x < 0 || y < 0 || x >= i64::from(buf.width) || y >= i64::from(buf.height) {
        return 0.0;
    }
    buf.data[(y as usize) * buf.width as usize + x as usize]
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/stamp_coverage.txt")
}

fn generated() -> String {
    let mut out = String::from(
        "# Generated by `just bless` from crates/penwise-core/tests/stamp_parity.rs; format in that file.\n",
    );
    for case in cases() {
        out.push_str(&render_case(&case));
    }
    for (i, scale) in [0.25_f32, 0.5, 1.0, 1.75, 3.0, 4.0].into_iter().enumerate() {
        for k in 0..12 {
            let x = (k as f32 * 83.37 - 300.0) * (1.0 + i as f32 * 0.5) + 0.31;
            let y = (k as f32 * 41.93 - 90.0) * (1.0 + i as f32 * 0.7) + 0.77;
            let _ = writeln!(
                out,
                "height {x:?} {y:?} {scale:?} {:?}",
                paper_height(x, y, scale)
            );
        }
    }
    out
}

#[test]
fn stamp_coverage_fixture_matches_core() {
    let actual = generated();
    let blessing = std::env::var_os("PENWISE_BLESS").is_some_and(|v| !v.is_empty() && v != "0");
    if blessing {
        std::fs::write(fixture_path(), &actual).unwrap();
        eprintln!("blessed {}", fixture_path().display());
        return;
    }
    let expected = std::fs::read_to_string(fixture_path())
        .expect("missing stamp_coverage.txt; run `just bless` (PENWISE_BLESS=1)");
    assert!(
        expected == actual,
        "core's stamp coverage changed; run `just bless`, then the tablet and browser parity tests"
    );
}
