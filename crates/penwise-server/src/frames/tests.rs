//! Real-socket tests for the project ports: static serving and injection, live reload, proxying (HTML, assets,
//! gzip, WebSocket upgrades) and the token rule.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use std::io::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::header;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::response::{Html, IntoResponse, Response as AxumResponse};
use axum::routing::get;
use flate2::Compression;
use flate2::write::GzEncoder;
use futures_util::{SinkExt as _, StreamExt as _};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_util::rt::TokioIo;
use penwise_testkit::TempDir;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsFrame;

use super::inject::{decode, inject};
use super::{ProjectConfig, ProjectServer, Target, parse_projects};

const TOKEN: &str = "test-token";
const BRIDGE_TAG: &str = "<script src=\"/__penwise/bridge.js\"></script>";
const INDEX: &str = "<!doctype html><html><head><title>Penwise</title></head><body><h1>portfolio</h1></body></html>";
const APP_JS: &str = "console.log(\"hello from the project\");";

fn write_site(dir: &Path) {
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("index.html"), INDEX).unwrap();
    std::fs::write(
        dir.join("sub/index.html"),
        "<html><head><title>sub</title></head><body>sub page</body></html>",
    )
    .unwrap();
    std::fs::write(dir.join("app.js"), APP_JS).unwrap();
}

async fn static_project(dir: &Path) -> ProjectServer {
    ProjectServer::start(
        ProjectConfig::static_site("portfolio", "127.0.0.1", 0, dir),
        TOKEN.to_owned(),
    )
    .await
    .unwrap()
}

/// A tiny dev server: HTML, JS, gzip-encoded HTML and a WebSocket echo.
async fn upstream() -> (SocketAddr, JoinHandle<()>) {
    let app = Router::new()
        .route("/", get(|| async { Html(INDEX) }))
        .route(
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    APP_JS,
                )
            }),
        )
        .route("/packed", get(packed))
        .route("/echo", get(echo))
        .route("/_next/webpack-hmr", get(next_hmr))
        .route("/vite", get(vite_hmr));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (addr, task)
}

async fn packed() -> AxumResponse {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(INDEX.as_bytes()).unwrap();
    let body = encoder.finish().unwrap();
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_ENCODING, "gzip"),
        ],
        body,
    )
        .into_response()
}

async fn echo(upgrade: WebSocketUpgrade) -> AxumResponse {
    upgrade.on_upgrade(|mut socket| async move {
        while let Some(Ok(message)) = socket.recv().await {
            if socket.send(message).await.is_err() {
                break;
            }
        }
    })
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

/// A GET over a real socket; the `Host` header comes from the loopback address, so no token is needed.
async fn fetch(addr: SocketAddr, path: &str) -> Reply {
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("http://{addr}{path}"))
        .header(header::HOST, addr.to_string())
        .body(Full::new(Bytes::new()))
        .unwrap();
    send(addr, request).await
}

async fn send(addr: SocketAddr, request: Request<Full<Bytes>>) -> Reply {
    let stream = TcpStream::connect(addr).await.unwrap();
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let response = sender.send_request(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    Reply {
        status,
        headers,
        body,
    }
}

/// A hand-written request, so the test controls `Host` exactly (the auth rule depends on it).
async fn raw(addr: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    stream.read_to_end(&mut out).await.unwrap();
    String::from_utf8_lossy(&out).into_owned()
}

async fn wait_for_reload_listener(project: &ProjectServer) {
    for _ in 0..200 {
        if project.reload_listeners() > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the reload socket never subscribed");
}

#[tokio::test]
async fn static_port_serves_injected_pages_and_plain_assets() {
    let dir = TempDir::new("frames-static").unwrap();
    write_site(dir.path());
    let project = static_project(dir.path()).await;
    let addr = project.addr;

    let index = fetch(addr, "/").await;
    assert_eq!(index.status, StatusCode::OK);
    assert_eq!(
        index.headers.get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
    assert_eq!(
        index.headers.get(header::CACHE_CONTROL).unwrap(),
        "no-cache",
        "Chromium keeps the document for source mapping; no validators means it still refetches"
    );
    let html = String::from_utf8(index.body).unwrap();
    assert!(html.contains(BRIDGE_TAG), "{html}");
    assert!(html.contains("<h1>portfolio</h1>"), "{html}");

    let sub = fetch(addr, "/sub/").await;
    assert_eq!(sub.status, StatusCode::OK);
    let sub = String::from_utf8(sub.body).unwrap();
    assert!(
        sub.contains("sub page") && sub.contains(BRIDGE_TAG),
        "{sub}"
    );

    let js = fetch(addr, "/app.js").await;
    assert_eq!(js.status, StatusCode::OK);
    assert_eq!(js.body, APP_JS.as_bytes());
    assert_eq!(
        js.headers.get(header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8"
    );

    let bridge = fetch(addr, "/__penwise/bridge.js").await;
    assert_eq!(bridge.status, StatusCode::OK);
    let bridge = String::from_utf8(bridge.body).unwrap();
    assert!(bridge.contains("pw:hello"), "{bridge}");
    assert!(bridge.contains("PenwiseHost"), "{bridge}");

    assert_eq!(
        fetch(addr, "/missing.html").await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fetch(addr, "/..%2f..%2fetc/passwd").await.status,
        StatusCode::NOT_FOUND
    );
    let post = raw(
        addr,
        "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(post.starts_with("HTTP/1.1 405"), "{post}");
    project.shutdown().await;
}

#[tokio::test]
async fn a_file_change_tells_open_pages_to_reload() {
    let dir = TempDir::new("frames-reload").unwrap();
    write_site(dir.path());
    let project = static_project(dir.path()).await;
    let (mut socket, _) = connect_async(format!("ws://{}/__penwise/reload", project.addr))
        .await
        .unwrap();
    wait_for_reload_listener(&project).await;

    std::fs::write(dir.path().join("app.js"), "console.log(\"edited\");").unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .expect("a reload within five seconds")
        .expect("a websocket frame")
        .expect("a text frame");
    match frame {
        WsFrame::Text(text) => assert_eq!(text.as_str(), "{\"t\":\"pw:reload\"}"),
        other => panic!("unexpected frame {other:?}"),
    }
    project.shutdown().await;
}

#[tokio::test]
async fn proxy_injects_html_and_passes_assets_and_gzip_through() {
    let (origin, upstream_task) = upstream().await;
    let project = ProjectServer::start(
        ProjectConfig::dev_origin("vite", "127.0.0.1", 0, format!("http://{origin}")),
        TOKEN.to_owned(),
    )
    .await
    .unwrap();
    let addr = project.addr;

    let root = fetch(addr, "/").await;
    assert_eq!(root.status, StatusCode::OK);
    let html = String::from_utf8(root.body).unwrap();
    assert!(html.contains(BRIDGE_TAG), "{html}");
    assert!(html.contains("<h1>portfolio</h1>"), "{html}");
    let declared: usize = root
        .headers
        .get(header::CONTENT_LENGTH)
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(declared, html.len());

    let js = fetch(addr, "/app.js").await;
    assert_eq!(js.status, StatusCode::OK);
    assert_eq!(js.body, APP_JS.as_bytes());
    assert_eq!(
        js.headers.get(header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8"
    );

    let packed = fetch(addr, "/packed").await;
    assert_eq!(packed.status, StatusCode::OK);
    assert!(packed.headers.get(header::CONTENT_ENCODING).is_none());
    let packed = String::from_utf8(packed.body).unwrap();
    assert!(packed.contains(BRIDGE_TAG), "{packed}");
    assert!(packed.contains("<h1>portfolio</h1>"), "{packed}");

    project.shutdown().await;
    upstream_task.abort();
}

#[tokio::test]
async fn proxy_tunnels_websocket_frames() {
    let (origin, upstream_task) = upstream().await;
    let project = ProjectServer::start(
        ProjectConfig::dev_origin("vite", "127.0.0.1", 0, format!("http://{origin}")),
        TOKEN.to_owned(),
    )
    .await
    .unwrap();

    let (mut socket, _) = connect_async(format!("ws://{}/echo", project.addr))
        .await
        .unwrap();
    socket.send(WsFrame::Text("hmr".into())).await.unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .expect("the echo within five seconds")
        .expect("a websocket frame")
        .expect("a text frame");
    match frame {
        WsFrame::Text(text) => assert_eq!(text.as_str(), "hmr"),
        other => panic!("unexpected frame {other:?}"),
    }

    project.shutdown().await;
    upstream_task.abort();
}

/// Next's HMR socket says `sync` to every page that connects, then echoes what it is sent (so a test can make
/// the "server" announce a rebuild).
async fn next_hmr(upgrade: WebSocketUpgrade) -> AxumResponse {
    upgrade.on_upgrade(|mut socket| async move {
        if socket
            .send(axum::extract::ws::Message::Text(
                "{\"action\":\"sync\",\"hash\":\"abc\"}".into(),
            ))
            .await
            .is_err()
        {
            return;
        }
        while let Some(Ok(message)) = socket.recv().await {
            if socket.send(message).await.is_err() {
                break;
            }
        }
    })
}

/// Vite's HMR socket: the `vite-hmr` subprotocol, echoing.
async fn vite_hmr(upgrade: WebSocketUpgrade) -> AxumResponse {
    upgrade
        .protocols(["vite-hmr"])
        .on_upgrade(|mut socket| async move {
            while let Some(Ok(message)) = socket.recv().await {
                if socket.send(message).await.is_err() {
                    break;
                }
            }
        })
}

/// Connects to `ws://addr/path`, with `headers`, and reads the greeting when `greeted`.
async fn hmr_client(
    addr: SocketAddr,
    path: &str,
    headers: &[(&str, &str)],
    greeted: bool,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
    let mut request = format!("ws://{addr}{path}").into_client_request().unwrap();
    for (name, value) in headers {
        request.headers_mut().insert(
            axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
    }
    let (mut socket, _) = connect_async(request).await.unwrap();
    if greeted {
        socket.next().await.unwrap().unwrap();
    }
    socket
}

async fn say(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
    text: &str,
) {
    socket
        .send(WsFrame::Text(text.to_owned().into()))
        .await
        .unwrap();
    socket.next().await.unwrap().unwrap();
}

#[tokio::test]
async fn only_a_persons_hmr_socket_announcing_a_rebuild_is_a_page_change() {
    let (origin, upstream_task) = upstream().await;
    let (changes, mut heard) = tokio::sync::broadcast::channel(8);
    let project = ProjectServer::start(
        ProjectConfig::dev_origin("vite", "127.0.0.1", 0, format!("http://{origin}"))
            .with_changes(changes),
        TOKEN.to_owned(),
    )
    .await
    .unwrap();
    let addr = project.addr;
    let rebuilt = "{\"action\":\"built\",\"hash\":\"def\"}";

    // Next greets every page that connects with `sync`: opening a page is not a change, however many do.
    let mut first = hmr_client(addr, "/_next/webpack-hmr", &[], true).await;
    let _second = hmr_client(addr, "/_next/webpack-hmr", &[], true).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(heard.try_recv().is_err(), "connecting clients fire nothing");
    say(&mut first, rebuilt).await;
    assert_eq!(heard.recv().await.unwrap(), "vite", "a rebuild is a change");

    // Vite's socket is recognised by its subprotocol.
    let mut vite = hmr_client(
        addr,
        "/vite",
        &[("sec-websocket-protocol", "vite-hmr")],
        false,
    )
    .await;
    say(&mut vite, "{\"type\":\"update\",\"updates\":[]}").await;
    assert_eq!(heard.recv().await.unwrap(), "vite");

    // A socket that is not an HMR socket is never read.
    let mut other = hmr_client(addr, "/echo", &[], false).await;
    say(&mut other, rebuilt).await;
    // A capture page's socket (the server's own headless Chromium) is never read either.
    let mut capture = hmr_client(
        addr,
        "/_next/webpack-hmr",
        &[(crate::inspect::CAPTURE_HEADER, "1")],
        true,
    )
    .await;
    say(&mut capture, rebuilt).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(heard.try_recv().is_err(), "neither fired anything");

    project.shutdown().await;
    upstream_task.abort();
}

#[tokio::test]
async fn a_client_reset_frees_the_dev_server_socket_too() {
    let (closed, upstream_closed) = tokio::sync::oneshot::channel::<()>();
    let closed = std::sync::Arc::new(std::sync::Mutex::new(Some(closed)));
    let app = Router::new().route(
        "/hold",
        get(move |upgrade: WebSocketUpgrade| {
            let closed = std::sync::Arc::clone(&closed);
            async move {
                upgrade.on_upgrade(move |mut socket| async move {
                    // Silent: it only ever reads, until its connection is gone.
                    while socket.recv().await.is_some_and(|message| message.is_ok()) {}
                    if let Some(sender) = closed.lock().unwrap().take() {
                        let _ = sender.send(());
                    }
                })
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = listener.local_addr().unwrap();
    let upstream_task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let project = ProjectServer::start(
        ProjectConfig::dev_origin("vite", "127.0.0.1", 0, format!("http://{origin}")),
        TOKEN.to_owned(),
    )
    .await
    .unwrap();

    let mut socket = hmr_client(project.addr, "/hold", &[], false).await;
    // Close with a reset (SO_LINGER 0), the way a killed tablet connection ends.
    if let tokio_tungstenite::MaybeTlsStream::Plain(tcp) = socket.get_mut() {
        #[allow(
            deprecated,
            reason = "tokio names it set_zero_linger in newer releases"
        )]
        tcp.set_linger(Some(Duration::ZERO)).unwrap();
    }
    drop(socket);
    tokio::time::timeout(Duration::from_secs(5), upstream_closed)
        .await
        .expect("the dev server's socket closes within five seconds of the client's reset")
        .unwrap();

    project.shutdown().await;
    upstream_task.abort();
}

#[tokio::test]
async fn a_static_file_change_announces_a_page_change() {
    let dir = TempDir::new("frames-changes").unwrap();
    write_site(dir.path());
    let (changes, mut heard) = tokio::sync::broadcast::channel(8);
    let project = ProjectServer::start(
        ProjectConfig::static_site("portfolio", "127.0.0.1", 0, dir.path()).with_changes(changes),
        TOKEN.to_owned(),
    )
    .await
    .unwrap();
    // The watcher thread needs a moment to arm before the edit.
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(dir.path().join("app.js"), "console.log(\"edited\");").unwrap();
    let project_id = tokio::time::timeout(Duration::from_secs(5), heard.recv())
        .await
        .expect("a change within five seconds")
        .expect("the feed is open");
    assert_eq!(project_id, "portfolio");
    project.shutdown().await;
}

#[tokio::test]
async fn token_rule_allows_loopback_and_header_but_not_remote_pages() {
    let dir = TempDir::new("frames-auth").unwrap();
    write_site(dir.path());
    let project = static_project(dir.path()).await;
    let addr = project.addr;

    // Loopback peer, loopback Host: the rule of `GET /pair/local`.
    assert_eq!(fetch(addr, "/").await.status, StatusCode::OK);

    let denied = raw(
        addr,
        "GET / HTTP/1.1\r\nHost: penwise.example:17421\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(denied.starts_with("HTTP/1.1 401"), "{denied}");

    let with_header = raw(
        addr,
        &format!(
            "GET / HTTP/1.1\r\nHost: penwise.example:17421\r\nX-Penwise-Token: {TOKEN}\r\n\
             Connection: close\r\n\r\n"
        ),
    )
    .await;
    assert!(with_header.starts_with("HTTP/1.1 200"), "{with_header}");
    assert!(with_header.contains(BRIDGE_TAG), "{with_header}");

    let wrong_cookie = raw(
        addr,
        "GET / HTTP/1.1\r\nHost: penwise.example:17421\r\nCookie: penwise_token=nope\r\n\
         Connection: close\r\n\r\n",
    )
    .await;
    assert!(wrong_cookie.starts_with("HTTP/1.1 401"), "{wrong_cookie}");

    // The bridge itself is public: a page cannot load it otherwise.
    let bridge = raw(
        addr,
        "GET /__penwise/bridge.js HTTP/1.1\r\nHost: penwise.example:17421\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(bridge.starts_with("HTTP/1.1 200"), "{bridge}");

    project.shutdown().await;
}

#[tokio::test]
async fn enter_sets_the_cookie_and_redirects_within_the_project() {
    let dir = TempDir::new("frames-enter").unwrap();
    write_site(dir.path());
    let project = static_project(dir.path()).await;
    let addr = project.addr;

    let enter = raw(
        addr,
        &format!(
            "GET /__penwise/enter?token={TOKEN}&to=/sub/ HTTP/1.1\r\nHost: penwise.example:17421\r\n\
             Connection: close\r\n\r\n"
        ),
    )
    .await;
    assert!(enter.starts_with("HTTP/1.1 303"), "{enter}");
    assert!(enter.contains("location: /sub/"), "{enter}");
    assert!(
        enter.contains(&format!("set-cookie: {}={TOKEN}", super::COOKIE)),
        "{enter}"
    );

    let page = raw(
        addr,
        &format!(
            "GET /sub/ HTTP/1.1\r\nHost: penwise.example:17421\r\nCookie: {}={TOKEN}\r\n\
             Connection: close\r\n\r\n",
            super::COOKIE
        ),
    )
    .await;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(page.contains(BRIDGE_TAG), "{page}");

    let bad = raw(
        addr,
        "GET /__penwise/enter?token=nope&to=/ HTTP/1.1\r\nHost: penwise.example:17421\r\n\
         Connection: close\r\n\r\n",
    )
    .await;
    assert!(bad.starts_with("HTTP/1.1 401"), "{bad}");

    // `to` may not point at another origin.
    let off_site = raw(
        addr,
        &format!(
            "GET /__penwise/enter?token={TOKEN}&to=//evil.example/ HTTP/1.1\r\n\
             Host: penwise.example:17421\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert!(off_site.contains("location: /\r\n"), "{off_site}");

    project.shutdown().await;
}

#[test]
fn injection_lands_in_head_then_body_then_at_the_start() {
    let head = String::from_utf8(inject(
        b"<html><head><title>t</title></head><body>hi</body></html>",
    ))
    .unwrap();
    assert!(head.contains(&format!("{BRIDGE_TAG}</head>")), "{head}");

    let body = String::from_utf8(inject(b"<html><body class=\"x\">hi</body></html>")).unwrap();
    let in_body = format!("<body class=\"x\">{BRIDGE_TAG}");
    assert!(body.contains(&in_body), "{body}");

    let bare = String::from_utf8(inject(b"just text")).unwrap();
    assert!(bare.starts_with(BRIDGE_TAG), "{bare}");
    assert!(bare.ends_with("just text"), "{bare}");

    let twice = String::from_utf8(inject(head.as_bytes())).unwrap();
    assert_eq!(twice.matches(BRIDGE_TAG).count(), 1, "{twice}");
}

#[test]
fn gzip_and_deflate_bodies_are_decoded_and_brotli_is_refused() {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(b"<html><head></head><body>gz</body></html>")
        .unwrap();
    let packed = encoder.finish().unwrap();
    let decoded = String::from_utf8(decode(Some("gzip"), &packed).unwrap()).unwrap();
    assert!(decoded.contains("<body>gz</body>"));
    assert!(decode(None, b"plain").is_some());
    assert!(decode(Some("br"), b"compressed").is_none());
}

#[test]
fn project_json_reads_static_and_origin_entries() {
    let projects = parse_projects(
        r#"[
            {"id": "portfolio", "port": 17421, "static": "/tmp/site"},
            {"port": 17422, "origin": "http://127.0.0.1:5173"}
        ]"#,
    )
    .unwrap();
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].id, "portfolio");
    assert_eq!(projects[0].port, 17421);
    assert_eq!(
        projects[0].target,
        Target::Static(PathBuf::from("/tmp/site"))
    );
    assert_eq!(projects[1].id, "p17422");
    assert_eq!(
        projects[1].target,
        Target::Origin("http://127.0.0.1:5173".to_owned())
    );

    assert!(parse_projects(r#"[{"port": 1}]"#).is_err());
    assert!(parse_projects(r#"[{"port": 1, "static": "/a", "origin": "http://b"}]"#).is_err());
}
