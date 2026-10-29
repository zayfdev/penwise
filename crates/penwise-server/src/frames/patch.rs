//! Patch JSON (design-harness M4, P1): the CSS plus DOM ops an agent proposes as a ghost or a variant.
//!
//! The server owns validation and sanitizing: [`Patch::parse`] checks the shape and the limits (64 KiB, 200 ops,
//! 512-character selectors), then strips `<script>`, `on*` attributes, `javascript:` URLs, `<iframe>`, `<object>`
//! and `<embed>` from every `html` and value before storing the canonical JSON text. [`sanitize`] does the
//! stripping; the bridge does it again with the page's own parser, so a missed corner here cannot execute
//! anything.
//!
//! There is no arbitrary JS in v1 (spec §12.6): the vocabulary is the ten [`Op`]s below, and anything else is a
//! bad request.

mod sanitize;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

pub(crate) use sanitize::sanitize_html;

use sanitize::{dangerous_url, forbidden_name};

/// Largest patch text, in bytes.
pub(crate) const MAX_PATCH_BYTES: usize = 64 * 1024;
/// Most DOM ops in one patch.
pub(crate) const MAX_OPS: usize = 200;
/// Longest CSS selector, in characters.
pub(crate) const MAX_SELECTOR: usize = 512;

/// Why a patch was rejected.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum PatchError {
    #[error("patch is larger than 64 KiB")]
    TooLarge,
    #[error("patch is not valid JSON: {0}")]
    Invalid(String),
    #[error("patch has more than 200 DOM ops")]
    TooManyOps,
    #[error("op {index}: {reason}")]
    BadOp { index: usize, reason: &'static str },
}

/// One patch: a stylesheet and DOM operations, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Patch {
    #[serde(default)]
    pub(crate) css: String,
    #[serde(default)]
    pub(crate) dom: Vec<DomOp>,
}

/// One DOM operation. `html`, `name` and `value` are required by different ops (see [`Op`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DomOp {
    pub(crate) op: Op,
    pub(crate) selector: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<String>,
}

/// The op vocabulary (spec §12.6, contract P1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Op {
    Replace,
    Remove,
    InsertBefore,
    InsertAfter,
    Append,
    Prepend,
    SetAttr,
    RemoveAttr,
    SetText,
    AddClass,
    RemoveClass,
}

impl Op {
    fn needs_html(self) -> bool {
        matches!(
            self,
            Self::Replace | Self::InsertBefore | Self::InsertAfter | Self::Append | Self::Prepend
        )
    }

    fn needs_name(self) -> bool {
        matches!(
            self,
            Self::SetAttr | Self::RemoveAttr | Self::AddClass | Self::RemoveClass
        )
    }
}

impl Patch {
    /// Parses, validates and sanitizes patch text.
    ///
    /// # Errors
    /// [`PatchError`] for oversized text, bad JSON, unknown ops or fields, a missing/oversized selector, a
    /// missing field, or too many ops.
    pub(crate) fn parse(text: &str) -> Result<Self, PatchError> {
        if text.len() > MAX_PATCH_BYTES {
            return Err(PatchError::TooLarge);
        }
        let patch: Self =
            serde_json::from_str(text).map_err(|error| PatchError::Invalid(error.to_string()))?;
        if patch.dom.len() > MAX_OPS {
            return Err(PatchError::TooManyOps);
        }
        Self::sanitized(patch)
    }

    /// The canonical JSON text stored in the frame's `patch` field.
    #[must_use]
    pub(crate) fn to_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"css\":\"\",\"dom\":[]}".to_owned())
    }

    /// Every op checked for its required fields, then sanitized; an op that sanitizes away disappears.
    fn sanitized(patch: Self) -> Result<Self, PatchError> {
        let mut dom = Vec::with_capacity(patch.dom.len());
        for (index, op) in patch.dom.into_iter().enumerate() {
            let fail = |reason| PatchError::BadOp { index, reason };
            if op.selector.is_empty() || op.selector.chars().count() > MAX_SELECTOR {
                return Err(fail("selector is 1 to 512 characters"));
            }
            if op.html.is_some() != op.op.needs_html() {
                return Err(fail("this op needs html"));
            }
            if op.name.is_some() != op.op.needs_name() {
                return Err(fail("this op needs name"));
            }
            if op.op == Op::SetAttr && op.value.is_none() {
                return Err(fail("set_attr needs value"));
            }
            if op.op == Op::SetText && op.value.is_none() {
                return Err(fail("set_text needs value"));
            }
            if let Some(clean) = sanitize_op(op) {
                dom.push(clean);
            }
        }
        Ok(Self {
            css: patch.css,
            dom,
        })
    }
}

/// One op with its html/value sanitized; `None` when sanitizing leaves nothing safe to do.
fn sanitize_op(mut op: DomOp) -> Option<DomOp> {
    op.html = op.html.as_deref().map(sanitize_html);
    match op.op {
        Op::SetAttr => {
            let name = op.name.as_deref()?;
            if forbidden_name(name) || op.value.as_deref().is_some_and(dangerous_url) {
                return None;
            }
        }
        Op::RemoveAttr => {
            if forbidden_name(op.name.as_deref()?) {
                return None;
            }
        }
        Op::AddClass | Op::RemoveClass => {
            if !valid_class(op.name.as_deref()?) {
                return None;
            }
        }
        Op::SetText => {
            op.value = op.value.as_deref().map(sanitize_html);
        }
        _ => {}
    }
    Some(op)
}

/// A class token (`a-z`, `A-Z`, `0-9`, `-`, `_`), so a name cannot smuggle a second token or a selector escape.
fn valid_class(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}
