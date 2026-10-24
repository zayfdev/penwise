//! Owner: slice d. Shapes and handles: geometry, flattening accuracy, stroke-after-transform width, handle
//! hit priority, resize about the opposite handle, endpoint drags and commit edits.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    reason = "test harness: failures are reported by panicking"
)]

use penwise_core::edit::Edit;
use penwise_core::float::{max, min};
use penwise_core::item::{
    Affine, ArrowHead, Item, ItemId, ItemKind, Layer, PageKind, Scene, ShapeItem, ShapeKind,
};
use penwise_core::shape::path::{ARROW_COS, flatten_cubic};
use penwise_core::shape::resize::{
    create_frame, drag_endpoint, frame_delta, resize_edits, resize_frame,
};
use penwise_core::shape::{
    FLATTEN_TOLERANCE, Handle, HandleFrame, regular_polygon, shape_geometry, shape_path,
};
use penwise_core::{BrushParams, Color, Preset, StrokeGeometry};

const CORNERS: [Handle; 4] = [
    Handle::TopLeft,
    Handle::TopRight,
    Handle::BottomRight,
    Handle::BottomLeft,
];
const RESIZE: [Handle; 8] = [
    Handle::TopLeft,
    Handle::Top,
    Handle::TopRight,
    Handle::Right,
    Handle::BottomRight,
    Handle::Bottom,
    Handle::BottomLeft,
    Handle::Left,
];

fn brush() -> BrushParams {
    Preset::Fineliner.params()
}

fn shape(kind: ShapeKind, size: [f32; 2], fill: Option<Color>) -> ShapeItem {
    ShapeItem {
        kind,
        size,
        brush: brush(),
        fill,
    }
}

fn sheared() -> Affine {
    Affine {
        a: 1.2,
        b: 0.3,
        c: 0.5,
        d: 0.9,
        e: 200.0,
        f: 150.0,
    }
}

fn rotated(angle: f32, at: [f32; 2]) -> Affine {
    Affine::rotate(angle).then(&Affine::translate(at[0], at[1]))
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    (dx * dx + dy * dy).sqrt()
}

fn assert_close(a: [f32; 2], b: [f32; 2], eps: f32, what: &str) {
    assert!(dist(a, b) <= eps, "{what}: {a:?} vs {b:?}");
}

fn all_finite(points: &[[f32; 2]]) -> bool {
    points.iter().flatten().all(|v| v.is_finite())
}

fn outline_points(geometry: &StrokeGeometry) -> (&[[f32; 2]], &[u32]) {
    match geometry {
        StrokeGeometry::Outline { outline, .. } => (&outline.points, &outline.contour_ends),
        StrokeGeometry::Stamps { .. } => panic!("shape brush is an outline brush"),
    }
}

/// Distance from `p` to the nearest edge of any contour of an outline.
fn outline_edge_dist(p: [f32; 2], points: &[[f32; 2]], ends: &[u32]) -> f32 {
    let mut best = f32::INFINITY;
    let mut start = 0;
    for &end in ends {
        let end = usize::try_from(end).unwrap();
        let ring = &points[start..end];
        for (i, &a) in ring.iter().enumerate() {
            let b = ring[(i + 1) % ring.len()];
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 {
                (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            best = min(best, dist(p, [a[0] + dx * t, a[1] + dy * t]));
        }
        start = end;
    }
    best
}

#[test]
fn codes_round_trip() {
    for head in [ArrowHead::End, ArrowHead::Both] {
        assert_eq!(ArrowHead::from_code(head.code()), Some(head));
    }
    assert_eq!(ArrowHead::from_code(2), None);
    let kinds = [
        ShapeKind::Rect,
        ShapeKind::RoundedRect { radius: 1.0 },
        ShapeKind::Ellipse,
        ShapeKind::Line,
        ShapeKind::Arrow {
            head: ArrowHead::End,
        },
        regular_polygon(5),
    ];
    let codes: Vec<u8> = kinds.iter().map(ShapeKind::code).collect();
    assert_eq!(codes, [0, 1, 2, 3, 4, 5]);
    let linear: Vec<bool> = kinds.iter().map(ShapeKind::is_linear).collect();
    assert_eq!(linear, [false, false, false, true, true, false]);
}

#[test]
fn every_closed_kind_gives_closed_finite_geometry() {
    let fill = Color::rgb(200, 30, 30);
    let kinds = [
        ShapeKind::Rect,
        ShapeKind::RoundedRect { radius: 15.0 },
        ShapeKind::RoundedRect { radius: 500.0 },
        ShapeKind::Ellipse,
        regular_polygon(6),
    ];
    for kind in kinds {
        for transform in [Affine::IDENTITY, sheared(), rotated(0.8, [50.0, 60.0])] {
            let g = shape_geometry(&shape(kind.clone(), [120.0, 80.0], Some(fill)), &transform);
            let n = g.path.points.len();
            assert!(g.path.closed, "{kind:?}");
            assert_eq!(g.path.run_ends, [u32::try_from(n).unwrap()], "{kind:?}");
            assert!(n >= 3 && all_finite(&g.path.points), "{kind:?}");
            for i in 0..n {
                assert!(
                    dist(g.path.points[i], g.path.points[(i + 1) % n]) > 0.0,
                    "{kind:?}: repeated point {i}"
                );
            }
            let (points, ends) = outline_points(&g.stroke);
            assert!(!points.is_empty() && all_finite(points), "{kind:?}");
            assert_eq!(
                ends.last().copied(),
                Some(u32::try_from(points.len()).unwrap())
            );
            let (color, interior) = g.fill.expect("closed filled shape has a fill");
            assert_eq!(color, fill);
            assert_eq!(interior.points, g.path.points);
        }
    }
}

#[test]
fn linear_and_degenerate_shapes_stay_finite() {
    let fill = Some(Color::rgb(1, 2, 3));
    for kind in [
        ShapeKind::Line,
        ShapeKind::Arrow {
            head: ArrowHead::Both,
        },
    ] {
        let g = shape_geometry(&shape(kind.clone(), [90.0, 0.0], fill), &sheared());
        assert!(!g.path.closed && g.fill.is_none(), "{kind:?}");
        let (points, _) = outline_points(&g.stroke);
        assert!(!points.is_empty() && all_finite(points), "{kind:?}");
    }
    for kind in [ShapeKind::Rect, ShapeKind::Ellipse, ShapeKind::Line] {
        for size in [[0.0, 0.0], [50.0, 0.0]] {
            let g = shape_geometry(&shape(kind.clone(), size, fill), &sheared());
            assert!(all_finite(&g.path.points), "{kind:?} {size:?}");
            let (points, _) = outline_points(&g.stroke);
            assert!(
                !points.is_empty() && all_finite(points),
                "{kind:?} {size:?}"
            );
        }
    }
}

/// Distance from local point `q` to the ellipse `x²/a² + y²/b² = 1`, by a fine parametric search around
/// the point's eccentric-angle estimate.
fn ellipse_dist(q: [f64; 2], a: f64, b: f64) -> f64 {
    let centre = libm::atan2(q[1] / b, q[0] / a);
    let mut best = f64::INFINITY;
    for k in -2000_i32..=2000 {
        let t = centre + f64::from(k) * 2.5e-5;
        let (s, c) = libm::sincos(t);
        let (dx, dy) = (q[0] - a * c, q[1] - b * s);
        let d = (dx * dx + dy * dy).sqrt();
        if d < best {
            best = d;
        }
    }
    best
}

#[test]
fn ellipse_flattening_stays_within_tolerance() {
    let transform = rotated(0.7, [300.0, 200.0]);
    let inverse = transform.invert().unwrap();
    let (a, b) = (150.0, 70.0);
    let path = shape_path(
        &shape(ShapeKind::Ellipse, [2.0 * a, 2.0 * b], None),
        &transform,
    );
    let pts = &path.points;
    assert!(pts.len() >= 16 && pts.len() < 1024, "{} points", pts.len());
    let mut worst = 0.0_f64;
    for i in 0..pts.len() {
        let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
        for k in 0_u8..=16 {
            let t = f32::from(k) / 16.0;
            let page = [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
            let local = inverse.apply(page);
            let d = ellipse_dist(
                [f64::from(local[0]), f64::from(local[1])],
                f64::from(a),
                f64::from(b),
            );
            if d > worst {
                worst = d;
            }
        }
    }
    assert!(
        worst <= f64::from(FLATTEN_TOLERANCE),
        "max deviation {worst} px"
    );
}

#[test]
fn flatten_cubic_excludes_start_and_ends_on_end() {
    let mut out = Vec::new();
    flatten_cubic(
        [0.0, 0.0],
        [0.0, 100.0],
        [100.0, 100.0],
        [100.0, 0.0],
        &mut out,
    );
    assert!(out.len() > 4);
    assert_eq!(out.last().copied(), Some([100.0, 0.0]));
    assert_ne!(out[0], [0.0, 0.0]);
    let mut straight = Vec::new();
    flatten_cubic(
        [0.0, 0.0],
        [1.0, 0.0],
        [2.0, 0.0],
        [3.0, 0.0],
        &mut straight,
    );
    assert_eq!(straight, [[3.0, 0.0]]);
}

#[test]
fn stroke_width_is_uniform_under_shear_and_non_uniform_scale() {
    let half_width = brush().size_max / 2.0;
    let stretch = Affine::scale(3.0, 1.0).then(&Affine::translate(400.0, 300.0));
    for transform in [sheared(), stretch] {
        let g = shape_geometry(&shape(ShapeKind::Rect, [100.0, 100.0], None), &transform);
        let (points, ends) = outline_points(&g.stroke);
        let corners = &g.path.points;
        assert_eq!(corners.len(), 4);
        let mut widths = Vec::new();
        for i in 0..4 {
            let (p, q) = (corners[i], corners[(i + 1) % 4]);
            let mid = [f32::midpoint(p[0], q[0]), f32::midpoint(p[1], q[1])];
            widths.push(outline_edge_dist(mid, points, ends));
        }
        for w in &widths {
            assert!(
                (w - half_width).abs() < 0.3,
                "side half-widths {widths:?}, brush half-width {half_width}"
            );
        }
    }
}

#[test]
fn arrow_heads_sit_at_the_line_ends() {
    let transform = rotated(0.6, [50.0, 60.0]);
    let arrow = |head, length| {
        shape_path(
            &shape(ShapeKind::Arrow { head }, [length, 0.0], None),
            &transform,
        )
    };
    let path = arrow(ArrowHead::End, 200.0);
    assert_eq!(path.run_ends, [2, 5]);
    assert!(!path.closed);
    let (start, end) = (path.points[0], path.points[1]);
    assert_close(end, transform.apply([100.0, 0.0]), 1e-3, "shaft end");
    assert_eq!(path.points[3], end, "tip is the shaft end");
    let unit = [(end[0] - start[0]) / 200.0, (end[1] - start[1]) / 200.0];
    let head_length = 4.0 * brush().size_max;
    let mut sides = Vec::new();
    for barb in [path.points[2], path.points[4]] {
        let v = [barb[0] - end[0], barb[1] - end[1]];
        let len = dist(barb, end);
        assert!((len - head_length).abs() < 1e-3, "head length {len}");
        let cos = -(v[0] * unit[0] + v[1] * unit[1]) / len;
        assert!((cos - ARROW_COS).abs() < 1e-4, "barb angle cos {cos}");
        sides.push(unit[0] * v[1] - unit[1] * v[0]);
    }
    assert!(
        sides[0] * sides[1] < 0.0,
        "barbs on both sides of the shaft"
    );

    let both = arrow(ArrowHead::Both, 200.0);
    assert_eq!(both.run_ends, [2, 5, 8]);
    assert_eq!(both.points[6], both.points[0], "second head at the start");

    let short = arrow(ArrowHead::End, 10.0);
    assert!((dist(short.points[2], short.points[3]) - 4.0).abs() < 1e-3);
    assert_eq!(arrow(ArrowHead::Both, 0.0).run_ends, [2]);
}

fn opposite(handle: Handle) -> Handle {
    match handle {
        Handle::TopLeft => Handle::BottomRight,
        Handle::Top => Handle::Bottom,
        Handle::TopRight => Handle::BottomLeft,
        Handle::Right => Handle::Left,
        Handle::BottomRight => Handle::TopLeft,
        Handle::Bottom => Handle::Top,
        Handle::BottomLeft => Handle::TopRight,
        Handle::Left => Handle::Right,
        other => other,
    }
}

fn rotated_frame() -> HandleFrame {
    HandleFrame {
        transform: rotated(0.5, [400.0, 300.0]),
        size: [120.0, 80.0],
        linear: false,
    }
}

#[test]
fn resize_keeps_the_opposite_handle_fixed_in_page_space() {
    let frame = rotated_frame();
    let pointers = [
        [470.0, 380.0],
        [300.0, 200.0],
        [520.0, 250.0],
        [405.0, 302.0],
        [250.0, 420.0],
    ];
    for handle in RESIZE {
        let anchor = frame.handle_position(opposite(handle), 0.0);
        for pointer in pointers {
            for constrain in [false, true] {
                let new = resize_frame(&frame, handle, pointer, constrain);
                assert!(new.size.iter().all(|s| s.is_finite() && *s >= 0.0));
                assert_close(
                    new.handle_position(opposite(handle), 0.0),
                    anchor,
                    2e-3,
                    &format!("{handle:?} → {pointer:?} constrain {constrain}"),
                );
                let is_corner = CORNERS.contains(&handle);
                if is_corner && !constrain {
                    assert_close(
                        new.handle_position(handle, 0.0),
                        pointer,
                        2e-3,
                        "dragged corner follows the pointer",
                    );
                }
                if constrain && is_corner {
                    let ratio = new.size[0] / new.size[1];
                    assert!((ratio - 1.5).abs() < 1e-4, "aspect kept: {ratio}");
                }
            }
        }
    }
}

#[test]
fn resize_through_the_anchor_flips() {
    let frame = rotated_frame();
    let far = frame.transform.apply([-150.0, -100.0]);
    let new = resize_frame(&frame, Handle::BottomRight, far, false);
    assert!(
        new.transform.det() > 0.0,
        "both axes flipped: rotation, not mirror"
    );
    let far_x = frame.transform.apply([-150.0, 10.0]);
    let mirrored = resize_frame(&frame, Handle::Right, far_x, false);
    assert!(mirrored.transform.det() < 0.0, "one axis flipped mirrors");
    assert!((mirrored.size[0] - 90.0).abs() < 1e-3 && (mirrored.size[1] - 80.0).abs() < 1e-4);
}

#[test]
fn non_resize_handles_leave_the_frame_unchanged() {
    let frame = rotated_frame();
    for handle in [Handle::Rotate, Handle::Body, Handle::Start, Handle::End] {
        assert_eq!(resize_frame(&frame, handle, [0.0, 0.0], false), frame);
    }
    assert_eq!(
        drag_endpoint(&frame, Handle::TopLeft, [0.0, 0.0], false),
        frame
    );
}

#[test]
fn constrain_yields_squares_circles_and_snapped_lines() {
    let square = create_frame([10.0, 10.0], [110.0, 40.0], false, true);
    assert_eq!(square.size, [100.0, 100.0]);
    assert_close(
        square.transform.apply([0.0, 0.0]),
        [60.0, 60.0],
        1e-4,
        "centre",
    );
    let back = create_frame([10.0, 10.0], [-30.0, 0.0], false, true);
    assert_eq!(back.size, [40.0, 40.0]);
    assert_close(
        back.transform.apply([0.0, 0.0]),
        [-10.0, -10.0],
        1e-4,
        "centre",
    );

    let circle = shape_path(
        &shape(ShapeKind::Ellipse, square.size, None),
        &square.transform,
    );
    let xs = circle.points.iter().map(|p| p[0]);
    let ys = circle.points.iter().map(|p| p[1]);
    let w = xs.clone().fold(f32::MIN, max) - xs.fold(f32::MAX, min);
    let h = ys.clone().fold(f32::MIN, max) - ys.fold(f32::MAX, min);
    assert!(
        (w - h).abs() < 1e-3 && (w - 100.0).abs() < 1e-3,
        "{w} × {h}"
    );

    let free = create_frame([10.0, 10.0], [110.0, 40.0], false, false);
    assert_eq!(free.size, [100.0, 30.0]);

    let line = create_frame([0.0, 0.0], [100.0, 10.0], true, true);
    assert!(line.linear);
    assert!((line.size[0] - 10_100.0_f32.sqrt()).abs() < 1e-3);
    assert_close(
        line.handle_position(Handle::Start, 0.0),
        [0.0, 0.0],
        1e-3,
        "start",
    );
    assert!(
        line.handle_position(Handle::End, 0.0)[1].abs() < 1e-3,
        "snapped to 0°"
    );
}

#[test]
fn handle_hits_follow_priority() {
    let small = HandleFrame {
        transform: rotated(0.3, [100.0, 100.0]),
        size: [20.0, 16.0],
        linear: false,
    };
    let at = |h| small.handle_position(h, 16.0);
    let hit = |p| small.hit_handle(p, 12.0, 16.0).map(|h| h.handle);
    for corner in CORNERS {
        assert_eq!(hit(at(corner)), Some(corner));
    }
    // Between a corner and an edge midpoint (both within reach): the corner wins.
    let tl = at(Handle::TopLeft);
    let top = at(Handle::Top);
    assert_eq!(
        hit([f32::midpoint(tl[0], top[0]), f32::midpoint(tl[1], top[1])]),
        Some(Handle::TopLeft)
    );
    // Halfway between the top midpoint and the rotate handle: rotate wins over the edge.
    let rot = at(Handle::Rotate);
    assert!((dist(rot, top) - 16.0).abs() < 1e-3);
    assert_eq!(
        hit([f32::midpoint(rot[0], top[0]), f32::midpoint(rot[1], top[1])]),
        Some(Handle::Rotate)
    );

    let large = HandleFrame {
        size: [200.0, 160.0],
        ..small
    };
    let hit_large = |p| large.hit_handle(p, 12.0, 16.0).map(|h| h.handle);
    for handle in RESIZE {
        assert_eq!(hit_large(large.handle_position(handle, 16.0)), Some(handle));
    }
    assert_eq!(
        hit_large(large.transform.apply([30.0, 20.0])),
        Some(Handle::Body)
    );
    assert_eq!(hit_large(large.transform.apply([300.0, 20.0])), None);
    let position = large
        .hit_handle(large.transform.apply([-101.0, -79.0]), 12.0, 16.0)
        .unwrap()
        .position;
    assert_close(
        position,
        large.handle_position(Handle::TopLeft, 0.0),
        1e-4,
        "hit position",
    );
}

#[test]
fn linear_frames_hit_endpoints_then_body() {
    let line = create_frame([0.0, 0.0], [10.0, 0.0], true, false);
    assert_eq!(line.handles(), [Handle::Start, Handle::End]);
    let hit = |p| line.hit_handle(p, 6.0, 20.0).map(|h| h.handle);
    assert_eq!(hit([7.0, 0.0]), Some(Handle::End));
    assert_eq!(hit([2.0, 1.0]), Some(Handle::Start));
    let long = create_frame([0.0, 0.0], [100.0, 0.0], true, false);
    assert_eq!(
        long.hit_handle([50.0, 4.0], 6.0, 20.0).map(|h| h.handle),
        Some(Handle::Body)
    );
    assert_eq!(long.hit_handle([50.0, 9.0], 6.0, 20.0), None);
}

#[test]
fn endpoint_drag_moves_one_end_only() {
    let line = create_frame([10.0, 20.0], [110.0, 20.0], true, false);
    let moved = drag_endpoint(&line, Handle::End, [60.0, 90.0], false);
    assert_close(
        moved.handle_position(Handle::Start, 0.0),
        [10.0, 20.0],
        1e-3,
        "start stays",
    );
    assert_close(
        moved.handle_position(Handle::End, 0.0),
        [60.0, 90.0],
        1e-3,
        "end follows",
    );
    let start = drag_endpoint(&line, Handle::Start, [0.0, 0.0], false);
    assert_close(
        start.handle_position(Handle::Start, 0.0),
        [0.0, 0.0],
        1e-3,
        "start follows",
    );
    assert_close(
        start.handle_position(Handle::End, 0.0),
        [110.0, 20.0],
        1e-3,
        "end stays",
    );

    let snapped = drag_endpoint(&line, Handle::End, [60.0, 75.0], true);
    let (s, e) = (
        snapped.handle_position(Handle::Start, 0.0),
        snapped.handle_position(Handle::End, 0.0),
    );
    assert_close(s, [10.0, 20.0], 1e-3, "start stays");
    let angle = libm::atan2f(e[1] - s[1], e[0] - s[0]);
    let steps = angle / (core::f32::consts::PI / 12.0);
    assert!(
        (steps - steps.round()).abs() < 1e-3,
        "angle {angle} on a 15° step"
    );
}

#[test]
fn frame_delta_maps_old_corners_onto_new_corners() {
    let old = rotated_frame();
    for pointer in [[470.0, 380.0], [250.0, 420.0]] {
        let new = resize_frame(&old, Handle::TopRight, pointer, false);
        let delta = frame_delta(&old, &new);
        for corner in CORNERS {
            assert_close(
                delta.apply(old.handle_position(corner, 0.0)),
                new.handle_position(corner, 0.0),
                2e-3,
                &format!("{corner:?}"),
            );
        }
    }
    let group = HandleFrame::of_bounds(&penwise_core::Bounds {
        min_x: 10.0,
        min_y: 20.0,
        max_x: 50.0,
        max_y: 100.0,
    })
    .unwrap();
    assert_eq!(group.size, [40.0, 80.0]);
    assert_close(
        group.handle_position(Handle::TopLeft, 0.0),
        [10.0, 20.0],
        0.0,
        "group corner",
    );
    assert!(HandleFrame::of_bounds(&penwise_core::Bounds::EMPTY).is_none());
    let degenerate = HandleFrame {
        transform: Affine::scale(0.0, 1.0),
        ..old
    };
    assert_eq!(frame_delta(&degenerate, &old), Affine::IDENTITY);
}

#[test]
fn resize_edits_carry_only_changed_absolute_fields() {
    let id = ItemId([7; 16]);
    let item_shape = shape(ShapeKind::Ellipse, [120.0, 80.0], None);
    let transform = rotated(0.5, [400.0, 300.0]);
    let mut scene = Scene::new(PageKind::Infinite);
    scene
        .apply(&Edit::Add(Item {
            id,
            layer: Layer::Ink,
            z: 0,
            transform,
            kind: ItemKind::Shape(item_shape.clone()),
        }))
        .unwrap();
    let frame = HandleFrame::of_shape(&item_shape, &transform);
    assert!(!frame.linear);
    assert!(resize_edits(&scene, id, &frame).is_empty());
    assert!(resize_edits(&scene, ItemId([9; 16]), &frame).is_empty());

    let new = resize_frame(&frame, Handle::BottomLeft, [330.0, 420.0], false);
    let edits = resize_edits(&scene, id, &new);
    let [Edit::Update { id: target, patch }] = edits.as_slice() else {
        panic!("one update expected: {edits:?}");
    };
    assert_eq!(*target, id);
    assert_eq!(patch.shape_size, Some(new.size));
    assert_eq!(patch.transform, Some(new.transform));
    assert!(patch.brush.is_none() && patch.shape_kind.is_none());

    let moved = HandleFrame {
        transform: transform.then(&Affine::translate(5.0, 0.0)),
        ..frame
    };
    let edits = resize_edits(&scene, id, &moved);
    let [Edit::Update { patch, .. }] = edits.as_slice() else {
        panic!("one update expected");
    };
    assert!(patch.shape_size.is_none() && patch.transform.is_some());
}

#[test]
fn regular_polygons_are_clamped_and_start_at_the_top() {
    let ShapeKind::Polygon { points } = regular_polygon(4) else {
        panic!("polygon expected");
    };
    let expected = [[0.0, -0.5], [0.5, 0.0], [0.0, 0.5], [-0.5, 0.0]];
    for (p, e) in points.iter().zip(expected) {
        assert_close(*p, e, 1e-6, "square vertex");
    }
    for (n, len) in [(0, 3), (1, 3), (7, 7), (1000, 64)] {
        let ShapeKind::Polygon { points } = regular_polygon(n) else {
            panic!("polygon expected");
        };
        assert_eq!(points.len(), len);
    }
}
