//! Page → PNG with `penwise-core`'s `raster` feature (PLAN "Code layout": `render.rs`; MCP `get_page_png` in
//! M5 reuses it). Pixels need perceptual equality only (core-determinism.md §3).
//!
//! Owner: slice a.
//!
//! Pipeline for `render_page_png(page, replica, options)`:
//! 1. Region: `options.region`, else the fixed page rect, else (infinite) the union of live item bounds on
//!    `options.layers` inflated by `INFINITE_MARGIN_PX` (empty page → `EMPTY_INFINITE_SIZE` square at the
//!    origin). Pixmap = `ceil(region · scale)`; more than `MAX_RENDER_PIXELS` → [`RenderError::TooLarge`].
//! 2. Paper: opaque white, then the page [`Background`] pattern (`BACKGROUND_SPACING_PX`, light grey
//!    hairlines/dots) when `options.background`.
//! 3. Items: `scene.render_order(options.layers)` (bands Fill < Multiply < Normal, then `(z, id)`), each from
//!    its cached `ItemGeometry`:
//!    - stroke → `raster::outline::fill_outline` / `raster::stamps::render_stamps`;
//!    - shape → its `fill` outline (nonzero, the fill `Color`'s own alpha, `SourceOver`) then its stroke;
//!    - fill → one path of all rings of all polygons, `FillRule::EvenOdd`, `Paint` color × opacity;
//!    - text → not drawn in M3 (no font rasterizer; the text layer arrives in M6);
//!    - items with live masks (`scene.masks(id)` non-empty) render into a scratch pixmap first; each mask's
//!      page path is stroked there with width `2·radius`, round caps/joins, `BlendMode::DestinationOut`; the
//!      scratch is then composited (`SourceOver`) onto the page.
//! 4. `raster::png::encode_png`.

use penwise_core::bounds::Bounds;
use penwise_core::item::LayerSet;
use penwise_core::raster::RasterError;
use penwise_core::raster::tiny_skia::Pixmap;
use penwise_proto::{Background, Page, PageReplica};

/// Default scale (page px → image px).
pub const DEFAULT_SCALE: f32 = 1.0;
/// Accepted scale range (inclusive).
pub const SCALE_RANGE: (f32, f32) = (0.05, 4.0);
/// Largest output (pixels).
pub const MAX_RENDER_PIXELS: u64 = 40_000_000;
/// Margin around content on infinite pages, page px.
pub const INFINITE_MARGIN_PX: f32 = 64.0;
/// Side of the image for an empty infinite page, page px.
pub const EMPTY_INFINITE_SIZE: f32 = 512.0;

/// What to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
    /// Page px → pixels, within `SCALE_RANGE`.
    pub scale: f32,
    /// Page-space region; `None` = whole page / content bounds.
    pub region: Option<Bounds>,
    pub layers: LayerSet,
    /// Draw the paper pattern.
    pub background: bool,
}

/// Render failures.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// Scale outside `SCALE_RANGE`, or a non-finite/empty region.
    #[error("invalid render options")]
    InvalidOptions,
    #[error("render too large: {width}x{height}")]
    TooLarge { width: u32, height: u32 },
    #[error(transparent)]
    Raster(#[from] RasterError),
}

impl RenderOptions {
    /// `DEFAULT_SCALE`, whole page, `LayerSet::ALL`, background on.
    #[must_use]
    pub fn whole_page() -> Self {
        todo!()
    }
}

/// Renders `replica` (the page's materialized scene) as PNG bytes (module pipeline). CPU-heavy: call from
/// `spawn_blocking`.
///
/// # Errors
/// [`RenderError`].
pub fn render_page_png(
    page: &Page,
    replica: &PageReplica,
    options: &RenderOptions,
) -> Result<Vec<u8>, RenderError> {
    todo!()
}

/// Paper pattern for `background` over `region` at `scale` (step 2), exposed for golden tests.
///
/// # Errors
/// [`RenderError::Raster`].
pub fn draw_background(
    pixmap: &mut Pixmap,
    background: Background,
    region: &Bounds,
    scale: f32,
) -> Result<(), RenderError> {
    todo!()
}
