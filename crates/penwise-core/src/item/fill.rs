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
        todo!()
    }

    /// Filled area: `|shoelace(outer)| − Σ |shoelace(hole)|`, accumulated in `f64`.
    #[must_use]
    pub fn area(&self) -> f32 {
        todo!()
    }

    /// Copy with every ring mapped through `transform` (orientation flips when `det < 0`; callers that need
    /// canonical orientation run `fill::clean::normalize` afterwards).
    #[must_use]
    pub fn mapped(&self, transform: &super::Affine) -> Polygon {
        todo!()
    }
}

/// A filled region. Rendered in [`crate::item::RenderBand::Fill`], under every stroke and shape.
#[derive(Debug, Clone, PartialEq)]
pub struct FillItem {
    /// Disjoint polygons, nonzero rule (mutable as a whole: [`crate::edit::Field::Polygons`]).
    pub polygons: Vec<Polygon>,
    /// Color + opacity (mutable: [`crate::edit::Field::Paint`], recolor by tapping the fill).
    pub paint: Paint,
}
