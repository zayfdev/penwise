#![allow(
    clippy::float_cmp,
    reason = "exact outputs are the contract under test"
)]

use super::Affine;
use crate::bounds::Bounds;

fn close(a: [f32; 2], b: [f32; 2], eps: f32) -> bool {
    (a[0] - b[0]).abs() <= eps && (a[1] - b[1]).abs() <= eps
}

fn sample() -> Affine {
    Affine::rotate(0.7)
        .then(&Affine::scale(2.0, -3.0))
        .then(&Affine::translate(5.0, -7.0))
}

#[test]
fn identity_is_neutral_for_then() {
    let t = sample();
    assert!(t.then(&Affine::IDENTITY).bit_eq(&t));
    assert!(Affine::IDENTITY.then(&t).bit_eq(&t));
}

#[test]
fn then_applies_self_first() {
    let t = Affine::translate(10.0, 0.0).then(&Affine::scale(2.0, 2.0));
    assert_eq!(t.apply([1.0, 1.0]), [22.0, 2.0]);
    let u = Affine::scale(2.0, 2.0).then(&Affine::translate(10.0, 0.0));
    assert_eq!(u.apply([1.0, 1.0]), [12.0, 2.0]);
}

#[test]
fn then_is_associative_on_points() {
    let (a, b, c) = (
        Affine::rotate(0.3),
        Affine::scale(1.5, 0.5),
        Affine::translate(-2.0, 4.0),
    );
    let lhs = a.then(&b).then(&c);
    let rhs = a.then(&b.then(&c));
    for p in [[0.0, 0.0], [3.0, -1.0], [100.0, 50.0]] {
        assert!(close(lhs.apply(p), rhs.apply(p), 1e-3));
        assert!(close(lhs.apply(p), c.apply(b.apply(a.apply(p))), 1e-3));
    }
}

#[test]
fn invert_composes_to_identity() {
    let t = sample();
    let inv = t.invert().unwrap();
    for p in [[0.0, 0.0], [12.5, -3.25], [-400.0, 900.0]] {
        assert!(close(inv.apply(t.apply(p)), p, 5e-3));
        assert!(close(t.apply(inv.apply(p)), p, 5e-3));
    }
    let id = t.then(&inv);
    assert!(
        (id.a - 1.0).abs() < 1e-6
            && id.b.abs() < 1e-6
            && id.c.abs() < 1e-6
            && (id.d - 1.0).abs() < 1e-6
    );
    assert!(id.e.abs() < 1e-5 && id.f.abs() < 1e-5);
}

#[test]
fn invert_rejects_singular_and_non_finite() {
    assert!(Affine::scale(0.0, 1.0).invert().is_none());
    assert!(
        Affine {
            a: 1.0,
            b: 2.0,
            c: 2.0,
            d: 4.0,
            e: 0.0,
            f: 0.0
        }
        .invert()
        .is_none()
    );
    assert!(Affine::translate(f32::NAN, 0.0).invert().is_none());
    assert!(Affine::scale(f32::INFINITY, 1.0).invert().is_none());
}

#[test]
fn rotate_is_clockwise_on_screen() {
    let r = Affine::rotate(core::f32::consts::FRAC_PI_2);
    assert!(close(r.apply([1.0, 0.0]), [0.0, 1.0], 1e-6));
    assert!(r.is_similarity());
    assert!((r.det() - 1.0).abs() < 1e-6);
}

#[test]
fn about_keeps_anchor_fixed() {
    let anchor = [30.0, -12.0];
    let t = Affine::rotate(1.1)
        .then(&Affine::scale(2.0, 0.5))
        .about(anchor);
    assert!(close(t.apply(anchor), anchor, 1e-4));
    let expected = Affine::translate(-30.0, 12.0)
        .then(&Affine::rotate(1.1).then(&Affine::scale(2.0, 0.5)))
        .then(&Affine::translate(30.0, -12.0));
    for p in [[0.0, 0.0], [5.0, 9.0]] {
        assert!(close(t.apply(p), expected.apply(p), 1e-4));
    }
}

#[test]
fn vector_ignores_translation() {
    let t = Affine::translate(100.0, 100.0).then(&Affine::scale(2.0, 3.0));
    assert_eq!(t.apply_vector([1.0, 1.0]), [2.0, 3.0]);
}

#[test]
fn det_and_width_scale() {
    let t = Affine::scale(4.0, -9.0);
    assert_eq!(t.det(), -36.0);
    assert_eq!(t.width_scale(), 6.0);
}

#[test]
fn similarity_detection() {
    assert!(Affine::IDENTITY.is_similarity());
    assert!(
        Affine::scale(2.0, -2.0)
            .then(&Affine::rotate(0.4))
            .is_similarity()
    );
    assert!(!Affine::scale(2.0, 1.0).is_similarity());
    assert!(
        !Affine {
            c: 0.5,
            ..Affine::IDENTITY
        }
        .is_similarity()
    );
    assert!(!Affine::scale(0.0, 0.0).is_similarity());
}

#[test]
fn bit_eq_distinguishes_signed_zero() {
    let a = Affine::translate(0.0, 0.0);
    let b = Affine::translate(-0.0, 0.0);
    assert_eq!(a, b);
    assert!(!a.bit_eq(&b));
    assert!(a.bit_eq(&Affine::IDENTITY));
}

#[test]
fn map_bounds_covers_mapped_corners() {
    let b = Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 2.0,
        max_y: 1.0,
    };
    let m = Affine::rotate(core::f32::consts::FRAC_PI_2).map_bounds(&b);
    assert!((m.min_x + 1.0).abs() < 1e-6 && m.max_x.abs() < 1e-6);
    assert!(m.min_y.abs() < 1e-6 && (m.max_y - 2.0).abs() < 1e-6);
    assert!(sample().map_bounds(&Bounds::EMPTY).is_empty());
}
