//! Effective placement and visibility of one bound stroke (K1 "Coordinates", "Visibility").

use super::{Binding, BindingMode, FrameLayout, FrameRect, MarkStatus};
use crate::item::FramePreset;

/// How a stroke renders and answers input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Rendered, hittable, erasable, selectable.
    Visible,
    /// Rendered at about 30% opacity (its page state is not the frame's current one); not hittable.
    Dimmed,
    /// Not rendered, not hittable, not erasable, not selectable.
    Hidden,
}

/// The frame an effective placement is computed against.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameState {
    /// Page position of the frame's top-left corner (the frame item's translation).
    pub pos: [f32; 2],
    /// CSS size of the frame page (`css_w × css_h`).
    pub size: [f32; 2],
    /// The frame's current preset (from the frame item: the synced source of truth).
    pub preset: FramePreset,
    /// The bridge layout for this frame; `None` = no data yet (anchor fallback).
    pub layout: Option<FrameLayout>,
}

impl FrameState {
    /// The frame's page box (the clip rect for its ink).
    #[must_use]
    pub fn clip(&self) -> FrameRect {
        FrameRect::new(self.pos[0], self.pos[1], self.size[0], self.size[1])
    }
}

/// Where a visible/dimmed stroke goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Page position of the binding origin: effective page point = `offset + item transform (local)`.
    pub offset: [f32; 2],
    /// The frame's page box: renderers, hit tests, erasers and the lasso clip to it.
    pub clip: FrameRect,
    pub visibility: Visibility,
}

/// Everything needed to place one stroke.
#[derive(Debug, Clone, Copy)]
pub struct PlaceQuery<'a> {
    pub binding: &'a Binding,
    /// The frame's state; `None` when the frame item is missing (stroke hidden).
    pub frame: Option<&'a FrameState>,
    /// Status of the stroke's mark (`None` = not grouped yet).
    pub status: Option<MarkStatus>,
    /// The mark's recorded state (used when the stroke's own state is empty).
    pub mark_state: &'a str,
    pub show_done: bool,
}

/// The binding origin in page coords, or `None` when the stroke is hidden by its binding (gone element, not yet
/// watched, no layout → anchor fallback is handled by [`place`]).
#[must_use]
pub fn binding_origin(
    frame: &FrameState,
    mode: BindingMode,
    key: Option<&str>,
) -> Option<[f32; 2]> {
    let layout = frame.layout.as_ref()?;
    Some(match mode {
        BindingMode::Area => [
            frame.pos[0] - layout.scroll[0],
            frame.pos[1] - layout.scroll[1],
        ],
        BindingMode::Bound => {
            let rect = layout.rect(key?)??;
            [frame.pos[0] + rect.x, frame.pos[1] + rect.y]
        }
    })
}

/// Placement and visibility of the stroke described by `query`.
///
/// Rules (in order): hidden when the mark is done and show-done is off; hidden when the binding preset is not
/// the frame's preset; with no layout, placed at the binding anchor (state ignored); otherwise placed from the
/// layout (hidden when the bound key is absent or gone) and dimmed when the stroke's/mark's state is non-empty
/// and differs from the frame's current state.
#[must_use]
pub fn place(query: &PlaceQuery<'_>) -> Placement {
    let frame = query.frame;
    let clip = frame.map_or_else(empty_clip, FrameState::clip);
    let hidden = Placement {
        offset: [0.0; 2],
        clip,
        visibility: Visibility::Hidden,
    };
    let Some(frame) = frame else {
        return hidden;
    };
    if query.status.is_some_and(MarkStatus::is_done) && !query.show_done {
        return hidden;
    }
    if query.binding.preset != frame.preset {
        return hidden;
    }
    let Some(layout) = &frame.layout else {
        return Placement {
            offset: query.binding.anchor,
            clip,
            visibility: Visibility::Visible,
        };
    };
    let Some(offset) = binding_origin(frame, query.binding.mode, query.binding.key.as_deref())
    else {
        return hidden;
    };
    let state = if query.binding.state.is_empty() {
        query.mark_state
    } else {
        query.binding.state.as_str()
    };
    let dimmed = !state.is_empty() && state != layout.state;
    Placement {
        offset,
        clip,
        visibility: if dimmed {
            Visibility::Dimmed
        } else {
            Visibility::Visible
        },
    }
}

/// Clip rect used when the frame item vanished (nothing is drawable nonetheless).
fn empty_clip() -> FrameRect {
    FrameRect::new(0.0, 0.0, 0.0, 0.0)
}
