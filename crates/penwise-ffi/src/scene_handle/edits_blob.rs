//! Edit lists as bytes: the form `SceneHandle` tools return and `SyncCore::commit_edits` accepts.
//!
//! Layout: `u32 n`, then `n × (u32 len, PWED v1 blob)` (`penwise_core::codec::edit`).

use penwise_core::codec::edit::{decode_edit, encode_edit};
use penwise_core::edit::Edit;

use crate::bytes::{Reader, Writer};
use crate::error::SyncError;

/// Serializes `edits`.
#[must_use]
pub fn encode_edits(edits: &[Edit]) -> Vec<u8> {
    let mut w = Writer::new();
    w.count(edits.len());
    let mut blob = Vec::new();
    for edit in edits {
        blob.clear();
        encode_edit(edit, &mut blob);
        w.blob(&blob);
    }
    w.finish()
}

/// Parses [`encode_edits`] output.
///
/// # Errors
/// A truncated list, trailing bytes or an edit blob failing its codec.
pub fn decode_edits(bytes: &[u8]) -> Result<Vec<Edit>, SyncError> {
    let mut r = Reader::new(bytes);
    let n = r.u32()?;
    let mut edits = Vec::new();
    for _ in 0..n {
        edits.push(decode_edit(r.blob()?)?);
    }
    if !r.is_empty() {
        return Err(SyncError::BadArgument("trailing bytes after edit list"));
    }
    Ok(edits)
}
