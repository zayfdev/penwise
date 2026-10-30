//! The frame item and the one-call spec the server and the clients place it with (design harness C1).

use super::{
    FrameExtent, FramePreset, FrameSize, MAX_FRAME_SIDE, MAX_GROUP_LEN, MAX_LABEL_LEN,
    MAX_PATCH_LEN, MAX_PROJECT_LEN, MAX_ROUTE_LEN, invalid, side_px,
};
use crate::edit::{Edit, EditError};
use crate::item::{Affine, Item, ItemId, ItemKind, Layer};

/// A frame item: which project and route to show, at which preset size, with an optional temporary patch
/// (ghost or variant) and the variant group the owner picks from.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameItem {
    /// Linked project id (`1..=64` bytes).
    pub project: String,
    /// Route on that project, path and query (`/...`, `1..=2048` bytes).
    pub route: String,
    pub preset: FramePreset,
    /// Page box width in CSS px (`1 ..= `[`super::MAX_FRAME_SIDE`]).
    pub css_w: u32,
    /// Page box height in CSS px.
    pub css_h: u32,
    /// P1 patch JSON applied over the live page; `None` = the page as served. At most [`super::MAX_PATCH_LEN`] bytes.
    pub patch: Option<String>,
    /// Shown on the frame's label chip; empty for a plain frame. At most [`super::MAX_LABEL_LEN`] bytes.
    pub label: String,
    /// Variant group id; `None` for a plain frame. At most [`super::MAX_GROUP_LEN`] bytes, non-empty.
    pub group: Option<String>,
    /// The owner's pick within its group (choosing one frame un-chooses the group's others).
    pub chosen: bool,
    /// Whether the box on the board is the viewport or the whole document ([`FrameItem::box_size`]).
    pub extent: FrameExtent,
    /// The document height in CSS px last measured by the server; `0` = unknown. At most
    /// [`super::MAX_FRAME_SIDE`].
    pub doc_h: u32,
}

impl FrameItem {
    /// A plain frame item (no patch, no label, no group, [`FrameExtent::Full`], unknown document height); the
    /// caller is expected to build it from a [`FrameSpec`] or validated JSON.
    #[must_use]
    pub fn new(
        project: String,
        route: String,
        preset: FramePreset,
        css_w: u32,
        css_h: u32,
    ) -> Self {
        Self {
            project,
            route,
            preset,
            css_w,
            css_h,
            patch: None,
            label: String::new(),
            group: None,
            chosen: false,
            extent: FrameExtent::Full,
            doc_h: 0,
        }
    }

    /// The preset and size as one value.
    #[must_use]
    pub fn size(&self) -> FrameSize {
        FrameSize {
            preset: self.preset,
            css_w: self.css_w,
            css_h: self.css_h,
        }
    }

    /// Page size in page units, `[css_w, css_h]`.
    #[must_use]
    pub fn page_size(&self) -> [f32; 2] {
        [side_px(self.css_w), side_px(self.css_h)]
    }

    /// The frame's box on the board, `[width, height]` in page units. A [`FrameExtent::Viewport`] frame is
    /// `[css_w, css_h]`; a [`FrameExtent::Full`] one is `[css_w, max(css_h, h)]` where `h` is
    /// `layout_doc_h` (the live or still document height a rendering client knows; `None`, non-finite or
    /// `<= 0` mean unknown) or the synced `doc_h` hint; the height is clamped to [`MAX_FRAME_SIDE`] (the bridge is untrusted). Bounds, hit tests, the lasso, validation and mark clipping pass `None`.
    #[must_use]
    pub fn box_size(&self, layout_doc_h: Option<f32>) -> [f32; 2] {
        let [w, h] = self.page_size();
        match self.extent {
            FrameExtent::Viewport => [w, h],
            FrameExtent::Full => [w, full_box_height(h, layout_doc_h, side_px(self.doc_h))],
        }
    }

    /// The string fields' invariants: `project` non-empty and ≤ 64 bytes, `route` starts with `/` and is at most
    /// 2048 bytes, the size is valid, `patch` at most 64 KiB, `label` at most 80 bytes and `group` (when set)
    /// non-empty and at most 64 bytes, `doc_h` at most [`MAX_FRAME_SIDE`].
    ///
    /// # Errors
    /// [`EditError::InvalidValue`] naming `"frame_project"`, `"frame_route"`, `"frame_size"`,
    /// `"frame_patch"`, `"frame_label"`, `"frame_group"` or `"frame_doc_h"`.
    pub fn check(&self) -> Result<(), EditError> {
        if self.project.is_empty() || self.project.len() > MAX_PROJECT_LEN {
            return Err(invalid("frame_project"));
        }
        if !self.route.starts_with('/') || self.route.len() > MAX_ROUTE_LEN {
            return Err(invalid("frame_route"));
        }
        if self
            .patch
            .as_ref()
            .is_some_and(|patch| patch.len() > MAX_PATCH_LEN)
        {
            return Err(invalid("frame_patch"));
        }
        if self.label.len() > MAX_LABEL_LEN {
            return Err(invalid("frame_label"));
        }
        if self
            .group
            .as_ref()
            .is_some_and(|group| group.is_empty() || group.len() > MAX_GROUP_LEN)
        {
            return Err(invalid("frame_group"));
        }
        if self.doc_h > MAX_FRAME_SIDE {
            return Err(invalid("frame_doc_h"));
        }
        self.size().check()
    }
}

/// A frame to place on a page: its content, size and page position. The one-call constructor the server and the
/// clients use (C1).
#[derive(Debug, Clone, PartialEq)]
pub struct FrameSpec {
    pub project: String,
    pub route: String,
    pub preset: FramePreset,
    pub css_w: u32,
    pub css_h: u32,
    /// Page position of the frame's top-left corner.
    pub x: f32,
    pub y: f32,
    /// The box extent of the placed frame; [`FrameExtent::Full`] unless changed with
    /// [`FrameSpec::with_extent`].
    pub extent: FrameExtent,
}

impl FrameSpec {
    /// Resolves `preset` against `css_w`/`css_h` (fixed presets ignore them) and checks the content and
    /// position.
    ///
    /// # Errors
    /// [`EditError::InvalidValue`] for a bad project, route, size or non-finite position.
    pub fn new(
        project: String,
        route: String,
        preset: FramePreset,
        css_w: Option<u32>,
        css_h: Option<u32>,
        x: f32,
        y: f32,
    ) -> Result<Self, EditError> {
        let size = FrameSize::resolve(preset, css_w, css_h)?;
        let frame = FrameItem::new(project, route, size.preset, size.css_w, size.css_h);
        frame.check()?;
        if !(x.is_finite() && y.is_finite()) {
            return Err(invalid("frame_position"));
        }
        Ok(Self {
            project: frame.project,
            route: frame.route,
            preset: size.preset,
            css_w: size.css_w,
            css_h: size.css_h,
            x,
            y,
            extent: FrameExtent::Full,
        })
    }

    /// This spec with `extent` (new frames are [`FrameExtent::Full`]; pass [`FrameExtent::Viewport`] for the
    /// v1 behaviour).
    #[must_use]
    pub fn with_extent(mut self, extent: FrameExtent) -> Self {
        self.extent = extent;
        self
    }

    /// The placed item: identity transform plus a translation to `(x, y)`, so the page box is the CSS size at
    /// scale 1.
    #[must_use]
    pub fn item(&self, id: ItemId, layer: Layer, z: i64) -> Item {
        Item {
            id,
            layer,
            z,
            transform: Affine::translate(self.x, self.y),
            kind: ItemKind::Frame(FrameItem {
                extent: self.extent,
                ..FrameItem::new(
                    self.project.clone(),
                    self.route.clone(),
                    self.preset,
                    self.css_w,
                    self.css_h,
                )
            }),
        }
    }

    /// The `Add` edit placing this frame.
    #[must_use]
    pub fn edit(&self, id: ItemId, layer: Layer, z: i64) -> Edit {
        Edit::Add(self.item(id, layer, z))
    }
}

/// [`FrameSpec::edit`] as a free function (the server's one-call path).
#[must_use]
pub fn add_frame(spec: &FrameSpec, id: ItemId, layer: Layer, z: i64) -> Edit {
    spec.edit(id, layer, z)
}

/// Height of a full-length box: the one rule every box size goes through. The live document height `layout_doc`
/// (CSS px) wins when it is finite and above 0; otherwise the synced `hint` (`0` = unknown) applies. The result
/// is clamped to `0 ..= MAX_FRAME_SIDE` (the bridge is untrusted) and is never below the viewport `css_h`.
pub(crate) fn full_box_height(css_h: f32, layout_doc: Option<f32>, hint: f32) -> f32 {
    let doc = layout_doc
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or(hint);
    crate::float::max(css_h, doc.clamp(0.0, side_px(MAX_FRAME_SIDE)))
}
