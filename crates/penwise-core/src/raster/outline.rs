//! Outline polygons → pixels.
//!
//! Owner: slice e.

use super::{RasterError, Viewport, stroke_alpha};
use crate::brush::BlendMode;
use crate::geometry::{Outline, StrokeStyle};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

/// Fills `outline` (every contour as a closed subpath of one path, `FillRule::Winding`, anti-aliased) with
/// `style.color` at alpha `color.a/255 · style.opacity`, blend `SourceOver` for `Normal` and `Multiply` for
/// `Multiply`. Points are mapped through `viewport` before path building (lines only, no curves). Outlines with
/// fewer than 3 points in a contour skip that contour; an outline that yields no path draws nothing (`Ok`).
///
/// # Errors
/// [`RasterError::InvalidViewport`] for a non-finite or non-positive scale.
pub fn fill_outline(
    pixmap: &mut Pixmap,
    outline: &Outline,
    style: &StrokeStyle,
    viewport: &Viewport,
) -> Result<(), RasterError> {
    viewport.check()?;
    let alpha = stroke_alpha(style);
    if alpha <= 0.0 {
        return Ok(());
    }
    let Some(path) = build_path(outline, viewport) else {
        return Ok(());
    };
    let c = style.color;
    let Some(color) = Color::from_rgba(
        f32::from(c.r) / 255.0,
        f32::from(c.g) / 255.0,
        f32::from(c.b) / 255.0,
        alpha,
    ) else {
        return Ok(());
    };
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    paint.blend_mode = match style.blend {
        BlendMode::Normal => tiny_skia::BlendMode::SourceOver,
        BlendMode::Multiply => tiny_skia::BlendMode::Multiply,
    };
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    Ok(())
}

/// One closed subpath per contour with ≥ 3 finite points; `None` when nothing remains. Malformed contour ends
/// (non-increasing or past the point count) are clamped; no contour ends at all means one contour.
fn build_path(outline: &Outline, viewport: &Viewport) -> Option<tiny_skia::Path> {
    let points = &outline.points;
    let mut builder = PathBuilder::new();
    let mut start = 0usize;
    let whole = [u32::try_from(points.len()).unwrap_or(u32::MAX)];
    let ends: &[u32] = if outline.contour_ends.is_empty() {
        &whole
    } else {
        &outline.contour_ends
    };
    for &end in ends {
        let end = usize::try_from(end).map_or(points.len(), |e| e.min(points.len()));
        if end <= start {
            continue;
        }
        add_contour(&mut builder, &points[start..end], viewport);
        start = end;
    }
    builder.finish()
}

fn add_contour(builder: &mut PathBuilder, contour: &[[f32; 2]], viewport: &Viewport) {
    if contour.len() < 3
        || contour
            .iter()
            .any(|p| !p[0].is_finite() || !p[1].is_finite())
    {
        return;
    }
    for (i, p) in contour.iter().enumerate() {
        let [x, y] = viewport.map(p[0], p[1]);
        if i == 0 {
            builder.move_to(x, y);
        } else {
            builder.line_to(x, y);
        }
    }
    builder.close();
}

#[cfg(test)]
mod tests;
