#![allow(
    clippy::float_cmp,
    reason = "exact outputs are the contract under test"
)]

use super::Bounds;

fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Bounds {
    Bounds {
        min_x,
        min_y,
        max_x,
        max_y,
    }
}

#[test]
fn empty_is_empty_and_has_zero_extent() {
    assert!(Bounds::EMPTY.is_empty());
    assert_eq!(Bounds::EMPTY.width(), 0.0);
    assert_eq!(Bounds::EMPTY.height(), 0.0);
    assert!(rect(1.0, 0.0, 0.0, 1.0).is_empty(), "inverted x");
    assert!(rect(0.0, 1.0, 1.0, 0.0).is_empty(), "inverted y");
    assert!(rect(f32::NAN, 0.0, 1.0, 1.0).is_empty(), "NaN extent");
}

#[test]
fn a_point_box_is_not_empty() {
    let b = Bounds::from_point(3.0, -2.0);
    assert!(!b.is_empty());
    assert_eq!((b.width(), b.height()), (0.0, 0.0));
    assert!(b.contains(3.0, -2.0));
}

#[test]
fn include_point_grows_from_empty_and_ignores_nan() {
    let mut b = Bounds::EMPTY;
    b.include_point(1.0, 5.0);
    assert_eq!(b, Bounds::from_point(1.0, 5.0));
    b.include_point(-2.0, 7.0);
    b.include_point(f32::NAN, f32::NAN);
    assert_eq!(b, rect(-2.0, 5.0, 1.0, 7.0));
    assert_eq!((b.width(), b.height()), (3.0, 2.0));
}

#[test]
fn union_covers_both_and_empty_is_identity() {
    let a = rect(0.0, 0.0, 2.0, 1.0);
    let b = rect(-1.0, 0.5, 1.0, 4.0);
    let u = rect(-1.0, 0.0, 2.0, 4.0);
    assert_eq!(a.union(&b), u);
    assert_eq!(b.union(&a), u);
    assert_eq!(a.union(&Bounds::EMPTY), a);
    assert_eq!(Bounds::EMPTY.union(&a), a);
    assert!(Bounds::EMPTY.union(&Bounds::EMPTY).is_empty());
    // An inverted (empty) operand contributes nothing, even though its coordinates are finite.
    assert_eq!(a.union(&rect(10.0, 10.0, -10.0, -10.0)), a);
}

#[test]
fn contains_is_closed_on_every_edge() {
    let b = rect(0.0, 0.0, 2.0, 1.0);
    for (x, y) in [(0.0, 0.0), (2.0, 1.0), (1.0, 0.5), (0.0, 1.0), (2.0, 0.0)] {
        assert!(b.contains(x, y), "({x}, {y})");
    }
    for (x, y) in [
        (-0.001, 0.5),
        (2.001, 0.5),
        (1.0, -0.001),
        (1.0, 1.001),
        (f32::NAN, 0.5),
    ] {
        assert!(!b.contains(x, y), "({x}, {y})");
    }
    assert!(!Bounds::EMPTY.contains(0.0, 0.0));
}

#[test]
fn intersects_counts_touching_edges_and_never_empty() {
    let a = rect(0.0, 0.0, 2.0, 2.0);
    assert!(a.intersects(&rect(2.0, 2.0, 3.0, 3.0)), "corner touch");
    assert!(a.intersects(&rect(0.5, 0.5, 1.0, 1.0)), "nested");
    assert!(!a.intersects(&rect(2.1, 0.0, 3.0, 2.0)));
    assert!(!a.intersects(&Bounds::EMPTY));
    assert!(!Bounds::EMPTY.intersects(&Bounds::EMPTY));
}

#[test]
fn inflate_grows_shrinks_and_keeps_empty() {
    let a = rect(0.0, 0.0, 2.0, 2.0);
    assert_eq!(a.inflate(1.0), rect(-1.0, -1.0, 3.0, 3.0));
    assert_eq!(a.inflate(-0.5), rect(0.5, 0.5, 1.5, 1.5));
    assert!(a.inflate(-1.5).is_empty());
    assert!(Bounds::EMPTY.inflate(10.0).is_empty());
}
