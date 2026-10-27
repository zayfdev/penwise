//! Paper under the ink: ruled lines, a square grid or a dot grid at the page's scale.

use penwise_proto::{Background, notebook::BACKGROUND_SPACING_PX};
use tiny_skia::Pixmap;

const RULED_COLOR: (u8, u8, u8) = (0xC9, 0xD6, 0xE8);
const GRID_COLOR: (u8, u8, u8) = (0xDC, 0xE3, 0xEC);
const DOT_COLOR: (u8, u8, u8) = (0xC6, 0xCE, 0xDA);

/// Draws `background` into `pixmap` (already white), `scale` pixels per page px, `origin` the page position of the
/// pixmap's top-left corner.
///
/// The pattern is anchored to the page, not to the picture: rules, grid lines and dots sit at the positive
/// multiples of [`BACKGROUND_SPACING_PX`] in page coordinates, as on the tablet and in the browser. A render of
/// an infinite page starts wherever its content does, so counting from the pixmap edge would shift every line.
#[allow(
    clippy::cast_precision_loss,
    reason = "a pixmap side is a pixel count; the pattern only needs it approximately"
)]
pub(super) fn draw_background(
    pixmap: &mut Pixmap,
    background: Background,
    origin: [f32; 2],
    scale: f32,
) {
    if background == Background::Blank || BACKGROUND_SPACING_PX * scale < 4.0 {
        return;
    }
    let (width, height) = (pixmap.width() as f32, pixmap.height() as f32);
    let ys = rulings(origin[1], height / scale, scale);
    let xs = rulings(origin[0], width / scale, scale);
    let paint = |rgb: (u8, u8, u8)| {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(tiny_skia::Color::from_rgba8(rgb.0, rgb.1, rgb.2, 0xFF));
        paint.anti_alias = true;
        paint
    };
    let mut fill = |x: f32, y: f32, w: f32, h: f32, paint: &tiny_skia::Paint| {
        if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) {
            pixmap.fill_rect(rect, paint, tiny_skia::Transform::identity(), None);
        }
    };
    match background {
        Background::Lined => {
            let paint = paint(RULED_COLOR);
            for &y in &ys {
                fill(0.0, y, width, 1.0, &paint);
            }
        }
        Background::Grid => {
            let paint = paint(GRID_COLOR);
            for &y in &ys {
                fill(0.0, y, width, 1.0, &paint);
            }
            for &x in &xs {
                fill(x, 0.0, 1.0, height, &paint);
            }
        }
        Background::Dot => {
            let paint = paint(DOT_COLOR);
            for &y in &ys {
                for &x in &xs {
                    fill(x - 1.5, y - 1.5, 3.0, 3.0, &paint);
                }
            }
        }
        Background::Blank => {}
    }
}

/// Pixel positions of the page's rulings along one axis: the positive multiples of the spacing that fall strictly
/// inside the `extent` page px starting at page position `origin`.
fn rulings(origin: f32, extent: f32, scale: f32) -> Vec<f32> {
    let mut out = Vec::new();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "a ruling index is a small positive count"
    )]
    let mut k = ((origin / BACKGROUND_SPACING_PX).floor() as i64 + 1).max(1);
    loop {
        #[allow(
            clippy::cast_precision_loss,
            reason = "a ruling index is a small positive count"
        )]
        let page = k as f32 * BACKGROUND_SPACING_PX;
        if page >= origin + extent {
            return out;
        }
        out.push((page - origin) * scale);
        k += 1;
    }
}
