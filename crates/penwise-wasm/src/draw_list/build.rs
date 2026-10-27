//! Building a [`DrawList`] from core scene geometry (module rules in the parent's header).

use penwise_core::brush::{BlendMode, Color, GrainParams};
use penwise_core::geometry::{Dab, GeometryView, Outline, StrokeGeometry, StrokeStyle};
use penwise_core::item::{Item, ItemGeometry, ItemId, ItemKind, LayerSet, Paint, Polygon, Scene};

use super::{DAB_STRIDE, DrawKind, DrawList};

/// One `styles` record.
struct Style {
    color: Color,
    opacity: f32,
    blend: BlendMode,
    grain_scale: f32,
    grain_softness: f32,
}

impl Style {
    fn plain(color: Color, opacity: f32, blend: BlendMode) -> Self {
        Self {
            color,
            opacity,
            blend,
            grain_scale: 0.0,
            grain_softness: 0.0,
        }
    }

    /// Color-less style that only carries a group's composite blend (and images: opaque white, normal).
    fn neutral(blend: BlendMode) -> Self {
        Self::plain(Color::rgb(255, 255, 255), 1.0, blend)
    }

    fn stroke(style: &StrokeStyle, grain: Option<GrainParams>) -> Self {
        Self {
            grain_scale: grain.map_or(0.0, |g| g.scale),
            grain_softness: grain.map_or(0.0, |g| g.softness),
            ..Self::plain(style.color, style.opacity, style.blend)
        }
    }

    fn paint(paint: Paint) -> Self {
        Self::plain(paint.color, paint.opacity, BlendMode::Normal)
    }
}

impl DrawList {
    /// Committed items of `scene` on `layers` in `render_order`, each followed by its masks (module rules).
    #[must_use]
    pub fn from_scene(scene: &Scene, layers: LayerSet) -> Self {
        let mut list = Self::default();
        for id in scene.render_order(layers) {
            let (Some(item), Some(geometry)) = (scene.get(id), scene.geometry(id)) else {
                continue;
            };
            let masks = masks_of(scene, id);
            if masks.is_empty() {
                list.push_item(item, geometry);
                continue;
            }
            let group = list.push_style(&Style::neutral(item_blend(item)));
            list.command(DrawKind::GroupBegin, 0, 0, group, 0);
            list.push_item(item, geometry);
            for (path, radius) in masks {
                list.push_mask(path, radius, group);
            }
            list.command(DrawKind::GroupEnd, 0, 0, group, 0);
        }
        list
    }

    /// Appends one live stroke's geometry (outline: `OutlineView::write_polygon` as one nonzero run; stamps:
    /// stable then tentative dabs as one `Dabs` command).
    pub fn push_live(&mut self, view: &GeometryView<'_>) {
        match view {
            GeometryView::Outline { style, outline } => {
                let mut polygon = Vec::new();
                outline.write_polygon(&mut polygon);
                if polygon.is_empty() {
                    return;
                }
                let style = self.push_style(&Style::stroke(style, None));
                let first = self.runs.len();
                self.push_run(polygon);
                self.command(DrawKind::PathNonZero, first, 1, style, 0);
            }
            GeometryView::Stamps {
                style,
                grain,
                stable,
                tentative,
            } => self.push_dabs(style, *grain, &[stable, tentative]),
        }
    }

    fn push_item(&mut self, item: &Item, geometry: &ItemGeometry) {
        match geometry {
            ItemGeometry::Stroke { geometry, .. } => self.push_stroke(geometry),
            ItemGeometry::Shape(shape) => {
                if let Some((color, interior)) = &shape.fill {
                    let style = Style::plain(*color, 1.0, BlendMode::Normal);
                    self.push_outline(&style, interior);
                }
                self.push_stroke(&shape.stroke);
            }
            ItemGeometry::Fill { polygons, paint } => {
                for polygon in polygons {
                    self.push_polygon(polygon, *paint);
                }
            }
            ItemGeometry::Text { corners, paint } => {
                if let ItemKind::Text(text) = &item.kind {
                    let style = self.push_style(&Style::paint(*paint));
                    let aux = self.texts.len();
                    self.texts.push(text.content.clone());
                    self.text_sizes.push(text.font_size);
                    self.push_box(DrawKind::Text, corners, style, aux);
                }
            }
            ItemGeometry::Image { corners } => {
                if let ItemKind::Image(image) = &item.kind {
                    let style = self.push_style(&Style::neutral(BlendMode::Normal));
                    let aux = self.images.len();
                    self.images.push(hex(&image.asset));
                    self.push_box(DrawKind::Image, corners, style, aux);
                }
            }
            ItemGeometry::Erase { .. } => {}
        }
    }

    fn push_stroke(&mut self, geometry: &StrokeGeometry) {
        match geometry {
            StrokeGeometry::Outline { style, outline } => {
                self.push_outline(&Style::stroke(style, None), outline);
            }
            StrokeGeometry::Stamps { style, grain, dabs } => {
                self.push_dabs(style, *grain, &[dabs]);
            }
        }
    }

    /// All contours of `outline` as one nonzero path.
    fn push_outline(&mut self, style: &Style, outline: &Outline) {
        let first = self.runs.len();
        let mut start = 0_usize;
        for &end in &outline.contour_ends {
            let end = end as usize;
            if let Some(contour) = outline.points.get(start..end).filter(|c| !c.is_empty()) {
                self.push_run(contour.iter().copied());
            }
            start = end;
        }
        let count = self.runs.len() - first;
        if count > 0 {
            let style = self.push_style(style);
            self.command(DrawKind::PathNonZero, first, count, style, 0);
        }
    }

    /// Outer ring then holes as one even-odd path.
    fn push_polygon(&mut self, polygon: &Polygon, paint: Paint) {
        if polygon.outer.is_empty() {
            return;
        }
        let first = self.runs.len();
        self.push_run(polygon.outer.iter().copied());
        for hole in polygon.holes.iter().filter(|h| !h.is_empty()) {
            self.push_run(hole.iter().copied());
        }
        let count = self.runs.len() - first;
        let style = self.push_style(&Style::paint(paint));
        self.command(DrawKind::PathEvenOdd, first, count, style, 0);
    }

    /// Stable then tentative dabs as one `Dabs` command.
    fn push_dabs(&mut self, style: &StrokeStyle, grain: Option<GrainParams>, parts: &[&[Dab]]) {
        let first = self.dabs.len() / DAB_STRIDE;
        for dab in parts.iter().copied().flatten() {
            self.dabs.extend_from_slice(&[
                dab.x,
                dab.y,
                dab.radius_x,
                dab.radius_y,
                dab.angle,
                dab.opacity,
                dab.hardness,
                dab.grain_strength,
                dab.grain_level,
            ]);
        }
        let count = self.dabs.len() / DAB_STRIDE - first;
        if count > 0 {
            let style = self.push_style(&Style::stroke(style, grain));
            self.command(DrawKind::Dabs, first, count, style, 0);
        }
    }

    /// One mask polyline; a single point is doubled so round caps still draw its dot.
    fn push_mask(&mut self, path: &[[f32; 2]], radius: f32, group: u32) {
        let first = self.runs.len();
        let dot = path.first().filter(|_| path.len() == 1).copied();
        self.push_run(path.iter().copied().chain(dot));
        let aux = self.mask_widths.len();
        self.mask_widths.push(2.0 * radius);
        self.command(DrawKind::Mask, first, 1, group, aux);
    }

    /// Text/image box: one run of four corners.
    fn push_box(&mut self, kind: DrawKind, corners: &[[f32; 2]; 4], style: u32, aux: usize) {
        let first = self.runs.len();
        self.push_run(corners.iter().copied());
        self.command(kind, first, 1, style, aux);
    }

    fn push_run(&mut self, points: impl IntoIterator<Item = [f32; 2]>) {
        for p in points {
            self.points.extend_from_slice(&p);
        }
        self.runs.push(index(self.points.len() / 2));
    }

    fn push_style(&mut self, style: &Style) -> u32 {
        let id = index(self.styles.len() / super::STYLE_STRIDE);
        let Color { r, g, b, a } = style.color;
        self.styles.extend_from_slice(&[
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            f32::from(a) / 255.0,
            style.opacity,
            if style.blend == BlendMode::Multiply {
                1.0
            } else {
                0.0
            },
            style.grain_scale,
            style.grain_softness,
        ]);
        id
    }

    fn command(&mut self, kind: DrawKind, first: usize, count: usize, style: u32, aux: usize) {
        self.commands.extend_from_slice(&[
            kind as u32,
            index(first),
            index(count),
            style,
            index(aux),
        ]);
    }
}

/// `(page path, page radius)` of `target`'s live non-empty masks, ascending mask id.
fn masks_of(scene: &Scene, target: ItemId) -> Vec<(&[[f32; 2]], f32)> {
    scene
        .masks(target)
        .into_iter()
        .filter_map(|mask| match scene.geometry(mask) {
            Some(ItemGeometry::Erase { path, radius }) if !path.is_empty() => {
                Some((path.as_slice(), *radius))
            }
            _ => None,
        })
        .collect()
}

/// The blend an item's group composites with (as the server raster does).
fn item_blend(item: &Item) -> BlendMode {
    match &item.kind {
        ItemKind::Stroke(s) => s.params.blend,
        ItemKind::Shape(s) => s.brush.blend,
        _ => BlendMode::Normal,
    }
}

/// Buffer index as `u32` (saturating: a list never gets near 2³² entries).
fn index(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

fn hex(hash: &[u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in hash {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    out
}
