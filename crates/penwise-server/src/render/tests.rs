#![allow(clippy::cast_precision_loss, reason = "test fixture coordinates")]

use penwise_core::brush::BrushParams;
use penwise_core::edit::Edit;
use penwise_core::item::{
    Affine, FramePreset, FrameSpec, ImageItem, Item, ItemKind, Layer, LayerSet,
};
use penwise_core::raster::render_scene;
use penwise_proto::notebook::BACKGROUND_SPACING_PX;
use penwise_proto::{Background, PageKind};
use tiny_skia::{Pixmap, PixmapPaint, Transform};

use super::*;
use crate::assets;
use crate::testing::Rig;

fn decode(png: &[u8]) -> Pixmap {
    Pixmap::decode_png(png).expect("png")
}

fn wave(x: f32, y: f32) -> Vec<[f32; 2]> {
    (0..40)
        .map(|i| [x + i as f32 * 6.0, y + (i as f32 * 0.5).sin() * 8.0])
        .collect()
}

fn picture_item(rig: &mut Rig, asset: [u8; 32], at: [f32; 2], size: [f32; 2]) -> Item {
    Item {
        id: rig.item_id(),
        layer: Layer::Ink,
        z: 0,
        transform: Affine::translate(at[0], at[1]),
        kind: ItemKind::Image(ImageItem {
            asset,
            w: size[0],
            h: size[1],
        }),
    }
}

#[tokio::test]
async fn the_png_is_the_scene_render_over_white_paper() {
    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    for (i, seed) in [0_u64, 1, u64::MAX, i64::MIN.cast_unsigned()]
        .into_iter()
        .enumerate()
    {
        rig.stroke(page, Layer::Ink, seed, &wave(20.0, 60.0 + i as f32 * 70.0))
            .await;
    }
    rig.stroke(page, Layer::Agent, 9, &wave(20.0, 500.0)).await;
    let view = rig.view(page).await;
    let served = decode(&page_png(&view, &rig.assets, 1.0).expect("render"));

    let region = Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 1000.0,
        max_y: 1000.0,
    };
    let scene =
        render_scene(view.replica.scene(), LayerSet::ALL, &region, 1.0, &()).expect("scene");
    let mut expected = Pixmap::new(1000, 1000).expect("pixmap");
    expected.fill(tiny_skia::Color::WHITE);
    expected.draw_pixmap(
        0,
        0,
        scene.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    assert_eq!(expected.data(), served.data());
    assert!(
        served.data().chunks_exact(4).any(|p| p[0] < 200),
        "ink is on the page"
    );
}

#[tokio::test]
async fn a_ruled_page_draws_its_lines_and_a_blank_one_stays_white() {
    let mut rig = Rig::new();
    let kind = PageKind::Fixed {
        width: 400.0,
        height: 400.0,
    };
    let lined = rig.page(kind, Background::Lined).await;
    let blank = rig.page(kind, Background::Blank).await;
    let lined = decode(&page_png(&rig.view(lined).await, &rig.assets, 1.0).expect("render"));
    let blank = decode(&page_png(&rig.view(blank).await, &rig.assets, 1.0).expect("render"));
    assert!(
        blank
            .data()
            .chunks_exact(4)
            .all(|p| p == [255, 255, 255, 255])
    );
    assert!(
        lined
            .data()
            .chunks_exact(4)
            .any(|p| p != [255, 255, 255, 255])
    );
}

#[tokio::test]
async fn a_frame_draws_a_labelled_placeholder_box() {
    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    let spec = FrameSpec::new(
        "portfolio".to_owned(),
        "/".to_owned(),
        FramePreset::Phone,
        None,
        None,
        100.0,
        100.0,
    )
    .expect("spec");
    let item = spec.item(rig.item_id(), Layer::Ink, 0);
    rig.edit(page, Edit::Add(item)).await;
    let png = decode(&page_png(&rig.view(page).await, &rig.assets, 1.0).expect("render"));

    // The label strip runs along the top edge; the body below it, the paper outside.
    let label = png.pixel(200, 108).expect("pixel");
    let body = png.pixel(200, 400).expect("pixel");
    let outside = png.pixel(50, 50).expect("pixel");
    assert_eq!(
        (body.red(), body.green(), body.blue()),
        (243, 244, 247),
        "the frame body is the placeholder panel"
    );
    assert!(
        label.red() < 100 && label.blue() > label.red(),
        "the label strip is the dark band: {label:?}"
    );
    assert_eq!(
        (outside.red(), outside.green(), outside.blue()),
        (255, 255, 255),
        "nothing is drawn outside the frame"
    );
}

#[tokio::test]
async fn a_placed_picture_is_composited_under_the_ink() {
    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    let stored = rig.picture(4, 4, [255, 0, 0, 255]);
    let digest = assets::digest_of(&stored.asset).expect("digest");
    let image = picture_item(&mut rig, digest, [100.0, 100.0], [200.0, 100.0]);
    rig.edit(page, Edit::Add(image)).await;
    let png = decode(&page_png(&rig.view(page).await, &rig.assets, 1.0).expect("render"));
    let inside = png.pixel(200, 150).expect("pixel");
    assert_eq!((255, 0, 0), (inside.red(), inside.green(), inside.blue()));
    let outside = png.pixel(50, 50).expect("pixel");
    assert_eq!(
        (255, 255, 255),
        (outside.red(), outside.green(), outside.blue())
    );
}

#[tokio::test]
async fn a_picture_with_a_missing_asset_is_skipped_not_fatal() {
    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    let image = picture_item(&mut rig, [7; 32], [0.0, 0.0], [50.0, 50.0]);
    rig.edit(page, Edit::Add(image)).await;
    assert!(page_png(&rig.view(page).await, &rig.assets, 1.0).is_ok());
}

#[tokio::test]
async fn scale_and_size_limits_hold() {
    let mut rig = Rig::new();
    let infinite = rig.page(PageKind::Infinite, Background::Blank).await;
    let view = rig.view(infinite).await;
    let small = decode(&page_png(&view, &rig.assets, 1.0).expect("render"));
    assert_eq!(
        (1200, 900),
        (small.width(), small.height()),
        "infinite pages have a minimum window"
    );
    let clamped = decode(&page_png(&view, &rig.assets, 100.0).expect("render"));
    assert_eq!(
        (4800, 3600),
        (clamped.width(), clamped.height()),
        "scale is capped at 4"
    );
    let garbage = decode(&page_png(&view, &rig.assets, f32::NAN).expect("render"));
    assert_eq!(1200, garbage.width(), "a non-finite scale means 1");

    let huge = rig
        .page(
            PageKind::Fixed {
                width: 30_000.0,
                height: 10_000.0,
            },
            Background::Blank,
        )
        .await;
    let capped = decode(&page_png(&rig.view(huge).await, &rig.assets, 4.0).expect("render"));
    assert!(
        capped.width().max(capped.height()) <= 8000,
        "no side beyond 8000 px"
    );
    assert!(capped.width() >= 7990, "and it still uses the budget");
}

#[tokio::test]
async fn an_infinite_page_wraps_its_content_with_padding() {
    let mut rig = Rig::new();
    let page = rig.page(PageKind::Infinite, Background::Blank).await;
    rig.stroke(page, Layer::Ink, 1, &[[5000.0, 5000.0], [5400.0, 5300.0]])
        .await;
    let png = decode(&page_png(&rig.view(page).await, &rig.assets, 1.0).expect("render"));
    assert!(
        png.width() >= 1200 && png.width() < 1200 + 100,
        "{}x{}",
        png.width(),
        png.height()
    );
    assert!(
        png.data().chunks_exact(4).any(|p| p[0] < 200),
        "the far-away ink is inside the window"
    );
}

#[tokio::test]
async fn the_ink_crop_holds_only_handwriting_and_is_smaller_than_the_page() {
    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    assert!(
        ink_crop_png(&rig.view(page).await, 1.0, 48.0)
            .expect("crop")
            .is_none()
    );
    rig.stroke(page, Layer::Agent, 2, &wave(600.0, 600.0)).await;
    assert!(
        ink_crop_png(&rig.view(page).await, 1.0, 48.0)
            .expect("crop")
            .is_none(),
        "agent marks are not handwriting"
    );
    rig.stroke(page, Layer::Ink, 1, &wave(400.0, 300.0)).await;
    let view = rig.view(page).await;
    let found = ink_crop_png(&view, 1.0, 48.0).expect("crop").expect("some");
    assert!(
        found.bounds.min_x > 300.0 && found.bounds.max_x < 700.0,
        "stroke bounds, no padding: {:?}",
        found.bounds
    );
    let crop = decode(&found.png);
    let full = decode(&page_png(&view, &rig.assets, 1.0).expect("render"));
    assert!(crop.width() < full.width() / 2 && crop.height() < full.height() / 2);
    assert!(crop.data().chunks_exact(4).any(|p| p[0] < 200));
}

#[tokio::test]
async fn the_ink_crop_is_capped_for_the_model() {
    let mut rig = Rig::new();
    let page = rig.page(PageKind::Infinite, Background::Blank).await;
    rig.stroke(page, Layer::Ink, 1, &[[0.0, 0.0], [20_000.0, 20_000.0]])
        .await;
    let crop = decode(
        &ink_crop_png(&rig.view(page).await, 2.0, 48.0)
            .expect("crop")
            .expect("some")
            .png,
    );
    assert!(
        crop.width().max(crop.height()) <= 1400,
        "{}x{}",
        crop.width(),
        crop.height()
    );
}

/// A horizontal yellow highlighter stroke (multiply blend) at height `y`.
fn highlighter(rig: &mut Rig, y: f32) -> Item {
    let mut item = penwise_testkit::stroke_item(
        rig.item_id(),
        1,
        Layer::Ink,
        3,
        &[[100.0, y], [180.0, y], [260.0, y], [340.0, y]],
    );
    if let ItemKind::Stroke(stroke) = &mut item.kind {
        stroke.params = BrushParams::for_preset(5, 0xFFFF_FF00, 1.0);
    }
    item
}

#[tokio::test]
async fn a_highlighter_multiplies_over_the_paper_so_the_ruling_shows_through() {
    let mut rig = Rig::new();
    let page = rig
        .page(
            PageKind::Fixed {
                width: 600.0,
                height: 300.0,
            },
            Background::Lined,
        )
        .await;
    let item = highlighter(&mut rig, 94.0);
    rig.edit(page, Edit::Add(item)).await;
    let png = decode(&page_png(&rig.view(page).await, &rig.assets, 1.0).expect("render"));
    // Row 94 is the rule (page y 93.7..94.7). Multiply keeps the rule's red/green; a normal composite would
    // paint the highlighter's own yellow over it.
    let on_rule = png.pixel(220, 94).expect("pixel").demultiply();
    let off_rule = png.pixel(220, 100).expect("pixel").demultiply();
    assert!(
        on_rule.red() < 225 && on_rule.green() < 235,
        "the rule shows through the highlighter: {on_rule:?}"
    );
    assert!(
        off_rule.red() > 230 && off_rule.blue() < 200,
        "yellow on bare paper: {off_rule:?}"
    );
}

#[tokio::test]
async fn ruling_phase_follows_page_coordinates_on_infinite_pages() {
    let mut rig = Rig::new();
    let page = rig.page(PageKind::Infinite, Background::Lined).await;
    rig.stroke(page, Layer::Ink, 1, &[[5000.0, 5000.0], [5400.0, 5300.0]])
        .await;
    let view = rig.view(page).await;
    let frame = page_frame(&view, 1.0);
    let png = decode(&page_png(&view, &rig.assets, 1.0).expect("render"));
    // A row holds some rule when its left-most pixel is not paper white (rules are anti-aliased on a fraction).
    let rows: Vec<u32> = (0..png.height())
        .filter(|&y| png.pixel(2, y).is_some_and(|p| p.red() < 250))
        .collect();
    assert!(rows.len() > 5, "ruled lines are drawn: {rows:?}");
    for row in rows {
        let page_y = frame.region.min_y + row as f32;
        let k = (page_y / BACKGROUND_SPACING_PX).round();
        // The row [page_y, page_y + 1) meets the rule [k·S, k·S + 1).
        assert!(
            (page_y - k * BACKGROUND_SPACING_PX).abs() < 1.0,
            "row {row} (page y {page_y}) is not a multiple of {BACKGROUND_SPACING_PX}"
        );
    }
}
