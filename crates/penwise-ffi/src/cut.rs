//! Precise erase of one flat stroke: densify the stretches the eraser can reach, then keep the surviving runs.
//!
//! The segment rule in [`crate::erase`] removes a whole segment the eraser touches. A ruled line is a single
//! 1500 px segment, so cutting it as-is would delete it; instead the stroke is first subdivided so no segment
//! is longer than half the eraser radius, and the cut then removes only the part under the eraser.

use crate::erase::{STRIDE, surviving_runs};

/// Most pieces one segment is split into, so a huge segment with a tiny radius cannot allocate without bound.
const MAX_SPLIT_F: f32 = 4096.0;

/// One surviving fragment: interleaved samples (see [`STRIDE`]) and one time per sample.
#[derive(Debug, PartialEq)]
pub struct Fragment {
    pub samples: Vec<f32>,
    pub times: Vec<i32>,
}

/// Outcome of cutting one stroke.
#[derive(Debug, PartialEq)]
pub enum Cut {
    /// The eraser touches no segment: the stroke stays exactly as it was.
    Unchanged,
    /// What is left; empty means the whole stroke is gone.
    Pieces(Vec<Fragment>),
}

/// Cuts a stroke of `count` samples (with `times`, one per sample) at the eraser `path`.
///
/// Only a stroke the eraser actually touches is densified; an untouched one is reported [`Cut::Unchanged`].
#[must_use]
pub fn cut(
    samples: &[f32],
    times: &[i32],
    count: usize,
    path: &[f32],
    path_count: usize,
    radius: f32,
) -> Cut {
    if count == 0 || samples.len() < count * STRIDE || times.len() < count {
        return Cut::Unchanged;
    }
    let untouched = [(0, count)];
    if surviving_runs(samples, count, path, path_count, radius) == untouched {
        return Cut::Unchanged;
    }
    let (dense, dense_times) = densify(samples, times, count, radius / 2.0);
    let dense_count = dense_times.len();
    let pieces = surviving_runs(&dense, dense_count, path, path_count, radius)
        .into_iter()
        .map(|(from, to)| Fragment {
            samples: dense[from * STRIDE..to * STRIDE].to_vec(),
            times: dense_times[from..to].to_vec(),
        })
        .collect();
    Cut::Pieces(pieces)
}

/// Subdivides every segment longer than `step` into equal parts no longer than `step`, interpolating all
/// channels linearly (time rounded to an integer).
fn densify(samples: &[f32], times: &[i32], count: usize, step: f32) -> (Vec<f32>, Vec<i32>) {
    let mut out = Vec::with_capacity(count * STRIDE);
    let mut out_times = Vec::with_capacity(count);
    for index in 0..count {
        let a = &samples[index * STRIDE..(index + 1) * STRIDE];
        if index > 0 {
            let prev = &samples[(index - 1) * STRIDE..index * STRIDE];
            let parts = parts_for(prev, a, step);
            for k in 1..parts {
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "k and parts are bounded by MAX_SPLIT_F"
                )]
                let t = k as f32 / parts as f32;
                for channel in 0..STRIDE {
                    out.push(prev[channel] + (a[channel] - prev[channel]) * t);
                }
                let t0 = f64::from(times[index - 1]);
                let t1 = f64::from(times[index]);
                #[allow(clippy::cast_possible_truncation, reason = "between two i32 times")]
                out_times.push((t0 + (t1 - t0) * f64::from(t)).round() as i32);
            }
        }
        out.extend_from_slice(a);
        out_times.push(times[index]);
    }
    (out, out_times)
}

/// Number of equal parts the segment `a–b` is split into: 1 when it is short enough or not finite.
fn parts_for(a: &[f32], b: &[f32], step: f32) -> usize {
    let length = (b[0] - a[0]).hypot(b[1] - a[1]);
    if !length.is_finite() || step <= 0.0 || length <= step {
        return 1;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "length / step is positive and clamped to MAX_SPLIT"
    )]
    let parts = (length / step).ceil().min(MAX_SPLIT_F) as usize;
    parts.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x0: f32, x1: f32) -> (Vec<f32>, Vec<i32>) {
        (
            vec![x0, 0.0, 0.2, 0.0, 0.0, x1, 0.0, 1.0, 0.0, 0.0],
            vec![0, 1000],
        )
    }

    fn x_span(fragment: &Fragment) -> (f32, f32) {
        let first = fragment.samples[0];
        let last = fragment.samples[fragment.samples.len() - STRIDE];
        (first, last)
    }

    #[test]
    fn a_long_two_sample_line_is_split_with_a_gap_at_the_eraser() {
        let (samples, times) = line(0.0, 1000.0);
        let Cut::Pieces(pieces) = cut(&samples, &times, 2, &[500.0, -30.0, 500.0, 30.0], 2, 10.0)
        else {
            panic!("the eraser crosses the line");
        };
        assert_eq!(2, pieces.len());
        let (left_start, left_end) = x_span(&pieces[0]);
        let (right_start, right_end) = x_span(&pieces[1]);
        assert!(left_start.abs() < 1e-3);
        assert!(
            (480.0..=495.0).contains(&left_end),
            "left ends at {left_end}"
        );
        assert!(
            (505.0..=520.0).contains(&right_start),
            "right starts at {right_start}"
        );
        assert!((right_end - 1000.0).abs() < 1e-3);
    }

    #[test]
    fn densified_channels_interpolate_linearly() {
        let (samples, times) = line(0.0, 1000.0);
        let Cut::Pieces(pieces) = cut(&samples, &times, 2, &[500.0, -30.0, 500.0, 30.0], 2, 10.0)
        else {
            panic!("the eraser crosses the line");
        };
        let left = &pieces[0];
        let last = left.times.len() - 1;
        let x = left.samples[last * STRIDE];
        let pressure = left.samples[last * STRIDE + 2];
        assert!((pressure - (0.2 + 0.8 * x / 1000.0)).abs() < 1e-3);
        assert!((f64::from(left.times[last]) - f64::from(x)).abs() <= 1.0);
    }

    #[test]
    fn a_stroke_the_eraser_misses_is_unchanged() {
        let (samples, times) = line(0.0, 1000.0);
        assert_eq!(
            Cut::Unchanged,
            cut(&samples, &times, 2, &[500.0, 50.0, 500.0, 90.0], 2, 10.0)
        );
    }

    #[test]
    #[allow(clippy::cast_precision_loss, reason = "test geometry: small integers")]
    fn a_dense_stroke_is_cut_without_adding_samples() {
        let mut samples = Vec::new();
        let mut times = Vec::new();
        for i in 0..101 {
            samples.extend_from_slice(&[i as f32 * 3.0, 0.0, 0.5, 0.0, 0.0]);
            times.push(i);
        }
        let Cut::Pieces(pieces) = cut(&samples, &times, 101, &[150.0, -20.0, 150.0, 20.0], 2, 10.0)
        else {
            panic!("the eraser crosses the stroke");
        };
        assert_eq!(2, pieces.len());
        // 3 px segments are already finer than radius / 2: only the ones under the eraser go.
        assert_eq!(pieces[0].times.len() + pieces[1].times.len(), 101 - 7);
    }
}
