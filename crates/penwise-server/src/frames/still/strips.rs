//! Cutting a tall document into strips a browser can screenshot.
//!
//! Chromium captures a region of the document (`captureBeyondViewport`) without resizing the page, so `100vh`
//! stays the preset height. A single capture is kept to [`STRIP_HEIGHT`] CSS px and [`STRIP_AREA`] pixels:
//! well under the texture limit, and a strip is the unit clients fetch, cache and draw.

use axum::body::Bytes;

use crate::inspect::{Clip, Error, Inspect, Page};

/// The tallest strip, in CSS px (device scale 1, so also in image px).
pub(crate) const STRIP_HEIGHT: u32 = 4096;
/// The most pixels in one strip (contract §4): a wide frame gets shorter strips instead of a huge bitmap.
pub(crate) const STRIP_AREA: u32 = 6_000_000;

/// One horizontal slice of the document.
#[derive(Debug, Clone)]
pub(crate) struct Strip {
    /// Document y of the strip's top edge.
    pub(crate) y: u32,
    /// The strip's height in CSS px.
    pub(crate) h: u32,
    /// The slice as PNG, `width × h` pixels, with no ink.
    pub(crate) png: Bytes,
}

/// The tallest strip for a document `width` px wide: `min(4096, 6_000_000 / width)`, at least 1.
pub(crate) fn strip_height(width: u32) -> u32 {
    (STRIP_AREA / width.max(1)).clamp(1, STRIP_HEIGHT)
}

/// The `(y, h)` of every strip covering `0..height` of a document `width` px wide.
pub(crate) fn spans(width: u32, height: u32) -> Vec<(u32, u32)> {
    let tall = strip_height(width);
    let mut spans = Vec::new();
    let mut y = 0;
    while y < height {
        let h = (height - y).min(tall);
        spans.push((y, h));
        y += h;
    }
    spans
}

/// Screenshots `0..height` of the page at `width` wide, strip by strip. The page stays at scroll 0.
///
/// # Errors
/// A screenshot failed.
pub(crate) async fn capture(
    inspect: &Inspect,
    page: &Page,
    width: u32,
    height: u32,
) -> Result<Vec<Strip>, Error> {
    let mut strips = Vec::new();
    for (y, h) in spans(width, height.max(1)) {
        let clip = Clip::new(0.0, f64::from(y), f64::from(width), f64::from(h));
        let png = inspect.snapshot(page, Some(clip)).await?;
        strips.push(Strip {
            y,
            h,
            png: Bytes::from(png),
        });
    }
    Ok(strips)
}
