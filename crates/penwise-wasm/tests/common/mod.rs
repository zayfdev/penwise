//! Shared builders: ids, items, scenes and wire ops. Items go through the same constructors the tablet uses
//! (`BrushParams::for_preset`, quantized samples).
#![allow(dead_code, reason = "each test binary uses a subset")]

use penwise_core::brush::{BrushParams, Color};
use penwise_core::edit::Edit;
use penwise_core::item::{
    Affine, EraseItem, FillItem, ImageItem, Item, ItemId, ItemKind, Layer, PageKind as SceneKind,
    Paint, Polygon, Scene, ShapeItem, ShapeKind, StrokeItem, TextItem,
};
use penwise_core::sample::Sample;
use penwise_proto::{
    AnyOp, Background, DeviceId, Hlc, HlcClock, NotebookChange, NotebookId, NotebookOp, Op, OpId,
    PageChange, PageId, PageKind, PageOp,
};

/// Ballpoint (outline family), pencil (stamp family), highlighter (multiply outline) in `Preset::ALL`.
pub const BALLPOINT: usize = 0;
pub const HIGHLIGHTER: usize = 5;
pub const PENCIL: usize = 6;

pub fn item_id(n: u64) -> ItemId {
    ItemId(penwise_proto::ItemRef::from_parts(1_700_000_000_000, u128::from(n)).0)
}

pub fn device(n: u64) -> DeviceId {
    DeviceId::from_parts(1_700_000_000_000, u128::from(n))
}

pub fn page_id(n: u64) -> PageId {
    PageId::from_parts(1_700_000_000_000, u128::from(n))
}

pub fn notebook_id(n: u64) -> NotebookId {
    NotebookId::from_parts(1_700_000_000_000, u128::from(n))
}

pub fn op_id(n: u64) -> OpId {
    OpId::from_parts(1_700_000_000_000, u128::from(n))
}

pub fn hlc(wall_ms: u64, counter: u16) -> Hlc {
    Hlc {
        wall_ms,
        counter,
        device: device(1),
    }
}

fn item(id: ItemId, z: i64, kind: ItemKind) -> Item {
    Item {
        id,
        layer: Layer::Ink,
        z,
        transform: Affine::IDENTITY,
        kind,
    }
}

/// A stroke of `preset` through `points` (page px).
pub fn stroke(id: ItemId, z: i64, preset: usize, points: &[[f32; 2]]) -> Item {
    let samples = points
        .iter()
        .zip(0_u32..)
        .map(|(p, i)| {
            Sample {
                x: p[0],
                y: p[1],
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: i * 8_333,
            }
            .quantize()
        })
        .collect();
    item(
        id,
        z,
        ItemKind::Stroke(StrokeItem {
            params: BrushParams::for_preset(preset, 0xFF20_4080, 1.0),
            seed: 7,
            samples,
            trim: None,
        }),
    )
}

pub const WAVE: [[f32; 2]; 6] = [
    [10.0, 10.0],
    [40.0, 30.0],
    [70.0, 12.0],
    [100.0, 34.0],
    [130.0, 14.0],
    [160.0, 30.0],
];

/// Rectangle `100 × 60` at `(200, 200)` outlined with `preset`, optionally filled.
pub fn shape(id: ItemId, z: i64, preset: usize, fill: Option<Color>) -> Item {
    let mut it = item(
        id,
        z,
        ItemKind::Shape(ShapeItem {
            kind: ShapeKind::Rect,
            size: [100.0, 60.0],
            brush: BrushParams::for_preset(preset, 0xFF11_2233, 1.0),
            fill,
        }),
    );
    it.transform = Affine::translate(250.0, 230.0);
    it
}

/// Square `[300, 400]²` with a square hole.
pub fn fill(id: ItemId, z: i64) -> Item {
    let outer = vec![
        [300.0, 300.0],
        [400.0, 300.0],
        [400.0, 400.0],
        [300.0, 400.0],
    ];
    let hole = vec![
        [330.0, 330.0],
        [330.0, 370.0],
        [370.0, 370.0],
        [370.0, 330.0],
    ];
    item(
        id,
        z,
        ItemKind::Fill(FillItem {
            polygons: vec![Polygon {
                outer,
                holes: vec![hole],
            }],
            paint: Paint {
                color: Color::rgb(200, 100, 50),
                opacity: 0.5,
            },
        }),
    )
}

pub fn text(id: ItemId, z: i64, content: &str) -> Item {
    item(
        id,
        z,
        ItemKind::Text(TextItem {
            content: content.to_owned(),
            size: [120.0, 40.0],
            font_size: 24.0,
            paint: Paint {
                color: Color::rgb(0, 0, 0),
                opacity: 1.0,
            },
            sources: Vec::new(),
        }),
    )
}

pub fn image(id: ItemId, z: i64, asset: [u8; 32]) -> Item {
    item(
        id,
        z,
        ItemKind::Image(ImageItem {
            asset,
            w: 64.0,
            h: 32.0,
        }),
    )
}

/// Precise-eraser mask over `target`, one capsule through `path`.
pub fn erase(id: ItemId, z: i64, target: ItemId, path: &[[f32; 2]], radius: f32) -> Item {
    item(
        id,
        z,
        ItemKind::Erase(EraseItem {
            target,
            path: path.to_vec(),
            radius,
        }),
    )
}

pub fn scene(items: impl IntoIterator<Item = Item>) -> Scene {
    let mut scene = Scene::new(SceneKind::Fixed {
        width: 1000.0,
        height: 1000.0,
    });
    for it in items {
        scene.apply(&Edit::Add(it)).unwrap();
    }
    scene
}

pub fn item_op(clock: &mut HlcClock, now: u64, n: u64, page: PageId, edit: Edit) -> AnyOp {
    let hlc = clock.send(now);
    AnyOp::Item(Box::new(Op {
        op_id: op_id(n),
        device: hlc.device,
        hlc,
        page,
        edit,
    }))
}

pub fn notebook_create(
    clock: &mut HlcClock,
    n: u64,
    notebook: NotebookId,
    title: &str,
    position: i64,
) -> AnyOp {
    let hlc = clock.send(1_700_000_000_000);
    AnyOp::Notebook(NotebookOp {
        op_id: op_id(n),
        device: hlc.device,
        hlc,
        notebook,
        change: NotebookChange::Create {
            title: title.to_owned(),
            position,
        },
    })
}

pub fn page_create(
    clock: &mut HlcClock,
    n: u64,
    page: PageId,
    notebook: NotebookId,
    title: &str,
    position: i64,
) -> AnyOp {
    let hlc = clock.send(1_700_000_000_000);
    AnyOp::Page(PageOp {
        op_id: op_id(n),
        device: hlc.device,
        hlc,
        page,
        change: PageChange::Create {
            notebook,
            title: title.to_owned(),
            position,
            kind: PageKind::Fixed {
                width: 1000.0,
                height: 800.0,
            },
            background: Background::Lined,
        },
    })
}
