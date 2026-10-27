//! HLC send/recv rules and properties (monotone, causal, overflow bump, skew reject, storage order).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "test harness: failures are reported by panicking"
)]

mod common;

use common::{Rng, device, hlc};
use penwise_proto::hlc::{ClockOffset, HLC_BYTES, MAX_WALL_MS};
use penwise_proto::{Hlc, HlcClock, HlcError, MAX_SKEW_MS};

#[test]
fn send_follows_physical_time_and_counts_within_a_millisecond() {
    let mut clock = HlcClock::new(device(1));
    assert_eq!(clock.send(1_000), hlc(1_000, 0, 1));
    assert_eq!(clock.send(1_000), hlc(1_000, 1, 1));
    assert_eq!(clock.send(1_001), hlc(1_001, 0, 1));
    // Physical time going backwards never lowers the clock.
    assert_eq!(clock.send(500), hlc(1_001, 1, 1));
}

#[test]
fn counter_overflow_moves_wall_forward_instead_of_wrapping() {
    let mut clock = HlcClock::resume(hlc(7_000, u16::MAX, 1));
    let next = clock.send(7_000);
    assert_eq!(next, hlc(7_001, 0, 1));
    let again = clock.send(0);
    assert_eq!(again, hlc(7_001, 1, 1));
    assert!(again > next);
}

#[test]
fn recv_overflow_also_bumps_wall() {
    let mut clock = HlcClock::resume(hlc(7_000, 3, 1));
    let remote = hlc(7_000, u16::MAX, 2);
    let stamp = clock.recv(remote, 7_000).unwrap();
    assert_eq!(stamp, hlc(7_001, 0, 1));
    assert!(stamp > remote);
}

#[test]
fn recv_applies_each_branch_of_the_rule() {
    // Both at the winning wall: max counter + 1.
    let mut clock = HlcClock::resume(hlc(100, 5, 1));
    assert_eq!(clock.recv(hlc(100, 9, 2), 50).unwrap(), hlc(100, 10, 1));
    // Only the local clock at the winning wall.
    let mut clock = HlcClock::resume(hlc(100, 5, 1));
    assert_eq!(clock.recv(hlc(90, 9, 2), 50).unwrap(), hlc(100, 6, 1));
    // Only the remote stamp at the winning wall.
    let mut clock = HlcClock::resume(hlc(90, 5, 1));
    assert_eq!(clock.recv(hlc(100, 9, 2), 50).unwrap(), hlc(100, 10, 1));
    // Physical time wins: counter restarts.
    let mut clock = HlcClock::resume(hlc(90, 5, 1));
    assert_eq!(clock.recv(hlc(80, 9, 2), 200).unwrap(), hlc(200, 0, 1));
}

#[test]
fn recv_rejects_stamps_too_far_ahead_and_leaves_the_clock_alone() {
    let mut clock = HlcClock::resume(hlc(1_000, 2, 1));
    let before = clock.last();
    let ahead = hlc(10_000 + MAX_SKEW_MS + 1, 0, 2);
    assert_eq!(
        clock.recv(ahead, 10_000),
        Err(HlcError::ClockSkew {
            ahead_ms: MAX_SKEW_MS + 1
        })
    );
    assert_eq!(clock.last(), before);
    // Exactly at the limit is accepted.
    let edge = hlc(10_000 + MAX_SKEW_MS, 0, 2);
    assert!(clock.recv(edge, 10_000).is_ok());
}

#[test]
fn wall_saturates_at_the_ceiling() {
    let mut clock = HlcClock::new(device(1));
    let stamp = clock.send(u64::MAX);
    assert_eq!(stamp.wall_ms, MAX_WALL_MS);
    assert!(stamp.pack() < 1 << 63);
    // Stays at the ceiling without wrapping the counter.
    clock = HlcClock::resume(hlc(MAX_WALL_MS, u16::MAX, 1));
    let stuck = clock.send(u64::MAX);
    assert_eq!((stuck.wall_ms, stuck.counter), (MAX_WALL_MS, u16::MAX));
}

/// Three devices exchange stamps in random order with jittery clocks: every issued stamp is unique, each
/// clock is strictly monotone, and a stamp created after observing another is greater than it.
#[test]
fn random_exchange_is_monotone_causal_and_unique() {
    for seed in 0..50 {
        let mut rng = Rng(seed);
        let mut clocks: Vec<HlcClock> = (1..=3).map(|d| HlcClock::new(device(d))).collect();
        let mut issued: Vec<Hlc> = Vec::new();
        let mut now = 1_000_000_u64;
        for _ in 0..200 {
            now = now.saturating_add(rng.below(3) as u64);
            let who = rng.below(clocks.len());
            // Local clocks are skewed by a few ms either way, never beyond the limit.
            let local_now = (now + 5 * who as u64).saturating_sub(rng.below(4) as u64);
            let before = clocks[who].last();
            let stamp = if !issued.is_empty() && rng.chance(50) {
                let seen = issued[rng.below(issued.len())];
                let stamp = clocks[who].recv(seen, local_now).unwrap();
                assert!(stamp > seen, "recv result must follow the observed stamp");
                stamp
            } else {
                clocks[who].send(local_now)
            };
            assert!(stamp > before, "clock must be strictly monotone");
            assert_eq!(stamp.device, device(who as u64 + 1));
            issued.push(stamp);
        }
        let mut sorted = issued.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), issued.len(), "stamps must be unique");
    }
}

#[test]
fn storage_bytes_round_trip_and_sort_like_ord() {
    let mut rng = Rng(7);
    let mut stamps: Vec<Hlc> = (0..200)
        .map(|_| Hlc {
            wall_ms: rng.next_u64() % (MAX_WALL_MS + 1),
            counter: u16::try_from(rng.below(1 << 16)).unwrap(),
            device: device(rng.next_u64() % 4),
        })
        .collect();
    for stamp in &stamps {
        let bytes = stamp.to_bytes();
        assert_eq!(bytes.len(), HLC_BYTES);
        assert_eq!(Hlc::from_bytes(&bytes), Some(*stamp));
    }
    let mut by_bytes = stamps.clone();
    stamps.sort();
    by_bytes.sort_by_key(|h| h.to_bytes());
    assert_eq!(stamps, by_bytes, "memcmp order must equal Ord");
}

#[test]
fn from_bytes_rejects_wrong_length_and_oversized_wall() {
    let good = hlc(5, 1, 1).to_bytes();
    assert_eq!(Hlc::from_bytes(&good[..23]), None);
    let mut long = good.to_vec();
    long.push(0);
    assert_eq!(Hlc::from_bytes(&long), None);
    let mut too_big = good;
    too_big[..8].copy_from_slice(&u64::MAX.to_be_bytes());
    assert_eq!(Hlc::from_bytes(&too_big), None);
}

#[test]
fn ties_in_time_break_by_device_bytes() {
    assert!(hlc(5, 1, 2) > hlc(5, 1, 1));
    assert!(hlc(5, 2, 1) > hlc(5, 1, 9));
    assert!(hlc(6, 0, 1) > hlc(5, 9, 9));
}

#[test]
fn clock_offset_calibrates_a_wrong_device_clock() {
    // Device clock is 10 minutes ahead of the server: raw time would be rejected as skew by the server.
    let offset = ClockOffset::from_welcome(1_000_000, 1_600_000);
    assert_eq!(offset.millis(), -600_000);
    assert_eq!(offset.adjust(1_700_000), 1_100_000);
    // Behind: lifts the clock.
    let behind = ClockOffset::from_welcome(2_000_000, 1_000_000);
    assert_eq!(behind.adjust(1_500_000), 2_500_000);
    // Clamped to the valid range.
    assert_eq!(ClockOffset::from_millis(-10).adjust(3), 0);
    assert_eq!(
        ClockOffset::from_millis(i64::MAX).adjust(u64::MAX),
        MAX_WALL_MS
    );
    assert_eq!(ClockOffset::ZERO.adjust(42), 42);
    assert_eq!(ClockOffset::from_millis(offset.millis()), offset);
}
