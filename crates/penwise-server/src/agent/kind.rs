//! The mark-kind hint and the ink-colour meaning (design-harness C1).
//!
//! Both live in `penwise_core::marks` and are pure geometry, so the bundle reports exactly what the tablet sees:
//! the classifier over the mark's member strokes (with the bound element's rect, which separates a strike from
//! an underline) and the colour meaning of the first stroke's ink.

use penwise_core::item::StrokeItem;
use penwise_core::marks::{MarkKind, classify_with_rect, colour_meaning};

/// One classification result, in the bundle's own terms (the JSON names core uses).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Shape {
    /// One of `circle`, `arrow`, `cross`, `wavy`, `box_new`, `strike`, `underline`, `free`.
    pub(crate) guess: &'static str,
    /// `0..=1`.
    pub(crate) confidence: f32,
    /// Arrow tail, in binding-local coordinates.
    pub(crate) from: Option<[f32; 2]>,
    /// Arrow head tip, in binding-local coordinates.
    pub(crate) to: Option<[f32; 2]>,
}

/// The kind hint for a mark's strokes. `rect` is the bound element's box in the strokes' space, when known.
pub(crate) fn shape(strokes: &[&StrokeItem], rect: Option<[f32; 4]>) -> Shape {
    let classified = classify_with_rect(strokes, rect);
    Shape {
        guess: name(classified.guess),
        confidence: classified.confidence,
        from: classified.from,
        to: classified.to,
    }
}

/// What the ink colour means: `problem` (red), `idea` (blue), `good` (green), else `none`.
pub(crate) fn colour(argb: u32) -> &'static str {
    colour_meaning(argb).as_str()
}

/// The vocabulary name of a guess (core's serde names, spelled out so a rename cannot drift silently).
fn name(kind: MarkKind) -> &'static str {
    match kind {
        MarkKind::Circle => "circle",
        MarkKind::Arrow => "arrow",
        MarkKind::Cross => "cross",
        MarkKind::Wavy => "wavy",
        MarkKind::BoxNew => "box_new",
        MarkKind::Strike => "strike",
        MarkKind::Underline => "underline",
        MarkKind::Free => "free",
    }
}
