//! Randomness for ids (`/dev/urandom`, as `auth.rs` does for the token).

use std::io::Read;

/// 128 random bits. Falls back to a time/pid mix only if `/dev/urandom` is unreadable.
#[must_use]
pub fn random_u128() -> u128 {
    let mut bytes = [0_u8; 16];
    let filled = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok();
    if filled {
        return u128::from_le_bytes(bytes);
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    nanos ^ (u128::from(std::process::id()) << 64).rotate_left(23)
}
