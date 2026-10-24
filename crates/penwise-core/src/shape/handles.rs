//! Selection handles: positions and hit testing (editing-algorithms.md §5 "Handles").
//!
//! Owner: slice d.
//!
//! Handles act on a [`HandleFrame`] (an oriented box), so the same code serves one shape (its transform and
//! size) and a multi-item selection (its axis-aligned group bounds). Hit radii are screen-space constants
//! (e.g. 12 dp) converted to page px by the caller.

use crate::bounds::Bounds;
use crate::item::{Affine, ShapeItem};

/// Oriented box: local box `[−w/2, w/2] × [−h/2, h/2]` (`size = [w, h]`) mapped by `transform`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandleFrame {
    pub transform: Affine,
    pub size: [f32; 2],
    /// Line/arrow frames show the two endpoint handles only.
    pub linear: bool,
}

/// A handle. Corner/edge names are in the frame's **local** axes (local y down).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    /// Above the top edge's midpoint by the caller's rotate offset (page px, along the frame's local −y).
    Rotate,
    /// Line/arrow start `(−w/2, 0)`.
    Start,
    /// Line/arrow end `(w/2, 0)`.
    End,
    /// Inside the frame (move).
    Body,
}

/// Result of [`hit_handle`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandleHit {
    pub handle: Handle,
    /// Handle position in page px (for drag anchoring).
    pub position: [f32; 2],
}

impl HandleFrame {
    /// Frame of a shape: its transform and size; `linear` from [`crate::item::ShapeKind::is_linear`].
    #[must_use]
    pub fn of_shape(shape: &ShapeItem, transform: &Affine) -> Self {
        todo!()
    }

    /// Axis-aligned frame of a group selection: translate to the bounds centre, size = bounds extent
    /// (`None` for empty bounds).
    #[must_use]
    pub fn of_bounds(bounds: &Bounds) -> Option<Self> {
        todo!()
    }

    /// Page position of `handle` (`Body` → the frame centre). `rotate_offset` (page px) only affects `Rotate`:
    /// the top midpoint pushed outward along the mapped local −y direction normalized in page space.
    #[must_use]
    pub fn handle_position(&self, handle: Handle, rotate_offset: f32) -> [f32; 2] {
        todo!()
    }

    /// Handles this frame shows, in drawing order: box frames the 8 resize handles (`TopLeft` … `Left`) then
    /// `Rotate`; linear frames `Start`, `End`.
    #[must_use]
    pub fn handles(&self) -> Vec<Handle> {
        todo!()
    }

    /// Handle under page point `p`: the frame's handles within `radius` (page px, page-space distance to the
    /// handle position), by priority `Rotate > corners > edges > Start/End`, ties by smaller distance, then
    /// [`Handle`] order; otherwise `Body` when `p` is inside the frame (inverse-transformed into the local box;
    /// for linear frames: within `radius` of the segment); otherwise `None`.
    #[must_use]
    pub fn hit_handle(&self, p: [f32; 2], radius: f32, rotate_offset: f32) -> Option<HandleHit> {
        todo!()
    }
}
