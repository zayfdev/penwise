//! Agent annotations: polylines in page coordinates become ordinary stroke [`Item`]s on the `Agent` layer.
//!
//! Keeping them as strokes means they render, sync, appear in the PNG, can be promoted to ink (`Update{layer}`)
//! and cleared as a set, with no second rendering path. Sampling is the same dense walk the shape tool uses, so
//! marks look like drawn ink. Identity is a fresh ULID per item and the seed is 64 random bits
//! (SERVER-DATA-04: the old counter restarted at 1 after every server start).

use penwise_core::item::{
    Affine, Item, ItemId, ItemKind, Layer, MAX_STROKE_SAMPLES, StrokeItem, Z_STEP,
};
use penwise_core::{BrushParams, Sample};
use serde::Deserialize;

/// Distance between synthesised samples along a segment, page px.
const STEP_PX: f32 = 4.0;
/// Microseconds between synthesised samples (matches a slow, deliberate hand).
const MICROS_PER_SAMPLE: u32 = 4000;
/// Segments shorter than this are skipped (repeated points carry no direction).
const MIN_SEGMENT_PX: f32 = 0.01;
/// Steady pressure so annotations read as one width.
const PRESSURE: f32 = 0.7;
/// Ballpoint: the preset whose brush shape annotations use.
const PRESET: usize = 0;
/// Default mark colour (red).
const DEFAULT_ARGB: u32 = 0xFFE0_3131;
/// Ballpoint's nominal width, used to turn a requested width into a size scale.
const NOMINAL_WIDTH: f32 = 3.2;

#[derive(Debug, Deserialize)]
pub(crate) struct AnnotateRequest {
    /// Target page; defaults to the current one.
    #[serde(default)]
    pub page: Option<String>,
    pub strokes: Vec<AnnotatedStroke>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AnnotatedStroke {
    /// Page-pixel points, `[[x, y], …]`; two or more make a line.
    pub points: Vec<[f32; 2]>,
    /// `#RRGGBB` or `#AARRGGBB`.
    #[serde(default)]
    pub color: Option<String>,
    /// Stroke width in page px (default 3).
    #[serde(default)]
    pub width: Option<f32>,
}

/// Where new items get their identity and seed (injectable so tests are deterministic).
pub(crate) trait Identity {
    fn next(&mut self) -> (ItemId, u64);
}

/// Converts the request into agent-layer strokes stacked from `first_z` up; entries with fewer than two finite
/// points, too many samples, or a brush the core refuses are dropped.
pub(crate) fn to_items(
    request: &AnnotateRequest,
    first_z: i64,
    identity: &mut impl Identity,
) -> Vec<Item> {
    let mut z = first_z;
    let mut items = Vec::new();
    for annotation in &request.strokes {
        let Some(samples) = samples_of(annotation) else {
            continue;
        };
        let (id, seed) = identity.next();
        let scale = annotation
            .width
            .map_or(1.0, |w| (w / NOMINAL_WIDTH).clamp(0.5, 6.0));
        let argb = annotation
            .color
            .as_deref()
            .map_or(DEFAULT_ARGB, parse_color);
        let item = Item {
            id,
            layer: Layer::Agent,
            z,
            transform: Affine::IDENTITY,
            kind: ItemKind::Stroke(StrokeItem {
                params: BrushParams::for_preset(PRESET, argb, scale),
                seed,
                samples,
                trim: None,
                binding: None,
            }),
        };
        if item.validate().is_ok() {
            items.push(item);
            z = z.saturating_add(Z_STEP);
        }
    }
    items
}

fn samples_of(annotation: &AnnotatedStroke) -> Option<Vec<Sample>> {
    let points: Vec<[f32; 2]> = annotation
        .points
        .iter()
        .copied()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .collect();
    let first = *points.first()?;
    if points.len() < 2 {
        return None;
    }
    let cap = usize::try_from(MAX_STROKE_SAMPLES).unwrap_or(usize::MAX);
    let mut samples = vec![sample_at(first, 0)];
    for pair in points.windows(2) {
        append_segment(pair[0], pair[1], &mut samples);
        if samples.len() > cap {
            return None;
        }
    }
    (samples.len() > 1).then_some(samples)
}

fn sample_at(point: [f32; 2], index: usize) -> Sample {
    let micros = u32::try_from(index)
        .unwrap_or(u32::MAX / MICROS_PER_SAMPLE)
        .saturating_mul(MICROS_PER_SAMPLE);
    Sample {
        x: point[0],
        y: point[1],
        pressure: PRESSURE,
        tilt: 0.0,
        azimuth: 0.0,
        t_us: micros,
    }
    .quantize()
}

/// Appends samples along one segment, excluding the start and including the end.
///
/// A segment never adds more than `MAX_STROKE_SAMPLES + 1` samples, however long (or infinite) it is, so the
/// caller can reject an oversized stroke after the fact without having allocated it.
fn append_segment(from: [f32; 2], to: [f32; 2], samples: &mut Vec<Sample>) {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let length = (dx * dx + dy * dy).sqrt();
    if !length.is_finite() || length <= MIN_SEGMENT_PX {
        return;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "length is finite and positive; the ceiling is capped to 2^20 + 1, exact in f32 and a valid usize"
    )]
    let steps = ((length / STEP_PX)
        .ceil()
        .min(MAX_STROKE_SAMPLES as f32 + 1.0) as usize)
        .max(1);
    for step in 1..=steps {
        let t = ratio(step, steps);
        let index = samples.len();
        samples.push(sample_at([from[0] + dx * t, from[1] + dy * t], index));
    }
}

/// `step / steps` in `f32`; both counts are bounded by the sample cap.
#[allow(
    clippy::cast_precision_loss,
    reason = "steps is at most 2^20 + 1, exact in f32"
)]
fn ratio(step: usize, steps: usize) -> f32 {
    step as f32 / steps as f32
}

/// `#RRGGBB` / `#AARRGGBB` (the `#` is optional); anything unparseable falls back to the default colour.
fn parse_color(text: &str) -> u32 {
    let hex = text.trim().trim_start_matches('#');
    let value = u32::from_str_radix(hex, 16).ok();
    match (value, hex.len()) {
        (Some(rgb), 6) => 0xFF00_0000 | rgb,
        (Some(argb), 8) => argb,
        _ => DEFAULT_ARGB,
    }
}

#[cfg(test)]
mod tests;
