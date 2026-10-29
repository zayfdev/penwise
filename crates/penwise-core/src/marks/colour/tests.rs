//! Colour-meaning tests: the three vocabulary colours, greys/blacks/whites, and JSON names.

use super::*;

#[test]
fn colour_meanings() {
    assert_eq!(colour_meaning(0xFFFF_0000), ColourMeaning::Problem);
    assert_eq!(colour_meaning(0xFFB0_0000), ColourMeaning::Problem);
    assert_eq!(colour_meaning(0xFFFF_8000), ColourMeaning::Problem);
    assert_eq!(colour_meaning(0xFF00_00FF), ColourMeaning::Idea);
    assert_eq!(colour_meaning(0xFF00_88FF), ColourMeaning::Idea);
    assert_eq!(colour_meaning(0xFF00_C000), ColourMeaning::Good);
    assert_eq!(colour_meaning(0xFF30_FF30), ColourMeaning::Good);
    assert_eq!(colour_meaning(0xFF00_0000), ColourMeaning::Other);
    assert_eq!(colour_meaning(0xFF80_8080), ColourMeaning::Other);
    assert_eq!(colour_meaning(0xFFFF_FFFF), ColourMeaning::Other);
    assert_eq!(colour_meaning(0x00FF_0000), ColourMeaning::Other);
    assert_eq!(colour_meaning(0xFF14_141E), ColourMeaning::Other);
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
