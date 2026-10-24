//! Value readers/writers shared by the item (`PWIT`) and edit (`PWED`) blobs (private to the codec).
//!
//! Owner: slice e (M2).
//!
//! Every reader that allocates for a decoded count bounds its pre-allocation by the bytes actually left
//! ([`read_count`]), so a hostile count costs nothing until the elements are really there.

use super::{decode_points, encode_points};
use crate::brush::Color;
use crate::codec::error::CodecError;
use crate::codec::params_fields::{invalid, write_f32s};
use crate::codec::varint::{Reader, write_uv32};
use crate::item::{Affine, ArrowHead, ItemId, Layer, Paint, Polygon, ShapeKind};

/// Byte tags of [`ShapeKind`] (frozen; same values as `ShapeKind::code`).
const SHAPE_RECT: u8 = 0;
const SHAPE_ROUNDED_RECT: u8 = 1;
const SHAPE_ELLIPSE: u8 = 2;
const SHAPE_LINE: u8 = 3;
const SHAPE_ARROW: u8 = 4;
const SHAPE_POLYGON: u8 = 5;

/// Writes `len` as a `uv` count (saturating at `u32::MAX`) and returns how many elements to write after it.
pub(in crate::codec) fn write_len(out: &mut Vec<u8>, len: usize) -> usize {
    let count = u32::try_from(len).unwrap_or(u32::MAX);
    write_uv32(out, count);
    usize::try_from(count).unwrap_or(usize::MAX)
}

/// Reads a `uv` count and a safe capacity hint: `count` capped at `remaining / min_bytes` (every element
/// takes at least `min_bytes` encoded bytes).
pub(in crate::codec) fn read_count(
    r: &mut Reader<'_>,
    min_bytes: usize,
) -> Result<(u32, usize), CodecError> {
    let count = r.uv32()?;
    let hint = usize::try_from(count).unwrap_or(usize::MAX);
    Ok((count, hint.min(r.remaining() / min_bytes.max(1))))
}

pub(in crate::codec) fn write_id(out: &mut Vec<u8>, id: ItemId) {
    out.extend_from_slice(&id.0);
}

pub(in crate::codec) fn read_id(r: &mut Reader<'_>) -> Result<ItemId, CodecError> {
    let mut id = [0_u8; 16];
    id.copy_from_slice(r.bytes(16)?);
    Ok(ItemId(id))
}

pub(in crate::codec) fn write_flag(out: &mut Vec<u8>, flag: bool) {
    out.push(u8::from(flag));
}

/// A `0`/`1` byte; anything else is `InvalidValue { field }`.
pub(in crate::codec) fn read_flag(
    r: &mut Reader<'_>,
    field: &'static str,
) -> Result<bool, CodecError> {
    match r.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid(field)),
    }
}

pub(in crate::codec) fn write_z(out: &mut Vec<u8>, z: i64) {
    out.extend_from_slice(&z.cast_unsigned().to_le_bytes());
}

pub(in crate::codec) fn read_z(r: &mut Reader<'_>) -> Result<i64, CodecError> {
    Ok(r.u64_le()?.cast_signed())
}

pub(in crate::codec) fn read_layer(r: &mut Reader<'_>) -> Result<Layer, CodecError> {
    Layer::from_code(r.u8()?).ok_or(invalid("layer"))
}

pub(in crate::codec) fn read_pair(
    r: &mut Reader<'_>,
    field: &'static str,
) -> Result<[f32; 2], CodecError> {
    Ok([r.f32_le(field)?, r.f32_le(field)?])
}

pub(in crate::codec) fn write_affine(out: &mut Vec<u8>, t: &Affine) {
    write_f32s(out, &[t.a, t.b, t.c, t.d, t.e, t.f]);
}

pub(in crate::codec) fn read_affine(r: &mut Reader<'_>) -> Result<Affine, CodecError> {
    const FIELD: &str = "transform";
    Ok(Affine {
        a: r.f32_le(FIELD)?,
        b: r.f32_le(FIELD)?,
        c: r.f32_le(FIELD)?,
        d: r.f32_le(FIELD)?,
        e: r.f32_le(FIELD)?,
        f: r.f32_le(FIELD)?,
    })
}

fn write_color(out: &mut Vec<u8>, c: Color) {
    out.extend_from_slice(&[c.r, c.g, c.b, c.a]);
}

fn read_color(r: &mut Reader<'_>) -> Result<Color, CodecError> {
    let [red, g, b, a] = [r.u8()?, r.u8()?, r.u8()?, r.u8()?];
    Ok(Color { r: red, g, b, a })
}

/// Shape fill: flag `u8` [+ `r g b a`].
pub(in crate::codec) fn write_opt_color(out: &mut Vec<u8>, c: Option<Color>) {
    write_flag(out, c.is_some());
    if let Some(c) = c {
        write_color(out, c);
    }
}

pub(in crate::codec) fn read_opt_color(r: &mut Reader<'_>) -> Result<Option<Color>, CodecError> {
    if read_flag(r, "fill flag")? {
        Ok(Some(read_color(r)?))
    } else {
        Ok(None)
    }
}

/// Paint: `r g b a u8`, opacity `f32`.
pub(in crate::codec) fn write_paint(out: &mut Vec<u8>, p: Paint) {
    write_color(out, p.color);
    write_f32s(out, &[p.opacity]);
}

pub(in crate::codec) fn read_paint(r: &mut Reader<'_>) -> Result<Paint, CodecError> {
    let color = read_color(r)?;
    let opacity = r.f32_le("paint.opacity")?;
    Ok(Paint { color, opacity })
}

/// UTF-8 string: `len uv` + bytes.
pub(in crate::codec) fn write_str(out: &mut Vec<u8>, s: &str) {
    let n = write_len(out, s.len());
    out.extend_from_slice(s.as_bytes().get(..n).unwrap_or(s.as_bytes()));
}

pub(in crate::codec) fn read_str(r: &mut Reader<'_>) -> Result<String, CodecError> {
    let len = usize::try_from(r.uv32()?).map_err(|_| invalid("text"))?;
    let bytes = r.bytes(len)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| invalid("text"))
}

/// Shape kind: tag `u8` [+ `RoundedRect` radius `f32` | `Arrow` head `u8` | `Polygon` count `uv` + `f32` pairs].
pub(in crate::codec) fn write_shape_kind(out: &mut Vec<u8>, kind: &ShapeKind) {
    match kind {
        ShapeKind::Rect => out.push(SHAPE_RECT),
        ShapeKind::RoundedRect { radius } => {
            out.push(SHAPE_ROUNDED_RECT);
            write_f32s(out, &[*radius]);
        }
        ShapeKind::Ellipse => out.push(SHAPE_ELLIPSE),
        ShapeKind::Line => out.push(SHAPE_LINE),
        ShapeKind::Arrow { head } => out.extend_from_slice(&[SHAPE_ARROW, head.code()]),
        ShapeKind::Polygon { points } => {
            out.push(SHAPE_POLYGON);
            let n = write_len(out, points.len());
            for p in points.iter().take(n) {
                write_f32s(out, p);
            }
        }
    }
}

pub(in crate::codec) fn read_shape_kind(r: &mut Reader<'_>) -> Result<ShapeKind, CodecError> {
    Ok(match r.u8()? {
        SHAPE_RECT => ShapeKind::Rect,
        SHAPE_ROUNDED_RECT => ShapeKind::RoundedRect {
            radius: r.f32_le("shape.radius")?,
        },
        SHAPE_ELLIPSE => ShapeKind::Ellipse,
        SHAPE_LINE => ShapeKind::Line,
        SHAPE_ARROW => ShapeKind::Arrow {
            head: ArrowHead::from_code(r.u8()?).ok_or(invalid("shape.head"))?,
        },
        SHAPE_POLYGON => {
            let (count, cap) = read_count(r, 8)?;
            let mut points = Vec::with_capacity(cap);
            for _ in 0..count {
                points.push(read_pair(r, "shape.points")?);
            }
            ShapeKind::Polygon { points }
        }
        _ => return Err(invalid("shape.kind")),
    })
}

/// Polygons: count `uv`; per polygon the outer ring, hole count `uv`, holes; ring = [`encode_points`] run.
pub(in crate::codec) fn write_polygons(out: &mut Vec<u8>, polygons: &[Polygon]) {
    let n = write_len(out, polygons.len());
    for polygon in polygons.iter().take(n) {
        encode_points(&polygon.outer, out);
        let holes = write_len(out, polygon.holes.len());
        for hole in polygon.holes.iter().take(holes) {
            encode_points(hole, out);
        }
    }
}

pub(in crate::codec) fn read_polygons(r: &mut Reader<'_>) -> Result<Vec<Polygon>, CodecError> {
    // A polygon takes at least 2 bytes (empty outer ring count + hole count), a ring at least 1.
    let (count, cap) = read_count(r, 2)?;
    let mut polygons = Vec::with_capacity(cap);
    for _ in 0..count {
        let mut outer = Vec::new();
        decode_points(r, &mut outer)?;
        let (hole_count, hole_cap) = read_count(r, 1)?;
        let mut holes = Vec::with_capacity(hole_cap);
        for _ in 0..hole_count {
            let mut hole = Vec::new();
            decode_points(r, &mut hole)?;
            holes.push(hole);
        }
        polygons.push(Polygon { outer, holes });
    }
    Ok(polygons)
}
