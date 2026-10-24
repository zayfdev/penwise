use super::pixels::{self, PixelRect, WHITE};
use super::{Rotation, SheetOptions, render_brush_sheet};
use crate::raster::RasterError;
use crate::sample::Sample;

const RED: [u8; 4] = [255, 0, 0, 255];

fn px(pm: &tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 4] {
    let c = pm.pixel(x, y).unwrap();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

/// 4×2 white pixmap with the top-left pixel red.
fn marked() -> tiny_skia::Pixmap {
    let mut pm = pixels::filled(4, 2, WHITE).unwrap();
    let one = PixelRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    pixels::fill(&mut pm, one, RED);
    pm
}

/// A horizontal 120 px stroke.
fn horizontal() -> Vec<Vec<Sample>> {
    let stroke = (0..60u16)
        .map(|i| {
            Sample {
                x: 100.0 + 2.0 * f32::from(i),
                y: 50.0,
                pressure: 0.6,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: u32::from(i) * 4000,
            }
            .quantize()
        })
        .collect();
    vec![stroke]
}

#[test]
fn quarter_turns_move_the_corner_and_swap_sides() {
    let cw = pixels::rotate(&marked(), Rotation::Cw).unwrap();
    assert_eq!((cw.width(), cw.height()), (2, 4));
    assert_eq!(px(&cw, 1, 0), RED, "top-left goes top-right");
    let ccw = pixels::rotate(&marked(), Rotation::Ccw).unwrap();
    assert_eq!(px(&ccw, 0, 3), RED, "top-left goes bottom-left");
    let same = pixels::rotate(&marked(), Rotation::None).unwrap();
    assert_eq!(px(&same, 0, 0), RED);
}

#[test]
fn content_rect_pads_and_clips() {
    let pm = marked();
    let r = pixels::content_rect(&pm, WHITE, 1).unwrap();
    assert_eq!((r.x, r.y, r.width, r.height), (0, 0, 2, 2));
    assert!(pixels::content_rect(&pixels::filled(3, 3, WHITE).unwrap(), WHITE, 1).is_none());
    let cropped = pixels::crop(&pm, r).unwrap();
    assert_eq!(px(&cropped, 0, 0), RED);
}

#[test]
fn sheet_needs_samples() {
    let err = render_brush_sheet(&[], &SheetOptions::DEFAULT).unwrap_err();
    assert_eq!(
        err,
        RasterError::InvalidSize {
            width: 0,
            height: 0
        }
    );
}

#[test]
fn sheet_lays_out_one_cell_per_preset_cropped_to_ink() {
    let one_col = render_brush_sheet(&horizontal(), &SheetOptions::DEFAULT).unwrap();
    // Uncropped pages are 120 px tall (zero-height stroke + 2 × 60 px margin); cropping to ink shrinks rows.
    assert!(one_col.height() < 13 * 120);
    let grid = SheetOptions {
        columns: 13,
        ..SheetOptions::DEFAULT
    };
    let wide = render_brush_sheet(&horizontal(), &grid).unwrap();
    assert!(wide.width() > one_col.width(), "13 cells side by side");
    assert!(wide.height() < one_col.height(), "a single row");
}

#[test]
fn rotation_turns_the_cells() {
    let opts = SheetOptions {
        columns: 13,
        rotation: Rotation::Cw,
        ..SheetOptions::DEFAULT
    };
    let turned = render_brush_sheet(&horizontal(), &opts).unwrap();
    let plain = SheetOptions {
        columns: 13,
        ..SheetOptions::DEFAULT
    };
    let flat = render_brush_sheet(&horizontal(), &plain).unwrap();
    assert!(
        turned.height() > flat.height(),
        "vertical strokes make taller cells"
    );
}
