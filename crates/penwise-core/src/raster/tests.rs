use super::outline::fill_outline;
use super::stamps::CoverageBuffer;
use super::{RasterError, Viewport, render_strokes};
use crate::brush::{BlendMode, Color};
use crate::geometry::{Dab, Outline, StrokeGeometry, StrokeStyle};
use tiny_skia::Pixmap;

fn square(x0: f32, y0: f32, x1: f32, y1: f32) -> Outline {
    Outline {
        points: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
        contour_ends: vec![4],
    }
}

fn style(color: Color, opacity: f32, blend: BlendMode) -> StrokeStyle {
    StrokeStyle {
        color,
        opacity,
        blend,
    }
}

fn white_page(w: u32, h: u32) -> Pixmap {
    let mut pm = Pixmap::new(w, h).unwrap();
    pm.fill(tiny_skia::Color::WHITE);
    pm
}

fn rgba(pm: &Pixmap, x: u32, y: u32) -> [u8; 4] {
    let p = pm.pixel(x, y).unwrap().demultiply();
    [p.red(), p.green(), p.blue(), p.alpha()]
}

#[test]
fn empty_geometry_renders_transparent() {
    let ink = style(Color::INK, 1.0, BlendMode::Normal);
    let strokes = [
        StrokeGeometry::Outline {
            style: ink,
            outline: Outline::default(),
        },
        StrokeGeometry::Stamps {
            style: ink,
            grain: None,
            dabs: Vec::new(),
        },
    ];
    let mut pm = Pixmap::new(32, 32).unwrap();
    let mut scratch = CoverageBuffer::default();
    render_strokes(&strokes, &Viewport::IDENTITY, &mut pm, &mut scratch).unwrap();
    assert!(pm.data().iter().all(|&b| b == 0));
}

#[test]
fn invalid_viewport_is_rejected() {
    let mut pm = Pixmap::new(4, 4).unwrap();
    let mut scratch = CoverageBuffer::default();
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let vp = Viewport {
            scale,
            ..Viewport::IDENTITY
        };
        assert_eq!(
            render_strokes(&[], &vp, &mut pm, &mut scratch),
            Err(RasterError::InvalidViewport)
        );
    }
}

#[test]
fn multiply_darkens_underlying_ink() {
    let mut pm = white_page(16, 16);
    let gray = Color::rgb(128, 128, 128);
    let vp = Viewport::IDENTITY;
    fill_outline(
        &mut pm,
        &square(0.0, 0.0, 16.0, 16.0),
        &style(gray, 1.0, BlendMode::Normal),
        &vp,
    )
    .unwrap();
    let hl = style(Color::HIGHLIGHTER, 1.0, BlendMode::Multiply);
    fill_outline(&mut pm, &square(0.0, 0.0, 16.0, 16.0), &hl, &vp).unwrap();
    let [r, g, b, a] = rgba(&pm, 8, 8);
    assert_eq!(a, 255);
    // 128 · (255, 225, 77)/255 ≈ (128, 113, 39).
    assert!(r.abs_diff(128) <= 1, "r {r}");
    assert!(g.abs_diff(113) <= 2, "g {g}");
    assert!(b.abs_diff(39) <= 2, "b {b}");
}

#[test]
fn translucent_multiply_never_lightens() {
    let mut pm = white_page(16, 16);
    let vp = Viewport::IDENTITY;
    let ink = Color::rgb(90, 140, 200);
    fill_outline(
        &mut pm,
        &square(0.0, 0.0, 16.0, 16.0),
        &style(ink, 1.0, BlendMode::Normal),
        &vp,
    )
    .unwrap();
    let before = rgba(&pm, 8, 8);
    let hl = style(Color::HIGHLIGHTER, 0.35, BlendMode::Multiply);
    fill_outline(&mut pm, &square(0.0, 0.0, 16.0, 16.0), &hl, &vp).unwrap();
    let after = rgba(&pm, 8, 8);
    for k in 0..3 {
        assert!(
            after[k] <= before[k],
            "channel {k}: {after:?} vs {before:?}"
        );
    }
    assert!(after[2] < before[2], "blue must darken under yellow");
}

#[test]
fn multiply_band_renders_under_normal_ink() {
    let ink = StrokeGeometry::Outline {
        style: style(Color::rgb(10, 20, 30), 1.0, BlendMode::Normal),
        outline: square(2.0, 2.0, 14.0, 14.0),
    };
    let highlighter = StrokeGeometry::Outline {
        style: style(Color::HIGHLIGHTER, 0.35, BlendMode::Multiply),
        outline: square(0.0, 0.0, 16.0, 16.0),
    };
    let mut pm = white_page(16, 16);
    let mut scratch = CoverageBuffer::default();
    // Highlighter listed last but still drawn first: opaque ink on top keeps its exact color.
    render_strokes(
        &[ink, highlighter],
        &Viewport::IDENTITY,
        &mut pm,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(rgba(&pm, 8, 8), [10, 20, 30, 255]);
    let edge = rgba(&pm, 0, 0);
    assert!(
        edge[2] < 255 && edge[0] == 255,
        "highlighter visible outside ink: {edge:?}"
    );
}

#[test]
fn strokes_outside_the_pixmap_are_skipped() {
    let far = StrokeGeometry::Stamps {
        style: style(Color::INK, 1.0, BlendMode::Normal),
        grain: None,
        dabs: vec![Dab {
            x: 5000.0,
            y: 5000.0,
            radius_x: 4.0,
            radius_y: 4.0,
            angle: 0.0,
            opacity: 1.0,
            hardness: 1.0,
            grain_strength: 0.0,
            grain_level: 1.0,
        }],
    };
    let mut pm = Pixmap::new(16, 16).unwrap();
    let mut scratch = CoverageBuffer::default();
    render_strokes(&[far], &Viewport::IDENTITY, &mut pm, &mut scratch).unwrap();
    assert!(pm.data().iter().all(|&b| b == 0));
}

#[test]
fn viewport_maps_page_to_pixels() {
    let ink = StrokeGeometry::Outline {
        style: style(Color::rgb(0, 0, 0), 1.0, BlendMode::Normal),
        outline: square(100.0, 100.0, 104.0, 104.0),
    };
    let vp = Viewport {
        origin_x: 100.0,
        origin_y: 100.0,
        scale: 2.0,
    };
    let mut pm = white_page(16, 16);
    let mut scratch = CoverageBuffer::default();
    render_strokes(&[ink], &vp, &mut pm, &mut scratch).unwrap();
    assert_eq!(rgba(&pm, 7, 7), [0, 0, 0, 255]);
    assert_eq!(rgba(&pm, 8, 8), [255, 255, 255, 255]);
}
