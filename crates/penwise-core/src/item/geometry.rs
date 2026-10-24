//! Derived page-space geometry of an item (cached by [`crate::item::Scene`]) and paint order.
//!
//! Owner: slice a (dispatch); per-kind builders belong to their slices
//! ([`crate::item::StrokeItem::build_geometry`]/[`crate::item::StrokeItem::centreline`] b, [`crate::shape::shape_geometry`] d).

use super::Item;
use super::affine::Affine;
use super::fill::Polygon;
use super::id::ItemId;
use super::paint::Paint;
use super::stroke::{Centreline, SampleRange};
use crate::bounds::Bounds;
use crate::geometry::StrokeGeometry;
use crate::shape::ShapeGeometry;

/// Inputs from the scene that an item's geometry depends on besides the item itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeriveContext<'a> {
    /// Effective visible ranges of a stroke (origin space; [`crate::item::Scene::visible_ranges`]). Ignored
    /// for other kinds.
    pub visible: &'a [SampleRange],
    /// Transform of an erase item's target (`None`: target unknown or deleted → empty mask geometry).
    pub target_transform: Option<&'a Affine>,
}

/// Page-space geometry of one item. Masks are **not** applied here (they are separate erase items; renderers
/// and hit tests combine them, see [`crate::edit::hit`]).
#[derive(Debug, Clone, PartialEq)]
pub enum ItemGeometry {
    /// Render geometry plus the visible centreline used by hit test, lasso and erasers.
    Stroke {
        geometry: StrokeGeometry,
        centreline: Centreline,
    },
    /// Page-space mask (see [`crate::item::EraseItem::page_mask`]).
    Erase {
        path: Vec<[f32; 2]>,
        radius: f32,
    },
    Shape(ShapeGeometry),
    /// Polygons mapped to page space.
    Fill {
        polygons: Vec<Polygon>,
        paint: Paint,
    },
    /// The text box's 4 page-space corners (top-left, top-right, bottom-right, bottom-left in local terms).
    Text {
        corners: [[f32; 2]; 4],
        paint: Paint,
    },
}

impl ItemGeometry {
    /// Conservative page bounds: stroke outline/dab bounds (dabs inflated by their `radius_x`) ∪ centreline
    /// points inflated by half-width; erase path inflated by radius; shape stroke ∪ fill bounds; polygon or
    /// corner bounds. Empty geometry → [`Bounds::EMPTY`] (never indexed).
    #[must_use]
    pub fn bounds(&self) -> Bounds {
        todo!()
    }
}

/// Composite band, lowest first. Renderers draw bands in this order, each band in `(z, id)` order, so fills sit
/// under all ink and multiply strokes (highlighter) sit under normal ink (M1 contract), whatever their z.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RenderBand {
    /// `ItemKind::Fill`.
    Fill,
    /// Strokes and shapes whose brush blend is `BlendMode::Multiply`.
    Multiply,
    /// Everything else (normal strokes, shapes, text). Erase items take their target's band.
    Normal,
}

/// Total paint order of live items: `(band, z, id)` ascending = bottom → top. "Topmost hit" means the largest
/// key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RenderKey {
    pub band: RenderBand,
    pub z: i64,
    pub id: ItemId,
}

impl Item {
    /// Band of this item (an erase item reports `Normal`; the scene reports its target's band instead).
    #[must_use]
    pub fn band(&self) -> RenderBand {
        todo!()
    }

    /// Derives page-space geometry: stroke → [`crate::item::StrokeItem::build_geometry`] + `centreline` over
    /// `ctx.visible`; erase → [`crate::item::EraseItem::page_mask`] under `ctx.target_transform` (empty when
    /// `None`); shape → [`crate::shape::shape_geometry`]; fill → [`Polygon::mapped`] each polygon; text → box
    /// corners mapped through the transform. Pure function of `(self, ctx)`.
    #[must_use]
    pub fn derive_geometry(&self, ctx: &DeriveContext<'_>) -> ItemGeometry {
        todo!()
    }
}
