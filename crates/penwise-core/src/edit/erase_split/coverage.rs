//! Coverage evaluation of one stroke's page centreline against one eraser (rule in the parent module doc).
//!
//! Owner: slice b.

use super::CUT_BISECTION_STEPS;
use crate::edit::erase_stroke::{EraserIndex, query_box};
use crate::edit::proximity::point_segment_dist2;
use crate::item::{CentrePoint, SampleParam, SampleRange, StrokeItem};

/// One stroke's stored-sample centre points against an indexed eraser.
pub(super) struct Coverage<'a> {
    pub(super) stroke: &'a StrokeItem,
    /// One centre point per stored sample.
    pub(super) points: &'a [CentrePoint],
    pub(super) index: &'a EraserIndex,
    pub(super) radius: f32,
}

impl Coverage<'_> {
    /// Origin point `u` is erased: `dist(c(u), E) < r_e + h(u)` (only eraser segments near `c(u)` are
    /// tested; any one closer than the tolerance decides, which equals the minimum-distance test).
    fn covered(&self, u: SampleParam) -> bool {
        let c = self.stroke.centre_at(self.points, u);
        let p = [c.x, c.y];
        let tol = self.radius + c.half_width;
        self.index.any(&query_box(p, p, tol), |e0, e1| {
            point_segment_dist2(p, e0, e1).0.sqrt() < tol
        })
    }

    /// [`super::bisect_cut`] on prepared points. Returns `a` when `a ≥ b` (no segment to bisect).
    pub(super) fn bisect(&self, a: SampleParam, b: SampleParam) -> SampleParam {
        if a.0 >= b.0 {
            return a;
        }
        let a_covered = self.covered(a);
        if a_covered == self.covered(b) {
            return a;
        }
        let (mut lo, mut hi) = (a.0, b.0);
        for _ in 0..CUT_BISECTION_STEPS {
            let mid = lo + (hi - lo) / 2;
            if mid == lo {
                break;
            }
            if self.covered(SampleParam(mid)) == a_covered {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        SampleParam(if a_covered { hi } else { lo })
    }

    /// Splits one clipped range (point params `params`) into uncovered runs appended to `kept`. Returns true
    /// when some point of the range is covered.
    pub(super) fn split_range(
        &self,
        range: &SampleRange,
        params: &[SampleParam],
        kept: &mut Vec<SampleRange>,
    ) -> bool {
        let mut run: Option<(SampleParam, bool)> = None;
        let mut prev: Option<(SampleParam, bool)> = None;
        let mut touched = false;
        let mut steps = Vec::new();
        for &u in params {
            steps.clear();
            if let Some((p, _)) = prev {
                self.substeps(p, u, &mut steps);
            }
            steps.push(u);
            for &v in &steps {
                let cov = self.covered(v);
                touched |= cov;
                match prev {
                    None if !cov => run = Some((range.start, range.start_cut)),
                    Some((p, p_cov)) if p_cov != cov => {
                        let cut = self.bisect(p, v);
                        if cov {
                            close_run(run.take(), cut, true, kept);
                        } else {
                            run = Some((cut, true));
                        }
                    }
                    _ => {}
                }
                prev = Some((v, cov));
            }
        }
        if range.start == range.end {
            kept.extend(run.map(|(start, start_cut)| SampleRange {
                start,
                start_cut,
                ..*range
            }));
        } else {
            close_run(run, range.end, range.end_cut, kept);
        }
        touched
    }

    /// Intermediate params between `a` and `b` (one segment) so consecutive test points are at most
    /// `radius / 2` apart on the page.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "step count is clamped to 1..=4096 before the cast"
    )]
    fn substeps(&self, a: SampleParam, b: SampleParam, out: &mut Vec<SampleParam>) {
        let ca = self.stroke.centre_at(self.points, a);
        let cb = self.stroke.centre_at(self.points, b);
        let (dx, dy) = (cb.x - ca.x, cb.y - ca.y);
        let len = (dx * dx + dy * dy).sqrt();
        let n = (len / (self.radius * 0.5)).ceil();
        if n.is_nan() || n <= 1.0 {
            return;
        }
        let span = b.0.saturating_sub(a.0);
        let n = if n < 4096.0 { n as u32 } else { 4096 };
        let n = if n > span { span } else { n };
        for j in 1..n {
            let off = u64::from(span) * u64::from(j) / u64::from(n);
            out.push(SampleParam(a.0 + u32::try_from(off).unwrap_or(0)));
        }
    }
}

/// Pushes the uncovered run that started at `run` and ends at `end` (cut runs must have positive length).
fn close_run(
    run: Option<(SampleParam, bool)>,
    end: SampleParam,
    end_cut: bool,
    kept: &mut Vec<SampleRange>,
) {
    if let Some((start, start_cut)) = run
        && start < end
    {
        kept.push(SampleRange {
            start,
            end,
            start_cut,
            end_cut,
        });
    }
}
