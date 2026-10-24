//! Shape item data (PLAN "Brushes & tools": shapes). Geometry and handle math live in [`crate::shape`].
//!
//! Owner: slice d.

use crate::brush::{BrushParams, Color};

/// Shape outline kind, in the local frame described on [`ShapeItem`].
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeKind {
    Rect,
    /// Corner radius in local px, clamped at build time to `min(w, h) / 2`.
    RoundedRect {
        radius: f32,
    },
    /// Ellipse inscribed in the box (4 cubic Béziers, `k = 0.5522847498`).
    Ellipse,
    /// Segment from `(−w/2, 0)` to `(w/2, 0)`; `size[1]` is ignored (kept 0).
    Line,
    /// Line with arrowhead(s); same local segment as `Line`.
    Arrow {
        head: ArrowHead,
    },
    /// Closed polygon, vertices in the unit box `[−0.5, 0.5]²` scaled by `size` (regular n-gons are generated
    /// once at creation by [`crate::shape::regular_polygon`]; afterwards the points are data). ≥ 3 points.
    Polygon {
        points: Vec<[f32; 2]>,
    },
}

/// Which ends of an arrow get a head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArrowHead {
    /// Head at `(w/2, 0)` only.
    End,
    /// Heads at both ends.
    Both,
}

impl ArrowHead {
    /// Frozen byte tag: `End = 0, Both = 1`.
    #[must_use]
    pub fn code(self) -> u8 {
        todo!()
    }

    /// Inverse of [`ArrowHead::code`].
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        todo!()
    }
}

impl ShapeKind {
    /// Frozen byte tag: `Rect = 0, RoundedRect = 1, Ellipse = 2, Line = 3, Arrow = 4, Polygon = 5`.
    #[must_use]
    pub fn code(&self) -> u8 {
        todo!()
    }

    /// True for `Line` and `Arrow` (2 endpoint handles instead of the 8 + rotate box handles; no fill).
    #[must_use]
    pub fn is_linear(&self) -> bool {
        todo!()
    }
}

/// A shape. Local frame: box `[−w/2, w/2] × [−h/2, h/2]` centred on the local origin (`size = [w, h]`), so
/// the item transform's translation is the shape centre and resizing edits `size` while rotation/shear stay
/// in the transform. The outline is flattened in local space, mapped through the transform, then stroked
/// in page space with `brush` at its own width (stroke width applied after transform: uniform under any
/// affine).
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeItem {
    /// Mutable: [`crate::edit::Field::ShapeKind`].
    pub kind: ShapeKind,
    /// `[w, h]` local px, `≥ 0`. Mutable: [`crate::edit::Field::ShapeSize`].
    pub size: [f32; 2],
    /// Outline brush (outline family only). Mutable: [`crate::edit::Field::Brush`].
    pub brush: BrushParams,
    /// Interior fill for closed kinds (`None` = unfilled; ignored for linear kinds). Mutable:
    /// [`crate::edit::Field::ShapeFill`].
    pub fill: Option<Color>,
}
