//! HTML sanitizing for patch fragments (P1): strips `<script>`, `<iframe>`, `<object>`, `<embed>`, `on*`
//! attributes and `javascript:` URLs, and keeps the rest verbatim.
//!
//! The bridge sanitizes again with the page's own parser; this pass is what makes the stored patch safe even
//! before a page sees it.

/// Tags removed outright (their content too, for `script`).
const FORBIDDEN: [&str; 4] = ["script", "iframe", "object", "embed"];

/// Attributes that run code: `on*`.
pub(super) fn forbidden_name(name: &str) -> bool {
    name.trim().to_ascii_lowercase().starts_with("on")
}

/// A value whose scheme is `javascript:` once whitespace, control bytes and character references are resolved.
pub(super) fn dangerous_url(value: &str) -> bool {
    let normalized: String = decode_references(value)
        .chars()
        .filter(|character| !character.is_ascii_whitespace() && !character.is_ascii_control())
        .collect::<String>()
        .to_ascii_lowercase();
    normalized.starts_with("javascript:")
}

/// Resolves numeric (`&#58;`, `&#x3a;`) and the named references that can hide a colon or a newline or a tab.
fn decode_references(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        let after = &rest[index..];
        let Some(semi) = after.find(';') else {
            break;
        };
        let entity = &after[1..semi];
        if let Some(character) = decode_one(entity) {
            out.push(character);
            rest = &after[semi + 1..];
        } else {
            out.push('&');
            rest = &after[1..];
        }
    }
    out.push_str(rest);
    out
}

fn decode_one(entity: &str) -> Option<char> {
    let lower = entity.to_ascii_lowercase();
    let named = match lower.as_str() {
        "colon" => return Some(':'),
        "tab" => return Some('\t'),
        "newline" => return Some('\n'),
        "nbsp" => return Some(' '),
        _ => entity,
    };
    let code = if let Some(hex) = named
        .strip_prefix("#x")
        .or_else(|| named.strip_prefix("#X"))
    {
        u32::from_str_radix(hex, 16).ok()?
    } else if let Some(decimal) = named.strip_prefix('#') {
        decimal.parse().ok()?
    } else {
        return None;
    };
    char::from_u32(code)
}

/// Strips forbidden tags and attributes from an HTML fragment, keeping the rest verbatim.
pub(crate) fn sanitize_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let Some(start) = rest.find('<') else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        if after.starts_with("<!--") {
            let Some(end) = after.find("-->") else {
                return out;
            };
            out.push_str(&after[..end + 3]);
            rest = &after[end + 3..];
            continue;
        }
        if !opens_tag(after) {
            // A literal `<` in text ("2 < 3"): not markup, so it survives as text.
            out.push('<');
            rest = &after[1..];
            continue;
        }
        let Some(end) = tag_end(after) else {
            return out;
        };
        let tag = &after[..=end];
        match rewrite_tag(tag) {
            Tag::Keep(rewritten) => {
                out.push_str(&rewritten);
                rest = &after[end + 1..];
            }
            Tag::Skip => {
                rest = &after[end + 1..];
            }
            Tag::SkipElement(name) => {
                let close = format!("</{name}");
                let body = &after[end + 1..];
                let lower = body.to_ascii_lowercase();
                rest = match lower.find(&close) {
                    Some(found) => {
                        let tail = &body[found..];
                        match tag_end(tail) {
                            Some(close_end) => &tail[close_end + 1..],
                            None => "",
                        }
                    }
                    None => "",
                };
            }
        }
    }
}

/// True when `<…` starts a real tag or declaration (`<p`, `</p`, `<!doctype`, `<?xml`).
fn opens_tag(text: &str) -> bool {
    let mut chars = text.chars().skip(1);
    match chars.next() {
        Some('/' | '!' | '?') => chars.next().is_some_and(|next| {
            next.is_ascii_alphabetic() || next == '-' && text.starts_with("<!--")
        }),
        Some(first) => first.is_ascii_alphabetic(),
        None => false,
    }
}

/// The offset of a tag's closing `>` (quotes respected), or `None` when the fragment ends inside the tag.
fn tag_end(text: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (index, character) in text.char_indices() {
        if let Some(open) = quote {
            if character == open {
                quote = None;
            }
        } else if character == '"' || character == '\'' {
            quote = Some(character);
        } else if character == '>' {
            return Some(index);
        }
    }
    None
}

enum Tag {
    Keep(String),
    /// Drop the tag alone (fallback content stays as text).
    Skip,
    /// Drop the element and everything up to its close tag.
    SkipElement(String),
}

/// One `<...>` token: forbidden tags are skipped, everything else keeps only safe attributes.
fn rewrite_tag(tag: &str) -> Tag {
    let inner = &tag[1..tag.len() - 1];
    let closing = inner.starts_with('/');
    let body = inner.trim_start_matches('/').trim_start();
    let name = tag_name(body);
    if name.is_empty() {
        // A declaration (`<!doctype`) or processing instruction: inert, kept as-is.
        return Tag::Keep(tag.to_owned());
    }
    let lower = name.to_ascii_lowercase();
    if FORBIDDEN.contains(&lower.as_str()) {
        return if closing {
            Tag::Skip
        } else if lower == "script" {
            Tag::SkipElement(lower)
        } else {
            Tag::Skip
        };
    }
    if closing {
        return Tag::Keep(format!("</{name}>"));
    }
    let self_closing = body.ends_with('/');
    let mut out = String::with_capacity(tag.len());
    out.push('<');
    out.push_str(name);
    attributes(&body[name.len()..], &mut out);
    out.push_str(if self_closing { "/>" } else { ">" });
    Tag::Keep(out)
}

fn tag_name(body: &str) -> &str {
    let end = body
        .find(|character: char| {
            !(character.is_ascii_alphanumeric()
                || character == '-'
                || character == ':'
                || character == '_')
        })
        .unwrap_or(body.len());
    &body[..end]
}

/// Appends the safe attributes of a tag body (`name attrs`), dropping `on*` and dangerous values.
fn attributes(mut body: &str, out: &mut String) {
    loop {
        body = body.trim_start();
        if body.is_empty() || body.starts_with('/') || body.starts_with('>') {
            return;
        }
        let name_end = body
            .find(|character: char| {
                !(character.is_ascii_alphanumeric()
                    || character == '-'
                    || character == '_'
                    || character == ':')
            })
            .unwrap_or(body.len());
        let name = &body[..name_end];
        if name.is_empty() {
            // A stray byte (a quote, an `=` with no name): consume it and carry on.
            body = &body[1..];
            continue;
        }
        body = &body[name_end..];
        let mut value: Option<String> = None;
        let trimmed = body.trim_start();
        if let Some(after_eq) = trimmed.strip_prefix('=') {
            let after_eq = after_eq.trim_start();
            let mut chars = after_eq.chars();
            match chars.next() {
                Some(quote @ ('"' | '\'')) => {
                    let rest = &after_eq[1..];
                    let end = rest.find(quote).unwrap_or(rest.len());
                    value = Some(rest[..end].to_owned());
                    body = &rest[end + 1..];
                }
                Some(_) => {
                    let end = after_eq
                        .find(|character: char| character.is_whitespace() || character == '>')
                        .unwrap_or(after_eq.len());
                    value = Some(after_eq[..end].to_owned());
                    body = &after_eq[end..];
                }
                None => body = after_eq,
            }
        }
        if name.is_empty() || forbidden_name(name) {
            continue;
        }
        if value.as_deref().is_some_and(dangerous_url) {
            continue;
        }
        out.push(' ');
        out.push_str(name);
        if let Some(value) = value {
            out.push_str("=\"");
            out.push_str(&value.replace('"', "&quot;"));
            out.push('"');
        }
    }
}
