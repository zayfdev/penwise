//! The one rule for turning a stored preset choice into brush params (tablet semantics, owner decision 4).

use super::{BrushParams, Color, Preset};

impl Color {
    /// `0xAARRGGBB` (Android `Color` int bits) → colour.
    #[must_use]
    pub const fn from_argb(argb: u32) -> Self {
        let [a, r, g, b] = argb.to_be_bytes();
        Self { r, g, b, a }
    }

    /// Colour → `0xAARRGGBB`.
    #[must_use]
    pub const fn to_argb(self) -> u32 {
        u32::from_be_bytes([self.a, self.r, self.g, self.b])
    }
}

impl BrushParams {
    /// Params of `Preset::ALL[preset_index]` (an index past the end selects the last preset) with the colour
    /// replaced by `argb` (`0xAARRGGBB`, alpha included) and every size-like length scaled by `size_scale`:
    /// `size_min`, `size_max`, both outline tapers, the nib thickness and the spray particle sizes. A scale
    /// that is not finite and positive counts as 1. This is the single definition used by the tablet, the
    /// server renderer and the browser.
    #[must_use]
    pub fn for_preset(preset_index: usize, argb: u32, size_scale: f32) -> Self {
        let index = preset_index.min(Preset::ALL.len() - 1);
        let scale = if size_scale.is_finite() && size_scale > 0.0 {
            size_scale
        } else {
            1.0
        };
        let mut p = Preset::ALL[index].params();
        p.color = Color::from_argb(argb);
        p.size_min *= scale;
        p.size_max *= scale;
        p.outline.taper_start *= scale;
        p.outline.taper_end *= scale;
        p.outline.nib.thickness *= scale;
        if let Some(spray) = p.stamp.spray.as_mut() {
            spray.particle_size_min *= scale;
            spray.particle_size_max *= scale;
        }
        p
    }
}
