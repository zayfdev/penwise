//! 16-byte identities. Every id is a ULID in canonical big-endian byte layout (48-bit Unix ms timestamp, then
//! 80 random bits), so the derived `Ord` (lexicographic bytes) is creation-time order.
//!
//! Proto never generates ids itself (no clock, no RNG): edges call [`OpId::from_parts`] (etc.) with their own
//! wall clock and 80 random bits (server: `getrandom`; browser: `crypto.getRandomValues`; Android:
//! `SecureRandom`). Text form (JSON, URLs, logs, the JS viewer) is the 26-char Crockford base32 ULID string.
//! Wire/storage form is the raw 16 bytes (postcard writes `[u8; 16]` as 16 fixed bytes).

use core::fmt;
use core::str::FromStr;

use penwise_core::item::ItemId;
use serde::{Deserialize, Serialize};

/// Length of the text form.
pub const ID_TEXT_LEN: usize = 26;
/// Largest ULID timestamp (48 bits).
pub const MAX_ID_TIMESTAMP_MS: u64 = (1 << 48) - 1;
/// Mask of the random part (80 bits).
pub const ID_RANDOM_MASK: u128 = (1 << 80) - 1;

/// A string that is not a 26-char Crockford base32 ULID (case-insensitive; `I`/`L` → 1, `O` → 0 accepted;
/// first char ≤ `7` so the value fits 128 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid id text")]
pub struct IdParseError;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn encode_text(bytes: [u8; 16]) -> String {
    let value = u128::from_be_bytes(bytes);
    (0..ID_TEXT_LEN)
        .map(|i| {
            let shift = 5 * (ID_TEXT_LEN - 1 - i);
            let digit = usize::try_from((value >> shift) & 31).unwrap_or(0);
            char::from(ALPHABET.get(digit).copied().unwrap_or(b'0'))
        })
        .collect()
}

fn decode_digit(c: u8) -> Option<u8> {
    let c = match c.to_ascii_uppercase() {
        b'O' => b'0',
        b'I' | b'L' => b'1',
        other => other,
    };
    ALPHABET
        .iter()
        .position(|&a| a == c)
        .and_then(|p| u8::try_from(p).ok())
}

fn decode_text(text: &str) -> Result<[u8; 16], IdParseError> {
    let bytes = text.as_bytes();
    if bytes.len() != ID_TEXT_LEN {
        return Err(IdParseError);
    }
    let mut value: u128 = 0;
    for (i, &c) in bytes.iter().enumerate() {
        let digit = decode_digit(c).ok_or(IdParseError)?;
        if i == 0 && digit > 7 {
            return Err(IdParseError);
        }
        value = (value << 5) | u128::from(digit);
    }
    Ok(value.to_be_bytes())
}

macro_rules! ulid_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub [u8; 16]);

        impl $name {
            /// ULID from a Unix-ms timestamp (`> MAX_ID_TIMESTAMP_MS` saturates) and random bits (only the
            /// low 80 bits of `random` are used). Pure; callers inject both.
            #[must_use]
            pub fn from_parts(timestamp_ms: u64, random: u128) -> Self {
                let ts = u128::from(timestamp_ms.min(MAX_ID_TIMESTAMP_MS));
                Self(((ts << 80) | (random & ID_RANDOM_MASK)).to_be_bytes())
            }

            /// The 48-bit timestamp part, Unix ms.
            #[must_use]
            pub fn timestamp_ms(self) -> u64 {
                // 128 − 80 = 48 bits, always fits.
                u64::try_from(u128::from_be_bytes(self.0) >> 80).unwrap_or(MAX_ID_TIMESTAMP_MS)
            }

            /// Raw bytes (wire and `SQLite` `BLOB` form).
            #[must_use]
            pub fn to_bytes(self) -> [u8; 16] {
                self.0
            }

            /// From a 16-byte slice (`SQLite` `BLOB` column); `None` for any other length.
            #[must_use]
            pub fn from_slice(bytes: &[u8]) -> Option<Self> {
                <[u8; 16]>::try_from(bytes).ok().map(Self)
            }
        }

        /// 26-char uppercase Crockford base32 ULID.
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&encode_text(self.0))
            }
        }

        /// Same as `Display`, prefixed with the type name (ids are not secret).
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), encode_text(self.0))
            }
        }

        /// Parses the text form (see [`IdParseError`]).
        impl FromStr for $name {
            type Err = IdParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                decode_text(s).map(Self)
            }
        }
    };
}

ulid_id!(
    /// A paired client (tablet, browser, agent). Chosen by the client at pairing time and bound to its token;
    /// also the LWW tie-breaker inside [`crate::Hlc`].
    DeviceId
);
ulid_id!(
    /// One op; the idempotency key of the server log (`ops.op_id UNIQUE`): re-pushing an op is a no-op that
    /// returns the original seq.
    OpId
);
ulid_id!(
    /// A notebook.
    NotebookId
);
ulid_id!(
    /// A page.
    PageId
);
ulid_id!(
    /// One server data directory (random, created on first start, stored in `server_meta`). Shown in the
    /// pairing QR (`sid`) so a tablet can tell two servers apart and refuse a swapped endpoint.
    ServerId
);
ulid_id!(
    /// Wire form of a core [`ItemId`] (core types carry no serde). Same bytes.
    ItemRef
);

impl From<ItemId> for ItemRef {
    fn from(id: ItemId) -> Self {
        Self(id.0)
    }
}

impl From<ItemRef> for ItemId {
    fn from(id: ItemRef) -> Self {
        Self(id.0)
    }
}
