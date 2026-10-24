//! Selection handles: positions and hit testing (editing-algorithms.md §5 "Handles").
//!
//! Owner: slice d.
//!
//! Handles act on a [`HandleFrame`] (an oriented box), so the same code serves one shape (its transform and
//! size) and a multi-item selection (its axis-aligned group bounds). Hit radii are screen-space constants
//! (e.g. 12 dp) converted to page px by the caller.

use crate::bounds::Bounds;
use crate::edit::proximity::point_segment_dist2;
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
        Self {
            transform: *transform,
            size: shape.size,
            linear: shape.kind.is_linear(),
        }
    }

    /// Axis-aligned frame of a group selection: translate to the bounds centre, size = bounds extent
    /// (`None` for empty bounds).
    #[must_use]
    pub fn of_bounds(bounds: &Bounds) -> Option<Self> {
        if bounds.is_empty() {
            return None;
        }
        Some(Self {
            transform: Affine::translate(
                (bounds.min_x + bounds.max_x) * 0.5,
                (bounds.min_y + bounds.max_y) * 0.5,
            ),
            size: [bounds.width(), bounds.height()],
            linear: false,
        })
    }

    /// Page position of `handle` (`Body` → the frame centre). `rotate_offset` (page px) only affects `Rotate`:
    /// the top midpoint pushed outward along the mapped local −y direction normalized in page space.
    #[must_use]
    pub fn handle_position(&self, handle: Handle, rotate_offset: f32) -> [f32; 2] {
        let p = self.transform.apply(local_point(handle, self.size));
        if handle != Handle::Rotate {
            return p;
        }
        let up = self.transform.apply_vector([0.0, -1.0]);
        let len = (up[0] * up[0] + up[1] * up[1]).sqrt();
        if !(len > 0.0 && len.is_finite()) {
            return p;
        }
        let k = rotate_offset / len;
        [p[0] + up[0] * k, p[1] + up[1] * k]
    }

    /// Handles this frame shows, in drawing order: box frames the 8 resize handles (`TopLeft` … `Left`) then
    /// `Rotate`; linear frames `Start`, `End`.
    #[must_use]
    pub fn handles(&self) -> Vec<Handle> {
        if self.linear {
            return vec![Handle::Start, Handle::End];
        }
        let mut all = BOX_RESIZE_HANDLES.to_vec();
        all.push(Handle::Rotate);
        all
    }

    /// Handle under page point `p`: the frame's handles within `radius` (page px, page-space distance to the
    /// handle position), by priority `Rotate > corners > edges > Start/End`, ties by smaller distance, then
    /// [`Handle`] order; otherwise `Body` when `p` is inside the frame (inverse-transformed into the local box;
    /// for linear frames: within `radius` of the segment); otherwise `None`.
    #[must_use]
    pub fn hit_handle(&self, p: [f32; 2], radius: f32, rotate_offset: f32) -> Option<HandleHit> {
        let r2 = radius * radius;
        let best = self
            .handles()
            .into_iter()
            .filter_map(|handle| {
                let position = self.handle_position(handle, rotate_offset);
                let d2 = dist2(p, position);
                (d2 <= r2).then_some((priority(handle), d2, handle, position))
            })
            .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        if let Some((_, _, handle, position)) = best {
            return Some(HandleHit { handle, position });
        }
        self.body_contains(p, radius).then(|| HandleHit {
            handle: Handle::Body,
            position: self.handle_position(Handle::Body, rotate_offset),
        })
    }

    /// Body test of [`HandleFrame::hit_handle`].
    fn body_contains(&self, p: [f32; 2], radius: f32) -> bool {
        if self.linear {
            let start = self.handle_position(Handle::Start, 0.0);
            let end = self.handle_position(Handle::End, 0.0);
            return point_segment_dist2(p, start, end).0 <= radius * radius;
        }
        let Some(inverse) = self.transform.invert() else {
            return false;
        };
        let q = inverse.apply(p);
        q[0].abs() <= self.size[0] * 0.5 && q[1].abs() <= self.size[1] * 0.5
    }
}

/// The 8 box resize handles in drawing order.
const BOX_RESIZE_HANDLES: [Handle; 8] = [
    Handle::TopLeft,
    Handle::Top,
    Handle::TopRight,
    Handle::Right,
    Handle::BottomRight,
    Handle::Bottom,
    Handle::BottomLeft,
    Handle::Left,
];

/// Local position of `handle` on the box `size` (`Rotate` → top midpoint before the offset, `Body` → centre).
pub(crate) fn local_point(handle: Handle, size: [f32; 2]) -> [f32; 2] {
    let (hx, hy) = (size[0] * 0.5, size[1] * 0.5);
    match handle {
        Handle::TopLeft => [-hx, -hy],
        Handle::Top | Handle::Rotate => [0.0, -hy],
        Handle::TopRight => [hx, -hy],
        Handle::Right | Handle::End => [hx, 0.0],
        Handle::BottomRight => [hx, hy],
        Handle::Bottom => [0.0, hy],
        Handle::BottomLeft => [-hx, hy],
        Handle::Left | Handle::Start => [-hx, 0.0],
        Handle::Body => [0.0, 0.0],
    }
}

/// Hit priority rank (lower wins): rotate, corners, edges, endpoints, body.
fn priority(handle: Handle) -> u8 {
    match handle {
        Handle::Rotate => 0,
        Handle::TopLeft | Handle::TopRight | Handle::BottomRight | Handle::BottomLeft => 1,
        Handle::Top | Handle::Right | Handle::Bottom | Handle::Left => 2,
        Handle::Start | Handle::End => 3,
        Handle::Body => 4,
    }
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    dx * dx + dy * dy
}
