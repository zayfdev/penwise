//! Shape creation drag, resize about the opposite handle, endpoint drags, and the resulting edits
//! (editing-algorithms.md §5 "Resize about an anchor").
//!
//! Owner: slice d.
//!
//! All results carry absolute values and are emitted once at gesture end (`Update { shape_size, transform }`);
//! live drags preview with the same functions.

use super::handles::{Handle, HandleFrame, local_point};
use crate::edit::transform::SNAP_STEP;
use crate::edit::{Edit, ItemPatch};
use crate::float::max;
use crate::item::{Affine, ItemId, ItemKind, Scene};

/// Frame of a new shape dragged from `start` to `end` (page px, axis-aligned): centre at the midpoint, size
/// `|end − start|`. With `constrain`, both sides become `max(|dx|, |dy|)` keeping the drag direction's signs
/// (square/circle). Linear kinds: the frame spans `start → end` (rotation from `libm::atan2f`, `size =
/// [length, 0]`), and `constrain` snaps the angle with [`crate::edit::transform::snap_angle`] at 15° steps
/// (always, not only within the tolerance).
///
/// The unconditional snap rounds to the nearest multiple of [`SNAP_STEP`] directly (`snap_angle` only snaps
/// within its tolerance); the snapped end keeps the drag length.
#[must_use]
pub fn create_frame(start: [f32; 2], end: [f32; 2], linear: bool, constrain: bool) -> HandleFrame {
    if linear {
        let end = if constrain {
            snap_direction(start, end)
        } else {
            end
        };
        return segment_frame(start, end);
    }
    let (mut dx, mut dy) = (end[0] - start[0], end[1] - start[1]);
    if constrain {
        let side = max(dx.abs(), dy.abs());
        dx = with_sign(side, dx);
        dy = with_sign(side, dy);
    }
    HandleFrame {
        transform: Affine::translate(start[0] + dx * 0.5, start[1] + dy * 0.5),
        size: [dx.abs(), dy.abs()],
        linear: false,
    }
}

/// Resize `frame` by dragging resize handle `handle` to page point `pointer`, about the opposite handle:
/// 1. `q = T⁻¹ · pointer` (local); anchor = opposite corner / edge midpoint in local coords;
/// 2. `s_x = (q.x − anchor.x) / (handle.x − anchor.x)`, `s_y` likewise; edge handles keep the other axis at 1;
/// 3. `constrain`: `s_x = s_y = sign · max(|s_x|, |s_y|)` (edge handles scale both axes);
/// 4. the new frame keeps the anchor fixed in page space: new `size = [|s_x|·w, |s_y|·h]`, the transform gains
///    the anchor-preserving translation and a mirror for negative factors (`Tr(anchor)·S(sign)·Tr(−anchor)`
///    composed in local space).
///
/// Non-resize handles (`Rotate`, `Body`, `Start`, `End`) and non-invertible frames return `frame` unchanged.
/// Zero-extent axes stay 0 (the factor is ignored for them).
///
/// With `constrain` on an edge handle the passive axis takes the positive factor `max(|s|)` of the dragged
/// axis (it never mirrors). The new frame is `T · Tr(c) · S(sign)` with centre `c = anchor·(1 − s)`, so
/// every point `v` of the old box lands on `anchor + s·(v − anchor)`.
#[must_use]
pub fn resize_frame(
    frame: &HandleFrame,
    handle: Handle,
    pointer: [f32; 2],
    constrain: bool,
) -> HandleFrame {
    let Some(axes) = resize_axes(handle) else {
        return *frame;
    };
    let Some(inverse) = frame.transform.invert() else {
        return *frame;
    };
    let q = inverse.apply(pointer);
    let h = local_point(handle, frame.size);
    let anchor = [-h[0], -h[1]];
    let active = [0, 1].map(|i| axes[i] && frame.size[i] > 0.0);
    let mut s = [0, 1].map(|i| {
        if active[i] {
            (q[i] - anchor[i]) / (h[i] - anchor[i])
        } else {
            1.0
        }
    });
    if constrain {
        let m = max(
            if active[0] { s[0].abs() } else { 0.0 },
            if active[1] { s[1].abs() } else { 0.0 },
        );
        s = [0, 1].map(|i| {
            if active[i] {
                with_sign(m, s[i])
            } else if frame.size[i] > 0.0 {
                m
            } else {
                s[i]
            }
        });
    }
    if !(s[0].is_finite() && s[1].is_finite()) {
        return *frame;
    }
    let centre = [anchor[0] * (1.0 - s[0]), anchor[1] * (1.0 - s[1])];
    let transform = Affine::scale(with_sign(1.0, s[0]), with_sign(1.0, s[1]))
        .then(&Affine::translate(centre[0], centre[1]))
        .then(&frame.transform);
    HandleFrame {
        transform,
        size: [s[0].abs() * frame.size[0], s[1].abs() * frame.size[1]],
        linear: frame.linear,
    }
}

/// Line/arrow endpoint drag: the moved endpoint follows `pointer`, the other stays; with `constrain`, the
/// segment angle snaps to 15° steps. Non-endpoint handles return `frame` unchanged.
///
/// The result is a fresh segment frame (rotation + translation, `size = [length, 0]`) through the two page
/// endpoints, like [`create_frame`].
#[must_use]
pub fn drag_endpoint(
    frame: &HandleFrame,
    handle: Handle,
    pointer: [f32; 2],
    constrain: bool,
) -> HandleFrame {
    let fixed_handle = match handle {
        Handle::Start => Handle::End,
        Handle::End => Handle::Start,
        _ => return *frame,
    };
    let fixed = frame.handle_position(fixed_handle, 0.0);
    let moved = if constrain {
        snap_direction(fixed, pointer)
    } else {
        pointer
    };
    let mut result = if handle == Handle::Start {
        segment_frame(moved, fixed)
    } else {
        segment_frame(fixed, moved)
    };
    result.linear = frame.linear;
    result
}

/// The affine `D` with `D ∘ old.box = new.box`: `new.transform · S(new.size / old.size) · old.transform⁻¹`
/// (axes with zero old extent use factor 1). Group resize applies it to every selected item with
/// [`crate::edit::transform::transform_items`]. Identity when `old` is not invertible.
#[must_use]
pub fn frame_delta(old: &HandleFrame, new: &HandleFrame) -> Affine {
    let Some(inverse) = old.transform.invert() else {
        return Affine::IDENTITY;
    };
    let ratio = |n: f32, o: f32| if o.abs() > 0.0 { n / o } else { 1.0 };
    inverse
        .then(&Affine::scale(
            ratio(new.size[0], old.size[0]),
            ratio(new.size[1], old.size[1]),
        ))
        .then(&new.transform)
}

/// Edit committing a single shape's new frame: `Update { id, shape_size: Some(new.size), transform:
/// Some(new.transform) }` with unchanged fields omitted (empty `Vec` when nothing changed or `id` is not a
/// live shape).
#[must_use]
pub fn resize_edits(scene: &Scene, id: ItemId, new: &HandleFrame) -> Vec<Edit> {
    let Some(item) = scene.get(id) else {
        return Vec::new();
    };
    let ItemKind::Shape(shape) = &item.kind else {
        return Vec::new();
    };
    let size_changed = shape
        .size
        .iter()
        .zip(new.size)
        .any(|(a, b)| a.to_bits() != b.to_bits());
    let patch = ItemPatch {
        shape_size: size_changed.then_some(new.size),
        transform: (!item.transform.bit_eq(&new.transform)).then_some(new.transform),
        ..ItemPatch::default()
    };
    if patch.shape_size.is_none() && patch.transform.is_none() {
        return Vec::new();
    }
    vec![Edit::Update { id, patch }]
}

/// Which local axes `[x, y]` a resize handle drives (`None` for non-resize handles).
fn resize_axes(handle: Handle) -> Option<[bool; 2]> {
    match handle {
        Handle::TopLeft | Handle::TopRight | Handle::BottomRight | Handle::BottomLeft => {
            Some([true, true])
        }
        Handle::Left | Handle::Right => Some([true, false]),
        Handle::Top | Handle::Bottom => Some([false, true]),
        Handle::Rotate | Handle::Start | Handle::End | Handle::Body => None,
    }
}

/// `magnitude` with the sign of `sign_of` (zero counts as positive).
fn with_sign(magnitude: f32, sign_of: f32) -> f32 {
    if sign_of < 0.0 { -magnitude } else { magnitude }
}

/// Segment frame `start → end`: rotation from `atan2f`, centre at the midpoint, `size = [length, 0]`.
fn segment_frame(start: [f32; 2], end: [f32; 2]) -> HandleFrame {
    let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
    let length = (dx * dx + dy * dy).sqrt();
    let angle = libm::atan2f(dy, dx);
    HandleFrame {
        transform: Affine::rotate(angle)
            .then(&Affine::translate(start[0] + dx * 0.5, start[1] + dy * 0.5)),
        size: [length, 0.0],
        linear: true,
    }
}

/// `to` moved so the direction `from → to` is the nearest multiple of 15°, keeping the distance.
fn snap_direction(from: [f32; 2], to: [f32; 2]) -> [f32; 2] {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let length = (dx * dx + dy * dy).sqrt();
    let angle = libm::roundf(libm::atan2f(dy, dx) / SNAP_STEP) * SNAP_STEP;
    let (s, c) = libm::sincosf(angle);
    [from[0] + length * c, from[1] + length * s]
}
