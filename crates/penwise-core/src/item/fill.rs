//! Fill-bucket items: vector polygons with holes (PLAN "Data model": fill).
//!
//! Owner: slice c.

use super::paint::Paint;
use crate::bounds::Bounds;

/// One polygon with holes, local frame. Canonical orientation (as produced by `fill::clean::normalize`, i.e.
/// `i_overlay`'s counter-clockwise outer / clockwise holes in y-up maths) is defined numerically: the
/// shoelace sum `Σ (x_i·y_{i+1} − x_{i+1}·y_i)` is `> 0` for `outer` and `< 0` for every hole (on screen,
/// y down, outer rings therefore run clockwise). Rings are implicitly closed (first point not repeated),
/// ≥ 3 points, on the 1/16 px grid.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Polygon {
    pub outer: Vec<[f32; 2]>,
    pub holes: Vec<Vec<[f32; 2]>>,
}

impl Polygon {
    /// Bounds of the outer ring (holes lie inside it).
    #[must_use]
    pub fn bounds(&self) -> Bounds {
        let mut b = Bounds::EMPTY;
        for p in &self.outer {
            b.include_point(p[0], p[1]);
        }
        b
    }

    /// Filled area: `|shoelace(outer)| − Σ |shoelace(hole)|`, accumulated in `f64`.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "one rounding of the f64 area to f32"
    )]
    pub fn area(&self) -> f32 {
        let holes: f64 = self.holes.iter().map(|h| shoelace(h).abs()).sum();
        ((shoelace(&self.outer).abs() - holes) * 0.5) as f32
    }

    /// Copy with every ring mapped through `transform` (orientation flips when `det < 0`; callers that need
    /// canonical orientation run `fill::clean::normalize` afterwards).
    #[must_use]
    pub fn mapped(&self, transform: &super::Affine) -> Polygon {
        let map = |ring: &[[f32; 2]]| -> Vec<[f32; 2]> {
            ring.iter().map(|&p| transform.apply(p)).collect()
        };
        Polygon {
            outer: map(&self.outer),
            holes: self.holes.iter().map(|h| map(h)).collect(),
        }
    }
}

/// Shoelace sum `Σ (x_i·y_{i+1} − x_{i+1}·y_i)` of an implicitly closed ring (twice the signed area), `f64`.
fn shoelace(ring: &[[f32; 2]]) -> f64 {
    let Some(&last) = ring.last() else {
        return 0.0;
    };
    let mut prev = last;
    let mut sum = 0.0;
    for &p in ring {
        let (x0, y0) = (f64::from(prev[0]), f64::from(prev[1]));
        let (x1, y1) = (f64::from(p[0]), f64::from(p[1]));
        sum += x0 * y1 - x1 * y0;
        prev = p;
    }
    sum
}

/// A filled region. Rendered in [`crate::item::RenderBand::Fill`], under every stroke and shape.
#[derive(Debug, Clone, PartialEq)]
pub struct FillItem {
    /// Disjoint polygons, nonzero rule (mutable as a whole: [`crate::edit::Field::Polygons`]).
    pub polygons: Vec<Polygon>,
    /// Color + opacity (mutable: [`crate::edit::Field::Paint`], recolor by tapping the fill).
    pub paint: Paint,
}
