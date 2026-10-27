//! The JSON `/pages`, `/notebooks`, `/health`, history and restore routes on the op-log server: auth, CRUD that
//! reaches a tablet over `/sync`, the "never the last page" rules, restore errors (404 / 410).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::similar_names
)]

mod common;

use std::time::Duration;

use common::{TestServer, converge, line};
use reqwest::{Client, Method, StatusCode};
use serde_json::{Value, json};

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

struct Api<'a> {
    server: &'a TestServer,
    client: Client,
}

impl<'a> Api<'a> {
    fn new(server: &'a TestServer) -> Self {
        Self {
            server,
            client: Client::new(),
        }
    }

    async fn send(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let url = format!("http://{}{path}", self.server.addr);
        let mut request = self
            .client
            .request(method, url)
            .header("X-Penwise-Token", common::TOKEN);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.expect("request");
        let status = response.status();
        let text = response.text().await.expect("body");
        let json = serde_json::from_str(&text).unwrap_or(Value::String(text));
        (status, json)
    }

    async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.send(Method::GET, path, None).await
    }

    async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.send(Method::POST, path, Some(body)).await
    }
}

#[tokio::test]
async fn routes_need_the_token_except_pairing() {
    let server = TestServer::start().await;
    let client = Client::new();
    let base = format!("http://{}", server.addr);
    for path in ["/pages", "/health", "/pair/code", "/pair.png"] {
        let r = client.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
    let wrong = client
        .get(format!("{base}/pages"))
        .header("X-Penwise-Token", "nope")
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    let by_query = client
        .get(format!("{base}/pages?token={}", common::TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(by_query.status(), StatusCode::OK);
    let bad_code = client
        .post(format!("{base}/pair"))
        .json(&json!({"code": "ZZZZZZ"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        bad_code.status(),
        StatusCode::UNAUTHORIZED,
        "reached the handler, not the middleware"
    );
    assert_eq!(bad_code.text().await.unwrap(), "bad pairing code");
}

#[tokio::test]
async fn catalog_crud_over_http_reaches_a_tablet() {
    let server = TestServer::start().await;
    let api = Api::new(&server);
    let mut tablet = server.tablet(1);
    tablet.online().await.expect("online");

    let (status, health) = api.get("/health").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an empty library is healthy: {health}"
    );
    assert_eq!(health["pages"], 0);
    let (status, empty) = api.get("/pages").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty["notebooks"], json!([]));
    assert_eq!(empty["active"], "");

    let (status, created) = api.post("/pages", json!({"title": "First"})).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let first = created["page"]["id"].as_str().unwrap().to_owned();
    assert_eq!(created["page"]["kind"]["t"], "fixed");
    assert_eq!(created["page"]["background"], "blank");
    let (_, second) = api
        .post(
            "/pages",
            json!({"background": "ruled", "kind": {"t": "infinite"}}),
        )
        .await;
    let second_id = second["page"]["id"].as_str().unwrap().to_owned();
    assert_eq!(second["page"]["title"], "Page 2", "next free title");
    assert_eq!(
        second["meta"]["notebooks"].as_array().unwrap().len(),
        1,
        "both pages share the default notebook"
    );

    let (status, patched) = api
        .send(
            Method::PATCH,
            &format!("/pages/{first}"),
            Some(json!({"title": "Renamed", "background": "dots"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{patched}");
    assert_eq!(patched["page"]["title"], "Renamed");
    assert_eq!(patched["page"]["background"], "dots");

    let (status, selected) = api
        .post(&format!("/pages/{second_id}/select"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(selected["active"], second_id);
    assert_eq!(api.get("/pages").await.1["active"], second_id);

    let (_, notebook) = api.post("/notebooks", json!({"title": "Work"})).await;
    let nb = notebook["notebook"]["id"].as_str().unwrap().to_owned();
    let (status, _) = api
        .send(
            Method::PATCH,
            &format!("/notebooks/{nb}"),
            Some(json!({"title": "Projects"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = api
        .send(Method::DELETE, &format!("/notebooks/{nb}"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "empty notebooks can go");

    let (status, gone) = api
        .send(Method::DELETE, &format!("/pages/{second_id}"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(
        gone["meta"]["active"], first,
        "the current page fell back to a live one"
    );
    let (status, body) = api
        .send(Method::DELETE, &format!("/pages/{first}"), None)
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "never the last page: {body}"
    );
    let (status, _) = api
        .send(
            Method::DELETE,
            &format!("/notebooks/{}", first_notebook(&api).await),
            None,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "last / non-empty notebook stays"
    );

    let fp = converge(&server, &mut [&mut tablet]).await;
    assert_eq!(fp.items.len(), 0);
    let page = first.parse().unwrap();
    let seen = tablet.replica.page(page);
    assert!(
        seen.is_some(),
        "the tablet replicated the HTTP-created page"
    );
}

async fn first_notebook(api: &Api<'_>) -> String {
    api.get("/pages").await.1["notebooks"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn malformed_and_unknown_ids_are_400_and_404() {
    let server = TestServer::start().await;
    let api = Api::new(&server);
    api.post("/pages", json!({})).await;
    assert_eq!(
        api.get("/health?page=not-an-id").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        api.get("/health?page=pg_0123456789abcdef").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.get("/health?page=00000000000000000000000001").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.post("/pages/00000000000000000000000001/select", json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.post("/pages", json!({"title": "  "})).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        api.post(
            "/annotate",
            json!({"page": "00000000000000000000000001", "strokes": []})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (status, health) = api.get("/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["pages"], 1);
    assert_eq!(health["strokes"], 0);
}

#[tokio::test]
async fn history_lists_batches_and_restore_below_the_floor_is_gone() {
    let server = TestServer::start().await;
    let api = Api::new(&server);
    let mut a = server.tablet(1);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    a.draw(page, &line(10.0, 10.0, 10));
    converge(&server, &mut [&mut a]).await;
    let early = server.head();
    a.draw(page, &line(10.0, 60.0, 10));
    converge(&server, &mut [&mut a]).await;

    let (status, history) = api.get(&format!("/pages/{page}/history")).await;
    assert_eq!(status, StatusCode::OK, "{history}");
    let batches = history["batches"].as_array().unwrap();
    assert!(batches.len() >= 2, "{history}");
    assert!(
        batches[0]["last_seq"].as_u64() > batches[1]["last_seq"].as_u64(),
        "newest first"
    );
    assert_eq!(
        api.post(&format!("/pages/{page}/restore"), json!({}))
            .await
            .0,
        StatusCode::BAD_REQUEST,
        "to_seq is required"
    );

    // Everything is 8 days old; the tablet keeps syncing, so it holds nothing back and the log is compacted.
    server.advance(8 * DAY_MS);
    a.offline();
    a.online().await.expect("refresh last seen");
    a.draw(page, &line(10.0, 110.0, 10));
    a.settle(Duration::from_secs(10)).await.expect("settle");
    server.storage.compact().await.expect("compact");
    let floor = server.storage.status().await.unwrap().log_floor;
    assert!(
        floor > early,
        "compaction raised the floor above {early}: {floor}"
    );
    let (status, body) = api
        .post(&format!("/pages/{page}/restore?to_seq={early}"), json!({}))
        .await;
    assert_eq!(status, StatusCode::GONE, "{body}");
}
