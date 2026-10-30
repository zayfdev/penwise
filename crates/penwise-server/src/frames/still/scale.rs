//! Downscaled strips (`GET /frames/<id>/strip/<hash>.png?w=<px>`).
//!
//! A phone or a board zoomed far out does not need a 1440 px strip. The requested width snaps up to one of a few
//! buckets so the cache stays small; the scaled PNG is stored beside the original under `<hash>@<bucket>` and,
//! like the strip, never changes.

use axum::body::Bytes;
use tiny_skia::Pixmap;

use super::{Inner, StillError};

/// The widths a strip can be served at.
pub(super) const BUCKETS: [u32; 3] = [360, 720, 1440];

/// The bucket a request for `width` px is served at: the smallest one that is at least as wide, at most the
/// largest.
pub(super) fn bucket(width: u32) -> u32 {
    BUCKETS
        .into_iter()
        .find(|bucket| *bucket >= width)
        .unwrap_or(BUCKETS[BUCKETS.len() - 1])
}

/// The width of a PNG, from its header.
fn png_width(png: &[u8]) -> Option<u32> {
    let bytes = png.get(16..20)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// `strip` (the PNG named `hash`) at the bucket of `width`; the original when it is not wider than the bucket.
///
/// # Errors
/// Only if the scaling task cannot run; a PNG that does not decode is served as it is.
pub(super) async fn variant(
    inner: &Inner,
    hash: &str,
    strip: Bytes,
    width: u32,
) -> Result<Option<Bytes>, StillError> {
    let target = bucket(width);
    if png_width(&strip).is_none_or(|native| native <= target) {
        return Ok(Some(strip));
    }
    let name = format!("{hash}@{target}");
    if let Some(done) = inner.blob(&name).await {
        return Ok(Some(done));
    }
    let source = strip.clone();
    let scaled = tokio::task::spawn_blocking(move || downscale_png(&source, target))
        .await
        .map_err(|error| StillError::Write(format!("scaling task: {error}")))?;
    let Some(scaled) = scaled else {
        tracing::warn!(hash, "a strip would not downscale; serving it as it is");
        return Ok(Some(strip));
    };
    let scaled = Bytes::from(scaled);
    inner.keep_blob(&name, scaled.clone()).await;
    Ok(Some(scaled))
}

fn downscale_png(png: &[u8], width: u32) -> Option<Vec<u8>> {
    let source = Pixmap::decode_png(png).ok()?;
    downscale(&source, width)?.encode_png().ok()
}

/// `source` shrunk to `width` px wide by averaging the source pixels under each target pixel.
pub(super) fn downscale(source: &Pixmap, width: u32) -> Option<Pixmap> {
    let (sw, sh) = (u64::from(source.width()), u64::from(source.height()));
    let ow = u64::from(width.max(1));
    let oh = ((sh * ow + sw / 2) / sw).max(1);
    let mut out = Pixmap::new(u32::try_from(ow).ok()?, u32::try_from(oh).ok()?)?;
    let from = source.data();
    let to = out.data_mut();
    for oy in 0..oh {
        let (y0, y1) = span(oy, oh, sh);
        for ox in 0..ow {
            let (x0, x1) = span(ox, ow, sw);
            let mut sum = [0_u64; 4];
            for y in y0..y1 {
                for x in x0..x1 {
                    let at = usize::try_from((y * sw + x) * 4).ok()?;
                    for (channel, total) in sum.iter_mut().enumerate() {
                        *total += u64::from(*from.get(at + channel)?);
                    }
                }
            }
            let count = (y1 - y0) * (x1 - x0);
            let at = usize::try_from((oy * ow + ox) * 4).ok()?;
            for (channel, total) in sum.iter().enumerate() {
                *to.get_mut(at + channel)? = u8::try_from((total + count / 2) / count).ok()?;
            }
        }
    }
    Some(out)
}

/// The source range `[start, end)` under target pixel `index` of `count` over `source` pixels.
fn span(index: u64, count: u64, source: u64) -> (u64, u64) {
    let start = index * source / count;
    let end = ((index + 1) * source / count).max(start + 1).min(source);
    (start.min(source - 1), end)
}
