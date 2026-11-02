//! Colour-meaning tests: the three pinned colours and their near neighbours, everything else a plain note, and JSON
//! names. The maths is f32 with no libm calls beyond min, so native and wasm agree bit for bit.

use super::*;

/// Colour from hue (degrees), saturation and value, for drift tests (values stay in 0..=1, so the casts are exact).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn hsv(hue: f32, saturation: f32, value: f32) -> u32 {
    let c = value * saturation;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = value - c;
    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |v: f32| ((v + m) * 255.0 + 0.5) as u32;
    0xFF00_0000 | (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

#[test]
fn the_pinned_colours_keep_their_meanings() {
    assert_eq!(colour_meaning(0xFFD6_2F2F), ColourMeaning::Problem);
    assert_eq!(colour_meaning(0xFF1F_5FD6), ColourMeaning::Idea);
    assert_eq!(colour_meaning(0xFF2E_9E4F), ColourMeaning::Good);
}

#[test]
fn small_drift_around_a_pinned_colour_still_matches() {
    for (hue, meaning) in [
        (0.0, ColourMeaning::Problem),
        (219.0, ColourMeaning::Idea),
        (138.0, ColourMeaning::Good),
    ] {
        for drift in [-10.0, 0.0, 10.0] {
            for (saturation, value) in [(0.8, 0.85), (0.6, 0.95), (0.85, 0.5)] {
                let h = (hue + drift + 360.0) % 360.0;
                assert_eq!(
                    colour_meaning(hsv(h, saturation, value)),
                    meaning,
                    "{h} {saturation} {value}"
                );
            }
        }
    }
}

#[test]
fn colours_between_the_pinned_ones_are_plain_notes() {
    assert_eq!(colour_meaning(0xFFF0_8A1C), ColourMeaning::Other, "orange");
    assert_eq!(colour_meaning(0xFFF5_D200), ColourMeaning::Other, "yellow");
    assert_eq!(colour_meaning(0xFF7B_3FD1), ColourMeaning::Other, "purple");
    assert_eq!(
        colour_meaning(0xFFFF_8000),
        ColourMeaning::Other,
        "pure orange"
    );
    assert_eq!(colour_meaning(0xFF00_FFFF), ColourMeaning::Other, "cyan");
    assert_eq!(colour_meaning(0xFF00_B0A0), ColourMeaning::Other, "teal");
    assert_eq!(colour_meaning(0xFFFF_2D9B), ColourMeaning::Other, "pink");
}

#[test]
fn pure_primaries_off_the_pinned_hues_are_plain_notes() {
    // Pure green is 120 degrees (18 from the pinned green), pure blue 240 (21 from the pinned blue).
    assert_eq!(colour_meaning(0xFF00_FF00), ColourMeaning::Other);
    assert_eq!(colour_meaning(0xFF00_00FF), ColourMeaning::Other);
    assert_eq!(colour_meaning(0xFFFF_0000), ColourMeaning::Problem);
}

#[test]
fn black_grey_white_pastel_dark_and_transparent_ink_are_plain_notes() {
    assert_eq!(colour_meaning(0xFF00_0000), ColourMeaning::Other);
    assert_eq!(colour_meaning(0xFF80_8080), ColourMeaning::Other);
    assert_eq!(colour_meaning(0xFFFF_FFFF), ColourMeaning::Other);
    assert_eq!(colour_meaning(0x00D6_2F2F), ColourMeaning::Other);
    assert_eq!(colour_meaning(0xFF14_141E), ColourMeaning::Other);
    assert_eq!(
        colour_meaning(0xFF3A_0A0A),
        ColourMeaning::Other,
        "too dark"
    );
    assert_eq!(
        colour_meaning(0xFFE8_B0B0),
        ColourMeaning::Other,
        "washed out"
    );
}

#[test]
fn the_tablets_starter_recents_and_ink_pins_are_plain_notes() {
    // android `Palette`: orange, purple, yellow, teal, pink, and the ink black and graphite buttons.
    for argb in [
        0xFFF0_8A1C,
        0xFF7B_3FD1,
        0xFFF5_D200,
        0xFF0F_9D9A,
        0xFFE0_457B,
        0xFF1A_1A1A,
        0xFF6B_6B6B,
    ] {
        assert_eq!(colour_meaning(argb), ColourMeaning::Other, "{argb:#X}");
    }
}

#[test]
fn colour_meaning_json_names() {
    assert_eq!(
        serde_json::to_string(&ColourMeaning::Problem).unwrap(),
        r#""problem""#
    );
    assert_eq!(
        serde_json::to_string(&ColourMeaning::Idea).unwrap(),
        r#""idea""#
    );
    assert_eq!(
        serde_json::to_string(&ColourMeaning::Good).unwrap(),
        r#""good""#
    );
    assert_eq!(
        serde_json::to_string(&ColourMeaning::Other).unwrap(),
        r#""none""#
    );
    assert_eq!(ColourMeaning::Other.as_str(), "none");
}
