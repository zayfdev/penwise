//! Counter-based `SplitMix64` keyed by `(seed, dab_index, channel)` (core-determinism.md §2).
//!
//! Owner: slice c.
//!
//! Frozen as part of the stroke format: `seed` is stored per stroke forever, so any change here is a
//! brush-engine version bump. Keying by dab (not a sequential stream) keeps old strokes stable when a brush adds
//! a channel and lets dabs be computed out of order.
//!
//! Test vectors (seed 0): `rand_u64(0, 0, 0) = 0xe220_a839_7b1d_cdaf`,
//! `rand_u64(0, 0, 1) = 0x6e78_9e6a_a1b9_65f4`, `rand_u64(0, 0, 2) = 0x06c4_5d18_8009_454f`.

/// `SplitMix64` increment (golden ratio).
pub const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
/// Fixed channel slots per dab.
pub const CHANNELS_PER_DAB: u64 = 8;
/// Upper bound of particles per spray burst; particle `i` of dab `d` is keyed as dab `d·MAX_PARTICLES + i`
/// in the particle domain (see [`rand_particle_unit`]).
pub const MAX_PARTICLES: u64 = 64;
/// Domain separator xor-ed into the seed for particle draws.
pub const PARTICLE_DOMAIN: u64 = 0x5052_5452_4943_4C45;

/// Channel ids (stable; append only).
pub mod channel {
    /// Scatter radius `f0`.
    pub const SCATTER_RADIUS: u32 = 0;
    /// Scatter angle `f1`.
    pub const SCATTER_ANGLE: u32 = 1;
    /// Size jitter `f2`.
    pub const SIZE: u32 = 2;
    /// Opacity jitter `f3`.
    pub const OPACITY: u32 = 3;
    /// Angle jitter `f4`.
    pub const ANGLE: u32 = 4;
    /// Particle size (particle domain).
    pub const PARTICLE_SIZE: u32 = 5;
    /// Second Box–Muller uniform / spare.
    pub const SPARE: u32 = 6;
}

/// `SplitMix64` finalizer: `z ^= z>>30; z *= 0xBF58476D1CE4E5B9; z ^= z>>27; z *= 0x94D049BB133111EB; z ^ z>>31`
/// (wrapping multiplies).
#[must_use]
pub const fn mix64(z: u64) -> u64 {
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// `mix64(seed + (dab_index·CHANNELS_PER_DAB + channel + 1)·GAMMA)`, all wrapping. `channel` must be
/// `< CHANNELS_PER_DAB` (larger values alias the next dab's slots; callers use the [`channel`] constants).
#[must_use]
#[allow(clippy::cast_lossless, reason = "u64::from is not const")]
pub const fn rand_u64(seed: u64, dab_index: u64, channel: u32) -> u64 {
    let slot = dab_index
        .wrapping_mul(CHANNELS_PER_DAB)
        .wrapping_add(channel as u64)
        .wrapping_add(1);
    mix64(seed.wrapping_add(slot.wrapping_mul(GAMMA)))
}

/// Uniform `[0, 1)`: `(rand_u64(..) >> 40) as f32 · 2⁻²⁴` (exact).
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "24-bit integer, exact in f32")]
pub fn rand_unit(seed: u64, dab_index: u64, channel: u32) -> f32 {
    (rand_u64(seed, dab_index, channel) >> 40) as f32 * (1.0 / 16_777_216.0)
}

/// Uniform `[0, 1)` for spray particle `particle` (`< MAX_PARTICLES`) of dab `dab_index`:
/// `rand_unit(seed ^ PARTICLE_DOMAIN, dab_index·MAX_PARTICLES + particle, channel)` (wrapping).
#[must_use]
pub fn rand_particle_unit(seed: u64, dab_index: u64, particle: u32, channel: u32) -> f32 {
    let key = dab_index
        .wrapping_mul(MAX_PARTICLES)
        .wrapping_add(u64::from(particle));
    rand_unit(seed ^ PARTICLE_DOMAIN, key, channel)
}

#[cfg(test)]
mod tests;
