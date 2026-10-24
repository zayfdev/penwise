//! Pairing QR payload and SVG (server-stack.md §6).
//!
//! Owner: slice c.
//!
//! URI (≤ 160 bytes so the QR stays a low version, readable from the tablet camera):
//! ```text
//! penwise://pair?v=1&sid=<ServerId, 26>&code=<PairingCode, 22>&port=<u16>[&lan=<IPv4>][&ts=<MagicDNS name>]
//! ```
//! - `v` = `PROTOCOL_VERSION`; parameters in exactly this order; values percent-encoding-free by construction
//!   (base32/base64url/digits/dots/hyphens); at least one of `lan`, `ts` present.
//! - `ts` is the MagicDNS name without the trailing dot (`deskpc.tail000000.ts.net`).
//! - The tablet tries `ws://<lan>:<port>/sync` first, then `ws://<ts>:<port>/sync`, and pins `sid`.
//! - No `fp`: there is no TLS/Noise key in M3 (see contract "Security notes"); a later version adds `fp`
//!   as a new trailing parameter.

use std::net::Ipv4Addr;

use penwise_proto::{PairingCode, ServerId};

/// QR module size floor in px (`min_dimensions`).
pub const QR_MIN_PX: u32 = 320;
/// URI scheme + path prefix.
pub const PAIR_URI_PREFIX: &str = "penwise://pair?";

/// Everything the tablet needs to pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingPayload {
    pub version: u16,
    pub server: ServerId,
    pub code: PairingCode,
    pub port: u16,
    pub lan: Option<Ipv4Addr>,
    pub tailscale: Option<String>,
}

/// Bad payload or QR rendering failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QrError {
    /// Not a `penwise://pair?` URI in the module-doc grammar.
    #[error("malformed pairing uri")]
    Malformed,
    /// Neither `lan` nor `ts`.
    #[error("no endpoint")]
    NoEndpoint,
    /// The `qrcode` crate refused the data (too long).
    #[error("qr encode failed")]
    Encode,
}

impl PairingPayload {
    /// The module-doc URI.
    ///
    /// # Errors
    /// [`QrError::NoEndpoint`].
    pub fn to_uri(&self) -> Result<String, QrError> {
        todo!()
    }

    /// Strict inverse of [`PairingPayload::to_uri`] (tests; the tablet has its own Kotlin parser).
    ///
    /// # Errors
    /// [`QrError::Malformed`], [`QrError::NoEndpoint`].
    pub fn parse(uri: &str) -> Result<Self, QrError> {
        todo!()
    }
}

/// QR (ECC level M, quiet zone, `QR_MIN_PX`) of `data` as a standalone SVG document.
///
/// # Errors
/// [`QrError::Encode`].
pub fn render_svg(data: &str) -> Result<String, QrError> {
    todo!()
}
