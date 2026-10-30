//! Valid items of every kind for the slice e codec and undo tests.

use penwise_core::item::{
    Affine, ArrowHead, EraseItem, FillItem, FrameExtent, FrameItem, FramePreset, ImageItem, Item,
    ItemId, ItemKind, Layer, Paint, Polygon, SampleParam, SampleRange, ShapeItem, ShapeKind,
    StrokeItem, TextItem, Trim,
};
use penwise_core::smoothing::{HistoryPoint, PositionState, PressureState, SmootherState};
use penwise_core::{Color, Preset, Sample};

pub fn id(n: u8) -> ItemId {
    let mut bytes = [0_u8; 16];
    bytes[0] = 0x01;
    bytes[15] = n;
    ItemId(bytes)
}

pub const RED: Color = Color {
    r: 200,
    g: 10,
    b: 20,
    a: 255,
};

pub const BLUE: Color = Color {
    r: 0,
    g: 40,
    b: 220,
    a: 128,
};

/// `n` quantized samples along a gentle curve starting at `(x0, y0)`.
pub fn samples(n: u32, x0: f32, y0: f32) -> Vec<Sample> {
    (0..n)
        .map(|i| {
            let t = f32::from(u16::try_from(i).unwrap());
            Sample {
                x: x0 + t * 3.25,
                y: y0 + t * t * 0.125,
                pressure: 0.3 + 0.04 * t,
                tilt: 0.2,
                azimuth: -1.0 + 0.1 * t,
                t_us: i * 2075,
            }
            .quantize()
        })
        .collect()
}

pub fn item(id: ItemId, z: i64, kind: ItemKind) -> Item {
    Item {
        id,
        layer: Layer::Ink,
        z,
        transform: Affine::IDENTITY,
        kind,
    }
}

/// Untrimmed ballpoint stroke.
pub fn stroke(id: ItemId, z: i64, x0: f32) -> Item {
    item(
        id,
        z,
        ItemKind::Stroke(StrokeItem {
            params: Preset::Ballpoint.params(),
            seed: 0x0123_4567_89AB_CDEF,
            samples: samples(16, x0, 40.0),
            trim: None,
            binding: None,
        }),
    )
}

/// Untrimmed pencil (stamp family) stroke: precise erase masks it.
pub fn stamp_stroke(id: ItemId, z: i64, x0: f32) -> Item {
    item(
        id,
        z,
        ItemKind::Stroke(StrokeItem {
            params: Preset::Pencil.params(),
            seed: 77,
            samples: samples(12, x0, 90.0),
            trim: None,
            binding: None,
        }),
    )
}

/// Precise-erase piece of `origin`: origin samples 4..16 stored, renders `[6, 10]`.
pub fn piece(id: ItemId, origin: ItemId, z: i64, x0: f32) -> Item {
    let all = samples(16, x0, 40.0);
    item(
        id,
        z,
        ItemKind::Stroke(StrokeItem {
            params: Preset::Ballpoint.params(),
            seed: 0x0123_4567_89AB_CDEF,
            samples: all[4..].to_vec(),
            trim: Some(Trim {
                origin,
                sample_offset: 4,
                visible: vec![
                    SampleRange {
                        start: SampleParam(0),
                        end: SampleParam((2 << 12) | 1024),
                        start_cut: false,
                        end_cut: true,
                    },
                    SampleRange {
                        start: SampleParam(6 << 12),
                        end: SampleParam(10 << 12),
                        start_cut: true,
                        end_cut: false,
                    },
                ],
                own: 1,
                warm: Some(SmootherState {
                    position: PositionState {
                        x: 101.5,
                        y: 202.25,
                        dx: -3.5,
                        dy: 12.0,
                        t_us: 8300,
                        dt_s: 0.002_075,
                    },
                    pressure: PressureState {
                        value: 0.4,
                        derivative: 0.01,
                        t_us: 8300,
                        dt_s: 0.002_075,
                    },
                    history: vec![
                        HistoryPoint {
                            x: 101.0,
                            y: 201.5,
                            t_us: 6225,
                        },
                        HistoryPoint {
                            x: 101.5,
                            y: 202.25,
                            t_us: 8300,
                        },
                    ],
                }),
            }),
            binding: None,
        }),
    )
}

/// Mask on `target` (path on the 1/16 px grid).
pub fn erase(id: ItemId, target: ItemId, z: i64) -> Item {
    item(
        id,
        z,
        ItemKind::Erase(EraseItem {
            target,
            path: vec![[10.0, 90.0], [12.5, 90.0625], [15.0, 91.0]],
            radius: 4.5,
        }),
    )
}

pub fn shape(id: ItemId, z: i64, kind: ShapeKind) -> Item {
    let mut it = item(
        id,
        z,
        ItemKind::Shape(ShapeItem {
            kind,
            size: [120.0, 80.0],
            brush: Preset::Fineliner.params(),
            fill: Some(BLUE),
        }),
    );
    it.transform = Affine {
        a: 0.0,
        b: 1.0,
        c: -1.0,
        d: 0.0,
        e: 300.0,
        f: 200.0,
    };
    it
}

pub fn square(x: f32, y: f32, side: f32) -> Vec<[f32; 2]> {
    vec![[x, y], [x + side, y], [x + side, y + side], [x, y + side]]
}

pub fn fill(id: ItemId, z: i64) -> Item {
    let mut hole = square(20.0, 20.0, 10.0);
    hole.reverse();
    item(
        id,
        z,
        ItemKind::Fill(FillItem {
            polygons: vec![
                Polygon {
                    outer: square(0.0, 0.0, 64.0),
                    holes: vec![hole],
                },
                Polygon {
                    outer: square(100.0, 0.5, 16.0625),
                    holes: Vec::new(),
                },
            ],
            paint: Paint {
                color: RED,
                opacity: 0.75,
            },
        }),
    )
}

pub fn text(id: ItemId, z: i64, sources: Vec<ItemId>) -> Item {
    let mut it = item(
        id,
        z,
        ItemKind::Text(TextItem {
            content: "Grüße, ink ✍".to_owned(),
            size: [180.0, 32.0],
            font_size: 18.5,
            paint: Paint {
                color: BLUE,
                opacity: 1.0,
            },
            sources,
        }),
    );
    it.layer = Layer::Text;
    it.transform = Affine {
        e: 12.0,
        f: 500.0,
        ..Affine::IDENTITY
    };
    it
}

/// A 300 × 200 picture rotated a little and moved.
pub fn image(id: ItemId, z: i64) -> Item {
    let mut asset = [0_u8; 32];
    for (i, b) in asset.iter_mut().enumerate() {
        *b = u8::try_from(i).unwrap() * 7 + 1;
    }
    let mut it = item(
        id,
        z,
        ItemKind::Image(ImageItem {
            asset,
            w: 300.0,
            h: 200.0,
        }),
    );
    it.transform = Affine {
        a: 0.9,
        b: 0.1,
        c: -0.1,
        d: 0.9,
        e: 64.0,
        f: -32.5,
    };
    it
}

/// A phone-sized frame moved up the page (`css_w × css_h` = 390 × 844 page units).
pub fn frame(id: ItemId, z: i64) -> Item {
    let mut it = item(
        id,
        z,
        ItemKind::Frame(FrameItem {
            project: "portfolio".to_owned(),
            route: "/work?tab=2".to_owned(),
            preset: FramePreset::Phone,
            css_w: 390,
            css_h: 844,
            patch: Some(r#"{"css":"h1{color:red}"}"#.to_owned()),
            label: "variant A".to_owned(),
            group: Some("grp-1".to_owned()),
            chosen: true,
            extent: FrameExtent::Full,
            doc_h: 5120,
        }),
    );
    it.transform = Affine::translate(100.75, -40.0);
    it
}

/// Every shape kind.
pub fn shape_kinds() -> Vec<ShapeKind> {
    vec![
        ShapeKind::Rect,
        ShapeKind::RoundedRect { radius: 6.5 },
        ShapeKind::Ellipse,
        ShapeKind::Line,
        ShapeKind::Arrow {
            head: ArrowHead::End,
        },
        ShapeKind::Arrow {
            head: ArrowHead::Both,
        },
        ShapeKind::Polygon {
            points: vec![[-0.5, 0.5], [0.0, -0.5], [0.5, 0.5], [0.25, 0.125]],
        },
    ]
}

/// One valid item of every kind and variant (layers, negative z, non-identity transforms included).
pub fn every_kind() -> Vec<Item> {
    let mut items = vec![
        stroke(id(1), 1 << 16, 10.0),
        stamp_stroke(id(2), -(1 << 40), 30.0),
        piece(id(3), id(1), i64::MAX, 10.0),
        erase(id(4), id(2), i64::MIN),
        fill(id(5), 0),
        text(id(6), 7, vec![id(1), id(2)]),
        text(id(7), 8, Vec::new()),
        image(id(9), -5),
        frame(id(10), 1 << 20),
    ];
    let mut agent = stroke(id(8), 9, 0.0);
    agent.layer = Layer::Agent;
    agent.transform = Affine {
        a: 1.5,
        b: 0.25,
        c: -0.125,
        d: 0.75,
        e: -40.0,
        f: 1.0e5,
    };
    items.push(agent);
    for (k, kind) in (20_u8..).zip(shape_kinds()) {
        let mut s = shape(id(k), i64::from(k), kind);
        if k % 2 == 0
            && let ItemKind::Shape(data) = &mut s.kind
        {
            data.fill = None;
        }
        items.push(s);
    }
    items
}
