//! The pure parts of `look`: the text, base64, the badges. The route itself needs a browser and is covered by
//! the end-to-end round in `penwise-mcp/tests/round.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use penwise_core::item::{ItemId, PageKind, Scene};
use serde_json::json;
use tiny_skia::Pixmap;

use super::b64;
use super::digits;
use super::overview::{Numbered, badge_marks};
use super::text::{FrameLine, Header, MarkRow, Pick, mark_line, render};

fn bundle() -> serde_json::Value {
    json!({
        "id": "0123456789abcdef0123456789abcdef",
        "status": "reopened",
        "kind": { "guess": "arrow", "confidence": 0.9, "from": null, "to": { "selector": "section#work", "text": "Work" } },
        "colour": "problem",
        "text": "less  heavy\n\"please\"",
        "rect": [100.0, 200.0, 300.0, 40.0],
        "element": {
            "selector": "h1.hero-title",
            "styles": { "font-size": "72px", "font-weight": "800", "color": "rgb(0, 0, 0)", "display": "block", "width": "10px" },
            "source": [
                { "file": "src/Hero.tsx", "line": 12, "kind": "component" },
                { "file": "index.html", "line": 42 },
                { "file": "styles.css", "line": 118, "rule": ".hero-title" },
                { "file": "styles.css", "line": 118, "rule": ".hero h1" },
                { "file": "extra.css", "line": 3 }
            ]
        }
    })
}

#[test]
fn a_mark_line_has_the_index_kind_colour_words_target_sources_and_styles() {
    let line = mark_line(2, 0, &bundle());
    assert_eq!(
        "  2 0123456789abcdef0123456789abcdef f1 reopened arrow red/problem \"less heavy 'please'\" h1.hero-title \u{2192} section#work \u{b7} src/Hero.tsx:12 \u{b7} index.html:42 \u{b7} styles.css:118 {font-size:72px; font-weight:800; color:rgb(0, 0, 0)}",
        line
    );
}

#[test]
fn a_mark_without_an_element_names_its_area_and_one_without_words_has_no_quotes() {
    let line = mark_line(
        1,
        1,
        &json!({ "id": "ab", "kind": { "guess": "circle" }, "colour": "none", "text": null, "rect": [10.4, 20.6, 30.0, 40.0], "element": null }),
    );
    assert_eq!("  1 ab f2 new circle ink @10,21 30x40", line);
    let nothing = mark_line(3, 0, &json!({ "id": "cd" }));
    assert_eq!("  3 cd f1 new free ink (no element)", nothing);
}

#[test]
fn the_text_lists_header_frames_marks_and_picks() {
    let header = Header {
        project: "site (prj_site)".to_owned(),
        board: Some("PAGE".to_owned()),
        round: 3,
        status: "sent",
        total: 5,
        open: 1,
    };
    let frames = [FrameLine {
        id: "f".repeat(32),
        preset: "desktop",
        route: "/".to_owned(),
        label: String::new(),
        css: [1440, 900],
        box_size: [1440.0, 3200.0],
    }];
    let bundle = bundle();
    let marks = [MarkRow {
        frame: 0,
        bundle: &bundle,
    }];
    let picks = [Pick {
        group: "vg_1".to_owned(),
        variants: vec![
            ("calm".to_owned(), "a".repeat(32), false),
            ("bold".to_owned(), "b".repeat(32), true),
        ],
    }];
    let text = render(&header, &frames, &marks, &[], &picks);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        "Penwise: site (prj_site) | board PAGE | round 3 sent | 5 mark(s), 1 open",
        lines[0]
    );
    assert_eq!("frames:", lines[1]);
    assert_eq!(
        format!("  f1 {} Desktop / 1440x900 (box 1440x3200)", "f".repeat(32)),
        lines[2]
    );
    assert_eq!("marks:", lines[3]);
    assert!(lines[4].starts_with("  1 0123456789abcdef"), "{}", lines[4]);
    assert_eq!("picks:", lines[5]);
    assert_eq!(
        format!(
            "  vg_1: picked \"bold\" {} | \"calm\" {}, \"bold\" {}",
            "b".repeat(32),
            "a".repeat(32),
            "b".repeat(32)
        ),
        lines[6]
    );
}

#[test]
fn an_empty_board_says_so() {
    let header = Header {
        project: "site (prj_site)".to_owned(),
        board: None,
        round: 1,
        status: "open",
        total: 0,
        open: 0,
    };
    let text = render(&header, &[], &[], &[], &[]);
    assert!(text.contains("board none yet"), "{text}");
    assert!(text.contains("(none)"));
    assert!(text.contains("(none open)"));
    assert!(!text.contains("picks:"));
}

#[test]
fn base64_follows_rfc_4648() {
    assert_eq!("", b64::encode(b""));
    assert_eq!("Zg==", b64::encode(b"f"));
    assert_eq!("Zm8=", b64::encode(b"fo"));
    assert_eq!("Zm9v", b64::encode(b"foo"));
    assert_eq!("Zm9vYmFy", b64::encode(b"foobar"));
    assert_eq!("/wCr", b64::encode(&[0xFF, 0x00, 0xAB]));
}

#[test]
fn every_digit_has_strokes_and_a_two_digit_badge_is_wider() {
    for digit in 0..10 {
        let strokes = digits::badge(digit, [0.0, 0.0], 1.0);
        assert!(strokes.len() >= 3, "digit {digit} has a disc and a glyph");
        assert!(strokes.iter().all(|stroke| stroke.points.len() >= 2));
    }
    assert!(digits::radius(12, 1.0) > digits::radius(2, 1.0));
    assert!(
        digits::radius(2, 3.0) > digits::radius(2, 1.0),
        "badges grow with the scale"
    );
}

fn badged(numbers: &[usize]) -> Vec<u8> {
    let mut pixmap = Pixmap::new(200, 120).expect("pixmap");
    pixmap.fill(tiny_skia::Color::WHITE);
    let ink = Scene::from_items(PageKind::Infinite, std::iter::empty()).expect("scene");
    let marks: Vec<Numbered> = numbers
        .iter()
        .enumerate()
        .map(|(index, number)| Numbered {
            number: *number,
            mark: ItemId([u8::try_from(index).unwrap(); 16]),
            rect: [
                40.0 + 60.0 * f32::from(u8::try_from(index).unwrap()),
                50.0,
                30.0,
                20.0,
            ],
            strokes: Vec::new(),
        })
        .collect();
    badge_marks(&mut pixmap, &ink, &marks, [0.0, 0.0]).expect("badges");
    pixmap.data().to_vec()
}

#[test]
fn badges_are_drawn_where_the_marks_are_and_are_deterministic() {
    let white = {
        let mut blank = Pixmap::new(200, 120).unwrap();
        blank.fill(tiny_skia::Color::WHITE);
        blank.data().to_vec()
    };
    let first = badged(&[1, 12]);
    assert_ne!(white, first, "something was drawn");
    assert_eq!(
        first,
        badged(&[1, 12]),
        "the same marks give the same pixels"
    );
    assert_ne!(first, badged(&[2, 12]), "the number shows");
}

#[test]
fn marks_drawn_since_the_send_are_listed_apart_and_numbered_after_the_round() {
    let header = Header {
        project: "site (prj_site)".to_owned(),
        board: Some("PAGE".to_owned()),
        round: 1,
        status: "sent",
        total: 1,
        open: 1,
    };
    let bundle = bundle();
    let late = json!({ "id": "late", "status": "new", "kind": { "guess": "circle" }, "colour": "idea", "rect": [1.0, 2.0, 3.0, 4.0] });
    let now = [MarkRow {
        frame: 0,
        bundle: &bundle,
    }];
    let after = [MarkRow {
        frame: 0,
        bundle: &late,
    }];
    let text = render(&header, &[], &now, &after, &[]);
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|line| *line == "since send (next round):")
        .expect("section");
    assert_eq!("  2 late f1 new circle blue/idea @1,2 3x4", lines[at + 1]);
    assert!(
        lines[..at].iter().any(|line| line.starts_with("  1 0123")),
        "the round's mark is listed first"
    );
}
