//! Text edits for the adapter install: the framework config and `package.json`.
//!
//! These functions are layout-preserving string edits, not parsers. Anything they cannot place with certainty
//! returns `None`, and `penwise link` prints manual instructions instead of writing a guess.

use std::fmt::Write as _;

/// `vite.config.*` with the adapter imported and `penwise()` first in `plugins`.
///
/// Recognised shapes: an existing `plugins: [...]` array, or an object literal after
/// `export default defineConfig({` / `export default {`, where a `plugins` key is added.
#[must_use]
pub fn vite_edit(text: &str, import: &str) -> Option<String> {
    let text = format!("{import}\n{text}");
    if let Some(open) = plugins_array(&text) {
        return add_to_plugins(&text, open);
    }
    object_open(&text).map(|open| {
        let indent = next_line_indent(&text, open)
            .unwrap_or_else(|| format!("{}  ", line_indent(&text, open)));
        let mut out = text;
        out.insert_str(open + 1, &format!("\n{indent}plugins: [penwise()],"));
        out
    })
}

/// `next.config.*` with the adapter imported and the exported config wrapped in `withPenwise(...)`.
///
/// `module.exports = nextConfig`, `export default nextConfig`, object literals and multi-line calls all work;
/// anything else is left to the manual instructions.
#[must_use]
pub fn next_edit(text: &str, import: &str) -> Option<String> {
    let text = format!("{import}\n{text}");
    let rhs_start = find_export(&text)?;
    let end = statement_end(&text, rhs_start);
    let value = text[rhs_start..end].trim_end();
    if value.is_empty() || value.starts_with("withPenwise(") {
        return None;
    }
    let mut out = String::with_capacity(text.len() + 12);
    out.push_str(&text[..rhs_start]);
    write!(out, "withPenwise({value})").ok()?;
    out.push_str(&text[end..]);
    Some(out)
}

/// Where the exported config's value starts (after `module.exports =` or `export default`).
fn find_export(text: &str) -> Option<usize> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let start = offset + (line.len() - line.trim_start().len());
        offset += line.len();
        let (keyword, common_js) = if line.trim_start().starts_with("module.exports") {
            ("module.exports", true)
        } else if line.trim_start().starts_with("export default") {
            ("export default", false)
        } else {
            continue;
        };
        let mut at = start + keyword.len();
        at = skip_spaces(text, at);
        if common_js {
            if !text[at..].starts_with('=') {
                continue;
            }
            at = skip_spaces(text, at + 1);
        }
        return Some(at);
    }
    None
}

/// The first non-space, non-tab byte at or after `at`.
fn skip_spaces(text: &str, mut at: usize) -> usize {
    while text[at..].starts_with([' ', '\t']) {
        at += 1;
    }
    at
}

/// The byte after the statement's value: the `;` (exclusive) or the end of the line, at bracket depth 0.
fn statement_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0u32;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (index, &byte) in bytes.iter().enumerate().skip(start) {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == open {
                quote = None;
            }
            continue;
        }
        match byte {
            b'"' | b'\'' | b'`' => quote = Some(byte),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 {
                    return index;
                }
                depth -= 1;
            }
            b';' | b'\n' => {
                if depth == 0 {
                    return index;
                }
            }
            _ => {}
        }
    }
    bytes.len()
}

/// The index of the `[` after a `plugins:` key, when the config has one.
fn plugins_array(text: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(offset) = text[from..].find("plugins") {
        let mut at = from + offset + "plugins".len();
        while text[at..].starts_with([' ', '\t']) {
            at += 1;
        }
        if text[at..].starts_with(':') {
            at += 1;
            while text[at..].starts_with([' ', '\t', '\n']) {
                at += 1;
            }
            if text[at..].starts_with('[') {
                return Some(at);
            }
        }
        from = at;
    }
    None
}

/// Puts `penwise()` first in the array opened at `open`, keeping the file's line layout.
fn add_to_plugins(text: &str, open: usize) -> Option<String> {
    let inner = &text[open + 1..];
    let content = inner.trim_start();
    let mut out = text.to_owned();
    if content.starts_with(']') {
        out.insert_str(open + 1, "penwise()");
    } else if inner[..inner.len() - content.len()].contains('\n') {
        let indent = next_line_indent(text, open)?;
        out.insert_str(open + 1, &format!("\n{indent}penwise(),"));
    } else {
        out.insert_str(open + 1, "penwise(), ");
    }
    Some(out)
}

/// The `{` of the config object: the first one after `defineConfig(` or `export default`.
fn object_open(text: &str) -> Option<usize> {
    for needle in ["defineConfig({", "export default {"] {
        if let Some(at) = text.find(needle) {
            return Some(at + needle.len() - 1);
        }
    }
    None
}

/// The indentation of the first non-empty line after `index`.
fn next_line_indent(text: &str, index: usize) -> Option<String> {
    text[index..]
        .lines()
        .skip(1)
        .find(|line| !line.trim().is_empty())
        .map(|line| line[..line.len() - line.trim_start().len()].to_owned())
}

/// The indentation of the line containing `index`.
fn line_indent(text: &str, index: usize) -> String {
    let start = text[..index].rfind('\n').map_or(0, |newline| newline + 1);
    let line = &text[start..index];
    line[..line.len() - line.trim_start().len()].to_owned()
}

/// `text` with `"<name>": "<value>"` as the first member of `devDependencies` (created when absent).
///
/// The edit is textual so the owner's key order and formatting survive; a result that no longer parses is
/// discarded.
#[must_use]
pub fn dev_dependency(text: &str, name: &str, value: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(text).ok()?;
    let entry = format!("\"{name}\": \"{value}\"");
    let out = if let Some(key) = find_key(text, "devDependencies") {
        let open = key + text[key..].find('{')?;
        insert_member(text, open, &entry)
    } else {
        let open = text.find('{')?;
        let body = next_line_indent(text, open).unwrap_or_else(|| line_indent(text, open));
        let block = format!("\"devDependencies\": {{\n{body}  {entry}\n{body}}}");
        insert_member(text, open, &block)
    };
    serde_json::from_str::<serde_json::Value>(&out)
        .ok()
        .map(|_| out)
}

/// The offset of a `"<key>":` member.
fn find_key(text: &str, key: &str) -> Option<usize> {
    let needle = format!("\"{key}\"");
    let mut from = 0;
    while let Some(offset) = text[from..].find(&needle) {
        let at = from + offset;
        if text[at + needle.len()..].trim_start().starts_with(':') {
            return Some(at);
        }
        from = at + needle.len();
    }
    None
}

/// Inserts `entry` as the first member of the object opened at `open`.
fn insert_member(text: &str, open: usize, entry: &str) -> String {
    let indent = line_indent(text, open);
    let inner = &text[open + 1..];
    let content = inner.trim_start();
    let multiline = inner[..inner.len() - content.len()].contains('\n');
    let mut out = String::with_capacity(text.len() + entry.len() + 8);
    out.push_str(&text[..=open]);
    if content.is_empty() || content.starts_with('}') {
        write!(out, "\n{indent}  {entry}\n{indent}").ok();
    } else if multiline {
        let body = next_line_indent(text, open).unwrap_or_else(|| format!("{indent}  "));
        write!(out, "\n{body}{entry},").ok();
    } else {
        write!(out, " {entry},").ok();
    }
    out.push_str(&text[open + 1..]);
    out
}
