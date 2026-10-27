//! Shared test helpers: deterministic RNG, id makers, item builders and op wrappers.
#![allow(dead_code, reason = "each test binary uses a subset")]

pub mod world;

use penwise_core::brush::{BrushParams, Color};
use penwise_core::edit::Edit;
use penwise_core::item::{
    Affine, Item, ItemId, ItemKind, Layer, Paint, ShapeItem, ShapeKind, StrokeItem, TextItem,
};
use penwise_core::sample::Sample;
use penwise_proto::{
    AnyOp, DeviceId, Hlc, HlcClock, NotebookChange, NotebookId, NotebookOp, Op, OpId, PageChange,
    PageId, PageOp,
};

/// splitmix64: tiny, deterministic, good enough for test data.
#[derive(Debug, Clone)]
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next_u64() % u64::try_from(n).unwrap()).unwrap()
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.next_u64() % 100 < percent
    }

    /// Uniform float in `[lo, hi)`.
    #[allow(clippy::cast_precision_loss)]
    pub fn float(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * ((self.next_u64() >> 40) as f32 / (1_u64 << 24) as f32)
    }

    pub fn u128(&mut self) -> u128 {
        (u128::from(self.next_u64()) << 64) | u128::from(self.next_u64())
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

pub fn device(n: u64) -> DeviceId {
    DeviceId::from_parts(1_700_000_000_000, u128::from(n))
}

pub fn op_id(n: u64) -> OpId {
    OpId::from_parts(1_700_000_000_000, u128::from(n))
}

pub fn page_id(n: u64) -> PageId {
    PageId::from_parts(1_700_000_000_000, u128::from(n))
}

pub fn notebook_id(n: u64) -> NotebookId {
    NotebookId::from_parts(1_700_000_000_000, u128::from(n))
}

pub fn item_id(n: u64) -> ItemId {
    ItemId(penwise_proto::ItemRef::from_parts(1_700_000_000_000, u128::from(n)).0)
}

pub fn hlc(wall_ms: u64, counter: u16, dev: u64) -> Hlc {
    Hlc {
        wall_ms,
        counter,
        device: device(dev),
    }
}

pub fn text_item(id: ItemId, z: i64, content: &str) -> Item {
    Item {
        id,
        layer: Layer::Ink,
        z,
        transform: Affine::IDENTITY,
        kind: ItemKind::Text(TextItem {
            content: content.to_owned(),
            size: [120.0, 40.0],
            font_size: 24.0,
            paint: Paint {
                color: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                opacity: 1.0,
            },
            sources: Vec::new(),
        }),
    }
}

pub fn shape_item(id: ItemId, z: i64) -> Item {
    Item {
        id,
        layer: Layer::Ink,
        z,
        transform: Affine::translate(10.0, 20.0),
        kind: ItemKind::Shape(ShapeItem {
            kind: ShapeKind::Rect,
            size: [100.0, 60.0],
            brush: BrushParams::for_preset(0, 0xFF11_2233, 1.0),
            fill: None,
        }),
    }
}

pub fn stroke_item(id: ItemId, z: i64, seed: u64) -> Item {
    let samples = (0..6_u16)
        .map(|i| {
            Sample {
                x: f32::from(i) * 8.0,
                y: 5.0 + f32::from(i % 2) * 3.0,
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: u32::from(i) * 8_000,
            }
            .quantize()
        })
        .collect();
    Item {
        id,
        layer: Layer::Ink,
        z,
        transform: Affine::IDENTITY,
        kind: ItemKind::Stroke(StrokeItem {
            params: BrushParams::for_preset(0, 0xFF00_0000, 1.0),
            seed,
            samples,
            trim: None,
        }),
    }
}

/// Item op authored by `clock` at physical time `now`.
pub fn item_op(clock: &mut HlcClock, now: u64, op: u64, page: PageId, edit: Edit) -> AnyOp {
    let hlc = clock.send(now);
    AnyOp::Item(Box::new(Op {
        op_id: op_id(op),
        device: hlc.device,
        hlc,
        page,
        edit,
    }))
}

pub fn page_op(clock: &mut HlcClock, now: u64, op: u64, page: PageId, change: PageChange) -> AnyOp {
    let hlc = clock.send(now);
    AnyOp::Page(PageOp {
        op_id: op_id(op),
        device: hlc.device,
        hlc,
        page,
        change,
    })
}

pub fn notebook_op(
    clock: &mut HlcClock,
    now: u64,
    op: u64,
    notebook: NotebookId,
    change: NotebookChange,
) -> AnyOp {
    let hlc = clock.send(now);
    AnyOp::Notebook(NotebookOp {
        op_id: op_id(op),
        device: hlc.device,
        hlc,
        notebook,
        change,
    })
}
