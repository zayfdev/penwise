use super::{BlendMode, BrushFamily, Preset};

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
