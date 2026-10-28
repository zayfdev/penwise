//! HTML injection for project responses: put the bridge `<script>` into `text/html` while leaving every other
//! content type byte-identical. Dev servers may compress HTML; a `gzip`/`deflate` body is decoded first and the
//! response is re-encoded as identity (the forwarded request asks for identity, so this is the exception).

use std::io::Read as _;

/// The tag inserted into HTML. It is absolute so it works from any route of the project.
pub const SCRIPT_TAG: &str = "<script src=\"/__penwise/bridge.js\"></script>";

/// `true` for the media types that get the bridge. Parameters (`; charset=…`) are ignored.
#[must_use]
pub fn is_html(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|value| {
        let media = value.split(';').next().unwrap_or_default().trim();
        media.eq_ignore_ascii_case("text/html")
            || media.eq_ignore_ascii_case("application/xhtml+xml")
    })
}

/// Decodes `body` according to a `Content-Encoding` header value.
///
/// `None` means "cannot decode" (brotli, zstd, a corrupt stream): the caller must then pass the response
/// through untouched instead of injecting into compressed bytes.
#[must_use]
pub fn decode(encoding: Option<&str>, body: &[u8]) -> Option<Vec<u8>> {
    let Some(header) = encoding else {
        return Some(body.to_vec());
    };
    // A comma-separated chain is decoded outermost-first; only the single encodings we know are handled.
    let mut encodings: Vec<&str> = header
        .split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .collect();
    encodings.reverse();
    let mut current = body.to_vec();
    for encoding in encodings {
        current = match encoding.to_ascii_lowercase().as_str() {
            "identity" => current,
            "gzip" | "x-gzip" => gunzip(&current)?,
            "deflate" => inflate(&current)?,
            _ => return None,
        };
    }
    Some(current)
}

fn gunzip(body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(body)
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

/// `deflate` is zlib-wrapped in the spec and raw in the wild; both are accepted.
fn inflate(body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    if flate2::read::ZlibDecoder::new(body)
        .read_to_end(&mut out)
        .is_ok()
    {
        return Some(out);
    }
    out.clear();
    flate2::read::DeflateDecoder::new(body)
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

/// Inserts [`SCRIPT_TAG`] before `</head>`, else after `<body …>`, else at the very start.
///
/// A document that already carries the tag is returned unchanged, so proxying a page twice cannot double it.
#[must_use]
pub fn inject(body: &[u8]) -> Vec<u8> {
    if find_ci(body, SCRIPT_TAG.as_bytes(), 0).is_some() {
        return body.to_vec();
    }
    let at = find_ci(body, b"</head", 0).or_else(|| {
        find_ci(body, b"<body", 0).and_then(|body_at| {
            body[body_at..]
                .iter()
                .position(|byte| *byte == b'>')
                .map(|offset| body_at + offset + 1)
        })
    });
    let mut out = Vec::with_capacity(body.len() + SCRIPT_TAG.len());
    if let Some(index) = at {
        out.extend_from_slice(&body[..index]);
        out.extend_from_slice(SCRIPT_TAG.as_bytes());
        out.extend_from_slice(&body[index..]);
    } else {
        out.extend_from_slice(SCRIPT_TAG.as_bytes());
        out.extend_from_slice(body);
    }
    out
}

/// ASCII case-insensitive `needle` search from `from`.
fn find_ci(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    let last = haystack.len() - needle.len();
    (from..=last).find(|&start| {
        haystack[start..start + needle.len()]
            .iter()
            .zip(needle)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}
