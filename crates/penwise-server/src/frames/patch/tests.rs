//! Patch validation and sanitizing (P1): the limits, the op vocabulary and every strip rule.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{MAX_OPS, Op, Patch, PatchError, sanitize_html};

fn parse(text: &str) -> Patch {
    Patch::parse(text).expect("valid patch")
}

#[test]
fn a_patch_round_trips_to_canonical_json() {
    let patch = parse(
        r#"{"css":".a{color:red}","dom":[{"op":"replace","selector":".a","html":"<b>x</b>"},
            {"op":"remove","selector":".gone"},{"op":"set_attr","selector":".a","name":"title","value":"hi"}]}"#,
    );
    assert_eq!(patch.css, ".a{color:red}");
    assert_eq!(patch.dom.len(), 3);
    assert_eq!(patch.dom[0].op, Op::Replace);
    let text = patch.to_text();
    assert_eq!(Patch::parse(&text).unwrap(), patch);
    assert!(text.contains("\"op\":\"set_attr\""));
}

#[test]
fn oversized_text_is_rejected() {
    let big = format!(r#"{{"css":"{}"}}"#, "a".repeat(65 * 1024));
    assert_eq!(Patch::parse(&big), Err(PatchError::TooLarge));
}

#[test]
fn more_than_two_hundred_ops_are_rejected() {
    let ops = (0..=MAX_OPS)
        .map(|_| r#"{"op":"remove","selector":".x"}"#)
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        Patch::parse(&format!("{{\"dom\":[{ops}]}}")),
        Err(PatchError::TooManyOps)
    );
}

#[test]
fn an_unknown_op_or_field_is_not_a_patch() {
    assert!(matches!(
        Patch::parse(r#"{"dom":[{"op":"evaluate","selector":".x"}]}"#),
        Err(PatchError::Invalid(_))
    ));
    assert!(matches!(
        Patch::parse(r#"{"js":"alert(1)"}"#),
        Err(PatchError::Invalid(_))
    ));
}

#[test]
fn ops_must_carry_their_fields() {
    let cases = [
        r#"{"dom":[{"op":"replace","selector":".x"}]}"#,
        r#"{"dom":[{"op":"set_attr","selector":".x","name":"title"}]}"#,
        r#"{"dom":[{"op":"remove","selector":""}]}"#,
        r#"{"dom":[{"op":"remove","selector":".x","html":"<b>"}]}"#,
    ];
    for case in cases {
        assert!(
            matches!(Patch::parse(case), Err(PatchError::BadOp { .. })),
            "{case} must be rejected"
        );
    }
    let long = format!(
        r#"{{"dom":[{{"op":"remove","selector":"{}"}}]}}"#,
        "a".repeat(513)
    );
    assert_eq!(
        Patch::parse(&long),
        Err(PatchError::BadOp {
            index: 0,
            reason: "selector is 1 to 512 characters"
        })
    );
}

#[test]
fn script_tags_and_their_bodies_are_stripped() {
    let clean = sanitize_html("<p>ok</p><script>alert('x')</script><p>after</p>");
    assert_eq!(clean, "<p>ok</p><p>after</p>");
    let upper = sanitize_html("<SCRIPT type=\"x\">bad()</SCRIPT>fine");
    assert_eq!(upper, "fine");
}

#[test]
fn iframe_object_and_embed_are_stripped() {
    let clean = sanitize_html(
        r#"<iframe src="https://evil"></iframe><object data="x"></object><embed src="y">ok"#,
    );
    assert_eq!(clean, "ok");
}

#[test]
fn event_attributes_are_stripped() {
    let clean = sanitize_html(r#"<img src="a.png" onerror="alert(1)" ONLOAD='x'>"#);
    assert_eq!(clean, "<img src=\"a.png\">");
}

#[test]
fn javascript_urls_are_stripped() {
    let clean = sanitize_html(r#"<a href="javascript:alert(1)" title="t">x</a>"#);
    assert_eq!(clean, "<a title=\"t\">x</a>");
    let padded = sanitize_html("<a href=\" jav\u{0009}ascript:alert(1)\">x</a>");
    assert_eq!(padded, "<a>x</a>");
    let hidden = sanitize_html(r#"<a href="java&#115;cript:alert(1)">x</a>"#);
    assert_eq!(hidden, "<a>x</a>");
}

#[test]
fn ordinary_markup_is_kept_verbatim() {
    let html = r#"<div class="hero" data-x="1"><span style="color: red">Hi</span></div>"#;
    assert_eq!(sanitize_html(html), html);
    let comment = "<!-- a note --><p>text</p>";
    assert_eq!(sanitize_html(comment), comment);
    let literal = "<p>2 < 3 and 4 > 1</p>";
    assert_eq!(sanitize_html(literal), literal);
}

#[test]
fn an_unsafe_set_attr_op_is_dropped() {
    let patch = parse(
        r#"{"dom":[
            {"op":"set_attr","selector":".x","name":"onclick","value":"alert(1)"},
            {"op":"set_attr","selector":".x","name":"href","value":"javascript:alert(1)"},
            {"op":"set_attr","selector":".x","name":"href","value":"/safe"},
            {"op":"set_text","selector":".x","value":"<script>bad()</script>hello"}
        ]}"#,
    );
    assert_eq!(patch.dom.len(), 2);
    assert_eq!(patch.dom[0].value.as_deref(), Some("/safe"));
    assert_eq!(patch.dom[1].op, Op::SetText);
    assert_eq!(patch.dom[1].value.as_deref(), Some("hello"));
}

#[test]
fn a_bad_class_token_is_dropped() {
    let patch = parse(
        r#"{"dom":[
            {"op":"add_class","selector":".x","name":"a b"},
            {"op":"remove_class","selector":".x","name":"hero"},
            {"op":"add_class","selector":".x","name":"bad\"><script>"}
        ]}"#,
    );
    assert_eq!(patch.dom.len(), 1);
    assert_eq!(patch.dom[0].name.as_deref(), Some("hero"));
}
