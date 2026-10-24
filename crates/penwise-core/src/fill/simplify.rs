//! Ramer–Douglas–Peucker simplification of closed rings (editing-algorithms.md §3.7).
//!
//! Owner: slice c (slice-internal).

use crate::edit::proximity::point_segment_dist2;

/// RDP tolerance, cells (the staircase hides under ink). Above `√2 / 2`, the deviation of a 45° staircase's
/// outer corners from the chord through its inner corners, so marching-squares stairs collapse at any slope.
pub(crate) const RDP_EPSILON_CELLS: f32 = 0.75;

/// Simplifies an implicitly closed ring in place with tolerance `epsilon` (page px). Deterministic anchors:
/// the minimum `(y, x)` vertex and the vertex farthest from it (ties: lower index); iterative with an explicit
/// stack. Rings that would drop below 3 vertices are left unchanged. Kept vertices stay in ring order.
pub(crate) fn simplify_ring(ring: &mut Vec<[f32; 2]>, epsilon: f32) {
    let n = ring.len();
    if n < 4 || epsilon.is_nan() || epsilon <= 0.0 {
        return;
    }
    let a = min_vertex(ring);
    let b = farthest_from(ring, a);
    if a == b {
        return;
    }
    let mut keep = vec![false; n];
    keep[a] = true;
    keep[b] = true;
    let eps2 = epsilon * epsilon;
    rdp_chain(ring, a, b, eps2, &mut keep);
    rdp_chain(ring, b, a, eps2, &mut keep);
    if keep.iter().filter(|&&k| k).count() < 3 {
        return;
    }
    let mut flags = keep.into_iter();
    ring.retain(|_| flags.next().unwrap_or(true));
}

/// Index of the minimum `(y, x)` vertex (first on ties).
fn min_vertex(ring: &[[f32; 2]]) -> usize {
    ring.iter()
        .enumerate()
        .min_by(|(_, p), (_, q)| p[1].total_cmp(&q[1]).then(p[0].total_cmp(&q[0])))
        .map_or(0, |(i, _)| i)
}

/// Index of the vertex farthest from `ring[a]` (lowest index on ties), squared distance in `f64`.
fn farthest_from(ring: &[[f32; 2]], a: usize) -> usize {
    let p = ring[a];
    let mut best = (0.0, a);
    for (i, q) in ring.iter().enumerate() {
        let dx = f64::from(q[0]) - f64::from(p[0]);
        let dy = f64::from(q[1]) - f64::from(p[1]);
        let d2 = dx * dx + dy * dy;
        if d2 > best.0 {
            best = (d2, i);
        }
    }
    best.1
}

/// RDP over the cyclic chain `from → to` (exclusive interior), marking kept vertices.
fn rdp_chain(ring: &[[f32; 2]], from: usize, to: usize, eps2: f32, keep: &mut [bool]) {
    let n = ring.len();
    let at = |k: usize| (from + k) % n;
    let mut stack = vec![(0, (to + n - from) % n)];
    while let Some((s, e)) = stack.pop() {
        if e <= s + 1 {
            continue;
        }
        let (pa, pb) = (ring[at(s)], ring[at(e)]);
        let mut best = (eps2, None);
        for k in s + 1..e {
            let (d2, _) = point_segment_dist2(ring[at(k)], pa, pb);
            if d2 > best.0 {
                best = (d2, Some(k));
            }
        }
        if let Some(k) = best.1 {
            keep[at(k)] = true;
            stack.push((s, k));
            stack.push((k, e));
        }
    }
}
