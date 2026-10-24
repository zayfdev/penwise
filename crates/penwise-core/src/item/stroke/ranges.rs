//! Range-set intersection in origin sample-param space (family visibility, concurrent splits).
//!
//! Owner: slice b.

use std::cmp::Ordering;

use super::SampleRange;

/// See [`super::intersect_ranges`]: two-pointer sweep over both sorted, disjoint sets.
pub(super) fn intersect(a: &[SampleRange], b: &[SampleRange]) -> Vec<SampleRange> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while let (Some(x), Some(y)) = (a.get(i), b.get(j)) {
        if let Some(r) = overlap(x, y) {
            out.push(r);
        }
        match x.end.cmp(&y.end) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                i += 1;
                j += 1;
            }
        }
    }
    out
}

/// Intersection of two single ranges under the rules of [`super::intersect_ranges`].
fn overlap(x: &SampleRange, y: &SampleRange) -> Option<SampleRange> {
    let (start, start_cut) = match x.start.cmp(&y.start) {
        Ordering::Greater => (x.start, x.start_cut),
        Ordering::Less => (y.start, y.start_cut),
        Ordering::Equal => (x.start, x.start_cut || y.start_cut),
    };
    let (end, end_cut) = match x.end.cmp(&y.end) {
        Ordering::Less => (x.end, x.end_cut),
        Ordering::Greater => (y.end, y.end_cut),
        Ordering::Equal => (x.end, x.end_cut || y.end_cut),
    };
    let dot = x.start == x.end || y.start == y.end;
    (start < end || (start == end && dot)).then_some(SampleRange {
        start,
        end,
        start_cut,
        end_cut,
    })
}
