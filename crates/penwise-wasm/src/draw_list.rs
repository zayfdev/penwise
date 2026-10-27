//! Flat, typed-array draw list: core geometry of a page (or of its live strokes) in paint order, ready for
//! `Canvas2D`. Layout is part of the contract (mirrored as constants in `web/src/viewer.ts`).
//!
//! Arrays (all page px, y down):
//! - `points` `Float32Array`: `x, y` pairs.
//! - `runs` `Uint32Array`: exclusive end (in points) of each run; run `k` = points `runs[k-1] .. runs[k]`
//!   (first starts at 0). Closed contours for paths, open polylines for masks (a one-point mask path is
//!   written twice so round caps still draw its dot), 4 corners (tl, tr, br, bl) for text and images.
//! - `dabs` `Float32Array`, stride [`DAB_STRIDE`] = 9: `x, y, radius_x, radius_y, angle, opacity, hardness,
//!   grain_strength, grain_level` (core `Dab` field order).
//! - `styles` `Float32Array`, stride [`STYLE_STRIDE`] = 8: `r, g, b, a` (straight sRGB `0..=1`), `opacity`
//!   (stroke-level cap / paint opacity), `blend` (0 normal, 1 multiply), `grain_scale` (0 = no grain),
//!   `grain_softness`.
//! - `commands` `Uint32Array`, stride [`COMMAND_STRIDE`] = 5: `kind` ([`DrawKind`]), `first`, `count`,
//!   `style`, `aux`.
//! - `mask_widths` `Float32Array`: stroke width (`2·radius`) per mask, indexed by a `Mask` command's `aux`.
//! - `texts` `string[]` + `text_sizes` `Float32Array` (local font size), indexed by a `Text` command's `aux`.
//! - `images` `string[]`: 64-hex SHA-256 asset hashes, indexed by an `Image` command's `aux`.
//!
//! Commands in order (bands Fill < Multiply < Normal, then `(z, id)`; core `Scene::render_order`):
//! - `PathNonZero` / `PathEvenOdd`: fill runs `first .. first+count` as one path (outline strokes, shape
//!   fills and shape strokes nonzero; fill items even-odd, outer ring then holes) with `styles[style]` at
//!   alpha `a · opacity`.
//! - `Dabs`: dabs `first .. first+count` into a coverage canvas (each an elliptical radial falloff, `S ← S +
//!   a(1−S)` = source-over of white-alpha dabs), multiplied by the paper grain pattern when `grain_scale > 0`,
//!   then tinted with the style color and composited at `a · opacity` with the style blend.
//! - `GroupBegin` … `GroupEnd`: an item with erase masks. Draw the enclosed commands into an offscreen
//!   canvas with `source-over`, apply its `Mask` commands, then composite the offscreen with
//!   `styles[style].blend` (both group commands carry the same style; Multiply for a multiply item).
//! - `Mask`: stroke runs `first .. first+count` (open polylines) with width `mask_widths[aux]`, round caps and
//!   joins, `destination-out`.
//! - `Text`: run `first` holds the box corners; draw `texts[aux]` at local font size `text_sizes[aux]`
//!   wrapped to the box, transformed by the corner frame, color `styles[style]`.
//! - `Image`: run `first` holds the box corners; draw the picture `images[aux]` into that parallelogram
//!   (bilinear, normal blend).
//!
//! Erase items are never drawn on their own (only as masks inside their target's group).

use wasm_bindgen::prelude::wasm_bindgen;

mod build;

/// Floats per dab.
pub const DAB_STRIDE: usize = 9;
/// Floats per style.
pub const STYLE_STRIDE: usize = 8;
/// `u32`s per command.
pub const COMMAND_STRIDE: usize = 5;

/// Command kinds (JS enum; codes frozen).
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawKind {
    PathNonZero = 0,
    PathEvenOdd = 1,
    Dabs = 2,
    GroupBegin = 3,
    Mask = 4,
    GroupEnd = 5,
    Text = 6,
    Image = 7,
}

/// One draw list (owned buffers; getters copy into fresh typed arrays).
#[wasm_bindgen]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrawList {
    points: Vec<f32>,
    runs: Vec<u32>,
    dabs: Vec<f32>,
    styles: Vec<f32>,
    commands: Vec<u32>,
    mask_widths: Vec<f32>,
    texts: Vec<String>,
    text_sizes: Vec<f32>,
    images: Vec<String>,
}

#[wasm_bindgen]
impl DrawList {
    /// `x, y` pairs.
    #[must_use]
    pub fn points(&self) -> Vec<f32> {
        self.points.clone()
    }

    /// Run end indices.
    #[must_use]
    pub fn runs(&self) -> Vec<u32> {
        self.runs.clone()
    }

    /// Dab records (`DAB_STRIDE`).
    #[must_use]
    pub fn dabs(&self) -> Vec<f32> {
        self.dabs.clone()
    }

    /// Style records (`STYLE_STRIDE`).
    #[must_use]
    pub fn styles(&self) -> Vec<f32> {
        self.styles.clone()
    }

    /// Command records (`COMMAND_STRIDE`).
    #[must_use]
    pub fn commands(&self) -> Vec<u32> {
        self.commands.clone()
    }

    /// Mask stroke widths.
    #[must_use]
    pub fn mask_widths(&self) -> Vec<f32> {
        self.mask_widths.clone()
    }

    /// Text contents.
    #[must_use]
    pub fn texts(&self) -> Vec<String> {
        self.texts.clone()
    }

    /// Text font sizes (local px).
    #[must_use]
    pub fn text_sizes(&self) -> Vec<f32> {
        self.text_sizes.clone()
    }

    /// Image asset hashes (64 lowercase hex chars).
    #[must_use]
    pub fn images(&self) -> Vec<String> {
        self.images.clone()
    }

    /// True when there is nothing to draw.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}
