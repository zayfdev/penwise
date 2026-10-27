use std::collections::BTreeSet;

use super::*;

#[test]
fn ids_are_unique_and_stamped_with_the_clock() {
    let device = DeviceId::from_parts(1, 1);
    let mut mint = IdMint::new(device, 42);
    mint.set_now(1_700_000_000_000);
    let ids: BTreeSet<[u8; 16]> = (0..1_000).map(|_| mint.next_id().0).collect();
    assert_eq!(ids.len(), 1_000);
    assert_eq!(mint.op().timestamp_ms(), 1_700_000_000_000);
    let other = IdMint::new(DeviceId::from_parts(1, 2), 42).page();
    let same = IdMint::new(device, 42).page();
    assert_ne!(other, same, "streams differ per device");
}

#[test]
fn absurd_clocks_are_clamped() {
    let mut mint = IdMint::new(DeviceId::from_parts(1, 1), 0);
    mint.set_now(u64::MAX);
    assert_eq!(mint.op().timestamp_ms(), MAX_ID_TIMESTAMP_MS);
}
