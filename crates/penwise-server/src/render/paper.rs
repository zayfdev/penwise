//! Paper under the ink: ruled lines, a square grid or a dot grid at the page's scale.

use penwise_proto::{Background, notebook::BACKGROUND_SPACING_PX};
use tiny_skia::Pixmap;

const RULED_COLOR: (u8, u8, u8) = (0xC9, 0xD6, 0xE8);
const GRID_COLOR: (u8, u8, u8) = (0xDC, 0xE3, 0xEC);
const DOT_COLOR: (u8, u8, u8) = (0xC6, 0xCE, 0xDA);

/// Draws `background` into `pixmap` (already white), `scale` pixels per page px.
#[allow(
    clippy::cast_precision_loss,
    reason = "a pixmap side is a pixel count; the pattern only needs it approximately"
)]
pub(super) fn draw_background(pixmap: &mut Pixmap, background: Background, scale: f32) {
    let spacing = BACKGROUND_SPACING_PX * scale;
    if background == Background::Blank || spacing < 4.0 {
        return;
    }
    let width = pixmap.width() as f32;
    let height = pixmap.height() as f32;
    let paint = |rgb: (u8, u8, u8)| {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(tiny_skia::Color::from_rgba8(rgb.0, rgb.1, rgb.2, 0xFF));
        paint.anti_alias = true;
        paint
    };
    match background {
        Background::Lined => {
            let paint = paint(RULED_COLOR);
            let mut y = spacing;
            while y < height {
                if let Some(line) = tiny_skia::Rect::from_xywh(0.0, y, width, 1.0) {
                    pixmap.fill_rect(line, &paint, tiny_skia::Transform::identity(), None);
                }
                y += spacing;
            }
        }
        Background::Grid => {
            let paint = paint(GRID_COLOR);
            let mut x = spacing;
            while x < width {
                if let Some(line) = tiny_skia::Rect::from_xywh(x, 0.0, 1.0, height) {
                    pixmap.fill_rect(line, &paint, tiny_skia::Transform::identity(), None);
                }
                x += spacing;
            }
            let mut y = spacing;
            while y < height {
                if let Some(line) = tiny_skia::Rect::from_xywh(0.0, y, width, 1.0) {
                    pixmap.fill_rect(line, &paint, tiny_skia::Transform::identity(), None);
                }
                y += spacing;
            }
        }
        Background::Dot => {
            let paint = paint(DOT_COLOR);
            let mut y = spacing;
            while y < height {
                let mut x = spacing;
                while x < width {
                    if let Some(dot) = tiny_skia::Rect::from_xywh(x - 1.5, y - 1.5, 3.0, 3.0) {
                        pixmap.fill_rect(dot, &paint, tiny_skia::Transform::identity(), None);
                    }
                    x += spacing;
                }
                y += spacing;
            }
        }
        Background::Blank => {}
    }
}
