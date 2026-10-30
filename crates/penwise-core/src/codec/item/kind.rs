//! Kind-specific part of the item body (layout in [`super`]).
//!
//! Owner: slice e (M2).

use super::fields::{
    read_count, read_flag, read_id, read_opt_color, read_paint, read_pair, read_polygons,
    read_shape_kind, read_str, write_flag, write_id, write_len, write_opt_color, write_paint,
    write_polygons, write_shape_kind, write_str,
};
use super::frame::{decode_frame, encode_frame};
use super::{decode_points, encode_points};
use crate::codec::error::CodecError;
use crate::codec::params_fields::{invalid, write_f32s};
use crate::codec::stroke::{decode_params, decode_samples, encode_params, encode_samples};
use crate::codec::varint::{Reader, write_uv32};
use crate::item::{
    EraseItem, FillItem, FramePreset, ImageItem, ItemKind, ItemKindTag, SampleParam, SampleRange,
    ShapeItem, StrokeItem, TextItem, Trim,
};
use crate::marks::{Binding, BindingMode, MarkItem, MarkStatus};
use crate::sample::{GridSample, Sample};
use crate::smoothing::{HISTORY_LEN, HistoryPoint, PositionState, PressureState, SmootherState};

/// Range flag bits.
const START_CUT: u8 = 1;
const END_CUT: u8 = 2;

/// Appends the kind data of `kind`, written at item layout `version` (`v1` omits the stroke binding flag).
pub(super) fn encode_kind(kind: &ItemKind, version: u16, out: &mut Vec<u8>) {
    match kind {
        ItemKind::Stroke(stroke) => encode_stroke(stroke, version, out),
        ItemKind::Erase(erase) => {
            write_id(out, erase.target);
            write_f32s(out, &[erase.radius]);
            encode_points(&erase.path, out);
        }
        ItemKind::Shape(shape) => {
            write_shape_kind(out, &shape.kind);
            write_f32s(out, &shape.size);
            encode_params(&shape.brush, out);
            write_opt_color(out, shape.fill);
        }
        ItemKind::Fill(fill) => {
            write_paint(out, fill.paint);
            write_polygons(out, &fill.polygons);
        }
        ItemKind::Text(text) => {
            write_str(out, &text.content);
            write_f32s(out, &text.size);
            write_f32s(out, &[text.font_size]);
            write_paint(out, text.paint);
            let n = write_len(out, text.sources.len());
            for id in text.sources.iter().take(n) {
                write_id(out, *id);
            }
        }
        ItemKind::Image(image) => {
            out.extend_from_slice(&image.asset);
            write_f32s(out, &[image.w, image.h]);
        }
        ItemKind::Frame(frame) => encode_frame(frame, version, out),
        ItemKind::Mark(mark) => {
            write_id(out, mark.frame);
            out.push(mark.preset.code());
            out.push(mark.status.code());
            write_str(out, &mark.state);
            let n = write_len(out, mark.strokes.len());
            for id in mark.strokes.iter().take(n) {
                write_id(out, *id);
            }
        }
    }
}

/// Reads the kind data for `tag` (written at item layout `version`).
pub(super) fn decode_kind(
    r: &mut Reader<'_>,
    tag: ItemKindTag,
    version: u16,
) -> Result<ItemKind, CodecError> {
    Ok(match tag {
        ItemKindTag::Stroke => ItemKind::Stroke(decode_stroke(r, version)?),
        ItemKindTag::Erase => {
            let target = read_id(r)?;
            let radius = r.f32_le("erase.radius")?;
            let mut path = Vec::new();
            decode_points(r, &mut path)?;
            ItemKind::Erase(EraseItem {
                target,
                path,
                radius,
            })
        }
        ItemKindTag::Shape => ItemKind::Shape(ShapeItem {
            kind: read_shape_kind(r)?,
            size: read_pair(r, "shape.size")?,
            brush: decode_params(r)?,
            fill: read_opt_color(r)?,
        }),
        ItemKindTag::Fill => {
            let paint = read_paint(r)?;
            let polygons = read_polygons(r)?;
            ItemKind::Fill(FillItem { polygons, paint })
        }
        ItemKindTag::Text => ItemKind::Text(decode_text(r)?),
        ItemKindTag::Image => {
            let mut asset = [0_u8; 32];
            for byte in &mut asset {
                *byte = r.u8()?;
            }
            let [w, h] = read_pair(r, "image.size")?;
            ItemKind::Image(ImageItem { asset, w, h })
        }
        ItemKindTag::Frame => ItemKind::Frame(decode_frame(r, version)?),
        ItemKindTag::Mark => {
            let frame = read_id(r)?;
            let preset = FramePreset::from_code(r.u8()?).ok_or(invalid("mark.preset"))?;
            let status = MarkStatus::from_code(r.u8()?).ok_or(invalid("mark.status"))?;
            let state = read_str(r)?;
            let (count, cap) = read_count(r, 16)?;
            let mut strokes = Vec::with_capacity(cap);
            for _ in 0..count {
                strokes.push(read_id(r)?);
            }
            ItemKind::Mark(MarkItem {
                frame,
                preset,
                status,
                state,
                strokes,
            })
        }
    })
}

fn encode_stroke(stroke: &StrokeItem, version: u16, out: &mut Vec<u8>) {
    encode_params(&stroke.params, out);
    out.extend_from_slice(&stroke.seed.to_le_bytes());
    let grid: Vec<GridSample> = stroke
        .samples
        .iter()
        .copied()
        .map(Sample::to_grid)
        .collect();
    encode_samples(&grid, out);
    write_flag(out, stroke.trim.is_some());
    if let Some(trim) = &stroke.trim {
        write_id(out, trim.origin);
        write_uv32(out, trim.sample_offset);
        write_uv32(out, trim.own);
        let n = write_len(out, trim.visible.len());
        for range in trim.visible.iter().take(n) {
            write_uv32(out, range.start.0);
            write_uv32(out, range.end.0);
            let start = if range.start_cut { START_CUT } else { 0 };
            let end = if range.end_cut { END_CUT } else { 0 };
            out.push(start | end);
        }
        write_flag(out, trim.warm.is_some());
        if let Some(warm) = &trim.warm {
            encode_warm(warm, out);
        }
    }
    // v1 bodies predate bindings, so only v2 writes the flag (a bound stroke cannot be a v1 `Add`).
    if version >= 2 {
        write_flag(out, stroke.binding.is_some());
        if let Some(binding) = &stroke.binding {
            encode_binding(binding, out);
        }
    }
}

/// `frame id; mode u8; preset u8; key flag [+ len uv + UTF-8]; state (len uv + UTF-8); anchor f32×2`.
fn encode_binding(binding: &Binding, out: &mut Vec<u8>) {
    write_id(out, binding.frame);
    out.push(binding.mode.code());
    out.push(binding.preset.code());
    match &binding.key {
        Some(key) => {
            write_flag(out, true);
            write_str(out, key);
        }
        None => write_flag(out, false),
    }
    write_str(out, &binding.state);
    write_f32s(out, &binding.anchor);
}

fn decode_binding(r: &mut Reader<'_>) -> Result<Binding, CodecError> {
    let frame = read_id(r)?;
    let mode = BindingMode::from_code(r.u8()?).ok_or(invalid("binding.mode"))?;
    let preset = FramePreset::from_code(r.u8()?).ok_or(invalid("binding.preset"))?;
    let key = if read_flag(r, "binding.key flag")? {
        Some(read_str(r)?)
    } else {
        None
    };
    let state = read_str(r)?;
    let [x, y] = read_f32s(r, "binding.anchor")?;
    let binding = Binding {
        frame,
        mode,
        preset,
        key,
        state,
        anchor: [x, y],
    };
    binding.check().map_err(|_| invalid("binding"))?;
    Ok(binding)
}

fn encode_warm(warm: &SmootherState, out: &mut Vec<u8>) {
    let (p, q) = (&warm.position, &warm.pressure);
    write_f32s(out, &[p.x, p.y, p.dx, p.dy, p.dt_s]);
    write_uv32(out, p.t_us);
    write_f32s(out, &[q.value, q.derivative, q.dt_s]);
    write_uv32(out, q.t_us);
    let n = write_len(out, warm.history.len());
    for h in warm.history.iter().take(n) {
        write_f32s(out, &[h.x, h.y]);
        write_uv32(out, h.t_us);
    }
}

fn decode_warm(r: &mut Reader<'_>) -> Result<SmootherState, CodecError> {
    let [x, y, dx, dy, dt_s] = read_f32s(r, "trim.warm.position")?;
    let t_us = r.uv32()?;
    let position = PositionState {
        x,
        y,
        dx,
        dy,
        t_us,
        dt_s,
    };
    let [value, derivative, dt_s] = read_f32s(r, "trim.warm.pressure")?;
    let pressure = PressureState {
        value,
        derivative,
        t_us: r.uv32()?,
        dt_s,
    };
    // x, y f32 (8 bytes) + t uv (≥ 1): at least 9 bytes per entry.
    let (count, cap) = read_count(r, 9)?;
    if usize::try_from(count).map_or(true, |c| c > HISTORY_LEN) {
        return Err(invalid("trim.warm.history"));
    }
    let mut history = Vec::with_capacity(cap);
    for _ in 0..count {
        let [x, y] = read_f32s(r, "trim.warm.history")?;
        history.push(HistoryPoint {
            x,
            y,
            t_us: r.uv32()?,
        });
    }
    Ok(SmootherState {
        position,
        pressure,
        history,
    })
}

fn read_f32s<const N: usize>(
    r: &mut Reader<'_>,
    field: &'static str,
) -> Result<[f32; N], CodecError> {
    let mut out = [0.0_f32; N];
    for v in &mut out {
        *v = r.f32_le(field)?;
    }
    Ok(out)
}

fn decode_stroke(r: &mut Reader<'_>, version: u16) -> Result<StrokeItem, CodecError> {
    let params = decode_params(r)?;
    let seed = r.u64_le()?;
    let mut samples: Vec<Sample> = Vec::new();
    decode_samples(r, &mut samples)?;
    let trim = if read_flag(r, "trim flag")? {
        Some(decode_trim(r)?)
    } else {
        None
    };
    let binding = if version >= 2 && read_flag(r, "binding flag")? {
        Some(decode_binding(r)?)
    } else {
        None
    };
    Ok(StrokeItem {
        params,
        seed,
        samples,
        trim,
        binding,
    })
}

fn decode_trim(r: &mut Reader<'_>) -> Result<Trim, CodecError> {
    let origin = read_id(r)?;
    let sample_offset = r.uv32()?;
    let own = r.uv32()?;
    // start uv + end uv + flags: at least 3 bytes per range.
    let (count, cap) = read_count(r, 3)?;
    let mut visible = Vec::with_capacity(cap);
    for _ in 0..count {
        let start = SampleParam(r.uv32()?);
        let end = SampleParam(r.uv32()?);
        let flags = r.u8()?;
        if flags & !(START_CUT | END_CUT) != 0 {
            return Err(invalid("trim.flags"));
        }
        visible.push(SampleRange {
            start,
            end,
            start_cut: flags & START_CUT != 0,
            end_cut: flags & END_CUT != 0,
        });
    }
    let warm = if read_flag(r, "trim.warm flag")? {
        Some(decode_warm(r)?)
    } else {
        None
    };
    Ok(Trim {
        origin,
        sample_offset,
        visible,
        own,
        warm,
    })
}

fn decode_text(r: &mut Reader<'_>) -> Result<TextItem, CodecError> {
    let content = read_str(r)?;
    let size = read_pair(r, "text.size")?;
    let font_size = r.f32_le("text.font_size")?;
    let paint = read_paint(r)?;
    let (count, cap) = read_count(r, 16)?;
    let mut sources = Vec::with_capacity(cap);
    for _ in 0..count {
        sources.push(read_id(r)?);
    }
    Ok(TextItem {
        content,
        size,
        font_size,
        paint,
        sources,
    })
}
