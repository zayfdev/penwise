//! Compact on-disk form of one stroke.
//!
//! A page used to live in one JSON file whose strokes were arrays of raw floats: 11 bytes per float and the
//! whole page rewritten on every append. This carries the same numbers losslessly in a versioned blob, one
//! row per stroke, so an append touches one row. Quantizing (16-bit positions, 8-bit pressure) was
//! considered and rejected for now: it buys ~2x more and costs exactness for a page that is a few MB at most.

use super::model::StrokeMsg;

/// Marks the bytes as a `stroke_v1` blob.
const VERSION: u8 = 1;
/// `version + preset + argb + scale + seed + count`.
const HEADER_BYTES: usize = 1 + 4 + 4 + 4 + 8 + 4;

/// Encodes a stroke to its compact blob.
#[allow(
    clippy::cast_possible_truncation,
    reason = "sample count is bounded by the tablet's per-stroke limit"
)]
pub(crate) fn encode(stroke: &StrokeMsg) -> Vec<u8> {
    let count = stroke.times.len();
    let mut out = Vec::with_capacity(HEADER_BYTES + count * 24);
    out.push(VERSION);
    out.extend_from_slice(&stroke.preset.to_le_bytes());
    out.extend_from_slice(&stroke.argb.to_le_bytes());
    out.extend_from_slice(&stroke.scale.to_le_bytes());
    out.extend_from_slice(&stroke.seed.parse::<u64>().unwrap_or(0).to_le_bytes());
    out.extend_from_slice(&(count as u32).to_le_bytes());
    for index in 0..count {
        let base = index * 5;
        out.extend_from_slice(&stroke.samples[base].to_le_bytes());
        out.extend_from_slice(&stroke.samples[base + 1].to_le_bytes());
    }
    for index in 0..count {
        let base = index * 5;
        out.extend_from_slice(&stroke.samples[base + 2].to_le_bytes());
        out.extend_from_slice(&stroke.samples[base + 3].to_le_bytes());
        out.extend_from_slice(&stroke.samples[base + 4].to_le_bytes());
    }
    for time in &stroke.times {
        out.extend_from_slice(&time.to_le_bytes());
    }
    out
}

/// Decodes a `stroke_v1` blob; `None` when it is not one or is truncated.
pub(crate) fn decode(bytes: &[u8], layer: &str) -> Option<StrokeMsg> {
    if bytes.len() < HEADER_BYTES || bytes[0] != VERSION {
        return None;
    }
    let preset = u32::from_le_bytes(bytes[1..5].try_into().ok()?);
    let argb = i32::from_le_bytes(bytes[5..9].try_into().ok()?);
    let scale = f32::from_le_bytes(bytes[9..13].try_into().ok()?);
    let seed = u64::from_le_bytes(bytes[13..21].try_into().ok()?);
    let count = u32::from_le_bytes(bytes[21..25].try_into().ok()?) as usize;
    if bytes.len() != HEADER_BYTES + count * 24 {
        return None;
    }
    let mut samples = Vec::with_capacity(count * 5);
    samples.resize(count * 5, 0.0);
    let mut times = Vec::with_capacity(count);
    let mut at = HEADER_BYTES;
    for index in 0..count {
        samples[index * 5] = f32::from_le_bytes(bytes[at..at + 4].try_into().ok()?);
        samples[index * 5 + 1] = f32::from_le_bytes(bytes[at + 4..at + 8].try_into().ok()?);
        at += 8;
    }
    for index in 0..count {
        samples[index * 5 + 2] = f32::from_le_bytes(bytes[at..at + 4].try_into().ok()?);
        samples[index * 5 + 3] = f32::from_le_bytes(bytes[at + 4..at + 8].try_into().ok()?);
        samples[index * 5 + 4] = f32::from_le_bytes(bytes[at + 8..at + 12].try_into().ok()?);
        at += 12;
    }
    for _ in 0..count {
        times.push(i32::from_le_bytes(bytes[at..at + 4].try_into().ok()?));
        at += 4;
    }
    Some(StrokeMsg {
        layer: layer.to_owned(),
        preset,
        argb,
        scale,
        seed: seed.to_string(),
        samples,
        times,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke() -> StrokeMsg {
        StrokeMsg {
            layer: "ink".to_owned(),
            preset: 3,
            argb: -16_777_216,
            scale: 1.25,
            seed: "18446744073709551615".to_owned(),
            samples: vec![1.5, 2.25, 0.5, 0.1, -3.0, 4.0, 5.0, 0.9, 0.2, 3.0],
            times: vec![0, 8_333],
        }
    }

    #[test]
    fn a_stroke_round_trips_exactly() {
        let original = stroke();
        let decoded = decode(&encode(&original), "ink").expect("decodes");
        assert_eq!(decoded, original);
    }

    #[test]
    fn the_blob_is_smaller_than_the_json_it_replaces() {
        let original = stroke();
        let json = serde_json::to_vec(&original).expect("serializes");
        let blob = encode(&original);
        assert!(
            blob.len() < json.len(),
            "blob {} >= json {}",
            blob.len(),
            json.len()
        );
    }

    #[test]
    fn a_truncated_blob_is_rejected() {
        let blob = encode(&stroke());
        assert!(decode(&blob[..blob.len() - 1], "ink").is_none());
        assert!(decode(&[], "ink").is_none());
    }

    #[test]
    fn an_empty_stroke_round_trips() {
        let original = StrokeMsg {
            samples: Vec::new(),
            times: Vec::new(),
            ..stroke()
        };
        let decoded = decode(&encode(&original), "ink").expect("decodes");
        assert_eq!(decoded, original);
    }
}
