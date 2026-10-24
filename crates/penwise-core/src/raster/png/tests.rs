use super::{PixelDiff, PixelTolerance, compare_pixels, decode_png, diff_image, encode_png};
use crate::raster::RasterError;
use tiny_skia::{Color, Pixmap};

fn filled(w: u32, h: u32, color: Color) -> Pixmap {
    let mut pm = Pixmap::new(w, h).unwrap();
    pm.fill(color);
    pm
}

fn set_red(pm: &mut Pixmap, i: usize, r: u8) {
    pm.data_mut()[i * 4] = r;
}

#[test]
fn png_round_trip_preserves_pixels() {
    let mut pm = filled(7, 5, Color::from_rgba8(10, 200, 30, 255));
    set_red(&mut pm, 3, 99);
    let decoded = decode_png(&encode_png(&pm).unwrap()).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (7, 5));
    let diff = compare_pixels(&pm, &decoded).unwrap();
    assert_eq!(
        diff,
        PixelDiff {
            max_delta: 0,
            differing: 0,
            total: 35
        }
    );
}

#[test]
fn garbage_bytes_fail_to_decode() {
    assert_eq!(decode_png(b"not a png").err(), Some(RasterError::PngDecode));
}

#[test]
fn compare_counts_differing_pixels() {
    let a = filled(10, 10, Color::WHITE);
    let mut b = a.clone();
    set_red(&mut b, 0, 250);
    set_red(&mut b, 1, 254);
    let diff = compare_pixels(&a, &b).unwrap();
    assert_eq!(
        diff,
        PixelDiff {
            max_delta: 5,
            differing: 2,
            total: 100
        }
    );
}

#[test]
fn size_mismatch_is_an_error() {
    let a = filled(2, 3, Color::WHITE);
    let b = filled(3, 2, Color::WHITE);
    let want = RasterError::SizeMismatch {
        a_width: 2,
        a_height: 3,
        b_width: 3,
        b_height: 2,
    };
    assert_eq!(compare_pixels(&a, &b), Err(want.clone()));
    assert_eq!(diff_image(&a, &b, PixelTolerance::GOLDEN).err(), Some(want));
}

#[test]
fn tolerance_rules() {
    let tol = PixelTolerance::GOLDEN;
    let pass = |max_delta, differing, total| {
        PixelDiff {
            max_delta,
            differing,
            total,
        }
        .passes(tol)
    };
    assert!(pass(0, 0, 0));
    assert!(pass(2, 9, 10_000));
    assert!(!pass(2, 10, 10_000), "0.1 % is not < 0.1 %");
    assert!(!pass(3, 1, 10_000));
    assert!(pass(255, 0, 0), "empty images pass");
}

#[test]
fn diff_image_marks_pixels() {
    let a = filled(3, 1, Color::WHITE);
    let mut b = a.clone();
    set_red(&mut b, 1, 254);
    set_red(&mut b, 2, 100);
    let d = diff_image(&a, &b, PixelTolerance::GOLDEN).unwrap();
    let px: Vec<[u8; 4]> = d
        .data()
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
    assert_eq!(
        px,
        vec![[255, 255, 255, 255], [215, 215, 215, 255], [255, 0, 0, 255]]
    );
}
