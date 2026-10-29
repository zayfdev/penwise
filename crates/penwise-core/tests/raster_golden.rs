//! Server raster PNG goldens (core-determinism.md §3): render synthetic strokes and compare **decoded pixels**
//! with `tests/goldens/raster_*.png` under `PixelTolerance::GOLDEN` (max channel delta ≤ 2, < 0.1 % of pixels
//! differing). Pixels are platform-rasterized, so these goldens are native/server-only (feature `raster`).
//!
//! Bless (write/overwrite the goldens): `just bless` (`PENWISE_BLESS=1 cargo test -p penwise-core --features
//! raster`); `PENWISE_BLESS` counts when set to anything but empty or `0`. A missing golden fails with that hint.
//! On mismatch, `actual` and `diff` PNGs are written to `target/penwise-golden/`.
//!
//! Owner: slice e.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "test harness: helpers outside #[test] fns report failures directly"
)]

use penwise_core::brush::{BlendMode, Color};
use penwise_core::codec::decode_pwrec;
use penwise_core::geometry::{Dab, Outline, StrokeGeometry, StrokeStyle};
use penwise_core::raster::png::{
    PixelTolerance, compare_pixels, decode_png, diff_image, encode_png,
};
use penwise_core::raster::sheet::{SheetOptions, contact_strokes, render_brush_sheet};
use penwise_core::raster::stamps::CoverageBuffer;
use penwise_core::raster::tiny_skia::{Color as SkColor, Pixmap};
use penwise_core::raster::{Viewport, render_strokes};
use penwise_core::{Preset, Sample, StrokeBuilder};
use std::path::PathBuf;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 120;

/// A wavy left-to-right stroke at 482 Hz with a pressure swell and moderate tilt toward the lower right.
fn wave(y_center: f32, amplitude: f32) -> Vec<Sample> {
    (0..160u16)
        .map(|i| {
            let t = f32::from(i) / 159.0;
            Sample {
                x: 24.0 + 272.0 * t,
                y: y_center + amplitude * libm::sinf(t * 2.0 * core::f32::consts::TAU),
                pressure: 0.25 + 0.6 * libm::sinf(t * core::f32::consts::PI),
                tilt: 0.6,
                azimuth: 2.2,
                t_us: u32::from(i) * 2075,
            }
        })
        .collect()
}

fn straight(y: f32) -> Vec<Sample> {
    (0..80u16)
        .map(|i| Sample {
            x: 16.0 + 3.6 * f32::from(i),
            y,
            pressure: 0.5,
            tilt: 0.0,
            azimuth: 0.0,
            t_us: u32::from(i) * 2075,
        })
        .collect()
}

fn build(preset: Preset, seed: u64, samples: &[Sample]) -> StrokeGeometry {
    StrokeBuilder::build(&preset.params(), seed, samples)
}

fn render(strokes: &[StrokeGeometry]) -> Pixmap {
    let mut pixmap = Pixmap::new(WIDTH, HEIGHT).unwrap();
    pixmap.fill(SkColor::WHITE);
    let mut scratch = CoverageBuffer::default();
    render_strokes(strokes, &Viewport::IDENTITY, &mut pixmap, &mut scratch).unwrap();
    pixmap
}

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

fn artifacts_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"),
            PathBuf::from,
        )
        .join("penwise-golden")
}

/// `PENWISE_BLESS` set to anything but empty or `0` (same rule as `tests/golden.rs`).
fn blessing() -> bool {
    std::env::var_os("PENWISE_BLESS").is_some_and(|v| !v.is_empty() && v != "0")
}

/// Compares `actual` with `tests/goldens/<name>.png`; rewrites it when [`blessing`]. A missing golden fails.
fn check_golden(name: &str, actual: &Pixmap) {
    let path = goldens_dir().join(format!("{name}.png"));
    if blessing() {
        std::fs::create_dir_all(goldens_dir()).unwrap();
        std::fs::write(&path, encode_png(actual).unwrap()).unwrap();
        eprintln!("blessed {}", path.display());
        return;
    }
    assert!(
        path.exists(),
        "missing golden {}; run `just bless` (PENWISE_BLESS=1) to create it",
        path.display()
    );
    let expected = decode_png(&std::fs::read(&path).unwrap()).unwrap();
    // Round-trip through PNG so both sides are compared exactly as stored.
    let actual = decode_png(&encode_png(actual).unwrap()).unwrap();
    let diff = compare_pixels(&expected, &actual).unwrap();
    if diff.passes(PixelTolerance::GOLDEN) {
        return;
    }
    let out = artifacts_dir();
    std::fs::create_dir_all(&out).unwrap();
    let image = diff_image(&expected, &actual, PixelTolerance::GOLDEN).unwrap();
    std::fs::write(
        out.join(format!("{name}-actual.png")),
        encode_png(&actual).unwrap(),
    )
    .unwrap();
    std::fs::write(
        out.join(format!("{name}-diff.png")),
        encode_png(&image).unwrap(),
    )
    .unwrap();
    panic!(
        "{name}: {diff:?} exceeds {:?}; artifacts in {} (re-bless with PENWISE_BLESS=1 if intended)",
        PixelTolerance::GOLDEN,
        out.display()
    );
}

fn dab(
    x: f32,
    y: f32,
    radius_x: f32,
    radius_y: f32,
    angle: f32,
    opacity: f32,
    hardness: f32,
) -> Dab {
    Dab {
        x,
        y,
        radius_x,
        radius_y,
        angle,
        opacity,
        hardness,
        grain_strength: 0.0,
        grain_level: 1.0,
    }
}

/// Hand-built geometry: exercises the rasterizer alone, independent of the stroke engines.
fn primitives() -> Vec<StrokeGeometry> {
    let ink = StrokeStyle {
        color: Color::INK,
        opacity: 1.0,
        blend: BlendMode::Normal,
    };
    let triangle = Outline {
        points: vec![[20.0, 100.0], [70.0, 15.5], [110.25, 100.0]],
        contour_ends: vec![3],
    };
    let soft: Vec<Dab> = (0..40u8)
        .map(|i| dab(120.0 + 2.5 * f32::from(i), 40.0, 12.0, 12.0, 0.0, 0.3, 0.3))
        .collect();
    let chisel: Vec<Dab> = (0..12u8)
        .map(|i| {
            dab(
                130.0 + 14.0 * f32::from(i),
                85.0,
                10.0,
                3.0,
                0.15 * f32::from(i),
                0.9,
                0.9,
            )
        })
        .collect();
    let band = Outline {
        points: vec![[10.0, 55.0], [310.0, 55.0], [310.0, 75.0], [10.0, 75.0]],
        contour_ends: vec![4],
    };
    vec![
        StrokeGeometry::Outline {
            style: ink,
            outline: triangle,
        },
        StrokeGeometry::Stamps {
            style: StrokeStyle {
                color: Color::rgb(30, 90, 200),
                opacity: 0.7,
                ..ink
            },
            grain: None,
            dabs: soft,
        },
        StrokeGeometry::Stamps {
            style: StrokeStyle {
                color: Color::rgb(180, 30, 40),
                ..ink
            },
            grain: None,
            dabs: chisel,
        },
        StrokeGeometry::Outline {
            style: StrokeStyle {
                color: Color::HIGHLIGHTER,
                opacity: 0.35,
                blend: BlendMode::Multiply,
            },
            outline: band,
        },
    ]
}

#[test]
fn raster_primitives() {
    check_golden("raster_primitives", &render(&primitives()));
}

#[test]
fn raster_outline_family() {
    check_golden(
        "raster_outline",
        &render(&[build(Preset::Ballpoint, 1, &wave(60.0, 30.0))]),
    );
}

#[test]
fn raster_stamp_family() {
    check_golden(
        "raster_stamp",
        &render(&[build(Preset::Pencil, 2, &wave(60.0, 30.0))]),
    );
}

#[test]
fn raster_highlighter_over_ink() {
    // Highlighter listed last on purpose: the multiply band still renders it under the ink.
    let strokes = [
        build(Preset::Ballpoint, 3, &wave(60.0, 30.0)),
        build(Preset::Highlighter, 4, &straight(60.0)),
    ];
    check_golden("raster_highlighter_over_ink", &render(&strokes));
}

/// Real pen recording (five right→left strokes, pressure rising toward the left end), compiled in.
const PRESSURE_RAMP: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/pwrec/04-pressure-ramp.pwrec"
));

#[test]
fn raster_brush_sheet() {
    // Every preset in `Preset::ALL` order, 4 cells per row, ≤ 1200 px wide.
    let strokes = contact_strokes(&decode_pwrec(PRESSURE_RAMP).unwrap()).unwrap();
    let options = SheetOptions {
        columns: 4,
        max_width_px: 1200,
        ..SheetOptions::DEFAULT
    };
    let sheet = render_brush_sheet(&strokes, &options).unwrap();
    assert!(sheet.width() <= 1600 && sheet.height() <= 1600);
    check_golden("raster_brush_sheet", &sheet);
}

/// Scene fixture for [`raster_scene`]: a fill with a hole, a picture, a filled ellipse, a trimmed ballpoint
/// stroke and a masked pencil stroke.
mod scene_fixture {
    use penwise_core::edit::{Edit, erase_precise};
    use penwise_core::item::{
        Affine, FillItem, IdSource, ImageItem, Item, ItemId, ItemKind, Layer, LayerSet, PageKind,
        Paint, Polygon, Scene, ShapeItem, ShapeKind, StrokeItem,
    };
    use penwise_core::raster::tiny_skia::Pixmap;
    use penwise_core::{Color, Preset, Sample};
    use std::collections::BTreeMap;

    pub const ASSET: [u8; 32] = [7; 32];
    struct Counter(u128);

    impl IdSource for Counter {
        fn next_id(&mut self) -> ItemId {
            self.0 += 1;
            ItemId(self.0.to_be_bytes())
        }
    }

    fn id(n: u128) -> ItemId {
        ItemId(n.to_be_bytes())
    }

    fn ring(x: f32, y: f32, side: f32) -> Vec<[f32; 2]> {
        vec![[x, y], [x + side, y], [x + side, y + side], [x, y + side]]
    }

    fn line(y: f32, n: u16) -> Vec<Sample> {
        (0..n)
            .map(|i| {
                let t = f32::from(i) / f32::from(n - 1);
                Sample {
                    x: 20.0 + 280.0 * t,
                    y: y + 12.0 * libm::sinf(t * 6.0),
                    pressure: 0.6,
                    tilt: 0.0,
                    azimuth: 0.0,
                    t_us: u32::from(i) * 2075,
                }
                .quantize()
            })
            .collect()
    }

    fn item(n: u128, z: i64, kind: ItemKind, transform: Affine) -> Item {
        Item {
            id: id(n),
            layer: Layer::Ink,
            z,
            transform,
            kind,
        }
    }

    fn stroke(n: u128, preset: Preset, y: f32, color: Color) -> Item {
        let mut params = preset.params();
        params.color = color;
        let samples = line(y, 280);
        let kind = ItemKind::Stroke(StrokeItem {
            params,
            seed: 11,
            samples,
            trim: None,
            binding: None,
        });
        item(n, i64::try_from(n).unwrap() << 16, kind, Affine::IDENTITY)
    }

    /// The decoded picture: a 2 × 2 checker of red, green, blue, yellow.
    pub fn assets() -> BTreeMap<[u8; 32], Pixmap> {
        let mut pixmap = Pixmap::new(2, 2).unwrap();
        let colours = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]];
        for (px, c) in pixmap.pixels_mut().iter_mut().zip(colours) {
            *px = penwise_core::raster::tiny_skia::PremultipliedColorU8::from_rgba(
                c[0], c[1], c[2], 255,
            )
            .unwrap();
        }
        BTreeMap::from([(ASSET, pixmap)])
    }

    pub fn scene() -> Scene {
        let mut scene = Scene::new(PageKind::Infinite);
        let blue = Color::rgb(30, 80, 200);
        let items = [
            item(
                1,
                0,
                ItemKind::Fill(FillItem {
                    polygons: vec![Polygon {
                        outer: ring(20.0, 20.0, 120.0),
                        holes: vec![ring(50.0, 50.0, 40.0)],
                    }],
                    paint: Paint {
                        color: Color::rgb(120, 200, 140),
                        opacity: 0.8,
                    },
                }),
                Affine::IDENTITY,
            ),
            item(
                2,
                2 << 16,
                ItemKind::Image(ImageItem {
                    asset: ASSET,
                    w: 80.0,
                    h: 80.0,
                }),
                Affine::translate(210.0, 16.0),
            ),
            item(
                3,
                3 << 16,
                ItemKind::Shape(ShapeItem {
                    kind: ShapeKind::Ellipse,
                    size: [90.0, 50.0],
                    brush: {
                        let mut b = Preset::Fineliner.params();
                        b.color = blue;
                        b
                    },
                    fill: Some(Color::rgb(250, 220, 120)),
                }),
                Affine::translate(255.0, 130.0),
            ),
            stroke(4, Preset::Ballpoint, 80.0, Color::rgb(20, 20, 30)),
            stroke(5, Preset::Pencil, 190.0, Color::rgb(160, 40, 40)),
        ];
        for it in items {
            scene.apply(&Edit::Add(it)).unwrap();
        }
        // Trim the ballpoint stroke at x = 160, mask the pencil stroke at x = 100.
        let mut ids = Counter(100);
        for (x, y0, y1) in [(160.0, 60.0, 100.0), (100.0, 170.0, 210.0)] {
            let edits = erase_precise(&scene, &[[x, y0], [x, y1]], 7.0, LayerSet::ALL, &mut ids);
            scene.apply_all(&edits).unwrap();
        }
        scene
    }
}

#[test]
fn raster_scene() {
    use penwise_core::item::LayerSet;
    use penwise_core::raster::render_scene;
    use scene_fixture::{assets, scene};

    let scene = scene();
    let region = penwise_core::Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 320.0,
        max_y: 240.0,
    };
    let pixmap = render_scene(&scene, LayerSet::ALL, &region, 1.0, &assets()).unwrap();
    check_golden("raster_scene", &pixmap);

    let at = |x: u32, y: u32| {
        let p = pixmap.pixel(x, y).unwrap().demultiply();
        [p.red(), p.green(), p.blue(), p.alpha()]
    };
    // Fill: opaque-ish green ring, transparent hole.
    assert_eq!(at(30, 30)[3], 204, "0.8 opacity");
    assert_eq!(at(70, 70)[3], 0, "hole");
    let assert_near = |got: [u8; 4], want: [u8; 3]| {
        assert!(
            got[..3].iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 16) && got[3] == 255,
            "{got:?} vs {want:?}"
        );
    };
    // Picture: 80 px box over a 2×2 image, quadrants red/green/blue/yellow.
    assert_near(at(230, 36), [255, 0, 0]);
    assert_near(at(270, 36), [0, 255, 0]);
    assert_near(at(230, 76), [0, 0, 255]);
    assert_near(at(270, 76), [255, 255, 0]);
    // Shape: interior filled yellow, outside transparent.
    assert_eq!(at(255, 130)[..3], [250, 220, 120]);
    assert_eq!(at(255, 190)[3], 0);
    // Trimmed stroke: ink on both sides of the cut, none inside it.
    let column_has_ink = |x: u32, y0: u32, y1: u32| (y0..y1).any(|y| at(x, y)[3] > 128);
    assert!(column_has_ink(140, 55, 110) && column_has_ink(180, 55, 110));
    assert!(!column_has_ink(160, 55, 110), "gap under the eraser");
    // Masked pencil: the mask removes the stamps there, the stroke continues on both sides.
    assert!(column_has_ink(80, 165, 215) && column_has_ink(120, 165, 215));
    assert!(!column_has_ink(100, 165, 215), "mask hides the pencil");
}

#[test]
fn render_scene_rejects_unusable_regions_and_scales() {
    use penwise_core::item::{LayerSet, PageKind, Scene};
    use penwise_core::raster::{RasterError, render_scene};
    let scene = Scene::new(PageKind::Infinite);
    let ok = penwise_core::Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 10.0,
        max_y: 10.0,
    };
    for scale in [0.0, -1.0, f32::NAN] {
        assert_eq!(
            render_scene(&scene, LayerSet::ALL, &ok, scale, &()).unwrap_err(),
            RasterError::InvalidViewport
        );
    }
    let huge = penwise_core::Bounds {
        max_x: 1.0e6,
        max_y: 1.0e6,
        ..ok
    };
    assert!(matches!(
        render_scene(&scene, LayerSet::ALL, &huge, 1.0, &()),
        Err(RasterError::InvalidSize { .. })
    ));
    assert!(matches!(
        render_scene(
            &scene,
            LayerSet::ALL,
            &penwise_core::Bounds::EMPTY,
            1.0,
            &()
        ),
        Err(RasterError::InvalidSize { .. })
    ));
    // An empty scene renders a transparent pixmap of the region's size.
    let blank = render_scene(&scene, LayerSet::ALL, &ok, 2.0, &()).unwrap();
    assert_eq!((blank.width(), blank.height()), (20, 20));
    assert!(blank.data().iter().all(|&b| b == 0));
}
