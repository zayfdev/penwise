#![allow(
    clippy::naive_bytecount,
    reason = "test-only pixel counting; not worth a bytecount dependency"
)]

use super::fill_outline;
use crate::brush::{BlendMode, Color};
use crate::geometry::{Outline, StrokeStyle};
use crate::raster::Viewport;
use tiny_skia::Pixmap;

const BLACK: StrokeStyle = StrokeStyle {
    color: Color::rgb(0, 0, 0),
    opacity: 1.0,
    blend: BlendMode::Normal,
};

fn square(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<[f32; 2]> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

fn render(outline: &Outline, style: &StrokeStyle) -> Pixmap {
    let mut pm = Pixmap::new(32, 32).unwrap();
    fill_outline(&mut pm, outline, style, &Viewport::IDENTITY).unwrap();
    pm
}

fn alphas(pm: &Pixmap) -> Vec<u8> {
    pm.pixels().iter().map(|p| p.alpha()).collect()
}

#[test]
fn aligned_square_covers_exact_pixels() {
    let outline = Outline {
        points: square(10.0, 10.0, 20.0, 20.0),
        contour_ends: vec![4],
    };
    let a = alphas(&render(&outline, &BLACK));
    assert_eq!(a.iter().filter(|&&v| v == 255).count(), 100);
    assert_eq!(a.iter().filter(|&&v| v > 0).count(), 100);
}

#[test]
fn half_pixel_square_covers_area_with_aa_edges() {
    let outline = Outline {
        points: square(10.5, 10.5, 20.5, 20.5),
        contour_ends: vec![4],
    };
    let a = alphas(&render(&outline, &BLACK));
    let full = a.iter().filter(|&&v| v == 255).count();
    let touched = a.iter().filter(|&&v| v > 0).count();
    let area: f32 = a.iter().map(|&v| f32::from(v) / 255.0).sum();
    assert!((81..=100).contains(&full), "full {full}");
    assert!((100..=121).contains(&touched), "touched {touched}");
    assert!((area - 100.0).abs() < 2.0, "area {area}");
}

#[test]
fn alpha_is_color_alpha_times_opacity() {
    let outline = Outline {
        points: square(0.0, 0.0, 32.0, 32.0),
        contour_ends: vec![4],
    };
    let style = StrokeStyle {
        color: Color {
            r: 0,
            g: 0,
            b: 0,
            a: 128,
        },
        opacity: 0.5,
        ..BLACK
    };
    let a = render(&outline, &style).pixel(16, 16).unwrap().alpha();
    assert!(a.abs_diff(64) <= 1, "alpha {a}");
}

#[test]
fn contours_fill_with_nonzero_rule() {
    // Outer and inner square wound the same way: nonzero keeps the inner square filled (even-odd would not).
    let mut points = square(4.0, 4.0, 28.0, 28.0);
    points.extend(square(12.0, 12.0, 20.0, 20.0));
    let outline = Outline {
        points,
        contour_ends: vec![4, 8],
    };
    let pm = render(&outline, &BLACK);
    assert_eq!(pm.pixel(16, 16).unwrap().alpha(), 255);
    assert_eq!(pm.pixel(6, 6).unwrap().alpha(), 255);
    assert_eq!(pm.pixel(1, 1).unwrap().alpha(), 0);
}

#[test]
fn degenerate_contours_draw_nothing() {
    let outline = Outline {
        points: vec![
            [1.0, 1.0],
            [30.0, 30.0],
            [5.0, f32::NAN],
            [9.0, 9.0],
            [20.0, 2.0],
        ],
        contour_ends: vec![2, 5],
    };
    let pm = render(&outline, &BLACK);
    assert!(pm.data().iter().all(|&b| b == 0));
}
