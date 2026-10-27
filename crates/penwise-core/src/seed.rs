//! The per-stroke PRNG seed and its wire forms.
//!
//! In memory a seed is a `u64` (stamp jitter/scatter are pure functions of it). The tablet's `Long` and the
//! legacy decimal text carry the same 64 bits as a **signed** value; this is the only place that conversion
//! is defined, so a negative wire value always lands on the same `u64` on every surface.

/// A stroke's PRNG seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Seed(pub u64);

impl Seed {
    /// Seed of a signed wire value (two's complement: `-1` is `u64::MAX`).
    #[must_use]
    pub const fn from_i64_wire(wire: i64) -> Self {
        Self(wire.cast_unsigned())
    }

    /// The signed wire value of this seed; inverse of [`Seed::from_i64_wire`].
    #[must_use]
    pub const fn to_i64_wire(self) -> i64 {
        self.0.cast_signed()
    }

    /// The seed bits.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Seed of decimal text: a signed value in `i64` range is bit-cast, otherwise an unsigned value up to
    /// `u64::MAX` is taken as is. `None` when `text` is neither.
    #[must_use]
    pub fn parse_decimal(text: &str) -> Option<Self> {
        text.parse::<i64>()
            .map(Self::from_i64_wire)
            .or_else(|_| text.parse::<u64>().map(Self))
            .ok()
    }
}

#[cfg(test)]
mod tests;
