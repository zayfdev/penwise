//! Matched CSS rules → stylesheet file and line, and served URLs → project-relative files.
//!
//! `CSS.getMatchedStylesForNode` reports the rule's style sheet id and selector range, but not the sheet's URL;
//! the CDP reader keeps the `CSS.styleSheetAdded` headers. A rule's `range` is not always present on Chrome 155
//! (only the selector and style ranges are), so both are used as fallbacks.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Deserialize;

use super::element::SourceLocation;

pub(super) use super::html_map::html_line;

/// Most rules reported for one element; a dev stylesheet can match hundreds.
const MAX_CSS_RULES: usize = 64;

/// A stylesheet's `<link>`/`<style>` metadata, as reported by `CSS.styleSheetAdded`.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct Header {
    #[serde(rename = "styleSheetId")]
    pub style_sheet_id: String,
    #[serde(rename = "sourceURL", default)]
    pub source_url: String,
    #[serde(rename = "isInline", default)]
    pub is_inline: bool,
    #[serde(rename = "startLine", default)]
    pub start_line: u32,
}

/// The author rules that match an element, with their `file:line` where the project root is known.
pub(super) fn css_sources(
    page_url: &str,
    root: Option<&Path>,
    matched: &serde_json::Value,
    sheets: &HashMap<String, Header>,
) -> Vec<SourceLocation> {
    let Some(entries) = matched
        .get("matchedCSSRules")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for entry in entries {
        let rule = &entry["rule"];
        if rule["origin"]
            .as_str()
            .is_some_and(|origin| origin != "regular")
        {
            continue;
        }
        let selector = rule["selectorList"]["text"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned();
        if selector.is_empty() {
            continue;
        }
        let header = rule["styleSheetId"].as_str().and_then(|id| sheets.get(id));
        let selector_range = rule["selectorList"]["selectors"]
            .as_array()
            .and_then(|selectors| selectors.first())
            .and_then(|selector| selector["range"]["startLine"].as_u64());
        let start = rule["range"]["startLine"]
            .as_u64()
            .or(selector_range)
            .or_else(|| rule["style"]["range"]["startLine"].as_u64())
            .and_then(|n| u32::try_from(n).ok());
        let inline = header.is_some_and(|header| header.is_inline);
        // Ranges are zero-based; an inline sheet's rules are relative to its `<style>` content, whose first
        // line is the header's `startLine` in the document.
        let line = start.map(|n| {
            n.saturating_add(1).saturating_add(if inline {
                header.map_or(0, |header| header.start_line)
            } else {
                0
            })
        });
        let (url, file) = if inline {
            (
                Some(page_url.to_owned()),
                root.and_then(|root| document_file(page_url, root)),
            )
        } else if let Some(header) = header {
            (
                absolute_url(page_url, &header.source_url),
                root.and_then(|root| asset_file(page_url, &header.source_url, root)),
            )
        } else {
            (None, None)
        };
        let key = (url.clone(), file.clone(), line, selector.clone());
        if !seen.insert(key) {
            continue;
        }
        out.push(SourceLocation {
            url,
            file,
            line,
            rule: Some(selector),
        });
        if out.len() >= MAX_CSS_RULES {
            break;
        }
    }
    out
}

/// `index.html` for a directory URL, else the path relative to the server root.
pub(super) fn document_file(page_url: &str, root: &Path) -> Option<String> {
    let url = reqwest::Url::parse(page_url).ok()?;
    let mut path = url.path().trim_start_matches('/').to_owned();
    if path.is_empty() || path.ends_with('/') {
        path.push_str("index.html");
    }
    existing(root, &path)
}

/// The stylesheet path relative to `root`, when it is same-origin and exists on disk.
pub(super) fn asset_file(page_url: &str, source_url: &str, root: &Path) -> Option<String> {
    let page = reqwest::Url::parse(page_url).ok()?;
    let source = resolve(&page, source_url)?;
    if source.origin() != page.origin() {
        return None;
    }
    existing(root, source.path().trim_start_matches('/'))
}

/// The absolute stylesheet URL, or the page URL for an empty/inline source.
fn absolute_url(page_url: &str, source_url: &str) -> Option<String> {
    if source_url.is_empty() {
        return Some(page_url.to_owned());
    }
    let page = reqwest::Url::parse(page_url).ok()?;
    resolve(&page, source_url).map(|url| url.to_string())
}

fn resolve(page: &reqwest::Url, source_url: &str) -> Option<reqwest::Url> {
    reqwest::Url::parse(source_url)
        .ok()
        .or_else(|| page.join(source_url).ok())
}

fn existing(root: &Path, relative: &str) -> Option<String> {
    if relative.is_empty()
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    root.join(relative).is_file().then(|| relative.to_owned())
}
