//! Flat paint for fills and text.
//!
//! Owner: slice a.

use crate::brush::Color;

/// Straight `sRGB` color plus an opacity cap; effective alpha `color.a / 255 · opacity` (as for brushes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    pub color: Color,
    /// `0..=1`.
    pub opacity: f32,
}
