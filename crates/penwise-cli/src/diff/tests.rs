use std::path::Path;

use super::unified;

#[test]
fn equal_texts_produce_headers_only() {
    let diff = unified(Path::new("vite.config.ts"), "a\nb\n", "a\nb\n");
    assert_eq!(
        diff,
        "--- vite.config.ts\n+++ vite.config.ts (penwise adapter)\n"
    );
}

#[test]
fn a_replacement_shows_both_lines_with_context() {
    let diff = unified(Path::new("x"), "one\ntwo\nthree\n", "one\nTWO\nthree\n");
    assert!(diff.contains("@@ -1,3 +1,3 @@"), "{diff}");
    assert!(
        diff.contains("\n-one\n") || diff.contains("-two\n"),
        "{diff}"
    );
    assert!(diff.contains("-two\n"), "{diff}");
    assert!(diff.contains("+TWO\n"), "{diff}");
    assert!(diff.contains(" three\n"), "{diff}");
}

#[test]
fn an_insertion_at_the_start_keeps_the_old_line_count() {
    let diff = unified(Path::new("x"), "body\n", "import x from 'y';\nbody\n");
    assert!(diff.contains("+import x from 'y';\n"), "{diff}");
    assert!(diff.contains(" body\n"), "{diff}");
    assert!(diff.contains("@@ -1,1 +1,2 @@"), "{diff}");
}

#[test]
fn far_apart_changes_are_separate_hunks() {
    let mut before = String::new();
    for line in 1..=20 {
        use std::fmt::Write as _;
        writeln!(before, "line {line}").expect("write");
    }
    let after = before
        .replace("line 2\n", "line two\n")
        .replace("line 19\n", "line nineteen\n");
    let diff = unified(Path::new("x"), &before, &after);
    assert_eq!(diff.matches("@@ -").count(), 2, "{diff}");
}
