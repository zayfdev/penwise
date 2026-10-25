//! Precise erase for the app's flat strokes: which sample runs survive an eraser sweep.
//!
//! The app owns flat strokes (samples + timing), so the split is expressed as runs of surviving sample indices;
//! Kotlin slices the arrays and rebuilds geometry per fragment. The item-graph eraser in `penwise-core::edit`
//! remains the model for the op-log design; this is the same rule (distance from the centreline to the eraser
//! path) applied to one stroke at a time, which is what a drag needs: only strokes the drag already touched are
//! passed in.

/// Sample stride in the app's wire format: x, y, pressure, tilt, azimuth.
pub const STRIDE: usize = 5;

/// Half-open sample range `[start, end)` that survives.
pub type Run = (usize, usize);

/// Runs of `count` samples that are at least `radius` away from the eraser path.
///
/// `samples` is interleaved by [`STRIDE`]; `path` is `x, y` interleaved. Returns one run per surviving piece,
/// shortest piece of two samples (a single point paints nothing). An empty result means the whole stroke is gone.
#[must_use]
pub fn surviving_runs(
    samples: &[f32],
    count: usize,
    path: &[f32],
    path_count: usize,
    radius: f32,
) -> Vec<Run> {
    if count == 0 || samples.len() < count * STRIDE {
        return Vec::new();
    }
    if path_count == 0 || path.len() < path_count * 2 || !radius.is_finite() || radius <= 0.0 {
        return vec![(0, count)];
    }
    let path = &path[..path_count * 2];
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for index in 0..count {
        let x = samples[index * STRIDE];
        let y = samples[index * STRIDE + 1];
        let erased = distance_to_path(path, x, y) < radius;
        match (erased, start) {
            (false, None) => start = Some(index),
            (true, Some(from)) => {
                push_run(&mut runs, from, index);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        push_run(&mut runs, from, count);
    }
    runs
}

/// Records a run when it is long enough to paint.
fn push_run(runs: &mut Vec<Run>, start: usize, end: usize) {
    if end.saturating_sub(start) >= 2 {
        runs.push((start, end));
    }
}

/// Shortest distance from `(x, y)` to the polyline `path` (`x, y` interleaved, one point per pair).
///
/// The app appends every pen point of the sweep in order, so this walks **consecutive** points: reading the
/// array as independent segment pairs loses the links between them and tests geometry the eraser never crossed.
fn distance_to_path(path: &[f32], x: f32, y: f32) -> f32 {
    let points = path.len() / 2;
    if points == 1 {
        let (px, py) = (path[0], path[1]);
        return ((x - px).powi(2) + (y - py).powi(2)).sqrt();
    }
    let mut best = f32::MAX;
    for index in 0..points - 1 {
        let o = index * 2;
        let d = distance_to_segment(x, y, path[o], path[o + 1], path[o + 2], path[o + 3]);
        if d < best {
            best = d;
        }
    }
    best
}

/// Distance from `(x, y)` to the segment `(ax, ay)–(bx, by)`.
fn distance_to_segment(x: f32, y: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let length_sq = dx * dx + dy * dy;
    if length_sq <= f32::EPSILON {
        return ((x - ax).powi(2) + (y - ay).powi(2)).sqrt();
    }
    let t = (((x - ax) * dx + (y - ay) * dy) / length_sq).clamp(0.0, 1.0);
    let px = ax + dx * t;
    let py = ay + dy * t;
    ((x - px).powi(2) + (y - py).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A horizontal stroke from x=0 to x=100 at y=50, one sample per 10 px.
    #[allow(
        clippy::cast_precision_loss,
        reason = "test geometry: eleven small integers"
    )]
    fn straight() -> (Vec<f32>, usize) {
        let mut samples = Vec::new();
        for i in 0..11 {
            samples.extend_from_slice(&[i as f32 * 10.0, 50.0, 0.5, 0.0, 0.0]);
        }
        (samples, 11)
    }

    #[test]
    fn a_sweep_that_misses_keeps_the_whole_stroke() {
        let (samples, count) = straight();
        let runs = surviving_runs(&samples, count, &[0.0, 500.0, 100.0, 500.0], 2, 10.0);
        assert_eq!(vec![(0, 11)], runs);
    }

    #[test]
    fn a_sweep_through_the_middle_leaves_two_pieces() {
        let (samples, count) = straight();
        // Vertical eraser at x=50, below the stroke: it crosses the stroke once.
        let runs = surviving_runs(&samples, count, &[50.0, 0.0, 50.0, 100.0], 2, 12.0);
        assert_eq!(vec![(0, 4), (7, 11)], runs);
    }

    #[test]
    fn a_sweep_covering_every_sample_leaves_nothing() {
        let (samples, count) = straight();
        let runs = surviving_runs(&samples, count, &[0.0, 50.0, 100.0, 50.0], 2, 400.0);
        assert!(runs.is_empty());
    }

    #[test]
    #[allow(
        clippy::cast_precision_loss,
        reason = "test geometry: eleven small integers"
    )]
    fn pieces_shorter_than_two_samples_are_dropped() {
        let (samples, count) = straight();
        // Two separate cuts (x=40 and x=60) leave the single sample at x=50 stranded: it is dropped.
        let path = [40.0, 0.0, 40.0, 100.0, 60.0, 0.0, 60.0, 100.0];
        let runs = surviving_runs(&samples, count, &path, 4, 5.0);
        assert_eq!(vec![(0, 4), (7, 11)], runs);
    }

    #[test]
    fn an_empty_path_or_radius_keeps_everything() {
        let (samples, count) = straight();
        assert_eq!(vec![(0, 11)], surviving_runs(&samples, count, &[], 0, 10.0));
        assert_eq!(
            vec![(0, 11)],
            surviving_runs(&samples, count, &[0.0, 0.0], 1, 0.0)
        );
    }

    #[test]
    fn a_malformed_sample_buffer_is_refused() {
        assert!(surviving_runs(&[0.0, 0.0], 3, &[0.0, 0.0], 1, 5.0).is_empty());
    }

    #[test]
    fn distance_to_a_zoomed_out_segment_is_right() {
        assert!((distance_to_segment(5.0, 5.0, 0.0, 0.0, 10.0, 0.0) - 5.0).abs() < 1e-4);
        assert!((distance_to_segment(-5.0, 0.0, 0.0, 0.0, 10.0, 0.0) - 5.0).abs() < 1e-4);
        assert!((distance_to_segment(3.0, 4.0, 0.0, 0.0, 0.0, 0.0) - 5.0).abs() < 1e-4);
    }
}
