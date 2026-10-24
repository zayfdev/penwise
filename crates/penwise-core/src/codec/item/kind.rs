//! Kind-specific part of the item body (layout in [`super`]).
//!
//! Owner: slice e (M2).

use super::fields::{
    read_count, read_flag, read_id, read_opt_color, read_paint, read_pair, read_polygons,
    read_shape_kind, read_str, write_flag, write_id, write_len, write_opt_color, write_paint,
    write_polygons, write_shape_kind, write_str,
};
use super::{decode_points, encode_points};
use crate::codec::error::CodecError;
use crate::codec::params_fields::{invalid, write_f32s};
use crate::codec::stroke::{decode_params, decode_samples, encode_params, encode_samples};
use crate::codec::varint::{Reader, write_uv32};
use crate::item::{
    EraseItem, FillItem, ItemKind, ItemKindTag, SampleParam, SampleRange, ShapeItem, StrokeItem,
    TextItem, Trim,
};
use crate::sample::{GridSample, Sample};

/// Range flag bits.
const START_CUT: u8 = 1;
const END_CUT: u8 = 2;

/// Appends the kind data of `kind`.
pub(super) fn encode_kind(kind: &ItemKind, out: &mut Vec<u8>) {
    match kind {
        ItemKind::Stroke(stroke) => encode_stroke(stroke, out),
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
    }
}

/// Reads the kind data for `tag`.
pub(super) fn decode_kind(r: &mut Reader<'_>, tag: ItemKindTag) -> Result<ItemKind, CodecError> {
    Ok(match tag {
        ItemKindTag::Stroke => ItemKind::Stroke(decode_stroke(r)?),
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
    })
}

fn encode_stroke(stroke: &StrokeItem, out: &mut Vec<u8>) {
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
    }
}

fn decode_stroke(r: &mut Reader<'_>) -> Result<StrokeItem, CodecError> {
    let params = decode_params(r)?;
    let seed = r.u64_le()?;
    let mut samples: Vec<Sample> = Vec::new();
    decode_samples(r, &mut samples)?;
    let trim = if read_flag(r, "trim flag")? {
        Some(decode_trim(r)?)
    } else {
        None
    };
    Ok(StrokeItem {
        params,
        seed,
        samples,
        trim,
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
    Ok(Trim {
        origin,
        sample_offset,
        visible,
        own,
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
