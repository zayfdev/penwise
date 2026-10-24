//! Ramer–Douglas–Peucker simplification of closed rings (editing-algorithms.md §3.7).
//!
//! Owner: slice c (slice-internal).

/// RDP tolerance, cells (the staircase hides under ink).
pub(crate) const RDP_EPSILON_CELLS: f32 = 0.6;

/// Simplifies an implicitly closed ring in place with tolerance `epsilon` (page px). Deterministic anchors:
/// the minimum `(y, x)` vertex and the vertex farthest from it (ties: lower index); iterative with an explicit
/// stack. Rings that would drop below 3 vertices are left unchanged.
pub(crate) fn simplify_ring(ring: &mut Vec<[f32; 2]>, epsilon: f32) {
    todo!()
}
