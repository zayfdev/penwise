//! Agent annotations: polylines in page coordinates become ordinary strokes on the `agent` layer.
//!
//! Keeping them as strokes means they render, persist, appear in the PNG and can be cleared as a set — without a
//! second rendering path. Sampling is the same dense-walk the shape tool uses, so marks look like drawn ink.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Deserialize;

use crate::state::{AGENT_LAYER, StrokeMsg};

/// Distance between synthesised samples along a segment, page px.
const STEP_PX: f32 = 4.0;
/// Microseconds between synthesised samples (matches a slow, deliberate hand).
const MICROS_PER_SAMPLE: i32 = 4000;
/// Segments shorter than this are skipped (repeated points carry no direction).
const MIN_SEGMENT_PX: f32 = 0.01;
/// Steady pressure so annotations read as one width.
const PRESSURE: f32 = 0.7;
/// Ballpoint: the preset whose brush shape annotations use.
const PRESET: u32 = 0;
/// Default mark colour (red) and width in page px.
const DEFAULT_ARGB: i32 = 0xFFE0_3131_u32.cast_signed();
/// Ballpoint's nominal width, used to turn a requested width into a size scale.
const NOMINAL_WIDTH: f32 = 3.2;

static SEEDS: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Deserialize)]
pub struct AnnotateRequest {
    pub strokes: Vec<AnnotatedStroke>,
}

#[derive(Debug, Deserialize)]
pub struct AnnotatedStroke {
    /// Page-pixel points, `[[x, y], …]`; two or more make a line.
    pub points: Vec<[f32; 2]>,
    /// `#RRGGBB` or `#AARRGGBB`.
    #[serde(default)]
    pub color: Option<String>,
    /// Stroke width in page px (default 3).
    #[serde(default)]
    pub width: Option<f32>,
}

/// Converts the request into strokes; entries with fewer than two finite points are dropped.
pub fn to_strokes(request: &AnnotateRequest) -> Vec<StrokeMsg> {
    request.strokes.iter().filter_map(stroke_of).collect()
}

fn stroke_of(annotation: &AnnotatedStroke) -> Option<StrokeMsg> {
    let points: Vec<[f32; 2]> = annotation
        .points
        .iter()
        .copied()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .collect();
    if points.len() < 2 {
        return None;
    }
    let mut samples = Vec::new();
    let mut times = Vec::new();
    for pair in points.windows(2) {
        append_segment(pair[0], pair[1], &mut samples, &mut times);
    }
    if times.is_empty() {
        return None;
    }
    let scale = annotation
        .width
        .map_or(1.0, |w| (w / NOMINAL_WIDTH).clamp(0.5, 6.0));
    Some(StrokeMsg {
        layer: AGENT_LAYER.to_owned(),
        preset: PRESET,
        argb: annotation
            .color
            .as_deref()
            .map_or(DEFAULT_ARGB, parse_color),
        scale,
        seed: SEEDS.fetch_add(1, Ordering::Relaxed).to_string(),
        samples,
        times,
    })
}

/// Appends samples along one segment, excluding the start and including the end (the first point of the first
/// segment is seeded by the caller never being empty: the stroke starts at its first sample).
fn append_segment(from: [f32; 2], to: [f32; 2], samples: &mut Vec<f32>, times: &mut Vec<i32>) {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let length = (dx * dx + dy * dy).sqrt();
    if length <= MIN_SEGMENT_PX {
        return;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "length is finite and positive here; the ceiling of a small ratio fits usize"
    )]
    let steps = ((length / STEP_PX).ceil() as usize).max(1);
    for step in 1..=steps {
        let t = ratio(step, steps);
        samples.extend_from_slice(&[from[0] + dx * t, from[1] + dy * t, PRESSURE, 0.0, 0.0]);
        times.push(next_time(times.len()));
    }
}

/// `step / steps` in `f32`; both counts are small (one annotation's samples).
#[allow(
    clippy::cast_precision_loss,
    reason = "steps is the sample count of one annotation, far below 2^23"
)]
fn ratio(step: usize, steps: usize) -> f32 {
    step as f32 / steps as f32
}

#[allow(
    clippy::cast_possible_wrap,
    reason = "index is bounded by the sample count of one annotation"
)]
fn next_time(index: usize) -> i32 {
    i32::try_from(index)
        .unwrap_or(i32::MAX / MICROS_PER_SAMPLE)
        .saturating_mul(MICROS_PER_SAMPLE)
}

/// `#RRGGBB` / `#AARRGGBB` (the `#` is optional); anything unparseable falls back to the default colour.
fn parse_color(text: &str) -> i32 {
    let hex = text.trim().trim_start_matches('#');
    let value = u32::from_str_radix(hex, 16).ok();
    match (value, hex.len()) {
        (Some(rgb), 6) => (0xFF00_0000_u32 | rgb).cast_signed(),
        (Some(argb), 8) => argb.cast_signed(),
        _ => DEFAULT_ARGB,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(stroke: AnnotatedStroke) -> AnnotateRequest {
        AnnotateRequest {
            strokes: vec![stroke],
        }
    }

    fn line(points: Vec<[f32; 2]>, color: Option<&str>, width: Option<f32>) -> AnnotatedStroke {
        AnnotatedStroke {
            points,
            color: color.map(str::to_owned),
            width,
        }
    }

    #[test]
    fn a_line_becomes_a_dense_agent_stroke() {
        let strokes = to_strokes(&request(line(vec![[0.0, 0.0], [40.0, 0.0]], None, None)));
        assert_eq!(strokes.len(), 1);
        let stroke = &strokes[0];
        assert_eq!(stroke.layer, AGENT_LAYER);
        assert_eq!(stroke.samples.len(), stroke.times.len() * 5);
        // 40 px in 4 px steps = 10 samples, five floats each.
        assert_eq!(10 * 5, stroke.samples.len());
        let last = (stroke.times.len() - 1) * 5;
        assert!(
            (stroke.samples[last] - 40.0).abs() < 1e-4,
            "ends on the far point"
        );
        assert!(stroke.samples[last + 1].abs() < 1e-4);
        assert_eq!(0, stroke.times[0]);
        assert!(stroke.times.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn degenerate_or_non_finite_input_is_dropped() {
        assert!(to_strokes(&request(line(vec![[5.0, 5.0]], None, None))).is_empty());
        assert!(
            to_strokes(&request(line(
                vec![[f32::NAN, 0.0], [10.0, 10.0]],
                None,
                None
            )))
            .is_empty()
        );
        assert!(to_strokes(&request(line(vec![[1.0, 1.0], [1.0, 1.0]], None, None))).is_empty());
    }

    #[test]
    fn colours_and_width_map_to_argb_and_scale() {
        let red = to_strokes(&request(line(
            vec![[0.0, 0.0], [10.0, 0.0]],
            Some("#ff0000"),
            Some(6.4),
        )));
        assert_eq!(0xFFFF_0000_u32.cast_signed(), red[0].argb);
        assert!((red[0].scale - 2.0).abs() < 1e-6);
        let translucent = to_strokes(&request(line(
            vec![[0.0, 0.0], [10.0, 0.0]],
            Some("#8000ff00"),
            None,
        )));
        assert_eq!(0x8000_FF00_u32.cast_signed(), translucent[0].argb);
        let fallback = to_strokes(&request(line(
            vec![[0.0, 0.0], [10.0, 0.0]],
            Some("not-a-colour"),
            None,
        )));
        assert_eq!(DEFAULT_ARGB, fallback[0].argb);
        assert!((fallback[0].scale - 1.0).abs() < 1e-6);
    }

    #[test]
    fn seeds_differ_between_annotations() {
        let a = to_strokes(&request(line(vec![[0.0, 0.0], [10.0, 0.0]], None, None)));
        let b = to_strokes(&request(line(vec![[0.0, 0.0], [10.0, 0.0]], None, None)));
        assert_ne!(a[0].seed, b[0].seed);
    }
}
