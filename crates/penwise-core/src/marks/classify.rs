//! Owner: design-harness M3 (core). Geometric guesses for what a mark means (spec §5, §7.2).
//!
//! Pure geometry, no ML: the owner's vocabulary is small and the strokes are simple. Every guess carries a
//! calibrated confidence, and anything that fits nothing returns [`MarkKind::Free`] (handwriting, sketches,
//! scribbles), so the agent falls back to the image and the OCR text. Recognition is a hint, never a rule.
//!
//! Coordinates are binding-local — the same space as the strokes' samples. Thresholds are in CSS px; a mark is
//! normally tens to hundreds of px across, so the absolute limits (a ≥24 px shaft, a ≥24 px horizontal line)
//! are deliberately low.
//!
//! A mark's horizontal line is ambiguous between *strike* and *underline*. The caller passes the element rect
//! (in binding-local space) when it knows it: the line's mean y in the lower 10% of that rect is an underline,
//! inside it is a strike. Without a rect, a straight horizontal line is a strike, because the fixed vocabulary
//! has no plain-underline meaning while it does have "line through text".
//!
//! Detectors live next door: [`super::classify_shapes`] (loops, crosses, boxes, waves, lines) and
//! [`super::classify_arrow`] (shaft plus head V). This file owns the vocabulary, the decision order and the
//! shared stroke geometry.

use crate::Bounds;
use crate::item::StrokeItem;
use serde::{Deserialize, Serialize};

use super::classify_arrow::arrow;
use super::classify_shapes::{
    closed_shape, horizontal_line, two_stroke_box, two_stroke_cross, wavy,
};

/// Total ink length below which nothing is guessed (a stray dot or tap).
const MIN_MARK_LENGTH_PX: f32 = 10.0;
/// Shortest shaft an arrow may have.
pub(crate) const MIN_SHAFT_LEN_PX: f32 = 24.0;
/// A closed loop may leave this fraction of its bbox diagonal open (sloppy circles do not close).
pub(crate) const CLOSE_RATIO: f32 = 0.35;
/// Cosine of the widest interior angle that still counts as a sharp corner: `−0.342` ≈ 110°. A rectangle's
/// 90° corners count; the ~135° vertices of an eight-chord circle do not.
pub(crate) const SHARP_CORNER_COS: f32 = -0.342;

/// The owner's mark vocabulary (spec §5). Serialized with the names the agent sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    /// Loop around something: look here.
    Circle,
    /// Move this there; [`MarkShape::from`] is the tail, [`MarkShape::to`] the head tip.
    Arrow,
    /// X: remove.
    Cross,
    /// Squiggle under something: feels off.
    Wavy,
    /// Rectangle in empty space: new thing here.
    BoxNew,
    /// Straight line through text: change this text.
    Strike,
    /// Straight line under text: underline.
    Underline,
    /// Everything else: handwriting, a sketch, a scribble; read it from the images and OCR.
    Free,
}

/// One mark's kind guess: the vocabulary entry, a `0.0..=1.0` confidence, and — for arrows only — the
/// binding-local tail and head tip.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MarkShape {
    /// The guessed vocabulary entry.
    pub guess: MarkKind,
    /// How sure the geometry is, `0.0..=1.0`.
    pub confidence: f32,
    /// Arrow tail: the thing to move. Binding-local. `None` for every other kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<[f32; 2]>,
    /// Arrow head tip: where it should go. Binding-local. `None` for every other kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<[f32; 2]>,
}

impl MarkShape {
    /// A [`MarkKind::Free`] guess with `confidence` (low when the geometry was unsure).
    #[must_use]
    pub fn free(confidence: f32) -> Self {
        Self::guessed(MarkKind::Free, confidence)
    }

    pub(crate) fn guessed(guess: MarkKind, confidence: f32) -> Self {
        Self {
            guess,
            confidence: confidence.clamp(0.0, 1.0),
            from: None,
            to: None,
        }
    }

    pub(crate) fn arrow(confidence: f32, from: [f32; 2], to: [f32; 2]) -> Self {
        Self {
            from: Some(from),
            to: Some(to),
            ..Self::guessed(MarkKind::Arrow, confidence)
        }
    }
}

/// Classify the strokes of one mark, without the bound element's rect.
///
/// See [`classify_with_rect`].
#[must_use]
pub fn classify(strokes: &[&StrokeItem]) -> MarkShape {
    classify_with_rect(strokes, None)
}

/// Classify the strokes of one mark drawn together on one frame.
///
/// `element_rect` is the bound element's box `[x, y, w, h]` in the strokes' own (binding-local) space, when
/// the caller knows it; it only decides strike vs underline. Use `Some([0.0, 0.0, w, h])` for a bound mark.
#[must_use]
pub fn classify_with_rect(strokes: &[&StrokeItem], element_rect: Option<[f32; 4]>) -> MarkShape {
    let polys: Vec<Poly> = strokes.iter().filter_map(|s| Poly::new(s)).collect();
    let total: f32 = polys.iter().map(|p| p.len).sum();
    if polys.is_empty() || total < MIN_MARK_LENGTH_PX {
        return MarkShape::free(0.1);
    }
    if polys.len() == 1
        && let Some(shape) = closed_shape(&polys[0])
    {
        return shape;
    }
    if let [a, b] = polys.as_slice() {
        if let Some(shape) = two_stroke_cross(a, b) {
            return shape;
        }
        if let Some(shape) = two_stroke_box(a, b) {
            return shape;
        }
    }
    // A sine must not be read as a shaft with a barb at a crest, so it runs before the arrow.
    if polys.len() == 1
        && let Some(confidence) = wavy(&polys[0])
    {
        return MarkShape::guessed(MarkKind::Wavy, confidence);
    }
    if let Some(shape) = arrow(&polys) {
        return shape;
    }
    if polys.len() == 1
        && let Some(shape) = horizontal_line(&polys[0], element_rect)
    {
        return shape;
    }
    let confidence = if polys.len() >= 3 || total >= 120.0 {
        0.65
    } else {
        0.4
    };
    MarkShape::free(confidence)
}

/// One stroke's points (consecutive duplicates dropped), its path length and its box.
pub(crate) struct Poly {
    pub(crate) pts: Vec<[f32; 2]>,
    pub(crate) len: f32,
    pub(crate) bbox: Bounds,
}

impl Poly {
    fn new(stroke: &StrokeItem) -> Option<Self> {
        let mut pts: Vec<[f32; 2]> = Vec::with_capacity(stroke.samples.len());
        for s in &stroke.samples {
            let p = [s.x, s.y];
            if p[0].is_finite()
                && p[1].is_finite()
                && pts.last().is_none_or(|q| dist(*q, p) >= 0.25)
            {
                pts.push(p);
            }
        }
        if pts.len() < 2 {
            return None;
        }
        let mut bbox = Bounds::EMPTY;
        let mut len = 0.0;
        for (i, p) in pts.iter().enumerate() {
            bbox.include_point(p[0], p[1]);
            if let Some(q) = i.checked_sub(1).map(|j| pts[j]) {
                len += dist(q, *p);
            }
        }
        (len >= 1.0).then_some(Self { pts, len, bbox })
    }

    pub(crate) fn first(&self) -> [f32; 2] {
        self.pts[0]
    }

    pub(crate) fn last(&self) -> [f32; 2] {
        self.pts[self.pts.len() - 1]
    }

    pub(crate) fn diag(&self) -> f32 {
        libm::hypotf(self.bbox.width(), self.bbox.height())
    }

    /// Endpoint distance over path length: 1 for a perfectly straight stroke.
    pub(crate) fn straightness(&self) -> f32 {
        dist(self.first(), self.last()) / self.len
    }
}

pub(crate) fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

pub(crate) fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

pub(crate) fn cross2(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

pub(crate) fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    libm::hypotf(a[0] - b[0], a[1] - b[1])
}

/// Cosine of the angle at `b` between `b→a` and `b→c`.
pub(crate) fn cosine(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    let u = sub(a, b);
    let v = sub(c, b);
    let (lu, lv) = (libm::hypotf(u[0], u[1]), libm::hypotf(v[0], v[1]));
    if lu <= 0.0 || lv <= 0.0 {
        return 1.0;
    }
    (dot(u, v) / (lu * lv)).clamp(-1.0, 1.0)
}

/// Distance from `p` to the segment `a..b`.
fn dist_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    if l2 <= f32::EPSILON {
        return dist(p, a);
    }
    let t = (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0);
    dist(p, [a[0] + ab[0] * t, a[1] + ab[1] * t])
}

/// Ramer–Douglas–Peucker: indices of the points kept at `eps` deviation.
pub(crate) fn simplify_indices(points: &[[f32; 2]], eps: f32) -> Vec<usize> {
    let n = points.len();
    if n <= 2 {
        return (0..n).collect();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0_usize, n - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (mut best, mut best_d) = (a, -1.0_f32);
        for (i, p) in points.iter().enumerate().take(b).skip(a + 1) {
            let d = dist_to_segment(*p, points[a], points[b]);
            if d > best_d {
                best_d = d;
                best = i;
            }
        }
        if best_d > eps {
            keep[best] = true;
            stack.push((a, best));
            stack.push((best, b));
        }
    }
    (0..n).filter(|&i| keep[i]).collect()
}

/// Twice the signed area of the polygon through all points (closed implicitly).
pub(crate) fn shoelace(points: &[[f32; 2]]) -> f32 {
    let mut sum = 0.0;
    for (i, a) in points.iter().enumerate() {
        let b = points[(i + 1) % points.len()];
        sum += a[0] * b[1] - b[0] * a[1];
    }
    sum / 2.0
}

pub(crate) fn sharp_corners(verts: &[[f32; 2]], min_arm: f32) -> usize {
    verts
        .windows(3)
        .filter(|w| {
            dist(w[0], w[1]) >= min_arm
                && dist(w[1], w[2]) >= min_arm
                && cosine(w[0], w[1], w[2]) > SHARP_CORNER_COS
        })
        .count()
}

#[cfg(test)]
mod tests;
