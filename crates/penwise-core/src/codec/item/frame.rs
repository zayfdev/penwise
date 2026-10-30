//! Frame kind data of the item body (layout in [`super`]).

use super::fields::{read_flag, read_str, write_flag, write_str};
use crate::codec::error::CodecError;
use crate::codec::params_fields::invalid;
use crate::codec::varint::{Reader, write_uv32};
use crate::item::{FrameExtent, FrameItem, FramePreset};

/// Appends the frame kind data written at item layout `version` (v3 adds the variant fields, v4 the extent
/// and document height).
pub(super) fn encode_frame(frame: &FrameItem, version: u16, out: &mut Vec<u8>) {
    out.push(frame.preset.code());
    write_uv32(out, frame.css_w);
    write_uv32(out, frame.css_h);
    write_str(out, &frame.project);
    write_str(out, &frame.route);
    if version >= 3 {
        encode_variant(frame, out);
    }
    if version >= 4 {
        out.push(frame.extent.code());
        write_uv32(out, frame.doc_h);
    }
}

/// Reads frame kind data written at item layout `version`. Frames older than v4 predate full-length frames,
/// so they decode as [`FrameExtent::Viewport`] with an unknown document height.
pub(super) fn decode_frame(r: &mut Reader<'_>, version: u16) -> Result<FrameItem, CodecError> {
    let preset = FramePreset::from_code(r.u8()?).ok_or(invalid("frame.preset"))?;
    let css_w = r.uv32()?;
    let css_h = r.uv32()?;
    let project = read_str(r)?;
    let route = read_str(r)?;
    let mut frame = FrameItem::new(project, route, preset, css_w, css_h);
    frame.extent = FrameExtent::Viewport;
    if version >= 3 {
        decode_variant(r, &mut frame)?;
    }
    if version >= 4 {
        frame.extent = FrameExtent::from_code(r.u8()?).ok_or(invalid("frame.extent"))?;
        frame.doc_h = r.uv32()?;
    }
    Ok(frame)
}

/// v3 frame fields: `patch flag u8 [+ len uv + UTF-8]; label (len uv + UTF-8); group flag u8 [+ len uv +
/// UTF-8]; chosen flag u8`.
fn encode_variant(frame: &FrameItem, out: &mut Vec<u8>) {
    match &frame.patch {
        Some(patch) => {
            write_flag(out, true);
            write_str(out, patch);
        }
        None => write_flag(out, false),
    }
    write_str(out, &frame.label);
    match &frame.group {
        Some(group) => {
            write_flag(out, true);
            write_str(out, group);
        }
        None => write_flag(out, false),
    }
    write_flag(out, frame.chosen);
}

fn decode_variant(r: &mut Reader<'_>, frame: &mut FrameItem) -> Result<(), CodecError> {
    frame.patch = if read_flag(r, "frame.patch flag")? {
        Some(read_str(r)?)
    } else {
        None
    };
    frame.label = read_str(r)?;
    frame.group = if read_flag(r, "frame.group flag")? {
        Some(read_str(r)?)
    } else {
        None
    };
    frame.chosen = read_flag(r, "frame.chosen")?;
    Ok(())
}
