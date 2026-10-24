#![allow(
    clippy::cast_precision_loss,
    reason = "small test indices converted to page coordinates"
)]

use super::super::FILL_CELL;
use super::super::grid::{Window, get_bit, reset, words_for};
use super::super::scan::clear_capsule;
use super::{chain, draw, rasterize_walls};
use crate::edit::Edit;
use crate::item::{
    Affine, EraseItem, Item, ItemGeometry, ItemId, ItemKind, Layer, LayerSet, PageKind, Scene,
    StrokeItem, Z_STEP,
};
use crate::{Preset, Sample};

const STROKES: u8 = 24;

/// Solid stamp stroke from `a` to `b`, 101 quantized samples.
fn stroke(k: u8, a: [f32; 2], b: [f32; 2]) -> Item {
    let n = 100u32;
    let samples = (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            Sample {
                x: a[0] + (b[0] - a[0]) * t,
                y: a[1] + (b[1] - a[1]) * t,
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: i * 2000,
            }
            .quantize()
        })
        .collect();
    Item {
        id: ItemId([k; 16]),
        layer: Layer::Ink,
        z: i64::from(k) * Z_STEP,
        transform: Affine::IDENTITY,
        kind: ItemKind::Stroke(StrokeItem {
            params: Preset::Solid.params(),
            seed: u64::from(k),
            samples,
            trim: None,
        }),
    }
}

/// `STROKES` horizontal solid strokes 20 px apart, each cut by its own vertical mask at a different x.
fn masked_scene() -> Scene {
    let mut scene = Scene::new(PageKind::Infinite);
    for k in 1..=STROKES {
        let y = -240.0 + 20.0 * f32::from(k);
        scene
            .apply(&Edit::Add(stroke(k, [-200.0, y], [200.0, y])))
            .unwrap();
        let x = -150.0 + 10.0 * f32::from(k);
        let mask = Item {
            id: ItemId([k + 100; 16]),
            layer: Layer::Ink,
            z: i64::from(k) * Z_STEP,
            transform: Affine::IDENTITY,
            kind: ItemKind::Erase(EraseItem {
                target: ItemId([k; 16]),
                path: vec![[x, y - 20.0], [x, y + 20.0]],
                radius: 4.0,
            }),
        };
        scene.apply(&Edit::Add(mask)).unwrap();
    }
    scene
}

/// The pre-scratch algorithm: every masked item drawn into a full-window layer, masks cleared, merged.
fn reference(scene: &Scene, window: &Window) -> Vec<u64> {
    let mut bits = Vec::new();
    reset(&mut bits, window.width, window.height);
    for id in scene.query(&window.page_bounds().inflate(FILL_CELL), LayerSet::INK) {
        let (Some(item), Some(geometry)) = (scene.get(id), scene.geometry(id)) else {
            continue;
        };
        if !matches!(item.kind, ItemKind::Stroke(_)) {
            continue;
        }
        let mut layer = Vec::new();
        reset(&mut layer, window.width, window.height);
        draw(window, geometry, 0.0, &mut layer);
        for mask in scene.masks(id) {
            if let Some(ItemGeometry::Erase { path, radius }) = scene.geometry(mask) {
                chain(path, false, |a, b| {
                    clear_capsule(window, a, b, *radius, &mut layer);
                });
            }
        }
        for (w, l) in bits.iter_mut().zip(&layer) {
            *w |= l;
        }
    }
    bits
}

#[test]
fn masked_items_rasterize_like_full_window_layers() {
    let scene = masked_scene();
    // Width 509: item rows start mid-word, exercising the shifted merge.
    let window = Window {
        origin: [-253, -251],
        width: 509,
        height: 500,
        page_edges: [false; 4],
    };
    let (mut bits, mut layer) = (Vec::new(), Vec::new());
    rasterize_walls(&scene, &window, &mut bits, &mut layer);
    let expected = reference(&scene, &window);
    assert!(expected.iter().any(|&w| w != 0), "solid strokes are walls");
    assert_eq!(bits, expected);
    // The first stroke's mask cut it: wall left of the cut, none at the cut.
    let cell = |p: [f32; 2]| window.cell_of(p).unwrap();
    let [x, y] = cell([-140.0, -220.0]);
    assert!(!get_bit(&bits, window.width, x, y));
    let [x, y] = cell([-180.0, -220.0]);
    assert!(get_bit(&bits, window.width, x, y));
}

#[test]
fn masked_items_reuse_a_row_bounded_layer() {
    let scene = masked_scene();
    let window = Window::around([0.0, 0.0], 512, PageKind::Infinite).unwrap();
    let (mut bits, mut layer) = (Vec::new(), Vec::new());
    rasterize_walls(&scene, &window, &mut bits, &mut layer);
    // Each item's layer spans only the rows its bounds reach (a ~15 px stroke plus margins), far below the
    // 512-row window.
    let cap = layer.capacity();
    assert!(cap > 0);
    assert!(cap < words_for(window.width, 64), "layer capacity {cap}");
    // A second pass over all 24 masked items reuses the same buffer: no per-item (re)allocation.
    let ptr = layer.as_ptr();
    rasterize_walls(&scene, &window, &mut bits, &mut layer);
    assert_eq!(layer.as_ptr(), ptr);
    assert_eq!(layer.capacity(), cap);
}
