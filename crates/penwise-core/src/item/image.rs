//! Picture items: a placed raster asset, addressed by content hash.

/// A placed picture. Local frame: box `[0, w] × [0, h]` with its top-left at the local origin (like text), so the
/// item transform's translation is the picture's page position. Core never holds pixels: `asset` is the SHA-256 of
/// the encoded image, resolved by the host (server asset store, tablet cache). Immutable after `Add`; only
/// `layer`, `z` and `transform` change.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageItem {
    /// SHA-256 of the encoded image bytes.
    pub asset: [u8; 32],
    /// Box width and height in local px, each in `(0, 2²⁰]`.
    pub w: f32,
    pub h: f32,
}
