//! Marks (design harness M2): strokes bound to an element or pinned to a document area, grouped into marks that
//! follow the live page across edits.
//!
//! Owner: design-harness M2 (core). Contract: [`docs/design-harness-spec.md`] §4, K1.
//!
//! **Core is the source of truth.** A bound stroke stores its samples in *binding-local* CSS px plus one
//! [`Binding`]; the effective page position is derived from the frame's page position and the current layout the
//! bridge reported ([`FrameLayouts`], local only, never synced):
//! - `bound`: `frame.pos + rect.xy + local` (`rect` = the element's viewport rect, so fixed/sticky elements
//!   follow);
//! - `area`: `frame.pos + local − scroll`.
//!
//! No scaling ever. [`MarkView::presentation`] applies that placement to a copy of the scene, which is what
//! renderers, hit tests, the eraser and the lasso consume; [`Presentation::localize`] maps the resulting edits
//! back to binding-local data before they are committed.
//!
//! **Visibility** per stroke ([`Visibility`]): hidden when the mark is done and show-done is off, when the
//! binding preset is not the frame's preset, or when the bound element is gone (`null`/absent rect); dimmed when
//! its state is non-empty and the frame's current state differs. With no layout yet (server PNG, offline) the
//! stroke falls back to its [`Binding::anchor`] (the page position it was drawn at) with scroll 0 and no states.

mod binding;
mod group;
mod layout;
mod mark;
mod place;
mod view;

pub use binding::{Binding, BindingMode, MAX_KEY_LEN, MAX_STATE_LEN};
pub use group::{BoundStroke, GROUP_DISTANCE_PX, GROUP_GAP_MS, add_bound_stroke, prune_marks};
pub use layout::{FrameLayout, FrameLayoutUpdate, FrameLayouts, FrameRect};
pub use mark::{MAX_MARK_STROKES, MarkItem, MarkStatus, mark_of, set_mark_status};
pub use place::{FrameState, PlaceQuery, Placement, Visibility, binding_origin, place};
pub use view::{DIM_OPACITY, MarkEntry, MarkView, Presentation};

#[cfg(test)]
mod tests;
