//! The compact text of `look` (spec v2 §5): a header, the frames, one line per mark, the owner's picks.
//!
//! Everything here is pure: the route gathers the facts, this file only spells them.

use std::fmt::Write as _;

use serde_json::Value;

/// Most characters of handwriting kept on a mark line.
const MAX_TEXT_CHARS: usize = 80;
/// Most `file:line` entries kept on a mark line.
const MAX_SOURCES: usize = 3;
/// Most computed styles kept on a mark line.
const MAX_STYLES: usize = 3;
/// The computed styles worth a line, most telling first.
const STYLE_PRIORITY: [&str; 9] = [
    "font-size",
    "font-weight",
    "color",
    "background-color",
    "padding",
    "margin",
    "line-height",
    "display",
    "width",
];

/// The first line: where we are.
pub(super) struct Header {
    pub(super) project: String,
    pub(super) board: Option<String>,
    pub(super) round: u32,
    pub(super) status: &'static str,
    /// Marks of the round, done ones included.
    pub(super) total: usize,
    /// Marks listed below (not done).
    pub(super) open: usize,
}

/// A frame of the board.
pub(super) struct FrameLine {
    pub(super) id: String,
    pub(super) preset: &'static str,
    pub(super) route: String,
    pub(super) label: String,
    pub(super) css: [u32; 2],
    pub(super) box_size: [f32; 2],
}

/// A variant group and the owner's pick in it.
pub(super) struct Pick {
    pub(super) group: String,
    /// `(label, frame id, chosen)` in board order.
    pub(super) variants: Vec<(String, String, bool)>,
}

/// One listed mark: the frame it is on (index into the frame list) and its bundle.
pub(super) struct MarkRow<'a> {
    pub(super) frame: usize,
    pub(super) bundle: &'a Value,
}

/// The whole text.
pub(super) fn render(
    header: &Header,
    frames: &[FrameLine],
    marks: &[MarkRow<'_>],
    since_send: &[MarkRow<'_>],
    picks: &[Pick],
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Penwise: {} | board {} | round {} {} | {} mark(s), {} open",
        header.project,
        header.board.as_deref().unwrap_or("none yet (show a frame)"),
        header.round,
        header.status,
        header.total,
        header.open,
    );
    out.push_str("frames:\n");
    if frames.is_empty() {
        out.push_str("  (none)\n");
    }
    for (index, frame) in frames.iter().enumerate() {
        out.push_str(&frame_line(index, frame));
        out.push('\n');
    }
    out.push_str("marks:\n");
    if marks.is_empty() {
        out.push_str("  (none open)\n");
    }
    for (index, row) in marks.iter().enumerate() {
        out.push_str(&mark_line(index + 1, row.frame, row.bundle));
        out.push('\n');
    }
    if !since_send.is_empty() {
        out.push_str("since send (next round):\n");
        for (index, row) in since_send.iter().enumerate() {
            out.push_str(&mark_line(marks.len() + index + 1, row.frame, row.bundle));
            out.push('\n');
        }
    }
    if !picks.is_empty() {
        out.push_str("picks:\n");
        for pick in picks {
            out.push_str(&pick_line(pick));
            out.push('\n');
        }
    }
    out
}

fn frame_line(index: usize, frame: &FrameLine) -> String {
    let mut line = format!(
        "  f{} {} {} {}",
        index + 1,
        frame.id,
        capitalised(frame.preset),
        frame.route
    );
    if !frame.label.is_empty() {
        let _ = write!(line, " \"{}\"", frame.label);
    }
    let _ = write!(
        line,
        " {}x{} (box {:.0}x{:.0})",
        frame.css[0], frame.css[1], frame.box_size[0], frame.box_size[1]
    );
    line
}

fn pick_line(pick: &Pick) -> String {
    let chosen = pick.variants.iter().find(|variant| variant.2);
    let verdict = chosen.map_or_else(
        || "no pick yet".to_owned(),
        |(label, id, _)| format!("picked \"{label}\" {id}"),
    );
    let all: Vec<String> = pick
        .variants
        .iter()
        .map(|(label, id, _)| format!("\"{label}\" {id}"))
        .collect();
    format!("  {}: {verdict} | {}", pick.group, all.join(", "))
}

/// One mark: `2 <id> f1 new arrow red/problem "less heavy" h1.hero-title → section#work · index.html:42 {…}`.
pub(super) fn mark_line(index: usize, frame: usize, bundle: &Value) -> String {
    let text = |pointer: &str| bundle.pointer(pointer).and_then(Value::as_str);
    let mut line = format!(
        "  {index} {} f{} {} {} {}",
        text("/id").unwrap_or("?"),
        frame + 1,
        text("/status").unwrap_or("new"),
        text("/kind/guess").unwrap_or("free"),
        colour_name(text("/colour").unwrap_or("none")),
    );
    if let Some(words) = text("/text").map(clean).filter(|words| !words.is_empty()) {
        let _ = write!(line, " \"{words}\"");
    }
    line.push(' ');
    line.push_str(&target(bundle));
    if let Some(to) = text("/kind/to/selector") {
        let _ = write!(line, " \u{2192} {to}");
    }
    for source in sources(bundle) {
        let _ = write!(line, " \u{b7} {source}");
    }
    if let Some(styles) = styles(bundle) {
        let _ = write!(line, " {styles}");
    }
    line
}

/// `red/problem`, `blue/idea`, `green/good`, else plain ink.
fn colour_name(meaning: &str) -> &'static str {
    match meaning {
        "problem" => "red/problem",
        "idea" => "blue/idea",
        "good" => "green/good",
        _ => "ink",
    }
}

/// The handwriting on one line, quotes softened, cut to a sensible length.
fn clean(words: &str) -> String {
    let flat: String = words
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('"', "'");
    if flat.chars().count() <= MAX_TEXT_CHARS {
        return flat;
    }
    let mut cut: String = flat.chars().take(MAX_TEXT_CHARS).collect();
    cut.push('\u{2026}');
    cut
}

/// The element's selector, or the area the mark covers.
fn target(bundle: &Value) -> String {
    if let Some(selector) = bundle.pointer("/element/selector").and_then(Value::as_str) {
        return selector.to_owned();
    }
    let rect: Vec<f64> = bundle
        .get("rect")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    match rect.as_slice() {
        [x, y, w, h] => format!("@{x:.0},{y:.0} {w:.0}x{h:.0}"),
        _ => "(no element)".to_owned(),
    }
}

/// The distinct `file:line` places of the element, component first.
fn sources(bundle: &Value) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for source in bundle
        .pointer("/element/source")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(file) = source.get("file").and_then(Value::as_str) else {
            continue;
        };
        let place = match source.get("line").and_then(Value::as_u64) {
            Some(line) => format!("{file}:{line}"),
            None => file.to_owned(),
        };
        if !found.contains(&place) {
            found.push(place);
        }
        if found.len() == MAX_SOURCES {
            break;
        }
    }
    found
}

/// `{font-size:72px; font-weight:800}` from the element's computed styles.
fn styles(bundle: &Value) -> Option<String> {
    let all = bundle.pointer("/element/styles")?.as_object()?;
    let picked: Vec<String> = STYLE_PRIORITY
        .iter()
        .filter_map(|key| Some(format!("{key}:{}", all.get(*key)?.as_str()?)))
        .take(MAX_STYLES)
        .collect();
    (!picked.is_empty()).then(|| format!("{{{}}}", picked.join("; ")))
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
