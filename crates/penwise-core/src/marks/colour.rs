//! Owner: design-harness M3 (core). What the owner's ink colour means (spec §5).
//!
//! Red = a problem, blue = an idea, green = good/keep. Black, grey and white (and any transparent or very dark
//! ink) carry no colour meaning. The test is saturation plus nearest hue: orange reads as red, cyan as blue, a
//! washed-out pastel still counts if it is saturated enough.
//!
//! `argb` is `0xAARRGGBB`, the same bits as [`crate::Color`] (Android `Color` int).

use serde::{Deserialize, Serialize};

/// The meaning of an ink colour. Serialized as `"problem"`, `"idea"`, `"good"` or `"none"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColourMeaning {
    /// Red: something is wrong.
    #[serde(rename = "problem")]
    Problem,
    /// Blue: an idea.
    #[serde(rename = "idea")]
    Idea,
    /// Green: good, keep it.
    #[serde(rename = "good")]
    Good,
    /// Anything else: no colour meaning (black, grey, white, other ink).
    #[serde(rename = "none")]
    Other,
}

impl ColourMeaning {
    /// The lowercase name the clients and JSON use (`"problem"`, `"idea"`, `"good"`, `"none"`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Problem => "problem",
            Self::Idea => "idea",
            Self::Good => "good",
            Self::Other => "none",
        }
    }
}

/// Ink colour below this HSV value is too dark to carry hue (near-black, like the default ink `#14141E`).
const MIN_VALUE: f32 = 0.15;
/// Ink below this HSV saturation is grey or white, not a colour.
const MIN_SATURATION: f32 = 0.20;
/// Hue targets, in degrees: red, green, blue ink.
const RED: f32 = 0.0;
const GREEN: f32 = 130.0;
const BLUE: f32 = 220.0;

/// Meaning of an `0xAARRGGBB` ink colour.
#[must_use]
pub fn colour_meaning(argb: u32) -> ColourMeaning {
    let [alpha, r, g, b] = argb.to_be_bytes();
    if alpha == 0 {
        return ColourMeaning::Other;
    }
    let (r, g, b) = (
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    );
    let (max, channel) = if r >= g && r >= b {
        (r, 0_u8)
    } else if g >= b {
        (g, 1)
    } else {
        (b, 2)
    };
    let low = crate::float::min(r, crate::float::min(g, b));
    let delta = max - low;
    let saturation = if max > 0.0 { delta / max } else { 0.0 };
    if max < MIN_VALUE || saturation < MIN_SATURATION {
        return ColourMeaning::Other;
    }
    let hue = if delta <= 0.0 {
        0.0
    } else {
        let sector = match channel {
            0 => {
                let h = (g - b) / delta;
                if h < 0.0 { h + 6.0 } else { h }
            }
            1 => (b - r) / delta + 2.0,
            _ => (r - g) / delta + 4.0,
        };
        sector * 60.0
    };
    let distance = |target: f32| {
        let d = (hue - target).abs();
        if d > 180.0 { 360.0 - d } else { d }
    };
    let (to_red, to_green, to_blue) = (distance(RED), distance(GREEN), distance(BLUE));
    if to_red <= to_green && to_red <= to_blue {
        ColourMeaning::Problem
    } else if to_green <= to_blue {
        ColourMeaning::Good
    } else {
        ColourMeaning::Idea
    }
}

#[cfg(test)]
mod tests;
