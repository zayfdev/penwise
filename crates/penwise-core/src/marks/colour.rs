//! Owner: design-harness M3 (core). What the owner's ink colour means (spec §5).
//!
//! Only ink close to one of the three pinned colours carries a meaning: red = a problem, blue = an idea, green =
//! good/keep. The pinned colours are the ones the tablet's colour flyout writes (`android` `Palette`: red `#D62F2F`,
//! blue `#1F5FD6`, green `#2E9E4F`), so each band is centred on the exact hue of its colour and is [`HUE_TOLERANCE`]
//! wide on either side. Everything else is a plain note ([`ColourMeaning::Other`]): orange, yellow, purple, cyan,
//! teal, pink, and any black, grey, white, transparent or very dark ink. The ink must also be a real colour:
//! saturation at least [`MIN_SATURATION`] and value at least [`MIN_VALUE`], so a washed-out or near-black ink that
//! happens to sit at a pinned hue stays a plain note. The wheel's neighbours of a pinned colour (about 10 degrees of
//! drift, lighter or darker) still match; 30 degrees away does not.
//!
//! `argb` is `0xAARRGGBB`, the same bits as [`crate::Color`] (Android `Color` int).

use serde::{Deserialize, Serialize};

/// The meaning of an ink colour. Serialized as `"problem"`, `"idea"`, `"good"` or `"none"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColourMeaning {
    /// Red (the pinned `#D62F2F`, within 15 degrees of hue): something is wrong.
    #[serde(rename = "problem")]
    Problem,
    /// Blue (the pinned `#1F5FD6`): an idea.
    #[serde(rename = "idea")]
    Idea,
    /// Green (the pinned `#2E9E4F`): good, keep it.
    #[serde(rename = "good")]
    Good,
    /// Anything else: a plain note (black, grey, white, orange, yellow, purple, cyan, other ink).
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

/// Ink below this HSV value is too dark to read as a colour (near-black, like the default ink `#14141E`).
const MIN_VALUE: f32 = 0.35;
/// Ink below this HSV saturation is grey, white or a pastel, not one of the pinned colours.
const MIN_SATURATION: f32 = 0.40;
/// How far, in degrees of hue, ink may sit from a pinned colour and still carry its meaning.
const HUE_TOLERANCE: f32 = 15.0;
/// Hue of the pinned red `#D62F2F`, green `#2E9E4F` and blue `#1F5FD6`, in degrees.
const RED: f32 = 0.0;
const GREEN: f32 = 137.678_57;
const BLUE: f32 = 219.016_39;

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
    let near = |target: f32| {
        let d = (hue - target).abs();
        let d = if d > 180.0 { 360.0 - d } else { d };
        d <= HUE_TOLERANCE
    };
    if near(RED) {
        ColourMeaning::Problem
    } else if near(GREEN) {
        ColourMeaning::Good
    } else if near(BLUE) {
        ColourMeaning::Idea
    } else {
        ColourMeaning::Other
    }
}

#[cfg(test)]
mod tests;
