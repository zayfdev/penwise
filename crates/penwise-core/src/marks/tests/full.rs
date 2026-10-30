//! Full-length frames (design harness v2): the box is the document, the live view a sliding window.

use super::*;

/// A desktop frame (1440 × 900 viewport) at (500, 100).
fn full_frame(extent: FrameExtent, doc_h: u32) -> Item {
    let mut item = frame_item(id(1), 1, [500.0, 100.0], FramePreset::Desktop);
    let ItemKind::Frame(frame) = &mut item.kind else {
        unreachable!("frame_item builds a frame");
    };
    frame.extent = extent;
    frame.doc_h = doc_h;
    item
}

fn scene_with(item: Item) -> Scene {
    let mut scene = Scene::new(PageKind::Infinite);
    scene.apply(&Edit::Add(item)).unwrap();
    scene
}

/// A layout update at `scroll` with one element rect (viewport-relative, as the bridge reports it).
fn report(scroll_y: f32, rect: [f32; 4]) -> FrameLayoutUpdate {
    FrameLayoutUpdate {
        scroll: Some([0.0, scroll_y]),
        viewport: Some([1440.0, 900.0]),
        rects: vec![("k".into(), Some(FrameRect::from_array(rect)))],
        ..FrameLayoutUpdate::default()
    }
}

fn state(scene: &Scene, layouts: &FrameLayouts) -> FrameState {
    FrameState::of(scene, layouts, id(1)).unwrap()
}

#[test]
fn merge_stores_rects_in_document_coordinates() {
    let mut layouts = FrameLayouts::new();
    layouts.merge(
        id(1),
        FramePreset::Desktop,
        &report(2000.0, [10.0, 500.0, 40.0, 20.0]),
    );
    let layout = layouts.get(id(1)).unwrap();
    assert_eq!(
        layout.rect("k"),
        Some(Some(FrameRect::new(10.0, 2500.0, 40.0, 20.0)))
    );
    // A rects-only update uses the stored scroll; a scroll-only update leaves document rects alone.
    layouts.merge(
        id(1),
        FramePreset::Desktop,
        &FrameLayoutUpdate {
            scroll: Some([0.0, 100.0]),
            ..FrameLayoutUpdate::default()
        },
    );
    assert_eq!(
        layouts.get(id(1)).unwrap().rect("k"),
        Some(Some(FrameRect::new(10.0, 2500.0, 40.0, 20.0)))
    );
}

#[test]
fn full_area_placement_is_scroll_independent() {
    let scene = scene_with(full_frame(FrameExtent::Full, 4000));
    let area = binding(id(9), BindingMode::Area, None, "");
    for scroll in [0.0, 450.0, 2100.0] {
        let mut layouts = FrameLayouts::new();
        layouts.merge(id(1), FramePreset::Desktop, &report(scroll, [0.0; 4]));
        let frame = state(&scene, &layouts);
        let mut area = area.clone();
        area.preset = FramePreset::Desktop;
        let placement = place(&query(&area, Some(&frame), None, true));
        assert_eq!(placement.offset, [500.0, 100.0], "scroll {scroll}");
        assert_eq!(placement.clip, FrameRect::new(500.0, 100.0, 1440.0, 4000.0));
    }
}

#[test]
fn full_area_mark_at_doc_y_3000_sits_at_pos_plus_3000_at_any_scroll() {
    let frame = id(1);
    let mut scene = scene_with(full_frame(FrameExtent::Full, 4000));
    let mut layouts = FrameLayouts::new();
    layouts.merge(frame, FramePreset::Desktop, &report(450.0, [0.0; 4]));
    // The owner draws at board y = pos.y + 3000 (the camera shows that part of the box, scroll is irrelevant).
    let spec = BoundStroke {
        id: ulid(1_000, 1),
        mark_id: ulid(1_000, 2),
        frame,
        mode: BindingMode::Area,
        key: None,
        z: 2,
        mark_z: 3,
        layer: Layer::Ink,
        params: Preset::Ballpoint.params(),
        seed: 1,
        samples: samples(&[[520.0, 3100.0], [560.0, 3120.0]]),
        now_ms: 1_000,
    };
    let edits = add_bound_stroke(&scene, &layouts, &spec).unwrap();
    scene.apply_all(&edits).unwrap();
    let Some(ItemKind::Stroke(stroke)) = scene.get(spec.id).map(|i| &i.kind) else {
        panic!("stroke stored");
    };
    assert_eq!(
        (stroke.samples[0].x, stroke.samples[0].y),
        (20.0, 3000.0),
        "samples are document coordinates"
    );
    for scroll in [0.0, 450.0, 2100.0, 3100.0] {
        layouts.merge(frame, FramePreset::Desktop, &report(scroll, [0.0; 4]));
        let presentation = MarkView::new(&scene, &layouts, true).presentation();
        let b = presentation.scene.bounds(spec.id).unwrap();
        // The bounds include the brush radius (a few px) around the 3100 / 520 start point.
        assert!(
            (b.min_y - 3100.0).abs() < 4.0 && (b.min_x - 520.0).abs() < 4.0,
            "scroll {scroll}: {b:?}"
        );
    }
}

#[test]
fn full_bound_mark_follows_the_element_at_every_scroll() {
    let scene = scene_with(full_frame(FrameExtent::Full, 4000));
    let bound = {
        let mut b = binding(id(9), BindingMode::Bound, Some("k"), "");
        b.preset = FramePreset::Desktop;
        b
    };
    // The same element at doc y 2500, reported from three scroll positions.
    let mut offsets = Vec::new();
    for (scroll, viewport_y) in [(1800.0, 700.0), (2000.0, 500.0), (2400.0, 100.0)] {
        let mut layouts = FrameLayouts::new();
        layouts.merge(
            id(1),
            FramePreset::Desktop,
            &report(scroll, [10.0, viewport_y, 40.0, 20.0]),
        );
        let frame = state(&scene, &layouts);
        offsets.push(place(&query(&bound, Some(&frame), None, true)).offset);
    }
    assert_eq!(offsets, vec![[510.0, 2600.0]; 3], "pos + doc rect corner");
}

#[test]
fn viewport_bound_mark_still_follows_the_viewport_rect() {
    // Same reports as above on a viewport frame: the board point is pos + viewport rect (v1), so it differs
    // per scroll while the element's viewport position does.
    let scene = scene_with(full_frame(FrameExtent::Viewport, 0));
    let bound = {
        let mut b = binding(id(9), BindingMode::Bound, Some("k"), "");
        b.preset = FramePreset::Desktop;
        b
    };
    let mut offsets = Vec::new();
    for (scroll, viewport_y) in [(1800.0, 700.0), (2000.0, 500.0)] {
        let mut layouts = FrameLayouts::new();
        layouts.merge(
            id(1),
            FramePreset::Desktop,
            &report(scroll, [10.0, viewport_y, 40.0, 20.0]),
        );
        let frame = state(&scene, &layouts);
        offsets.push(place(&query(&bound, Some(&frame), None, true)).offset);
    }
    assert_eq!(offsets, vec![[510.0, 800.0], [510.0, 600.0]]);
}

#[test]
fn clip_is_the_box_with_the_synced_hint() {
    let mut layouts = FrameLayouts::new();
    layouts.merge(id(1), FramePreset::Desktop, &report(0.0, [0.0; 4]));
    let tall = state(&scene_with(full_frame(FrameExtent::Full, 4000)), &layouts);
    assert_eq!(tall.clip(), FrameRect::new(500.0, 100.0, 1440.0, 4000.0));
    let unknown = state(&scene_with(full_frame(FrameExtent::Full, 0)), &layouts);
    assert_eq!(unknown.clip().h, 900.0, "unknown height: the viewport");
    let short = state(&scene_with(full_frame(FrameExtent::Full, 300)), &layouts);
    assert_eq!(
        short.clip().h,
        900.0,
        "a short document never shrinks the box"
    );
    let viewport = state(
        &scene_with(full_frame(FrameExtent::Viewport, 4000)),
        &layouts,
    );
    assert_eq!(
        viewport.clip().h,
        900.0,
        "viewport ignores the document height"
    );
}

#[test]
fn live_window_follows_the_actual_scroll() {
    let mut layouts = FrameLayouts::new();
    layouts.merge(id(1), FramePreset::Desktop, &report(1200.0, [0.0; 4]));
    let layout = layouts.get(id(1)).unwrap().clone();

    let full = state(&scene_with(full_frame(FrameExtent::Full, 4000)), &layouts);
    assert_eq!(
        live_window(&full, &layout),
        FrameRect::new(500.0, 1300.0, 1440.0, 900.0),
        "pos + scroll"
    );
    let viewport = state(&scene_with(full_frame(FrameExtent::Viewport, 0)), &layouts);
    assert_eq!(
        live_window(&viewport, &layout),
        FrameRect::new(500.0, 100.0, 1440.0, 900.0),
        "a viewport frame's window is its box"
    );
}

#[test]
fn doc_and_board_points_are_inverse() {
    let scene_full = scene_with(full_frame(FrameExtent::Full, 4000));
    let scene_view = scene_with(full_frame(FrameExtent::Viewport, 0));
    let mut layouts = FrameLayouts::new();
    layouts.merge(id(1), FramePreset::Desktop, &report(1200.0, [0.0; 4]));
    let layout = layouts.get(id(1)).unwrap().clone();

    let full = state(&scene_full, &layouts);
    assert_eq!(doc_point(&full, &layout, [520.0, 3100.0]), [20.0, 3000.0]);
    assert_eq!(board_point(&full, &layout, [20.0, 3000.0]), [520.0, 3100.0]);
    let view = state(&scene_view, &layouts);
    assert_eq!(doc_point(&view, &layout, [520.0, 150.0]), [20.0, 1250.0]);
    assert_eq!(board_point(&view, &layout, [20.0, 1250.0]), [520.0, 150.0]);

    // The scene-level helpers use the layout the bridge reported, scroll 0 without one.
    assert_eq!(
        frame_doc_point(&scene_full, &layouts, id(1), [520.0, 3100.0]),
        Some([20.0, 3000.0])
    );
    assert_eq!(
        frame_live_window(&scene_full, &FrameLayouts::new(), id(1)),
        Some(FrameRect::new(500.0, 100.0, 1440.0, 900.0))
    );
    assert_eq!(
        frame_board_point(&scene_full, &layouts, id(2), [0.0; 2]),
        None
    );
}

/// A scroll-only update with the key omitted (the bridge resends a rect only when its document rect changed).
fn scroll_only(scroll_y: f32) -> FrameLayoutUpdate {
    FrameLayoutUpdate {
        scroll: Some([0.0, scroll_y]),
        ..FrameLayoutUpdate::default()
    }
}

fn desktop_area_and_bound() -> (Binding, Binding) {
    let mut bound = binding(id(9), BindingMode::Bound, Some("k"), "");
    bound.preset = FramePreset::Desktop;
    let mut area = binding(id(9), BindingMode::Area, None, "");
    area.preset = FramePreset::Desktop;
    (bound, area)
}

#[test]
fn an_omitted_key_after_a_scroll_keeps_its_document_rect_in_both_extents() {
    let (bound, area) = desktop_area_and_bound();
    for (extent, doc_h, bound_y, area_y) in [
        // Full: the element stays at doc y 600 (box top-left is the document origin), whatever the scroll.
        (FrameExtent::Full, 4000, 700.0, 100.0),
        // Viewport: the same element scrolled 500 up inside the box: 600 - 500.
        (FrameExtent::Viewport, 0, 200.0, -400.0),
    ] {
        let scene = scene_with(full_frame(extent, doc_h));
        let mut layouts = FrameLayouts::new();
        // Reported at scroll 100, viewport y 500 -> document y 600.
        layouts.merge(
            id(1),
            FramePreset::Desktop,
            &report(100.0, [10.0, 500.0, 40.0, 20.0]),
        );
        layouts.merge(id(1), FramePreset::Desktop, &scroll_only(500.0));
        let frame = state(&scene, &layouts);
        let at = |b: &Binding| place(&query(b, Some(&frame), None, true)).offset;
        assert_eq!(at(&bound), [510.0, bound_y], "{extent:?} bound");
        assert_eq!(at(&area), [500.0, area_y], "{extent:?} area");
    }
}

#[test]
fn a_resent_fixed_rect_moves_with_the_new_scroll_in_both_extents() {
    // A fixed header: viewport rect constant (10, 12), so the bridge resends it after each scroll and core
    // re-stores `rect + scroll`.
    let (bound, _) = desktop_area_and_bound();
    for (extent, doc_h, expect_y) in [
        (FrameExtent::Full, 4000, 100.0 + 12.0 + 500.0),
        (FrameExtent::Viewport, 0, 100.0 + 12.0),
    ] {
        let scene = scene_with(full_frame(extent, doc_h));
        let mut layouts = FrameLayouts::new();
        layouts.merge(
            id(1),
            FramePreset::Desktop,
            &report(0.0, [10.0, 12.0, 100.0, 24.0]),
        );
        layouts.merge(
            id(1),
            FramePreset::Desktop,
            &report(500.0, [10.0, 12.0, 100.0, 24.0]),
        );
        let frame = state(&scene, &layouts);
        let offset = place(&query(&bound, Some(&frame), None, true)).offset;
        assert_eq!(offset, [510.0, expect_y], "{extent:?}");
    }
}

#[test]
fn the_layout_document_height_extends_the_clip_when_the_hint_is_unknown() {
    let (_, area) = desktop_area_and_bound();
    let scene = scene_with(full_frame(FrameExtent::Full, 0));
    // No layout yet: the clip is the viewport, so doc y 2500 is outside it.
    let none = FrameLayouts::new();
    let before = place(&query(&area, Some(&state(&scene, &none)), None, true));
    let point = [510.0, 100.0 + 2500.0];
    assert!(!before.clip.contains(point));

    let mut layouts = FrameLayouts::new();
    layouts.merge(
        id(1),
        FramePreset::Desktop,
        &FrameLayoutUpdate {
            doc: Some([1440.0, 3000.0]),
            scroll: Some([0.0, 0.0]),
            ..FrameLayoutUpdate::default()
        },
    );
    let after = place(&query(&area, Some(&state(&scene, &layouts)), None, true));
    assert_eq!(after.clip, FrameRect::new(500.0, 100.0, 1440.0, 3000.0));
    assert!(
        after.clip.contains(point),
        "a mark at doc y 2500 is inside the clip"
    );
    assert_eq!(after.visibility, Visibility::Visible);

    // A hostile document height is clamped, not trusted.
    layouts.merge(
        id(1),
        FramePreset::Desktop,
        &FrameLayoutUpdate {
            doc: Some([1440.0, 1.0e30]),
            ..FrameLayoutUpdate::default()
        },
    );
    let hostile = place(&query(&area, Some(&state(&scene, &layouts)), None, true));
    assert_eq!(hostile.clip.h, 1_048_576.0);
}
