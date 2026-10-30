//! What one still is, and its JSON (`GET /frames/<id>/still`, contract v2 §4).
//!
//! A [`Still`] is metadata only: strips are named by the SHA-256 of their PNG ([`StripMeta::hash`]) and live in
//! the blob store, so the metadata of every frame stays in memory however many PNG bytes had to be evicted.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The note a still carries when the server switched an infinite feed to `viewport` mode.
pub(crate) const INFINITE_NOTE: &str = "infinite feed: viewport mode";

/// One strip of a still: where it sits and which PNG it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StripMeta {
    /// Document y of the strip's top edge.
    pub(crate) y: u32,
    /// The strip's height in CSS px.
    pub(crate) h: u32,
    /// Content address of the PNG: 32 hex characters (the first half of its SHA-256).
    pub(crate) hash: String,
}

/// The content address of `png`.
pub(crate) fn hash_of(png: &[u8]) -> String {
    let digest = Sha256::digest(png);
    let mut out = String::with_capacity(32);
    for byte in &digest[..16] {
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"));
    }
    out
}

/// Whether `text` can be a strip hash (it becomes a file name, so nothing else is accepted).
pub(crate) fn is_hash(text: &str) -> bool {
    text.len() == 32
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// One visible element of the captured page, for answering a pen-down without a browser.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct Hit {
    /// The bridge's stable element key.
    pub(crate) key: String,
    pub(crate) selector: String,
    /// `[x, y, w, h]` in **document** coordinates.
    pub(crate) rect: [f32; 4],
}

impl Hit {
    fn contains(&self, point: [f32; 2]) -> bool {
        point[0] >= self.rect[0]
            && point[0] <= self.rect[0] + self.rect[2]
            && point[1] >= self.rect[1]
            && point[1] <= self.rect[1] + self.rect[3]
    }
}

/// One captured version of a frame: the document in strips, the layout of the frame's bound elements and the
/// hit map.
#[derive(Debug)]
pub(crate) struct Still {
    pub(crate) version: u64,
    /// The linked project the frame shows, so a change to it finds its frames.
    pub(crate) project: String,
    /// Document size in CSS px (height capped at the pre-roll cap).
    pub(crate) doc: [u32; 2],
    /// The preset size the page was laid out at.
    pub(crate) viewport: [u32; 2],
    pub(crate) strips: Vec<StripMeta>,
    /// Watched key → rect `[x, y, w, h]` in **document** coordinates, `None` when the element is gone.
    pub(crate) rects: BTreeMap<String, Option<[f32; 4]>>,
    /// The state recipe the page was in (always the page as served: empty).
    pub(crate) state: String,
    pub(crate) note: Option<&'static str>,
    /// Every visible element, topmost first.
    pub(crate) hits: Arc<Vec<Hit>>,
}

impl Still {
    /// The `ETag` value: the version, quoted.
    pub(crate) fn etag(&self) -> String {
        format!("\"{}\"", self.version)
    }

    /// Whether `other` shows exactly the same thing (a re-still of an unchanged page keeps its version, so
    /// polling clients see a `304`).
    pub(crate) fn same_content(&self, other: &Self) -> bool {
        self.doc == other.doc
            && self.viewport == other.viewport
            && self.rects == other.rects
            && self.state == other.state
            && self.note == other.note
            && self.strips == other.strips
            && self.hits == other.hits
    }

    /// The topmost element at document point `point`, as the bridge's `pw:hit` payload with a document rect.
    pub(crate) fn hit_at(&self, point: [f32; 2]) -> Value {
        match self.hits.iter().find(|hit| hit.contains(point)) {
            Some(hit) => json!({ "key": hit.key, "selector": hit.selector, "rect": hit.rect }),
            None => json!({ "key": null, "selector": null, "rect": null }),
        }
    }

    /// Whether the still names this strip hash.
    pub(crate) fn has_strip(&self, hash: &str) -> bool {
        self.strips.iter().any(|strip| strip.hash == hash)
    }

    /// The contract JSON. `frame` is the frame id in hex, for the strip URLs.
    pub(crate) fn json(&self, frame: &str) -> Value {
        let strips: Vec<Value> = self
            .strips
            .iter()
            .map(|strip| {
                json!({
                    "y": strip.y,
                    "h": strip.h,
                    "url": format!("/frames/{frame}/strip/{}.png", strip.hash),
                })
            })
            .collect();
        let rects: serde_json::Map<String, Value> = self
            .rects
            .iter()
            .map(|(key, rect)| (key.clone(), rect.map_or(Value::Null, |rect| json!(rect))))
            .collect();
        let mut out = json!({
            "version": self.version,
            "doc": self.doc,
            "viewport": self.viewport,
            "strips": strips,
            "layout": { "rects": rects, "state": self.state },
        });
        if let (Some(note), Some(object)) = (self.note, out.as_object_mut()) {
            object.insert("note".to_owned(), json!(note));
        }
        out
    }
}

/// Whether an `If-None-Match` header value names `etag` (a list, weak validators and `*` included).
pub(crate) fn etag_matches(header: &str, etag: &str) -> bool {
    header.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
    })
}
