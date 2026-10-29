#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::indexing_slicing
)]

use penwise_testkit::TempDir;

use super::base64::decode_base64;
use super::source::{asset_file, component_ref, document_file, html_line};

const PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
  <title>T</title>
  <script>const s = "<div>fake</div>";</script>
  <style>.a { color: red }</style>
</head>
<body>
  <main id="top">
    <h1 id="headline" class="headline">Hello</h1>
  </main>
  <ul>
    <li>one
    <li>two
  </ul>
  <svg viewBox="0 0 10 10"><path d="M0 0"/><circle r="1"/></svg>
  <a title="a > b" href="/x">link</a>
  <!-- <div>comment</div> -->
  <table><tr><td>x</td></tr></table>
  <p>text
  <div>after</div>
</body>
</html>
"#;

fn line_of(text: &str, needle: &str) -> u32 {
    u32::try_from(
        text.lines()
            .position(|line| line.contains(needle))
            .expect("needle is in the fixture")
            + 1,
    )
    .expect("line fits")
}

#[test]
fn html_line_finds_elements_by_dom_path() {
    assert_eq!(
        html_line(PAGE, "html[1]/body[1]/main[1]/h1[1]"),
        Some(line_of(PAGE, "<h1"))
    );
    assert_eq!(
        html_line(PAGE, "html[1]/body[1]/ul[1]/li[2]"),
        Some(line_of(PAGE, "<li>two"))
    );
    assert_eq!(
        html_line(PAGE, "html[1]/body[1]/svg[1]/path[1]"),
        Some(line_of(PAGE, "<svg"))
    );
    assert_eq!(
        html_line(PAGE, "html[1]/body[1]/svg[1]/circle[1]"),
        Some(line_of(PAGE, "<svg"))
    );
    assert_eq!(
        html_line(PAGE, "html[1]/body[1]/a[1]"),
        Some(line_of(PAGE, "<a title"))
    );
    assert_eq!(
        html_line(PAGE, "html[1]/body[1]/table[1]/tbody[1]/tr[1]/td[1]"),
        Some(line_of(PAGE, "<table"))
    );
    assert_eq!(
        html_line(PAGE, "html[1]/body[1]/table[1]"),
        Some(line_of(PAGE, "<table"))
    );
}

#[test]
fn html_line_ignores_markup_in_scripts_comments_and_attribute_values() {
    assert_eq!(html_line(PAGE, "html[1]/body[1]/main[1]/div[1]"), None);
    assert_eq!(html_line(PAGE, "html[1]/body[1]/div[2]"), None);
    // The real div after the optional `p` is the first div in body.
    assert_eq!(
        html_line(PAGE, "html[1]/body[1]/div[1]"),
        Some(line_of(PAGE, "<div>after"))
    );
    // The `<a>` was parsed as one tag despite the `>` inside its title.
    assert_eq!(
        html_line(PAGE, "html[1]/body[1]/a[1]"),
        Some(line_of(PAGE, "<a title"))
    );
}

#[test]
fn html_line_handles_optional_end_tags_and_void_elements() {
    let html = "<html><body>\n<p>one\n<p>two\n<br><div>three</div></body></html>";
    assert_eq!(
        html_line(html, "html[1]/body[1]/p[2]"),
        Some(line_of(html, "<p>two"))
    );
    // `br` is void: the div is a sibling, not a child of `br`.
    assert_eq!(
        html_line(html, "html[1]/body[1]/div[1]"),
        Some(line_of(html, "<br>"))
    );
    assert_eq!(
        html_line(html, "html[1]/body[1]/p[2]/br[1]"),
        Some(line_of(html, "<br>"))
    );
}

#[test]
fn html_line_synthesizes_a_missing_html_element() {
    let html = "<!doctype html>\n<body>\n<p>text</p>\n</body>";
    assert_eq!(
        html_line(html, "html[1]/body[1]/p[1]"),
        Some(line_of(html, "<p>"))
    );
}

#[test]
fn html_line_matches_case_insensitively() {
    let html = "<HTML>\n<BODY>\n<SECTION>\n<H1>x</H1>\n</SECTION>\n</BODY>\n</HTML>";
    assert_eq!(
        html_line(html, "html[1]/body[1]/section[1]/h1[1]"),
        Some(line_of(html, "<H1>"))
    );
}

#[test]
fn url_mapping_is_relative_to_the_project_root() {
    let root = TempDir::new("inspect-source").expect("tempdir");
    std::fs::create_dir_all(root.path().join("assets")).expect("assets dir");
    std::fs::create_dir_all(root.path().join("deep")).expect("deep dir");
    std::fs::write(root.path().join("deep/index.html"), "").expect("deep index");
    std::fs::write(root.path().join("index.html"), "").expect("index");
    std::fs::write(root.path().join("styles.css"), "").expect("css");
    std::fs::write(root.path().join("assets/logo.png"), "").expect("png");

    assert_eq!(
        document_file("http://127.0.0.1:17421/", root.path()).as_deref(),
        Some("index.html")
    );
    assert_eq!(
        document_file("http://127.0.0.1:17421/deep/", root.path()).as_deref(),
        Some("deep/index.html")
    );
    assert_eq!(
        asset_file("http://127.0.0.1:17421/", "/styles.css", root.path()).as_deref(),
        Some("styles.css")
    );
    assert_eq!(
        asset_file("http://127.0.0.1:17421/", "assets/logo.png", root.path()).as_deref(),
        Some("assets/logo.png")
    );
    assert_eq!(
        asset_file(
            "http://127.0.0.1:17421/",
            "https://cdn.example/x.css",
            root.path()
        ),
        None
    );
    assert_eq!(
        asset_file("http://127.0.0.1:17421/", "/missing.css", root.path()),
        None
    );
}

#[test]
fn dev_server_source_paths_and_fs_paths_map_to_project_files() {
    let root = TempDir::new("inspect-vite-source").expect("tempdir");
    std::fs::create_dir_all(root.path().join("src")).expect("src dir");
    std::fs::create_dir_all(root.path().join("my dir")).expect("spaced dir");
    std::fs::write(root.path().join("src/app.css"), "").expect("app.css");
    std::fs::write(root.path().join("my dir/app.css"), "").expect("spaced css");
    let page = "http://127.0.0.1:7421/";
    // Vite serves a transformed source file at its source path.
    assert_eq!(
        asset_file(page, "/src/app.css", root.path()).as_deref(),
        Some("src/app.css")
    );
    assert_eq!(
        asset_file(
            page,
            "http://127.0.0.1:7421/src/app.css?direct",
            root.path()
        )
        .as_deref(),
        Some("src/app.css")
    );
    // `/@fs/` carries an absolute path; the file is root-relative when it is under the root.
    let fs_url = format!("{page}@fs{}", root.path().join("src/app.css").display());
    assert_eq!(
        asset_file(page, &fs_url, root.path()).as_deref(),
        Some("src/app.css")
    );
    // Percent-encoded spaces decode before the file is checked.
    assert_eq!(
        asset_file(page, "/my%20dir/app.css", root.path()).as_deref(),
        Some("my dir/app.css")
    );
    // A Next static chunk is not a source file: the entry keeps its URL.
    assert_eq!(
        asset_file(page, "/_next/static/css/x.css", root.path()),
        None
    );
    // `@fs` outside the root stays unmapped.
    let outside = TempDir::new("inspect-vite-outside").expect("tempdir");
    std::fs::write(outside.path().join("x.css"), "").expect("outside css");
    let outside_url = format!("{page}@fs{}", outside.path().join("x.css").display());
    assert_eq!(asset_file(page, &outside_url, root.path()), None);
}

#[test]
fn component_refs_are_root_relative_file_line_col() {
    assert_eq!(
        component_ref("src/App.tsx:12:5"),
        Some(("src/App.tsx".to_owned(), 12, 5))
    );
    assert_eq!(
        component_ref("./src/pages/Hero.vue:3:1"),
        Some(("src/pages/Hero.vue".to_owned(), 3, 1))
    );
    // Malformed or out-of-contract values fall back to the document/CSS entries.
    assert_eq!(component_ref("src/App.tsx"), None);
    assert_eq!(component_ref("src/App.tsx:0:1"), None);
    assert_eq!(component_ref("src/App.tsx:3:0"), None);
    assert_eq!(component_ref("/abs/App.tsx:3:1"), None);
    assert_eq!(component_ref("../outside/App.tsx:3:1"), None);
    assert_eq!(component_ref(":3:1"), None);
    assert_eq!(component_ref("src/App.tsx:three:1"), None);
}

#[test]
fn base64_decoding_round_trips_a_png_header() {
    assert_eq!(decode_base64("aGVsbG8=").expect("hello"), b"hello");
    assert_eq!(
        decode_base64("iVBORw0KGgo=").expect("png magic"),
        [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
    );
    assert!(decode_base64("aGV*sbG8=").is_err());
}
