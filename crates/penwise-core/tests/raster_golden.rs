//! Server raster PNG goldens (core-determinism.md §3): render synthetic strokes and compare **decoded pixels**
//! with `tests/goldens/raster_*.png` under `PixelTolerance::GOLDEN` (max channel delta ≤ 2, < 0.1 % of pixels
//! differing). Pixels are platform-rasterized, so these goldens are native/server-only (feature `raster`).
//!
//! Bless (write/overwrite the goldens): `PENWISE_BLESS=1 cargo test -p penwise-core --features raster --test
//! raster_golden`. A missing golden is blessed automatically. On mismatch, `actual` and `diff` PNGs are written
//! to `target/penwise-golden/`.
//!
//! Owner: slice e.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "test harness: helpers outside #[test] fns report failures directly"
)]

use penwise_core::brush::{BlendMode, Color};
use penwise_core::geometry::{Dab, Outline, StrokeGeometry, StrokeStyle};
use penwise_core::raster::png::{
    PixelTolerance, compare_pixels, decode_png, diff_image, encode_png,
};
use penwise_core::raster::stamps::CoverageBuffer;
use penwise_core::raster::tiny_skia::{Color as SkColor, Pixmap};
use penwise_core::raster::{Viewport, render_strokes};
use penwise_core::{Preset, Sample, StrokeBuilder};
use std::path::PathBuf;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 120;

/// A wavy left-to-right stroke at 482 Hz with a pressure swell and moderate tilt toward the lower right.
fn wave(y_center: f32, amplitude: f32) -> Vec<Sample> {
    (0..160u16)
        .map(|i| {
            let t = f32::from(i) / 159.0;
            Sample {
                x: 24.0 + 272.0 * t,
                y: y_center + amplitude * libm::sinf(t * 2.0 * core::f32::consts::TAU),
                pressure: 0.25 + 0.6 * libm::sinf(t * core::f32::consts::PI),
                tilt: 0.6,
                azimuth: 2.2,
                t_us: u32::from(i) * 2075,
            }
        })
        .collect()
}

fn straight(y: f32) -> Vec<Sample> {
    (0..80u16)
        .map(|i| Sample {
            x: 16.0 + 3.6 * f32::from(i),
            y,
            pressure: 0.5,
            tilt: 0.0,
            azimuth: 0.0,
            t_us: u32::from(i) * 2075,
        })
        .collect()
}

fn build(preset: Preset, seed: u64, samples: &[Sample]) -> StrokeGeometry {
    StrokeBuilder::build(&preset.params(), seed, samples)
}

fn render(strokes: &[StrokeGeometry]) -> Pixmap {
    let mut pixmap = Pixmap::new(WIDTH, HEIGHT).unwrap();
    pixmap.fill(SkColor::WHITE);
    let mut scratch = CoverageBuffer::default();
    render_strokes(strokes, &Viewport::IDENTITY, &mut pixmap, &mut scratch).unwrap();
    pixmap
}

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

fn artifacts_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"),
            PathBuf::from,
        )
        .join("penwise-golden")
}

/// Compares `actual` with `tests/goldens/<name>.png`; blesses when missing or `PENWISE_BLESS=1`.
fn check_golden(name: &str, actual: &Pixmap) {
    let path = goldens_dir().join(format!("{name}.png"));
    let bless = std::env::var("PENWISE_BLESS").is_ok_and(|v| v == "1");
    if bless || !path.exists() {
        std::fs::create_dir_all(goldens_dir()).unwrap();
        std::fs::write(&path, encode_png(actual).unwrap()).unwrap();
        eprintln!("blessed {}", path.display());
        return;
    }
    let expected = decode_png(&std::fs::read(&path).unwrap()).unwrap();
    // Round-trip through PNG so both sides are compared exactly as stored.
    let actual = decode_png(&encode_png(actual).unwrap()).unwrap();
    let diff = compare_pixels(&expected, &actual).unwrap();
    if diff.passes(PixelTolerance::GOLDEN) {
        return;
    }
    let out = artifacts_dir();
    std::fs::create_dir_all(&out).unwrap();
    let image = diff_image(&expected, &actual, PixelTolerance::GOLDEN).unwrap();
    std::fs::write(
        out.join(format!("{name}-actual.png")),
        encode_png(&actual).unwrap(),
    )
    .unwrap();
    std::fs::write(
        out.join(format!("{name}-diff.png")),
        encode_png(&image).unwrap(),
    )
    .unwrap();
    panic!(
        "{name}: {diff:?} exceeds {:?}; artifacts in {} (re-bless with PENWISE_BLESS=1 if intended)",
        PixelTolerance::GOLDEN,
        out.display()
    );
}

fn dab(
    x: f32,
    y: f32,
    radius_x: f32,
    radius_y: f32,
    angle: f32,
    opacity: f32,
    hardness: f32,
) -> Dab {
    Dab {
        x,
        y,
        radius_x,
        radius_y,
        angle,
        opacity,
        hardness,
        grain_strength: 0.0,
        grain_level: 1.0,
    }
}

/// Hand-built geometry: exercises the rasterizer alone, independent of the stroke engines.
fn primitives() -> Vec<StrokeGeometry> {
    let ink = StrokeStyle {
        color: Color::INK,
        opacity: 1.0,
        blend: BlendMode::Normal,
    };
    let triangle = Outline {
        points: vec![[20.0, 100.0], [70.0, 15.5], [110.25, 100.0]],
        contour_ends: vec![3],
    };
    let soft: Vec<Dab> = (0..40u8)
        .map(|i| dab(120.0 + 2.5 * f32::from(i), 40.0, 12.0, 12.0, 0.0, 0.3, 0.3))
        .collect();
    let chisel: Vec<Dab> = (0..12u8)
        .map(|i| {
            dab(
                130.0 + 14.0 * f32::from(i),
                85.0,
                10.0,
                3.0,
                0.15 * f32::from(i),
                0.9,
                0.9,
            )
        })
        .collect();
    let band = Outline {
        points: vec![[10.0, 55.0], [310.0, 55.0], [310.0, 75.0], [10.0, 75.0]],
        contour_ends: vec![4],
    };
    vec![
        StrokeGeometry::Outline {
            style: ink,
            outline: triangle,
        },
        StrokeGeometry::Stamps {
            style: StrokeStyle {
                color: Color::rgb(30, 90, 200),
                opacity: 0.7,
                ..ink
            },
            grain: None,
            dabs: soft,
        },
        StrokeGeometry::Stamps {
            style: StrokeStyle {
                color: Color::rgb(180, 30, 40),
                ..ink
            },
            grain: None,
            dabs: chisel,
        },
        StrokeGeometry::Outline {
            style: StrokeStyle {
                color: Color::HIGHLIGHTER,
                opacity: 0.35,
                blend: BlendMode::Multiply,
            },
            outline: band,
        },
    ]
}

#[test]
fn raster_primitives() {
    check_golden("raster_primitives", &render(&primitives()));
}

#[test]
fn raster_outline_family() {
    check_golden(
        "raster_outline",
        &render(&[build(Preset::Ballpoint, 1, &wave(60.0, 30.0))]),
    );
}

#[test]
fn raster_stamp_family() {
    check_golden(
        "raster_stamp",
        &render(&[build(Preset::Pencil, 2, &wave(60.0, 30.0))]),
    );
}

#[test]
fn raster_highlighter_over_ink() {
    // Highlighter listed last on purpose: the multiply band still renders it under the ink.
    let strokes = [
        build(Preset::Ballpoint, 3, &wave(60.0, 30.0)),
        build(Preset::Highlighter, 4, &straight(60.0)),
    ];
    check_golden("raster_highlighter_over_ink", &render(&strokes));
}
