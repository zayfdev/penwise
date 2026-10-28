//! DOM path → `file:line` for the served HTML, without a build plugin.
//!
//! The scanner is deliberately small: it understands comments, doctype, raw-text elements, attributes with `>`
//! in quotes, void elements, self-closing foreign elements and the optional end tags of `li`/`dt`/`dd`/`p`/
//! `tr`/`td`. That covers hand-written static sites; a framework adapter adds `data-pw-src` instead.

use std::collections::HashMap;

/// Elements the HTML parser never gives children.
const VOID_ELEMENTS: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
/// Elements whose content is text, not markup, until the matching end tag.
const RAW_TEXT: [&str; 4] = ["script", "style", "textarea", "title"];
/// Start tags that close an open `p` (the common ones from the HTML parsing spec).
const P_CLOSERS: [&str; 30] = [
    "address",
    "article",
    "aside",
    "blockquote",
    "details",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "main",
    "menu",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "ul",
];

/// The 1-based line of the start tag at `dom_path` in `html`.
pub(super) fn html_line(html: &str, dom_path: &str) -> Option<u32> {
    Scanner::new(html).run(dom_path)
}

/// Where the next start tag sits, and whether it closes itself.
struct Tag {
    name: String,
    end: usize,
    self_closing: bool,
}

/// One open element: its name, its `:nth-of-type` index, and how many children of each tag it has seen.
struct Frame {
    tag: String,
    index: usize,
    counts: HashMap<String, usize>,
}

struct Scanner<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    stack: Vec<Frame>,
}

impl<'a> Scanner<'a> {
    fn new(src: &'a str) -> Self {
        let mut stack = Vec::new();
        // Pages normally write `<html>`. When they do not, the browser still creates one; mirror that so the
        // paths match. A missing explicit `<body>`/`<head>` is not synthesised (no match, no wrong line).
        if !src.to_ascii_lowercase().contains("<html") {
            stack.push(Frame {
                tag: String::from("html"),
                index: 1,
                counts: HashMap::new(),
            });
        }
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            line: 1,
            stack,
        }
    }

    fn run(mut self, target: &str) -> Option<u32> {
        let target = target.to_ascii_lowercase();
        loop {
            let offset = self.src[self.pos..].find('<')?;
            let at = self.pos + offset;
            self.advance_to(at);
            if self.bytes[at..].starts_with(b"<!--") {
                self.skip_past("-->", at + 4);
                continue;
            }
            if self.bytes[at..].starts_with(b"<!") || self.bytes[at..].starts_with(b"<?") {
                self.skip_to_gt(at);
                continue;
            }
            if self.bytes[at..].starts_with(b"</") {
                if let Some((name, end)) = end_tag(self.bytes, at) {
                    self.advance_to(end);
                    self.close(&name);
                } else {
                    self.advance_to(at + 2);
                }
                continue;
            }
            let Some(tag) = start_tag(self.bytes, at) else {
                self.advance_to(at + 1);
                continue;
            };
            let line = self.line;
            self.advance_to(tag.end);
            if let Some(found) = self.open(&tag, line, &target) {
                return Some(found);
            }
        }
    }

    fn open(&mut self, tag: &Tag, line: u32, target: &str) -> Option<u32> {
        let name = tag.name.as_str();
        self.implicitly_close(name);
        if name == "tr" && self.top_is("table") {
            // The browser inserts a `tbody` for rows that are direct children of a table.
            if let Some(found) = self.push("tbody", line, target) {
                return Some(found);
            }
        }
        let found = self.push(name, line, target);
        if RAW_TEXT.contains(&name) {
            if let Some(end) = find_raw_end(self.src, name, self.pos) {
                self.advance_to(end);
            } else {
                self.advance_to(self.src.len());
            }
        } else if VOID_ELEMENTS.contains(&name) || (tag.self_closing && self.in_foreign()) {
            self.stack.pop();
        }
        found
    }

    /// Pushes `tag`, returning the line when the resulting path is the target.
    fn push(&mut self, tag: &str, line: u32, target: &str) -> Option<u32> {
        let index = if let Some(parent) = self.stack.last_mut() {
            let count = parent.counts.entry(tag.to_owned()).or_insert(0);
            *count += 1;
            *count
        } else {
            1
        };
        self.stack.push(Frame {
            tag: tag.to_owned(),
            index,
            counts: HashMap::new(),
        });
        self.path_matches(target).then_some(line)
    }

    fn path_matches(&self, target: &str) -> bool {
        let mut path = String::new();
        for frame in &self.stack {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(&frame.tag);
            path.push('[');
            path.push_str(&frame.index.to_string());
            path.push(']');
        }
        path.eq_ignore_ascii_case(target)
    }

    fn implicitly_close(&mut self, name: &str) {
        match name {
            "li" => {
                if self.top_is("li") {
                    self.stack.pop();
                }
            }
            "dt" | "dd" => {
                while self
                    .stack
                    .last()
                    .is_some_and(|frame| matches!(frame.tag.as_str(), "dt" | "dd"))
                {
                    self.stack.pop();
                }
            }
            "option" => {
                if self.top_is("option") {
                    self.stack.pop();
                }
            }
            "tr" => {
                while self
                    .stack
                    .last()
                    .is_some_and(|frame| matches!(frame.tag.as_str(), "td" | "th" | "tr"))
                {
                    self.stack.pop();
                }
            }
            "td" | "th" => {
                while self
                    .stack
                    .last()
                    .is_some_and(|frame| matches!(frame.tag.as_str(), "td" | "th"))
                {
                    self.stack.pop();
                }
            }
            "thead" | "tbody" | "tfoot" => {
                if self
                    .stack
                    .last()
                    .is_some_and(|frame| matches!(frame.tag.as_str(), "thead" | "tbody" | "tfoot"))
                {
                    self.stack.pop();
                }
            }
            _ => {}
        }
        if P_CLOSERS.contains(&name)
            && let Some(pos) = self.stack.iter().rposition(|frame| frame.tag == "p")
        {
            self.stack.truncate(pos);
        }
    }

    fn close(&mut self, name: &str) {
        if let Some(pos) = self.stack.iter().rposition(|frame| frame.tag == name) {
            self.stack.truncate(pos);
        }
    }

    fn top_is(&self, tag: &str) -> bool {
        self.stack.last().is_some_and(|frame| frame.tag == tag)
    }

    fn in_foreign(&self) -> bool {
        self.stack
            .iter()
            .any(|frame| frame.tag == "svg" || frame.tag == "math")
    }

    #[allow(
        clippy::naive_bytecount,
        reason = "HTML is a few KB; a dependency for one count is not worth it"
    )]
    fn advance_to(&mut self, to: usize) {
        let to = to.clamp(self.pos, self.src.len());
        let newlines = self.bytes[self.pos..to]
            .iter()
            .filter(|&&byte| byte == b'\n')
            .count();
        self.line = self
            .line
            .saturating_add(u32::try_from(newlines).unwrap_or(u32::MAX));
        self.pos = to;
    }

    fn skip_past(&mut self, needle: &str, from: usize) {
        match self.src.get(from..).and_then(|rest| rest.find(needle)) {
            Some(offset) => self.advance_to(from + offset + needle.len()),
            None => self.advance_to(self.src.len()),
        }
    }

    fn skip_to_gt(&mut self, at: usize) {
        match self.src.get(at..).and_then(|rest| rest.find('>')) {
            Some(offset) => self.advance_to(at + offset + 1),
            None => self.advance_to(self.src.len()),
        }
    }
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':')
}

/// Parses `<name …>` at `at`; `None` when `<` does not start a tag.
fn start_tag(bytes: &[u8], at: usize) -> Option<Tag> {
    let mut i = at.checked_add(1)?;
    if !bytes.get(i)?.is_ascii_alphabetic() {
        return None;
    }
    let name_start = i;
    while bytes.get(i).is_some_and(|byte| is_name_byte(*byte)) {
        i += 1;
    }
    let name = std::str::from_utf8(&bytes[name_start..i])
        .ok()?
        .to_ascii_lowercase();
    let mut quote = None;
    let mut last_significant = None;
    while let Some(&byte) = bytes.get(i) {
        match quote {
            Some(q) => {
                if byte == q {
                    quote = None;
                }
            }
            None if byte == b'"' || byte == b'\'' => quote = Some(byte),
            None if byte == b'>' => {
                return Some(Tag {
                    name,
                    end: i + 1,
                    self_closing: last_significant == Some(b'/'),
                });
            }
            None => {}
        }
        if !byte.is_ascii_whitespace() {
            last_significant = Some(byte);
        }
        i += 1;
    }
    None
}

/// Parses `</name …>` at `at`, returning the name and the index after `>`.
fn end_tag(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    let mut i = at.checked_add(2)?;
    if !bytes.get(i)?.is_ascii_alphabetic() {
        return None;
    }
    let start = i;
    while bytes.get(i).is_some_and(|byte| is_name_byte(*byte)) {
        i += 1;
    }
    let name = std::str::from_utf8(&bytes[start..i])
        .ok()?
        .to_ascii_lowercase();
    while bytes.get(i).is_some_and(|byte| *byte != b'>') {
        i += 1;
    }
    Some((name, (i + 1).min(bytes.len())))
}

/// The byte offset of `</tag` followed by a tag-name terminator, case-insensitively.
fn find_raw_end(src: &str, tag: &str, from: usize) -> Option<usize> {
    let needle = format!("</{tag}");
    let mut search = from;
    loop {
        let offset = find_ignore_ascii_case(src, &needle, search)?;
        let after = offset + needle.len();
        match src.as_bytes().get(after) {
            Some(b'>' | b'/' | b' ' | b'\t' | b'\n' | b'\r') | None => return Some(offset),
            Some(_) => search = after,
        }
    }
}

fn find_ignore_ascii_case(src: &str, needle: &str, from: usize) -> Option<usize> {
    src.get(from..)?
        .to_ascii_lowercase()
        .find(needle)
        .map(|offset| from + offset)
}
