//! Scaling a rendered pixmap down so its long side fits a limit (the overview image of `look`, a history
//! snapshot taller than the asset store accepts).

use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

/// `source` scaled so its longest side is at most `max_side` pixels, or `None` when it already fits (or the
/// scaled size is degenerate).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "pixmap sides are far below 2²⁴ and the scaled sides are rounded and at least 1"
)]
pub(crate) fn shrink(source: &Pixmap, max_side: u32) -> Option<Pixmap> {
    let longest = source.width().max(source.height());
    if longest <= max_side || max_side == 0 {
        return None;
    }
    let scale = max_side as f32 / longest as f32;
    let width = ((source.width() as f32 * scale).round() as u32).max(1);
    let height = ((source.height() as f32 * scale).round() as u32).max(1);
    let mut target = Pixmap::new(width, height)?;
    let paint = PixmapPaint {
        quality: FilterQuality::Bicubic,
        ..PixmapPaint::default()
    };
    target.draw_pixmap(
        0,
        0,
        source.as_ref(),
        &paint,
        Transform::from_scale(
            width as f32 / source.width() as f32,
            height as f32 / source.height() as f32,
        ),
        None,
    );
    Some(target)
}
