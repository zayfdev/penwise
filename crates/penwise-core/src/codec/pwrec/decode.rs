//! `.pwrec` v1 body decoder (header fields after magic/version, strokes); see the parent module layout.
//!
//! Owner: slice d.

use super::{
    CHANNEL_COUNT, Channels, RecordedStroke, Recording, RecordingHeader, RecordingSource, RunKind,
    T_SLOT, frame_from_values, has_channel, invalid,
};
use crate::codec::error::CodecError;
use crate::codec::varint::Reader;
use crate::sample::{AxisRange, PxScale};

/// Everything between magic/version and the trailer.
pub(super) fn body(r: &mut Reader<'_>) -> Result<Recording, CodecError> {
    let header = read_header(r)?;
    let count = r.uv32()?;
    // Every stroke takes at least 3 bytes (kind, t0, n); bound pre-allocation by the input.
    let hint = usize::try_from(count).unwrap_or(usize::MAX);
    let mut strokes = Vec::with_capacity(hint.min(r.remaining() / 3));
    for _ in 0..count {
        strokes.push(read_run(header.channels, r)?);
    }
    Ok(Recording { header, strokes })
}

fn read_header(r: &mut Reader<'_>) -> Result<RecordingHeader, CodecError> {
    let channels = Channels(r.u16_le()?);
    let known = (1_u16 << CHANNEL_COUNT) - 1;
    if channels.0 & !known != 0 || channels.0 == 0 {
        return Err(invalid("channels"));
    }
    let source = match r.u8()? {
        0 => RecordingSource::Getevent,
        1 => RecordingSource::MotionEvent,
        _ => return Err(invalid("source")),
    };
    if r.u8()? != 0 {
        return Err(invalid("reserved"));
    }
    let len = usize::try_from(r.uv32()?).map_err(|_| invalid("device"))?;
    let device = core::str::from_utf8(r.bytes(len)?)
        .map_err(|_| invalid("device"))?
        .to_owned();
    let mut ranges = [AxisRange { min: 0, max: 0 }; CHANNEL_COUNT];
    for (slot, range) in ranges.iter_mut().enumerate() {
        if has_channel(channels, slot) {
            range.min = r.sv32()?;
            range.max = r.sv32()?;
        }
    }
    let x_scale = read_scale(r)?;
    let y_scale = read_scale(r)?;
    Ok(RecordingHeader {
        channels,
        source,
        device,
        ranges,
        x_scale,
        y_scale,
        rate_hint: r.uv32()?,
    })
}

fn read_scale(r: &mut Reader<'_>) -> Result<PxScale, CodecError> {
    let num = r.uv32()?;
    let den = r.uv32()?;
    if den == 0 {
        return Err(invalid("px_den"));
    }
    Ok(PxScale { num, den })
}

fn read_run(channels: Channels, r: &mut Reader<'_>) -> Result<RecordedStroke, CodecError> {
    let kind = match r.u8()? {
        0 => RunKind::Contact,
        1 => RunKind::Hover,
        _ => return Err(invalid("kind")),
    };
    let t0_us = r.uv64()?;
    let n = r.uv32()?;
    if n == 0 {
        return Err(invalid("sample count"));
    }
    // Every frame takes at least one byte per present channel (the mask is non-empty).
    let hint = usize::try_from(n).unwrap_or(usize::MAX);
    let mut frames = Vec::with_capacity(hint.min(r.remaining()));
    let mut prev = [0_i32; CHANNEL_COUNT];
    for i in 0..n {
        let mut cur = prev;
        for (slot, value) in cur.iter_mut().enumerate() {
            if has_channel(channels, slot) {
                *value = value.checked_add(r.sv32()?).ok_or(CodecError::OutOfRange {
                    field: "frame delta",
                })?;
            }
        }
        let t = cur[T_SLOT];
        if (i == 0 && t != 0) || t < 0 {
            return Err(invalid("t_us"));
        }
        frames.push(frame_from_values(cur));
        prev = cur;
    }
    Ok(RecordedStroke {
        kind,
        t0_us,
        frames,
    })
}
