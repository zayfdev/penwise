//! Content-addressed pictures: `assets/<sha256>.png` under the data directory.
//!
//! One file per distinct picture, named by its digest, so re-placing the same screenshot costs nothing and a
//! page object only carries an id. PNG is accepted, not re-encoded: the pixels an agent extracts are exactly the
//! pixels that were captured.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

/// Largest accepted upload, in bytes.
pub(crate) const MAX_BYTES: usize = 24 * 1024 * 1024;
/// Largest accepted side, in pixels.
const MAX_SIDE: u32 = 10_000;
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// A picture that is now on disk.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StoredAsset {
    /// Digest: also the URL segment and the page object's `asset`.
    pub asset: String,
    pub width: u32,
    pub height: u32,
}

/// The picture store.
#[derive(Debug)]
pub struct Assets {
    dir: PathBuf,
}

impl Assets {
    /// Opens the store, creating `assets/` if needed.
    pub fn load(dir: PathBuf) -> Arc<Self> {
        if let Err(error) = std::fs::create_dir_all(&dir) {
            tracing::error!(dir = %dir.display(), %error, "cannot create the asset directory");
        }
        Arc::new(Self { dir })
    }

    /// Validates and stores a PNG, returning its digest and pixel size.
    pub fn put(&self, bytes: &[u8]) -> Result<StoredAsset, String> {
        if bytes.len() > MAX_BYTES {
            return Err(format!("picture is larger than {MAX_BYTES} bytes"));
        }
        let (width, height) = png_size(bytes)?;
        if width > MAX_SIDE || height > MAX_SIDE {
            return Err(format!("picture is larger than {MAX_SIDE} px a side"));
        }
        let digest = Sha256::digest(bytes);
        let asset = hex(&digest);
        let path = self.dir.join(format!("{asset}.png"));
        if !path.is_file() {
            let staging = self.dir.join(format!(".{asset}.tmp"));
            std::fs::write(&staging, bytes).map_err(|error| error.to_string())?;
            std::fs::rename(&staging, &path).map_err(|error| error.to_string())?;
        }
        Ok(StoredAsset {
            asset,
            width,
            height,
        })
    }

    /// The stored bytes, when the id names a file that exists.
    pub fn bytes(&self, id: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(id)?).ok()
    }

    /// Filesystem path of an asset; `None` for ids that are not digests or do not exist.
    pub fn path(&self, id: &str) -> Option<PathBuf> {
        if id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let path = self.dir.join(format!("{id}.png"));
        path.is_file().then_some(path)
    }

    /// The directory pictures live in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// Width and height from a PNG's `IHDR`, rejecting anything that is not a PNG.
fn png_size(bytes: &[u8]) -> Result<(u32, u32), String> {
    if bytes.len() < 24 || bytes[..8] != PNG_SIGNATURE {
        return Err("not a PNG".to_owned());
    }
    if &bytes[12..16] != b"IHDR" {
        return Err("PNG has no IHDR".to_owned());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().map_err(|_| "bad width")?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().map_err(|_| "bad height")?);
    if width == 0 || height == 0 {
        return Err("PNG has no pixels".to_owned());
    }
    Ok((width, height))
}

/// The 32-byte digest a 64-hex-digit asset id names; `None` for anything else.
pub(crate) fn digest_of(id: &str) -> Option<[u8; 32]> {
    if id.len() != 64 || !id.is_ascii() {
        return None;
    }
    let mut out = [0_u8; 32];
    for (byte, pair) in out.iter_mut().zip(id.as_bytes().chunks_exact(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

/// The asset id (lowercase hex) of a digest.
pub(crate) fn id_of(digest: &[u8; 32]) -> String {
    hex(digest)
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests;
