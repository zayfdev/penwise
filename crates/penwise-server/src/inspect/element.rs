//! What the inspector reports about one element: selector, role, text, document rect, outer HTML, a computed
//! style subset and the source (`file:line`) of its HTML and matching CSS rules.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use super::Error;
use super::Page;

/// Longest text excerpt in an [`ElementInfo`], in characters.
const MAX_TEXT_CHARS: usize = 200;
/// Longest outer-HTML excerpt in an [`ElementInfo`], in bytes.
const MAX_OUTER_HTML_BYTES: usize = 2048;
/// Computed properties agents ask about most; all values are CSS strings.
const STYLE_KEYS: [&str; 17] = [
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "line-height",
    "color",
    "background-color",
    "margin",
    "padding",
    "display",
    "position",
    "width",
    "height",
    "gap",
    "border-radius",
    "box-shadow",
    "opacity",
];

/// A screenshot crop, in document pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Clip {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

impl Clip {
    /// A crop at the page's own scale.
    #[must_use]
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
            scale: 1.0,
        }
    }
}

/// One place an element's source comes from: the HTML document, or a matched CSS rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceLocation {
    /// The document or stylesheet URL as the browser saw it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Path relative to the project root, when the URL is same-origin and the file exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// 1-based line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// The matched selector for a CSS rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
}

/// An element as an agent sees it.
#[derive(Debug, Clone, Serialize)]
pub struct ElementInfo {
    /// Unique CSS path (an id when it is unique, else a class/nth-of-type path).
    pub selector: String,
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Trimmed text, at most 200 characters.
    pub text: String,
    /// Document coordinates: `[x, y, width, height]`.
    pub rect: [f64; 4],
    /// At most 2 KB.
    pub outer_html: String,
    /// The computed-style subset ([`STYLE_KEYS`]).
    pub styles: BTreeMap<String, String>,
    /// The page URL the element was inspected on.
    pub url: String,
    pub source: Vec<SourceLocation>,
}

/// The value the in-page script returns.
#[derive(Debug, Deserialize)]
pub(super) struct JsInfo {
    pub selector: String,
    pub dom_path: String,
    pub tag: String,
    pub text: String,
    pub rect: [f64; 4],
    pub outer_html: String,
    pub styles: BTreeMap<String, String>,
    #[serde(default)]
    pub role_attr: Option<String>,
}

/// Builds an [`ElementInfo`] for a DOM node already resolved by the caller.
pub(super) async fn info_for(page: &Page, node_id: i64) -> Result<ElementInfo, Error> {
    let info = script_info(page, node_id).await?;
    let role = ax_role(page, node_id)
        .await
        .or(info.role_attr)
        .or_else(|| implicit_role(&info.tag).map(str::to_owned));
    let url = page.url().to_owned();
    let root = page.root().map(Path::to_path_buf);
    let mut source = Vec::new();
    if let Some(html) = page.document_html().await
        && let Some(line) = super::source::html_line(&html, &info.dom_path)
    {
        source.push(SourceLocation {
            url: Some(url.clone()),
            file: root
                .as_deref()
                .and_then(|root| super::source::document_file(&url, root)),
            line: Some(line),
            rule: None,
        });
    }
    match page
        .command(
            "CSS.getMatchedStylesForNode",
            serde_json::json!({"nodeId": node_id}),
        )
        .await
    {
        Ok(matched) => {
            let sheets = page.style_sheets().await;
            source.extend(super::source::css_sources(
                &url,
                root.as_deref(),
                &matched,
                &sheets,
            ));
        }
        Err(error) => tracing::warn!(%error, "matched styles unavailable"),
    }
    Ok(ElementInfo {
        selector: info.selector,
        tag: info.tag,
        role,
        text: truncate_chars(&info.text, MAX_TEXT_CHARS),
        rect: info.rect,
        outer_html: truncate_bytes(&info.outer_html, MAX_OUTER_HTML_BYTES).to_owned(),
        styles: info.styles,
        url,
        source,
    })
}

async fn script_info(page: &Page, node_id: i64) -> Result<JsInfo, Error> {
    let resolved = page
        .command("DOM.resolveNode", serde_json::json!({"nodeId": node_id}))
        .await?;
    let object_id = resolved["object"]["objectId"]
        .as_str()
        .ok_or_else(|| Error::Cdp("DOM.resolveNode returned no object".to_owned()))?;
    let evaluated = page
        .command(
            "Runtime.callFunctionOn",
            serde_json::json!({
                "objectId": object_id,
                "functionDeclaration": INFO_SCRIPT.as_str(),
                "returnByValue": true,
                "silent": true,
            }),
        )
        .await?;
    if evaluated.get("exceptionDetails").is_some() {
        let reason = evaluated["exceptionDetails"]["text"]
            .as_str()
            .unwrap_or("unknown error");
        return Err(Error::Cdp(format!("element script failed: {reason}")));
    }
    serde_json::from_value(evaluated["result"]["value"].clone())
        .map_err(|error| Error::Cdp(format!("element info: {error}")))
}

/// The accessibility role from the browser's own AX tree, when it exposes one.
async fn ax_role(page: &Page, node_id: i64) -> Option<String> {
    let tree = page
        .command(
            "Accessibility.getPartialAXTree",
            serde_json::json!({"nodeId": node_id, "fetchRelatives": false}),
        )
        .await
        .ok()?;
    tree["nodes"][0]["role"]["value"]
        .as_str()
        .map(str::to_owned)
}

/// Fallback when the AX tree is unavailable.
fn implicit_role(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
        "a" => "link",
        "button" => "button",
        "nav" => "navigation",
        "main" => "main",
        "header" => "banner",
        "footer" => "contentinfo",
        "aside" => "complementary",
        "section" => "region",
        "article" => "article",
        "form" => "form",
        "img" => "img",
        "ul" | "ol" => "list",
        "li" => "listitem",
        "table" => "table",
        "p" => "paragraph",
        "input" | "textarea" => "textbox",
        "select" => "combobox",
        "dialog" => "dialog",
        _ => return None,
    })
}

fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn truncate_bytes(text: &str, max: usize) -> &str {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Runs in the page with the element as `this`; keep it self-contained and dependency-free.
const INFO_JS: &str = r##"
function () {
  const el = this;
  const style = getComputedStyle(el);
  const keys = KEYS;
  const styles = {};
  for (const key of keys) styles[key] = style.getPropertyValue(key);
  const indexOf = (node) => {
    let index = 1;
    for (let sib = node.previousElementSibling; sib; sib = sib.previousElementSibling) {
      if (sib.localName === node.localName) index += 1;
    }
    return index;
  };
  const domPath = (node) => {
    const parts = [];
    let cur = node;
    while (cur && cur.nodeType === 1) {
      parts.unshift(cur.localName + "[" + indexOf(cur) + "]");
      cur = cur.parentElement;
    }
    return parts.join("/");
  };
  const uniqueSelector = (node) => {
    const byId = (n) => (n.id ? "#" + CSS.escape(n.id) : null);
    const direct = byId(node);
    if (direct && document.querySelectorAll(direct).length === 1) return direct;
    const parts = [];
    let cur = node;
    while (cur && cur.nodeType === 1 && cur !== document.documentElement) {
      const id = byId(cur);
      if (id && document.querySelectorAll(id).length === 1) {
        parts.unshift(id);
        return parts.join(" > ");
      }
      let part = cur.localName;
      const classes = Array.from(cur.classList || []).filter(
        (name) => /^[A-Za-z_-][A-Za-z0-9_-]*$/.test(name),
      );
      if (classes.length) part += "." + classes.map((name) => CSS.escape(name)).join(".");
      part += ":nth-of-type(" + indexOf(cur) + ")";
      parts.unshift(part);
      cur = cur.parentElement;
    }
    parts.unshift("html");
    return parts.join(" > ");
  };
  const rect = el.getBoundingClientRect();
  return {
    selector: uniqueSelector(el),
    dom_path: domPath(el),
    tag: el.localName,
    text: (el.innerText || "").trim(),
    rect: [rect.left + window.scrollX, rect.top + window.scrollY, rect.width, rect.height],
    outer_html: el.outerHTML,
    styles: styles,
    role_attr: el.getAttribute("role"),
  };
}
"##;

/// `INFO_JS` with the style key list substituted (kept out of the JS literal so Rust owns the list).
static INFO_SCRIPT: LazyLock<String> = LazyLock::new(|| {
    let keys = STYLE_KEYS
        .iter()
        .map(|key| format!("\"{key}\""))
        .collect::<Vec<_>>()
        .join(",");
    INFO_JS.replace("KEYS", &format!("[{keys}]"))
});
