//! End-to-end inspect flow against a real headless Chromium: a fixture site is served on a high loopback port,
//! then `element_at` must report the heading's `index.html` line and its `styles.css` rule line, and `snapshot`
//! must return a PNG of the requested viewport.
//!
//! Skipped (with a message) when Chromium is not installed; `PENWISE_CHROMIUM` overrides the path.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use penwise_server::inspect::{ElementInfo, Inspect, SourceLocation};

const HTML: &str = include_str!("fixtures/site/index.html");
const CSS: &str = include_str!("fixtures/site/styles.css");

/// Skipped unless a Chromium binary exists.
fn chromium() -> Option<PathBuf> {
    let path = std::env::var_os("PENWISE_CHROMIUM")
        .map_or_else(|| PathBuf::from("/usr/bin/chromium"), PathBuf::from);
    if path.is_file() {
        Some(path)
    } else {
        use std::io::Write as _;
        let _ = writeln!(
            std::io::stderr(),
            "skipping: no chromium at {} (set PENWISE_CHROMIUM)",
            path.display()
        );
        None
    }
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/site")
}

fn line_of(text: &str, needle: &str) -> u32 {
    u32::try_from(
        text.lines()
            .position(|line| line.contains(needle))
            .expect("needle is in the fixture")
            + 1,
    )
    .expect("line fits")
}

fn has_source(source: &[SourceLocation], file: &str, line: u32, rule: Option<&str>) -> bool {
    source.iter().any(|entry| {
        entry.file.as_deref() == Some(file)
            && entry.line == Some(line)
            && entry.rule.as_deref() == rule
    })
}

fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(
        &png[..8],
        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
        "png signature"
    );
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    (width, height)
}

async fn start_fixture() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .fallback(get(fixture_file))
        .with_state(fixture_root());
    for port in 17_421..=17_520_u16 {
        if let Ok(listener) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            let addr = listener.local_addr().expect("fixture address");
            let server = tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            return (addr, server);
        }
    }
    panic!("no free port in 17421..=17520");
}

async fn fixture_file(State(root): State<PathBuf>, uri: Uri) -> Response {
    let relative = uri.path().trim_start_matches('/');
    let relative = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    let path: &Path = &root.join(relative);
    match tokio::fs::read(path).await {
        Ok(body) => {
            let content_type = if Path::new(relative)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("css"))
            {
                "text/css; charset=utf-8"
            } else {
                "text/html; charset=utf-8"
            };
            ([(header::CONTENT_TYPE, content_type)], body).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

#[tokio::test]
async fn element_at_maps_the_heading_to_html_and_css_lines_and_snapshot_is_sized() {
    let Some(chromium) = chromium() else {
        return;
    };
    let (addr, server) = start_fixture().await;
    let inspect = Inspect::with_chromium(chromium);
    let root = fixture_root();
    let url = format!("http://{addr}/");
    let mut page = inspect
        .open(&url, 800, 600)
        .await
        .expect("fixture page opens");
    page.set_root(Some(root));

    // Point lookup first: it must request the DOM document itself (no selector call has run yet).
    // The heading is 600px wide with short text; (590, 40) is inside its box, outside the inline text.
    let hit = inspect
        .element_at(&page, 590.0, 40.0)
        .await
        .expect("element at the heading's padding");
    assert_eq!(hit.tag, "h1");
    let html_line = line_of(HTML, "<h1");
    let css_line = line_of(CSS, ".title {");
    assert!(
        has_source(&hit.source, "index.html", html_line, None),
        "{:#?}",
        hit.source
    );
    assert!(
        has_source(&hit.source, "styles.css", css_line, Some(".title")),
        "{:#?}",
        hit.source
    );

    let heading: ElementInfo = inspect.element(&page, "h1.title").await.expect("heading");
    assert_eq!(heading.tag, "h1");
    assert_eq!(heading.role.as_deref(), Some("heading"));
    assert_eq!(
        heading.styles.get("font-size").map(String::as_str),
        Some("48px")
    );
    assert!(heading.text.starts_with("Fixture heading"));
    assert!(heading.outer_html.len() <= 2048);
    assert_eq!(heading.url, url);
    assert!(
        has_source(&heading.source, "index.html", html_line, None),
        "{:#?}",
        heading.source
    );
    assert!(
        has_source(&heading.source, "styles.css", css_line, Some(".title")),
        "{:#?}",
        heading.source
    );

    // The reported selector is unique and resolves back to the same element.
    let again = inspect
        .element(&page, &hit.selector)
        .await
        .expect("reported selector resolves");
    assert_eq!(again.tag, "h1");
    assert_eq!(again.rect, hit.rect);

    let png = inspect.snapshot(&page, None).await.expect("snapshot");
    assert_eq!(png_size(&png), (800, 600));

    page.close().await;
    inspect.shutdown().await;
    server.abort();
}
