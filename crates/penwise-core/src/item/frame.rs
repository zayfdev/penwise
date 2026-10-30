//! Frame items: a live web page (a linked project's route) shown on the page at a size preset.
//!
//! Owner: design-harness M1 (item), M4 (patch, label, group, chosen + the edit helpers in [`edits`]).
//! Contract: [`docs/design-harness-spec.md`] §4, C1/P2.
//!
//! A frame's local frame is the page box `[0, css_w] × [0, css_h]` with its top-left at the local origin (like
//! text and pictures), so the item transform's translation is the frame's page position and the page box is
//! `css_w × css_h` page units at scale 1. Only `route`, `preset` and the CSS size are mutable after `Add`
//! ([`crate::edit::Field::FrameRoute`], [`crate::edit::Field::FramePreset`]), plus the ghost/variant fields
//! ([`crate::edit::Field::FramePatch`], `FrameLabel`, `FrameGroup`, `FrameChosen`) and the full-length fields
//! (`FrameExtent`, `FrameDocHeight`); the project is fixed. For a [`FrameExtent::Full`] frame the box is
//! taller than the page box: see [`FrameItem::box_size`].

use crate::edit::EditError;

mod edits;
mod item;

pub use edits::{
    VariantSpec, group_frames, set_chosen, set_doc_height, set_extent, set_patch, variant_frames,
};
pub(crate) use item::full_box_height;
pub use item::{FrameItem, FrameSpec, add_frame};

/// Longest project id, in bytes.
pub const MAX_PROJECT_LEN: usize = 64;
/// Longest route, in bytes.
pub const MAX_ROUTE_LEN: usize = 2048;
/// Largest CSS side, in px: a frame's page box must fit the codec's ±2²⁰ px position limit.
pub const MAX_FRAME_SIDE: u32 = 1 << 20;
/// Longest patch JSON text, in bytes (design harness P1).
pub const MAX_PATCH_LEN: usize = 64 * 1024;
/// Longest frame label, in bytes.
pub const MAX_LABEL_LEN: usize = 80;
/// Longest variant group id, in bytes.
pub const MAX_GROUP_LEN: usize = 64;
/// Gap between a base frame and its first variant, and between variants, in page px.
pub const VARIANT_GAP_PX: f32 = 40.0;
/// Most variants one group may hold (design harness P4).
pub const MAX_VARIANTS: usize = 4;

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

/// How tall a frame's box is on the board. Codes are frozen (stored in the item and edit codecs; append only).
///
/// The live view is always exactly `css_w × css_h` (the preset); the extent only decides whether the box
/// around it grows to the whole document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FrameExtent {
    /// The box is the preset viewport (design harness v1); the live view scrolls inside it.
    Viewport,
    /// The box is the whole document, `max(css_h, doc_h)` tall; the live view is a sliding window over it.
    Full,
}

impl FrameExtent {
    /// Frozen byte tag: `Viewport = 0, Full = 1`.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Viewport => 0,
            Self::Full => 1,
        }
    }

    /// Inverse of [`FrameExtent::code`]; `None` for unknown bytes.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Viewport),
            1 => Some(Self::Full),
            _ => None,
        }
    }

    /// The lowercase name the clients and JSON use (`"viewport"`, `"full"`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewport => "viewport",
            Self::Full => "full",
        }
    }

    /// Inverse of [`FrameExtent::as_str`]; `None` for any other text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "viewport" => Some(Self::Viewport),
            "full" => Some(Self::Full),
            _ => None,
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
