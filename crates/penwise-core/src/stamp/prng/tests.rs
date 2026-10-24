#![allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    reason = "test arithmetic on small counts"
)]

use super::*;

#[test]
fn splitmix64_reference_vectors() {
    // core-determinism.md §2: SplitMix64 stream from seed 0 (state += GAMMA; mix64(state)).
    assert_eq!(mix64(GAMMA), 0xe220_a839_7b1d_cdaf);
    assert_eq!(mix64(GAMMA.wrapping_mul(2)), 0x6e78_9e6a_a1b9_65f4);
    assert_eq!(mix64(GAMMA.wrapping_mul(3)), 0x06c4_5d18_8009_454f);
}

#[test]
fn keyed_vectors_match_module_doc() {
    assert_eq!(rand_u64(0, 0, 0), 0xe220_a839_7b1d_cdaf);
    assert_eq!(rand_u64(0, 0, 1), 0x6e78_9e6a_a1b9_65f4);
    assert_eq!(rand_u64(0, 0, 2), 0x06c4_5d18_8009_454f);
}

#[test]
fn dab_slots_are_contiguous_channels() {
    // Dab 1 channel 0 is the 9th draw of the seed-0 stream.
    assert_eq!(rand_u64(0, 1, 0), mix64(GAMMA.wrapping_mul(9)));
    assert_eq!(
        rand_u64(7, 3, 5),
        mix64(7u64.wrapping_add(GAMMA.wrapping_mul(3 * 8 + 5 + 1)))
    );
}

#[test]
fn rand_unit_is_exact_top_24_bits() {
    let u = rand_unit(0, 0, 0);
    assert_eq!(u, (0xe220_a839_7b1d_cdafu64 >> 40) as f32 / 16_777_216.0);
    for i in 0..10_000 {
        for ch in 0..8 {
            let v = rand_unit(42, i, ch);
            assert!((0.0..1.0).contains(&v));
        }
    }
}

#[test]
fn rand_unit_is_roughly_uniform() {
    let n = 20_000u64;
    let mean: f64 = (0..n).map(|i| f64::from(rand_unit(1, i, 0))).sum::<f64>() / n as f64;
    assert!((mean - 0.5).abs() < 0.01, "mean {mean}");
}

#[test]
fn seed_changes_output() {
    assert_ne!(rand_u64(1, 0, 0), rand_u64(2, 0, 0));
    assert_ne!(rand_unit(1, 5, 3), rand_unit(2, 5, 3));
}

#[test]
fn particle_domain_is_separate_and_keyed() {
    assert_eq!(
        rand_particle_unit(9, 2, 3, 1),
        rand_unit(9 ^ PARTICLE_DOMAIN, 2 * MAX_PARTICLES + 3, 1)
    );
    assert_ne!(rand_particle_unit(9, 2, 0, 1), rand_unit(9, 2, 1));
    assert_ne!(
        rand_particle_unit(9, 2, 0, 1),
        rand_particle_unit(9, 2, 1, 1)
    );
    // Huge indices wrap instead of panicking.
    let _ = rand_particle_unit(u64::MAX, u64::MAX, u32::MAX, 7);
}
