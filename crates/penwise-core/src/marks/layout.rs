//! The frame layout the bridge reports (client → core, local only, never synced) and its merge rule.

use std::collections::BTreeMap;

use super::MAX_STATE_LEN;
use crate::item::{FramePreset, ItemId};

/// An axis-aligned rect in CSS px (a `null` in the bridge's `rects` means "the element is gone"). In a
/// [`FrameLayoutUpdate`] it is relative to the viewport (what the bridge measures); once merged into a
/// [`FrameLayout`] it is in **document** coordinates (`rect + scroll`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl FrameRect {
    /// A rect from its components.
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// A rect from `[x, y, w, h]` (the JSON form).
    #[must_use]
    pub const fn from_array(v: [f32; 4]) -> Self {
        Self::new(v[0], v[1], v[2], v[3])
    }

    /// The `[x, y, w, h]` array (the JSON form).
    #[must_use]
    pub fn array(self) -> [f32; 4] {
        [self.x, self.y, self.w, self.h]
    }

    /// `x + w`.
    #[must_use]
    pub fn right(self) -> f32 {
        self.x + self.w
    }

    /// `y + h`.
    #[must_use]
    pub fn bottom(self) -> f32 {
        self.y + self.h
    }

    /// True when every component is finite and both sides are non-negative.
    #[must_use]
    pub fn is_valid(self) -> bool {
        [self.x, self.y, self.w, self.h]
            .iter()
            .all(|v| v.is_finite())
            && self.w >= 0.0
            && self.h >= 0.0
    }

    /// True when `p` lies inside the closed rect.
    #[must_use]
    pub fn contains(self, p: [f32; 2]) -> bool {
        p[0] >= self.x && p[0] <= self.right() && p[1] >= self.y && p[1] <= self.bottom()
    }
}

/// One frame's live layout: preset, scroll, page/viewport size, current state recipe and the rects of the
/// element keys its marks need (absent = not watched yet, `None` = the element is gone). `rects` are in
/// **document** coordinates, so they do not move when the page scrolls.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameLayout {
    pub preset: FramePreset,
    pub scroll: [f32; 2],
    pub doc: [f32; 2],
    pub viewport: [f32; 2],
    pub state: String,
    pub rects: BTreeMap<String, Option<FrameRect>>,
}

impl FrameLayout {
    /// An empty layout at `preset` (no scroll, empty state, no rects).
    #[must_use]
    pub fn new(preset: FramePreset) -> Self {
        Self {
            preset,
            scroll: [0.0; 2],
            doc: [0.0; 2],
            viewport: [0.0; 2],
            state: String::new(),
            rects: BTreeMap::new(),
        }
    }

    /// Merges `update`: every field it carries replaces the stored one; `rects` entries replace per key. The
    /// update's rects are viewport-relative; they are stored in document coordinates, `rect + scroll` with the
    /// scroll in effect after this update. Returns true when anything visible changed. Non-finite values are
    /// ignored (the bridge is untrusted).
    pub fn merge(&mut self, update: &FrameLayoutUpdate) -> bool {
        let mut changed = false;
        if let Some(preset) = update.preset
            && preset != self.preset
        {
            self.preset = preset;
            changed = true;
        }
        changed |= replace_pair(&mut self.scroll, update.scroll);
        changed |= replace_pair(&mut self.doc, update.doc);
        changed |= replace_pair(&mut self.viewport, update.viewport);
        if let Some(state) = &update.state
            && state.len() <= MAX_STATE_LEN
            && *state != self.state
        {
            self.state.clone_from(state);
            changed = true;
        }
        for (key, rect) in &update.rects {
            let rect = rect
                .filter(|r| r.is_valid())
                .map(|r| FrameRect::new(r.x + self.scroll[0], r.y + self.scroll[1], r.w, r.h));
            if key.len() <= super::MAX_KEY_LEN && self.rects.get(key) != Some(&rect) {
                self.rects.insert(key.clone(), rect);
                changed = true;
            }
        }
        changed
    }

    /// The element rect for `key`: `None` when the key is not watched, `Some(None)` when it is gone.
    #[must_use]
    pub fn rect(&self, key: &str) -> Option<Option<FrameRect>> {
        self.rects.get(key).copied()
    }
}

/// A partial layout update (one `pw:layout` message): `None` fields are left alone, `rects` merge per key.
/// `rects` are viewport-relative here (see [`FrameRect`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameLayoutUpdate {
    pub preset: Option<FramePreset>,
    pub scroll: Option<[f32; 2]>,
    pub doc: Option<[f32; 2]>,
    pub viewport: Option<[f32; 2]>,
    pub state: Option<String>,
    pub rects: Vec<(String, Option<FrameRect>)>,
}

/// Client-local layouts of the frames on a page (`BTreeMap`, so output order is deterministic).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameLayouts {
    pages: BTreeMap<ItemId, FrameLayout>,
}

impl FrameLayouts {
    /// Empty set (no bridge data at all: anchor fallback).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Merges one message for `frame`, creating the layout on first sight (preset from the message, else
    /// `fallback`). Returns true when visible output changed.
    pub fn merge(
        &mut self,
        frame: ItemId,
        fallback: FramePreset,
        update: &FrameLayoutUpdate,
    ) -> bool {
        let layout = self
            .pages
            .entry(frame)
            .or_insert_with(|| FrameLayout::new(fallback));
        layout.merge(update)
    }

    /// The layout of `frame`, if the bridge reported one.
    #[must_use]
    pub fn get(&self, frame: ItemId) -> Option<&FrameLayout> {
        self.pages.get(&frame)
    }

    /// Drops `frame`'s layout (the frame was deleted).
    pub fn remove(&mut self, frame: ItemId) {
        self.pages.remove(&frame);
    }

    /// True when no frame has a layout yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }
}

/// Replaces `slot` with a finite `value`; true when it changed.
fn replace_pair(slot: &mut [f32; 2], value: Option<[f32; 2]>) -> bool {
    let Some(value) = value.filter(|v| v[0].is_finite() && v[1].is_finite()) else {
        return false;
    };
    let same = slot[0].to_bits() == value[0].to_bits() && slot[1].to_bits() == value[1].to_bits();
    if same {
        return false;
    }
    *slot = value;
    true
}
