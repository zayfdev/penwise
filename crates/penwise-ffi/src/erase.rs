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

/// Runs of `count` samples whose centreline segments stay farther than `radius` from the eraser path.
///
/// `samples` is interleaved by [`STRIDE`]; `path` is `x, y` interleaved. The rule is segment-based and inclusive
/// (`<= radius` erases), matching the hit test in `EraseHit.kt`: a segment of the stroke survives only when the
/// whole segment is farther than `radius` from the eraser path, so a two-sample line crossed in the middle is
/// removed rather than returned unchanged. Consecutive surviving segments form one run `[from, to)`; a stroke
/// the sweep never reaches comes back as the single run `(0, count)`. An empty result means nothing survives.
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
    if count == 1 {
        let d = distance_to_path(path, samples[0], samples[1]);
        return if d <= radius {
            Vec::new()
        } else {
            vec![(0, 1)]
        };
    }
    let mut runs = Vec::new();
    // First sample of the current run of surviving segments.
    let mut start: Option<usize> = None;
    for index in 0..count - 1 {
        let o = index * STRIDE;
        let p = o + STRIDE;
        let erased = segment_distance_to_path(
            path,
            [samples[o], samples[o + 1]],
            [samples[p], samples[p + 1]],
        ) <= radius;
        match (erased, start) {
            (false, None) => start = Some(index),
            (true, Some(from)) => {
                runs.push((from, index + 1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        runs.push((from, count));
    }
    runs
}

/// Shortest distance between the segment `a–b` and the polyline `path`.
fn segment_distance_to_path(path: &[f32], a: [f32; 2], b: [f32; 2]) -> f32 {
    let points = path.len() / 2;
    if points == 1 {
        return distance_to_segment(path[0], path[1], a[0], a[1], b[0], b[1]);
    }
    let mut best = f32::MAX;
    for index in 0..points - 1 {
        let o = index * 2;
        let c = [path[o], path[o + 1]];
        let d = [path[o + 2], path[o + 3]];
        let dist = segment_distance(a, b, c, d);
        if dist < best {
            best = dist;
        }
    }
    best
}

/// Shortest distance between two segments: zero when they cross, else the best endpoint-to-segment distance.
fn segment_distance(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> f32 {
    if segments_cross(a, b, c, d) {
        return 0.0;
    }
    let candidates = [
        distance_to_segment(a[0], a[1], c[0], c[1], d[0], d[1]),
        distance_to_segment(b[0], b[1], c[0], c[1], d[0], d[1]),
        distance_to_segment(c[0], c[1], a[0], a[1], b[0], b[1]),
        distance_to_segment(d[0], d[1], a[0], a[1], b[0], b[1]),
    ];
    candidates.into_iter().fold(f32::MAX, f32::min)
}

/// True when the segments properly cross (touching and collinear cases are covered by the endpoint distances).
fn segments_cross(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> bool {
    let side = |p: [f32; 2], q: [f32; 2], r: [f32; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let (d1, d2) = (side(a, b, c), side(a, b, d));
    let (d3, d4) = (side(c, d, a), side(c, d, b));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
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

#[cfg(test)]
mod segment_tests {
    use super::*;

    fn line(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<f32> {
        vec![x0, y0, 0.5, 0.0, 0.0, x1, y1, 0.5, 0.0, 0.0]
    }

    #[test]
    fn a_two_sample_line_crossed_in_the_middle_is_removed() {
        let samples = line(0.0, 0.0, 100.0, 0.0);
        // Both endpoints are 50 px from the eraser, yet the line is cut: the old sample rule kept it whole.
        let runs = surviving_runs(&samples, 2, &[50.0, -20.0, 50.0, 20.0], 2, 5.0);
        assert!(runs.is_empty());
    }

    #[test]
    fn a_two_sample_line_grazed_at_the_radius_is_removed_and_just_outside_is_kept() {
        let samples = line(0.0, 0.0, 100.0, 0.0);
        assert!(surviving_runs(&samples, 2, &[50.0, 10.0], 1, 10.0).is_empty());
        assert_eq!(
            vec![(0, 2)],
            surviving_runs(&samples, 2, &[50.0, 10.5], 1, 10.0)
        );
    }
}
