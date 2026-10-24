use super::{BlendMode, BrushFamily, Preset};
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
