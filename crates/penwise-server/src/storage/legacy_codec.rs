//! Decoder of the v3 `stroke_v1` blob (moved from `library/stroke_codec.rs`; the encoder is gone with the v3
//! store). Layout: `[1] preset u32 | argb i32 | scale f32 | seed u64 | count u32`, then `count` × (x, y) f32,
//! then `count` × (pressure, tilt, azimuth) f32, then `count` × time i32, all little-endian.
//!
//! The blob's own seed bytes are ignored: the row id is the stroke's identity (see `penwise_proto::legacy`).

use penwise_proto::legacy::LegacyStroke;

const VERSION: u8 = 1;
const HEADER_BYTES: usize = 1 + 4 + 4 + 4 + 8 + 4;

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let end = self.at.checked_add(N)?;
        let out = self.bytes.get(self.at..end)?.try_into().ok()?;
        self.at = end;
        Some(out)
    }

    fn f32(&mut self) -> Option<f32> {
        self.take().map(f32::from_le_bytes)
    }
}

/// Decodes a `stroke_v1` blob; `None` when it is not one, is truncated or its length disagrees with `count`.
#[must_use]
pub fn decode_stroke(bytes: &[u8], layer: &str, seed: &str) -> Option<LegacyStroke> {
    if bytes.len() < HEADER_BYTES || *bytes.first()? != VERSION {
        return None;
    }
    let mut cur = Cursor { bytes, at: 1 };
    let preset = u32::from_le_bytes(cur.take()?);
    let argb = i32::from_le_bytes(cur.take()?);
    let scale = cur.f32()?;
    let _blob_seed: [u8; 8] = cur.take()?;
    let count = usize::try_from(u32::from_le_bytes(cur.take()?)).ok()?;
    if bytes.len() != HEADER_BYTES.checked_add(count.checked_mul(24)?)? {
        return None;
    }
    let mut samples = vec![0.0_f32; count * 5];
    for index in 0..count {
        samples[index * 5] = cur.f32()?;
        samples[index * 5 + 1] = cur.f32()?;
    }
    for index in 0..count {
        samples[index * 5 + 2] = cur.f32()?;
        samples[index * 5 + 3] = cur.f32()?;
        samples[index * 5 + 4] = cur.f32()?;
    }
    let mut times = Vec::with_capacity(count);
    for _ in 0..count {
        times.push(i32::from_le_bytes(cur.take()?));
    }
    Some(LegacyStroke {
        layer: layer.to_owned(),
        preset,
        argb,
        scale,
        seed: seed.to_owned(),
        samples,
        times,
    })
}
