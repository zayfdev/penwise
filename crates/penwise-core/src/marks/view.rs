//! Scene → effective presentation: placement, visibility, mark entries, element keys, and the inverse mapping
//! of edits back to binding-local data.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    FrameLayouts, FrameState, MarkStatus, PlaceQuery, Placement, Visibility, binding_origin, place,
};
use crate::bounds::Bounds;
use crate::edit::{Edit, ItemPatch};
use crate::item::{Affine, FramePreset, Item, ItemId, ItemKind, Scene};

/// Opacity of a dimmed stroke ("about 30%").
pub const DIM_OPACITY: f32 = 0.3;

/// One mark as the clients show it.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkEntry {
    pub id: ItemId,
    pub frame: ItemId,
    pub preset: FramePreset,
    pub status: MarkStatus,
    pub state: String,
    pub strokes: Vec<ItemId>,
    /// Effective page box of the visible member strokes; `None` when the mark is hidden.
    pub rect: Option<[f32; 4]>,
}

/// A scene with every bound/area stroke moved to its effective page position (and hidden ones removed), plus
/// the ids that render dimmed and the bases needed to map edits back.
#[derive(Debug, Clone)]
pub struct Presentation {
    pub scene: Scene,
    /// Strokes that render at [`DIM_OPACITY`] and do not answer input.
    pub dim: BTreeSet<ItemId>,
    frame_states: BTreeMap<ItemId, FrameState>,
    bases: BTreeMap<ItemId, [f32; 2]>,
}

impl Presentation {
    /// Maps presentation-space edits (from hit/lasso/erase/move tools run on [`Presentation::scene`]) back to
    /// the committed coordinate space: a bound/area stroke keeps its binding and only its local offset changes.
    #[must_use]
    pub fn localize(&self, edits: Vec<Edit>) -> Vec<Edit> {
        edits
            .into_iter()
            .map(|edit| self.localize_one(edit))
            .collect()
    }

    /// The scene the interactive tools may edit: the presentation without the dimmed strokes.
    #[must_use]
    pub fn interactive(&self) -> Scene {
        let mut scene = self.scene.clone();
        let deletes: Vec<Edit> = self
            .dim
            .iter()
            .filter(|id| scene.get(**id).is_some())
            .map(|id| Edit::Delete(*id))
            .collect();
        // Deleting cannot fail for ids that are live above.
        let _ = scene.apply_all(&deletes);
        scene
    }

    /// True when no stroke was moved (plain page): callers can then use the scene as is.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.bases.is_empty() && self.dim.is_empty()
    }

    fn localize_one(&self, edit: Edit) -> Edit {
        match edit {
            Edit::Add(mut item) => {
                if let Some(base) = self.add_base(&item) {
                    item.transform = unpremultiply(&item.transform, base);
                }
                Edit::Add(item)
            }
            Edit::Update { id, mut patch } => {
                if let (Some(t), Some(base)) = (patch.transform, self.bases.get(&id)) {
                    patch.transform = Some(unpremultiply(&t, *base));
                }
                Edit::Update { id, patch }
            }
            other @ Edit::Delete(_) => other,
        }
    }

    /// The base an `Add` was translated by (pieces cut from a bound stroke keep the source's binding).
    fn add_base(&self, item: &Item) -> Option<[f32; 2]> {
        let ItemKind::Stroke(stroke) = &item.kind else {
            return None;
        };
        let binding = stroke.binding.as_ref()?;
        let frame = self.frame_states.get(&binding.frame)?;
        Some(binding_origin(frame, binding.mode, binding.key.as_deref()).unwrap_or(binding.anchor))
    }
}

/// The scene under a layout and show-done flag: everything the renderers and tools need.
#[derive(Debug, Clone)]
pub struct MarkView<'a> {
    scene: &'a Scene,
    layouts: &'a FrameLayouts,
    show_done: bool,
}

impl<'a> MarkView<'a> {
    /// A view of `scene` with `layouts` and the show-done flag.
    #[must_use]
    pub fn new(scene: &'a Scene, layouts: &'a FrameLayouts, show_done: bool) -> Self {
        Self {
            scene,
            layouts,
            show_done,
        }
    }

    /// True when the scene has any mark or any bound stroke (otherwise the presentation is the scene itself).
    #[must_use]
    pub fn has_marks(&self) -> bool {
        self.scene.live_ids().into_iter().any(|id| {
            self.scene.get(id).is_some_and(|item| match &item.kind {
                ItemKind::Mark(_) => true,
                ItemKind::Stroke(stroke) => stroke.binding.is_some(),
                _ => false,
            })
        })
    }

    /// The scene with placement applied; identity when [`MarkView::has_marks`] is false.
    #[must_use]
    pub fn presentation(&self) -> Presentation {
        let frame_states = self.frame_states();
        let pins = self.pins();
        let mut dim = BTreeSet::new();
        let mut bases = BTreeMap::new();
        let mut edits = Vec::new();
        for id in self.scene.live_ids() {
            let Some(item) = self.scene.get(id) else {
                continue;
            };
            let ItemKind::Stroke(stroke) = &item.kind else {
                continue;
            };
            let Some(binding) = &stroke.binding else {
                continue;
            };
            let (status, mark_state) = pins
                .get(&id)
                .map_or((None, ""), |(s, m)| (Some(*s), m.as_str()));
            let query = PlaceQuery {
                binding,
                frame: frame_states.get(&binding.frame),
                status,
                mark_state,
                show_done: self.show_done,
            };
            let placement = place(&query);
            if placement.visibility == Visibility::Hidden {
                edits.push(Edit::Delete(id));
                continue;
            }
            bases.insert(id, placement.offset);
            edits.push(move_edit(item, placement));
            if placement.visibility == Visibility::Dimmed {
                dim.insert(id);
            }
        }
        let mut scene = self.scene.clone();
        // Updates and deletes of live ids: apply cannot fail in a way that matters here.
        let _ = scene.apply_all(&edits);
        Presentation {
            scene,
            dim,
            frame_states,
            bases,
        }
    }

    /// Every live mark with its effective page rect (`None` when hidden), in `(z, id)` order.
    #[must_use]
    pub fn mark_entries(&self) -> Vec<MarkEntry> {
        let frame_states = self.frame_states();
        let mut ids: Vec<ItemId> = self.scene.live_ids();
        ids.sort_by_key(|id| self.scene.get(*id).map_or((0, *id), |item| (item.z, *id)));
        let mut out = Vec::new();
        for id in ids {
            let Some(ItemKind::Mark(mark)) = self.scene.get(id).map(|item| &item.kind) else {
                continue;
            };
            let mut rect = Bounds::EMPTY;
            for stroke in &mark.strokes {
                let Some(item) = self.scene.get(*stroke) else {
                    continue;
                };
                let ItemKind::Stroke(s) = &item.kind else {
                    continue;
                };
                let Some(binding) = &s.binding else {
                    continue;
                };
                let query = PlaceQuery {
                    binding,
                    frame: frame_states.get(&binding.frame),
                    status: Some(mark.status),
                    mark_state: &mark.state,
                    show_done: self.show_done,
                };
                let placement = place(&query);
                if placement.visibility == Visibility::Hidden {
                    continue;
                }
                if let Some(bounds) = self.scene.bounds(*stroke) {
                    rect = rect.union(&shift(&bounds, placement.offset));
                }
            }
            out.push(MarkEntry {
                id,
                frame: mark.frame,
                preset: mark.preset,
                status: mark.status,
                state: mark.state.clone(),
                strokes: mark.strokes.clone(),
                rect: (!rect.is_empty())
                    .then(|| [rect.min_x, rect.min_y, rect.width(), rect.height()]),
            });
        }
        out
    }

    /// Distinct element keys of `frame`'s bound strokes (what the client asks the bridge to watch).
    #[must_use]
    pub fn keys(&self, frame: ItemId) -> Vec<String> {
        let mut keys = BTreeSet::new();
        for id in self.scene.live_ids() {
            let Some(ItemKind::Stroke(stroke)) = self.scene.get(id).map(|item| &item.kind) else {
                continue;
            };
            let Some(binding) = &stroke.binding else {
                continue;
            };
            if binding.frame == frame
                && let Some(key) = &binding.key
            {
                keys.insert(key.clone());
            }
        }
        keys.into_iter().collect()
    }

    /// The frame's state from its item (position, size, preset) and the bridge layout.
    fn frame_states(&self) -> BTreeMap<ItemId, FrameState> {
        self.scene
            .live_ids()
            .into_iter()
            .filter_map(|id| Some((id, FrameState::of(self.scene, self.layouts, id)?)))
            .collect()
    }

    /// Stroke id → (its mark's status, its mark's state).
    fn pins(&self) -> BTreeMap<ItemId, (MarkStatus, String)> {
        let mut out = BTreeMap::new();
        for id in self.scene.live_ids() {
            let Some(ItemKind::Mark(mark)) = self.scene.get(id).map(|item| &item.kind) else {
                continue;
            };
            for stroke in &mark.strokes {
                out.insert(*stroke, (mark.status, mark.state.clone()));
            }
        }
        out
    }
}

/// `Update { transform }` placing `item` at `placement.offset`.
fn move_edit(item: &Item, placement: Placement) -> Edit {
    Edit::Update {
        id: item.id,
        patch: ItemPatch {
            transform: Some(
                item.transform
                    .then(&Affine::translate(placement.offset[0], placement.offset[1])),
            ),
            ..ItemPatch::default()
        },
    }
}

/// `translate(−base) ∘ t`: the committed transform of a stroke the presentation moved by `base`.
fn unpremultiply(t: &Affine, base: [f32; 2]) -> Affine {
    t.then(&Affine::translate(-base[0], -base[1]))
}

/// `b` translated by `d`.
fn shift(b: &Bounds, d: [f32; 2]) -> Bounds {
    Bounds {
        min_x: b.min_x + d[0],
        min_y: b.min_y + d[1],
        max_x: b.max_x + d[0],
        max_y: b.max_y + d[1],
    }
}
