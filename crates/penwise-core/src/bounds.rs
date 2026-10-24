//! Axis-aligned bounding boxes in page px (y down). The M2 spatial index builds on this type.
//!
//! Owner: contract.

use crate::float::{max, min};

/// Axis-aligned box `[min_x, max_x] × [min_y, max_y]` in page px. Empty when `min_x > max_x` or
/// `min_y > max_y`; [`Bounds::EMPTY`] is the identity for [`Bounds::union`] and [`Bounds::include_point`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl Bounds {
    /// The empty box (inverted infinite extents); contains nothing, unions to the other operand.
    pub const EMPTY: Self = Self {
        min_x: f32::INFINITY,
        min_y: f32::INFINITY,
        max_x: f32::NEG_INFINITY,
        max_y: f32::NEG_INFINITY,
    };

    /// Box covering exactly one point.
    #[must_use]
    pub const fn from_point(x: f32, y: f32) -> Self {
        Self {
            min_x: x,
            min_y: y,
            max_x: x,
            max_y: y,
        }
    }

    /// True when the box contains no point.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !(self.min_x <= self.max_x && self.min_y <= self.max_y)
    }

    /// Width (`0.0` when empty).
    #[must_use]
    pub fn width(&self) -> f32 {
        if self.is_empty() {
            0.0
        } else {
            self.max_x - self.min_x
        }
    }

    /// Height (`0.0` when empty).
    #[must_use]
    pub fn height(&self) -> f32 {
        if self.is_empty() {
            0.0
        } else {
            self.max_y - self.min_y
        }
    }

    /// Grows the box to include `(x, y)`. NaN coordinates are ignored.
    pub fn include_point(&mut self, x: f32, y: f32) {
        self.min_x = min(self.min_x, x);
        self.min_y = min(self.min_y, y);
        self.max_x = max(self.max_x, x);
        self.max_y = max(self.max_y, y);
    }

    /// Smallest box containing both; empty operands are ignored.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        if other.is_empty() {
            return *self;
        }
        if self.is_empty() {
            return *other;
        }
        Self {
            min_x: min(self.min_x, other.min_x),
            min_y: min(self.min_y, other.min_y),
            max_x: max(self.max_x, other.max_x),
            max_y: max(self.max_y, other.max_y),
        }
    }

    /// Box grown by `d` on every side (`d < 0` shrinks; may become empty). Empty stays empty.
    #[must_use]
    pub fn inflate(&self, d: f32) -> Self {
        if self.is_empty() {
            return *self;
        }
        Self {
            min_x: self.min_x - d,
            min_y: self.min_y - d,
            max_x: self.max_x + d,
            max_y: self.max_y + d,
        }
    }

    /// True when the closed boxes overlap (touching edges count). Empty never intersects.
    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && self.min_x <= other.max_x
            && other.min_x <= self.max_x
            && self.min_y <= other.max_y
            && other.min_y <= self.max_y
    }

    /// True when `(x, y)` lies inside the closed box.
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }
}
