use super::{CoverageBuffer, MIN_FADE_HARDNESS, dab_falloff, min_fade, render_stamps};
use crate::brush::{BlendMode, Color, GrainParams};
use crate::geometry::{Dab, StrokeStyle};
use crate::raster::{RasterError, Viewport};
use tiny_skia::Pixmap;

fn dab(x: f32, y: f32, r: f32, opacity: f32, hardness: f32) -> Dab {
    Dab {
        x,
        y,
        radius_x: r,
        radius_y: r,
        angle: 0.0,
        opacity,
        hardness,
        grain_strength: 0.0,
        grain_level: 1.0,
    }
}

fn black(opacity: f32) -> StrokeStyle {
    StrokeStyle {
        color: Color::rgb(0, 0, 0),
        opacity,
        blend: BlendMode::Normal,
    }
}

fn render_on_white(dabs: &[Dab], style: &StrokeStyle, grain: Option<&GrainParams>) -> Pixmap {
    let mut pm = Pixmap::new(64, 64).unwrap();
    pm.fill(tiny_skia::Color::WHITE);
    let mut scratch = CoverageBuffer::default();
    render_stamps(
        &mut pm,
        dabs,
        grain,
        style,
        &Viewport::IDENTITY,
        &mut scratch,
    )
    .unwrap();
    pm
}

fn red(pm: &Pixmap, x: u32, y: u32) -> u8 {
    pm.pixel(x, y).unwrap().demultiply().red()
}

#[test]
fn falloff_profile() {
    let cases = [
        (0.0, 0.5, 1.0),
        (0.5, 0.5, 0.5),
        (1.0, 0.5, 0.0),
        (1.5, 0.5, 0.0),
        (0.99, 1.0, 1.0),
        (1.01, 1.0, 0.0),
        (f32::NAN, 0.5, 0.0),
    ];
    for (rr, h, want) in cases {
        let got = dab_falloff(rr, h);
        assert!(
            (got - want).abs() < 1e-6,
            "falloff({rr}, {h}) = {got}, want {want}"
        );
    }
    let mut prev = 1.0;
    for i in 0..=100u8 {
        let v = dab_falloff(f32::from(i) / 100.0, 0.3);
        assert!(v <= prev && (0.0..=1.0).contains(&v));
        prev = v;
    }
}

#[test]
fn min_fade_keeps_soft_dabs_and_optical_radius() {
    assert_eq!(min_fade(20.0, 0.5), (20.0, 0.5, false));
    let (r, h, faded) = min_fade(10.0, 1.0);
    assert!(faded);
    assert!((r - 10.5).abs() < 1e-5);
    assert!((h - 9.5 / 10.5).abs() < 1e-6);
    assert!((r * (1.0 - h) - 1.0).abs() < 1e-5, "fade is exactly 1 px");
    assert!(
        (r * (1.0 + h) / 2.0 - 10.0).abs() < 1e-4,
        "optical radius kept"
    );
    let (r, h, faded) = min_fade(0.3, 1.0);
    assert!(
        faded && h > 0.0 && r > 0.3 && r.is_finite(),
        "sub-pixel dabs stay visible: {r} {h}"
    );
}

#[test]
fn min_fade_reports_fade_when_hardness_sits_on_the_floor() {
    // The re-solved hardness equals the input bit for bit, yet the radius changed: the flag must say so.
    let (r, h, faded) = min_fade(0.5, MIN_FADE_HARDNESS);
    assert!(faded);
    assert_eq!(h.to_bits(), MIN_FADE_HARDNESS.to_bits());
    assert!(r > 0.5, "radius grew to the 1 px fade: {r}");
}

#[test]
fn sub_pixel_dab_at_the_fade_floor_stays_round() {
    let d = dab(32.5, 32.5, 0.5, 1.0, MIN_FADE_HARDNESS);
    let pm = render_on_white(&[d], &black(1.0), None);
    let (right, below) = (red(&pm, 33, 32), red(&pm, 32, 33));
    assert_eq!(right, below, "circular dab renders symmetric");
    assert!(right < 255, "neighbours get ink from the faded rim");
}

#[test]
fn reset_validates_and_zeroes() {
    let mut buf = CoverageBuffer::default();
    buf.reset(3, 4, 5, 6).unwrap();
    assert_eq!(
        (buf.origin_x, buf.origin_y, buf.width, buf.height),
        (3, 4, 5, 6)
    );
    assert_eq!(buf.data.len(), 30);
    buf.data.fill(0.7);
    buf.reset(0, 0, 2, 2).unwrap();
    assert_eq!(buf.data, vec![0.0; 4]);
    let err = RasterError::InvalidSize {
        width: u32::MAX,
        height: 2,
    };
    assert_eq!(buf.reset(0, 0, u32::MAX, 2), Err(err));
    assert!(buf.reset(0, 0, 1 << 15, 1 << 14).is_err());
}

#[test]
fn stroke_opacity_cap_holds_when_dabs_overlap() {
    let style = black(0.5);
    let one = render_on_white(&[dab(32.0, 32.0, 10.0, 1.0, 1.0)], &style, None);
    let many: Vec<Dab> = (0..50u8)
        .map(|i| dab(32.0 + f32::from(i) * 0.01, 32.0, 10.0, 0.6, 1.0))
        .collect();
    let many = render_on_white(&many, &style, None);
    let (c1, cn) = (red(&one, 32, 32), red(&many, 32, 32));
    assert!(c1.abs_diff(128) <= 1, "single dab at cap: {c1}");
    assert!(
        cn.abs_diff(128) <= 1,
        "50 overlapping dabs still at cap: {cn}"
    );
    assert_eq!(red(&many, 2, 2), 255);
}

#[test]
fn flow_builds_up_below_the_cap() {
    let style = black(1.0);
    let one = render_on_white(&[dab(32.0, 32.0, 10.0, 0.3, 1.0)], &style, None);
    let two = render_on_white(&[dab(32.0, 32.0, 10.0, 0.3, 1.0); 2], &style, None);
    let (c1, c2) = (red(&one, 32, 32), red(&two, 32, 32));
    // 1 − 0.3 = 0.7 → 178; union of two: 1 − 0.51 = 0.49 → 125.
    assert!(c1.abs_diff(179) <= 1, "{c1}");
    assert!(c2.abs_diff(125) <= 1, "{c2}");
}

#[test]
fn soft_dab_fades_toward_the_edge() {
    let pm = render_on_white(&[dab(32.0, 32.0, 16.0, 1.0, 0.2)], &black(1.0), None);
    let (center, mid, outside) = (red(&pm, 32, 32), red(&pm, 40, 32), red(&pm, 50, 32));
    assert!(center < mid && mid < 255, "{center} {mid}");
    assert_eq!(outside, 255);
}

#[test]
fn elliptical_dab_follows_angle() {
    let mut d = dab(32.0, 32.0, 12.0, 1.0, 1.0);
    d.radius_y = 3.0;
    d.angle = core::f32::consts::FRAC_PI_2;
    let pm = render_on_white(&[d], &black(1.0), None);
    assert!(red(&pm, 32, 38) < 128, "major axis is vertical");
    assert_eq!(red(&pm, 38, 32), 255, "minor axis is horizontal");
}

#[test]
fn grain_breaks_up_coverage() {
    let grain = GrainParams {
        scale: 1.0,
        strength: 1.0,
        softness: 0.05,
    };
    let mut d = dab(32.0, 32.0, 20.0, 1.0, 1.0);
    d.grain_strength = 1.0;
    d.grain_level = 0.4;
    let plain = render_on_white(&[d], &black(1.0), None);
    let grained = render_on_white(&[d], &black(1.0), Some(&grain));
    let ink = |pm: &Pixmap| -> u32 { (24..40).map(|x| 255 - u32::from(red(pm, x, 32))).sum() };
    assert!(
        ink(&grained) < ink(&plain),
        "grain removes ink from paper valleys"
    );
}

#[test]
fn invisible_and_malformed_dabs_draw_nothing() {
    let mut pm = Pixmap::new(16, 16).unwrap();
    let mut scratch = CoverageBuffer::default();
    let dabs = [
        dab(8.0, 8.0, 4.0, 0.0, 1.0),
        dab(8.0, 8.0, 0.0, 1.0, 1.0),
        dab(f32::NAN, 8.0, 4.0, 1.0, 1.0),
    ];
    render_stamps(
        &mut pm,
        &dabs,
        None,
        &black(1.0),
        &Viewport::IDENTITY,
        &mut scratch,
    )
    .unwrap();
    assert!(pm.data().iter().all(|&b| b == 0));
    assert!(scratch.data.is_empty());
}
