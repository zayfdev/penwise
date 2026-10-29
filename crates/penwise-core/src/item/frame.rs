//! Frame items: a live web page (a linked project's route) shown on the page at a size preset.
//!
//! Owner: design-harness M1. Contract: [`docs/design-harness-spec.md`] §4, C1.
//!
//! A frame's local frame is the page box `[0, css_w] × [0, css_h]` with its top-left at the local origin (like
//! text and pictures), so the item transform's translation is the frame's page position and the page box is
//! `css_w × css_h` page units at scale 1. Only `route`, `preset` and the CSS size are mutable after `Add`
//! ([`crate::edit::Field::FrameRoute`], [`crate::edit::Field::FramePreset`]); the project is fixed.

use super::{Affine, Item, ItemId, Layer};
use crate::edit::{Edit, EditError};

/// Longest project id, in bytes.
pub const MAX_PROJECT_LEN: usize = 64;
/// Longest route, in bytes.
pub const MAX_ROUTE_LEN: usize = 2048;
/// Largest CSS side, in px: a frame's page box must fit the codec's ±2²⁰ px position limit.
pub const MAX_FRAME_SIDE: u32 = 1 << 20;

/// A frame's size preset. Codes are frozen (stored in the item and edit codecs; append only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FramePreset {
    /// 390 × 844 (a phone).
    Phone,
    /// 834 × 1194 (a tablet).
    Tablet,
    /// 1440 × 900 (a desktop window).
    Desktop,
    /// An explicit `css_w × css_h`.
    Custom,
}

impl FramePreset {
    /// Every preset, in code order.
    pub const ALL: [Self; 4] = [Self::Phone, Self::Tablet, Self::Desktop, Self::Custom];

    /// Frozen byte tag: `Phone = 0, Tablet = 1, Desktop = 2, Custom = 3`.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Phone => 0,
            Self::Tablet => 1,
            Self::Desktop => 2,
            Self::Custom => 3,
        }
    }

    /// Inverse of [`FramePreset::code`]; `None` for unknown bytes.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Phone),
            1 => Some(Self::Tablet),
            2 => Some(Self::Desktop),
            3 => Some(Self::Custom),
            _ => None,
        }
    }

    /// The lowercase name the clients and JSON use (`"phone"`, `"tablet"`, `"desktop"`, `"custom"`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Phone => "phone",
            Self::Tablet => "tablet",
            Self::Desktop => "desktop",
            Self::Custom => "custom",
        }
    }

    /// Inverse of [`FramePreset::as_str`]; `None` for any other text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == text)
    }

    /// The preset's fixed CSS size; `None` for [`FramePreset::Custom`].
    #[must_use]
    pub fn size(self) -> Option<(u32, u32)> {
        match self {
            Self::Phone => Some((390, 844)),
            Self::Tablet => Some((834, 1194)),
            Self::Desktop => Some((1440, 900)),
            Self::Custom => None,
        }
    }
}

/// A frame's preset together with its resolved CSS size: one LWW register ([`crate::edit::Field::FramePreset`]),
/// because switching the preset changes the size with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameSize {
    pub preset: FramePreset,
    pub css_w: u32,
    pub css_h: u32,
}

impl FrameSize {
    /// `preset`'s fixed size, or `css_w × css_h` for [`FramePreset::Custom`] (both required). A fixed preset
    /// ignores the given sizes.
    ///
    /// # Errors
    /// [`EditError::InvalidValue`] (`"frame_size"`) for a custom preset without both sizes, or a side outside
    /// `1 ..= `[`MAX_FRAME_SIDE`].
    pub fn resolve(
        preset: FramePreset,
        css_w: Option<u32>,
        css_h: Option<u32>,
    ) -> Result<Self, EditError> {
        let (css_w, css_h) = match preset.size() {
            Some(size) => size,
            None => match (css_w, css_h) {
                (Some(w), Some(h)) => (w, h),
                _ => return Err(invalid("frame_size")),
            },
        };
        let size = Self {
            preset,
            css_w,
            css_h,
        };
        size.check()?;
        Ok(size)
    }

    /// True when both sides are within `1 ..= `[`MAX_FRAME_SIDE`].
    ///
    /// # Errors
    /// [`EditError::InvalidValue`] naming `"frame_size"`.
    pub fn check(self) -> Result<(), EditError> {
        let ok = |v: u32| (1..=MAX_FRAME_SIDE).contains(&v);
        if ok(self.css_w) && ok(self.css_h) {
            Ok(())
        } else {
            Err(invalid("frame_size"))
        }
    }
}

/// A frame item: which project and route to show, at which preset size.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameItem {
    /// Linked project id (`1..=64` bytes).
    pub project: String,
    /// Route on that project, path and query (`/...`, `1..=2048` bytes).
    pub route: String,
    pub preset: FramePreset,
    /// Page box width in CSS px (`1 ..= `[`MAX_FRAME_SIDE`]).
    pub css_w: u32,
    /// Page box height in CSS px.
    pub css_h: u32,
}

impl FrameItem {
    /// A frame item; the caller is expected to build it from a [`FrameSpec`] or validated JSON.
    #[must_use]
    pub fn new(
        project: String,
        route: String,
        preset: FramePreset,
        css_w: u32,
        css_h: u32,
    ) -> Self {
        Self {
            project,
            route,
            preset,
            css_w,
            css_h,
        }
    }

    /// The preset and size as one value.
    #[must_use]
    pub fn size(&self) -> FrameSize {
        FrameSize {
            preset: self.preset,
            css_w: self.css_w,
            css_h: self.css_h,
        }
    }

    /// Page size in page units, `[css_w, css_h]`.
    #[must_use]
    pub fn page_size(&self) -> [f32; 2] {
        [side_px(self.css_w), side_px(self.css_h)]
    }

    /// The string fields' invariants: `project` non-empty and ≤ 64 bytes, `route` starts with `/` and is at most
    /// 2048 bytes, and the size is valid.
    ///
    /// # Errors
    /// [`EditError::InvalidValue`] naming `"frame_project"`, `"frame_route"` or `"frame_size"`.
    pub fn check(&self) -> Result<(), EditError> {
        if self.project.is_empty() || self.project.len() > MAX_PROJECT_LEN {
            return Err(invalid("frame_project"));
        }
        if !self.route.starts_with('/') || self.route.len() > MAX_ROUTE_LEN {
            return Err(invalid("frame_route"));
        }
        self.size().check()
    }
}

/// A frame to place on a page: its content, size and page position. The one-call constructor the server and the
/// clients use (C1).
#[derive(Debug, Clone, PartialEq)]
pub struct FrameSpec {
    pub project: String,
    pub route: String,
    pub preset: FramePreset,
    pub css_w: u32,
    pub css_h: u32,
    /// Page position of the frame's top-left corner.
    pub x: f32,
    pub y: f32,
}

impl FrameSpec {
    /// Resolves `preset` against `css_w`/`css_h` (fixed presets ignore them) and checks the content and
    /// position.
    ///
    /// # Errors
    /// [`EditError::InvalidValue`] for a bad project, route, size or non-finite position.
    pub fn new(
        project: String,
        route: String,
        preset: FramePreset,
        css_w: Option<u32>,
        css_h: Option<u32>,
        x: f32,
        y: f32,
    ) -> Result<Self, EditError> {
        let size = FrameSize::resolve(preset, css_w, css_h)?;
        let frame = FrameItem::new(project, route, size.preset, size.css_w, size.css_h);
        frame.check()?;
        if !(x.is_finite() && y.is_finite()) {
            return Err(invalid("frame_position"));
        }
        Ok(Self {
            project: frame.project,
            route: frame.route,
            preset: size.preset,
            css_w: size.css_w,
            css_h: size.css_h,
            x,
            y,
        })
    }

    /// The placed item: identity transform plus a translation to `(x, y)`, so the page box is the CSS size at
    /// scale 1.
    #[must_use]
    pub fn item(&self, id: ItemId, layer: Layer, z: i64) -> Item {
        Item {
            id,
            layer,
            z,
            transform: Affine::translate(self.x, self.y),
            kind: super::ItemKind::Frame(FrameItem::new(
                self.project.clone(),
                self.route.clone(),
                self.preset,
                self.css_w,
                self.css_h,
            )),
        }
    }

    /// The `Add` edit placing this frame.
    #[must_use]
    pub fn edit(&self, id: ItemId, layer: Layer, z: i64) -> Edit {
        Edit::Add(self.item(id, layer, z))
    }
}

/// [`FrameSpec::edit`] as a free function (the server's one-call path).
#[must_use]
pub fn add_frame(spec: &FrameSpec, id: ItemId, layer: Layer, z: i64) -> Edit {
    spec.edit(id, layer, z)
}

/// A CSS side as an `f32` page length: exact, because validated sides are ≤ 2²⁰.
#[must_use]
pub(crate) fn side_px(v: u32) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "validated ≤ 2^20, exact in f32")]
    let px = v as f32;
    px
}

fn invalid(field: &'static str) -> EditError {
    EditError::InvalidValue { field }
}

#[cfg(test)]
mod tests;
