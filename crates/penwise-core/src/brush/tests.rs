use super::{BlendMode, BrushFamily, BrushParams, Color, Preset};
use crate::float::{lerp, min};

/// Normal handwriting pressure `c(p)` (Gate C reference).
const NORMAL_PRESSURE: f32 = 0.6;

#[test]
fn family_codes_round_trip_and_are_unique() {
    let mut seen = Vec::new();
    for preset in Preset::ALL {
        let family = preset.params().family;
        let code = family.code();
        assert_eq!(BrushFamily::from_code(code), Some(family));
        if !seen.contains(&code) {
            seen.push(code);
        }
    }
    assert_eq!(
        seen.len(),
        Preset::ALL.len(),
        "each preset has its own family kind"
    );
    assert_eq!(BrushFamily::from_code(0x06), None);
    assert_eq!(
        BlendMode::from_code(BlendMode::Multiply.code()),
        Some(BlendMode::Multiply)
    );
}

#[test]
fn presets_have_sane_ranges() {
    for preset in Preset::ALL {
        let p = preset.params();
        let name = preset.name();
        assert!(
            p.size_min > 0.0 && p.size_min <= p.size_max,
            "{name}: sizes"
        );
        assert!(p.opacity > 0.0 && p.opacity <= 1.0, "{name}: opacity");
        assert!(
            p.stamp.hardness > 0.0 && p.stamp.hardness <= 1.0,
            "{name}: hardness"
        );
        assert!(p.stamp.spacing > 0.0, "{name}: spacing");
    }
}

#[test]
fn highlighter_multiplies_and_ignores_pressure() {
    let p = Preset::Highlighter.params();
    assert_eq!(p.blend, BlendMode::Multiply);
    assert!(p.dynamics.pressure.saturation <= p.dynamics.pressure.floor);
}

#[test]
fn stamp_presets_are_visible_at_normal_pressure() {
    for preset in Preset::ALL {
        let p = preset.params();
        if !matches!(p.family, BrushFamily::Stamp(_)) {
            continue;
        }
        let st = p.stamp;
        // Alpha deposited per diameter of travel at normal pressure, under the stroke opacity cap.
        let flow = st.flow * lerp(1.0, NORMAL_PRESSURE, st.flow_pressure);
        let deposit = min(flow, 1.0) * p.opacity;
        assert!(deposit >= 0.3, "{}: deposit {deposit}", preset.name());
    }
}

#[test]
fn fountain_width_responds_to_pressure() {
    let p = Preset::Fountain.params();
    assert!(
        p.size_max >= 4.0 * p.size_min,
        "{} → {}",
        p.size_min,
        p.size_max
    );
}

/// Tablet semantics (`penwise-ffi` `brush_params`) at scale 2 for every preset: size range, end taper, nib
/// thickness and spray particle sizes all double. All values are exact in binary floating point.
#[test]
fn for_preset_scales_every_size_like_length_like_the_tablet() {
    // index, size_min, size_max, taper_end, nib thickness, spray particle sizes
    type Row = (usize, f32, f32, f32, f32, Option<(f32, f32)>);
    #[rustfmt::skip]
    let golden: [Row; 13] = [
        (0, 4.8, 10.0, 4.0, 0.0, None),
        (1, 7.6, 9.2, 0.0, 0.0, None),
        (2, 3.0, 20.0, 12.0, 0.0, None),
        (3, 20.0, 44.0, 0.0, 3.0, None),
        (4, 28.0, 36.0, 0.0, 0.0, None),
        (5, 52.0, 52.0, 0.0, 16.0, None),
        (6, 6.0, 12.0, 0.0, 0.0, None),
        (7, 16.0, 28.0, 0.0, 0.0, None),
        (8, 32.0, 120.0, 0.0, 0.0, None),
        (9, 48.0, 112.0, 0.0, 0.0, Some((2.0, 5.0))),
        (10, 24.0, 96.0, 0.0, 0.0, None),
        (11, 12.0, 48.0, 0.0, 0.0, None),
        (12, 32.0, 112.0, 0.0, 0.0, None),
    ];
    for (index, size_min, size_max, taper_end, nib, spray) in golden {
        let p = BrushParams::for_preset(index, 0x80FF_8000, 2.0);
        let base = Preset::ALL[index].params();
        assert_eq!(p.size_min.to_bits(), size_min.to_bits(), "{index} size_min");
        assert_eq!(p.size_max.to_bits(), size_max.to_bits(), "{index} size_max");
        assert_eq!(
            p.outline.taper_end.to_bits(),
            taper_end.to_bits(),
            "{index} taper_end"
        );
        assert_eq!(
            p.outline.nib.thickness.to_bits(),
            nib.to_bits(),
            "{index} nib"
        );
        let sizes = p
            .stamp
            .spray
            .map(|s| (s.particle_size_min, s.particle_size_max));
        assert_eq!(sizes, spray, "{index} spray");
        // Everything that is not a length is the preset's own: family, opacity, blend, dynamics, caps.
        assert_eq!(p.family, base.family);
        assert_eq!(p.opacity.to_bits(), base.opacity.to_bits());
        assert_eq!(p.blend, base.blend);
        assert_eq!(p.dynamics, base.dynamics);
        assert_eq!(p.smoothing, base.smoothing);
        assert_eq!(
            p.outline.nib.angle.to_bits(),
            base.outline.nib.angle.to_bits()
        );
    }
}

#[test]
fn for_preset_replaces_the_colour_including_alpha() {
    let p = BrushParams::for_preset(0, 0x8012_34FE, 1.0);
    assert_eq!(
        p.color,
        Color {
            r: 0x12,
            g: 0x34,
            b: 0xFE,
            a: 0x80
        }
    );
    assert_eq!(p.color.to_argb(), 0x8012_34FE);
    // Android's signed Int for opaque white is -1.
    assert_eq!(
        Color::from_argb((-1_i32).cast_unsigned()),
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255
        }
    );
}

#[test]
fn for_preset_at_scale_one_only_changes_the_colour() {
    for (index, preset) in Preset::ALL.into_iter().enumerate() {
        let p = BrushParams::for_preset(index, 0xFF00_00FF, 1.0);
        assert_eq!(
            p,
            BrushParams {
                color: p.color,
                ..preset.params()
            },
            "{index}"
        );
    }
}

#[test]
fn for_preset_clamps_the_index_and_ignores_unusable_scales() {
    let last = Preset::ALL.len() - 1;
    assert_eq!(
        BrushParams::for_preset(usize::MAX, 0xFF00_0000, 1.0),
        BrushParams::for_preset(last, 0xFF00_0000, 1.0)
    );
    let unit = BrushParams::for_preset(2, 0xFF00_0000, 1.0);
    for scale in [0.0, -2.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            BrushParams::for_preset(2, 0xFF00_0000, scale),
            unit,
            "{scale}"
        );
    }
}
