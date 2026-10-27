use penwise_core::edit::Edit;
use penwise_core::item::Layer;
use penwise_proto::ItemRef;
use serde_json::{Value, json};

use super::*;
use crate::testing::Rig;

fn anchor(selector: &str, rect: [f32; 4]) -> Value {
    json!({"selector": selector, "rect": rect, "role": "button", "text": "Get started", "url": "https://example.com/"})
}

fn dot(x: f32, y: f32) -> Vec<[f32; 2]> {
    vec![[x - 3.0, y], [x, y + 2.0], [x + 3.0, y]]
}

async fn put(
    client: &reqwest::Client,
    base: &str,
    page: impl std::fmt::Display,
    body: Value,
) -> reqwest::Response {
    client
        .post(format!("{base}/pages/{page}/anchors"))
        .json(&body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn anchors_round_trip_per_page_and_a_write_replaces_the_map() {
    let mut rig = Rig::new();
    let first = rig.fixed_page().await;
    let second = rig.fixed_page().await;
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();

    let stored: Value = put(
        &client,
        &base,
        first,
        json!({"anchors": [anchor(".nav", [10.0, 20.0, 300.0, 120.0]), anchor("#cta", [10.0, 20.0, 300.0, 120.0])]}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(2, stored["stored"]);
    let listed: Value = client
        .get(format!("{base}/pages/{first}/anchors"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let selectors: Vec<&str> = listed["anchors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["selector"].as_str().unwrap())
        .collect();
    assert_eq!(
        vec!["#cta", ".nav"],
        selectors,
        "ordered by selector ('#' before '.')"
    );
    assert_eq!(
        json!([10.0, 20.0, 300.0, 120.0]),
        listed["anchors"][0]["rect"]
    );
    assert_eq!("button", listed["anchors"][0]["role"]);
    let other: Value = client
        .get(format!("{base}/pages/{second}/anchors"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        other["anchors"].as_array().unwrap().is_empty(),
        "maps are per page"
    );

    // Replacing drops the old map; malformed anchors are skipped.
    let stored: Value = put(
        &client,
        &base,
        first,
        json!({"anchors": [anchor(".only", [0.0, 0.0, 5.0, 5.0]), anchor("", [0.0, 0.0, 1.0, 1.0]), anchor(&"x".repeat(2000), [0.0, 0.0, 1.0, 1.0])]}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(1, stored["stored"]);
    let listed: Value = client
        .get(format!("{base}/pages/{first}/anchors"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(1, listed["anchors"].as_array().unwrap().len());
}

#[tokio::test]
async fn bad_anchor_requests_are_refused() {
    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();
    let many: Vec<Value> = (0..=MAX_ANCHORS)
        .map(|i| anchor(&format!(".a{i}"), [0.0, 0.0, 1.0, 1.0]))
        .collect();
    assert_eq!(
        400,
        put(&client, &base, page, json!({"anchors": many}))
            .await
            .status()
            .as_u16()
    );
    let missing = ItemRef::from_parts(1, 1);
    assert_eq!(
        404,
        put(&client, &base, missing, json!({"anchors": []}))
            .await
            .status()
            .as_u16()
    );
    assert_eq!(
        400,
        put(&client, &base, "nonsense", json!({"anchors": []}))
            .await
            .status()
            .as_u16()
    );
}

#[tokio::test]
async fn marks_are_attributed_to_the_innermost_element_by_item_id() {
    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    let in_button = rig.stroke(page, Layer::Ink, 1, &dot(150.0, 150.0)).await;
    let agent_in_button = rig.stroke(page, Layer::Agent, 2, &dot(160.0, 150.0)).await;
    let in_card = rig.stroke(page, Layer::Ink, 3, &dot(60.0, 60.0)).await;
    let outside = rig.stroke(page, Layer::Ink, 4, &dot(900.0, 900.0)).await;
    let doomed = rig.stroke(page, Layer::Ink, 5, &dot(155.0, 155.0)).await;
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();
    put(
        &client,
        &base,
        page,
        json!({"anchors": [anchor(".card", [0.0, 0.0, 400.0, 400.0]), anchor(".card .btn", [100.0, 100.0, 100.0, 100.0])]}),
    )
    .await;
    let read = |client: &reqwest::Client| {
        let url = format!("{base}/pages/{page}/annotations");
        let client = client.clone();
        async move {
            client
                .get(url)
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        }
    };
    let ids = |element: &Value| -> Vec<String> {
        let mut ids: Vec<String> = element["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.as_str().unwrap().to_owned())
            .collect();
        ids.sort();
        ids
    };
    let text = |id| ItemRef::from(id).to_string();

    let body = read(&client).await;
    let (card, button) = (&body["elements"][0], &body["elements"][1]);
    assert_eq!(".card", card["selector"]);
    assert_eq!(vec![text(in_card)], ids(card));
    let mut want = vec![text(in_button), text(agent_in_button), text(doomed)];
    want.sort();
    assert_eq!(want, ids(button), "the innermost element wins");
    assert_eq!(3, button["strokes"]);
    assert_eq!(
        1, body["unanchored"],
        "{outside:?} is outside every element"
    );

    // Re-attribution follows the items: an erased mark leaves its element, a moved one changes element.
    rig.edit(page, Edit::Delete(doomed)).await;
    let body = read(&client).await;
    assert_eq!(2, body["elements"][1]["strokes"]);
    assert_eq!(1, body["unanchored"]);
    rig.edit(page, Edit::Delete(outside)).await;
    assert_eq!(0, read(&client).await["unanchored"]);
}

#[tokio::test]
async fn the_pre_rewrite_element_map_is_picked_up_through_the_page_alias() {
    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    {
        let conn = rusqlite::Connection::open(rig.dir.path().join(crate::serve::DB_FILE)).unwrap();
        conn.execute_batch(
            "CREATE TABLE anchors (page TEXT NOT NULL, selector TEXT NOT NULL, rect TEXT NOT NULL, role TEXT, text TEXT, url TEXT, stale INTEGER NOT NULL DEFAULT 0);
             INSERT INTO anchors (page, selector, rect, role) VALUES ('pg_old', '.hero', '[1,2,3,4]', 'banner');",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO id_alias (legacy, kind, id) VALUES ('pg_old', 1, ?1)",
            [&page.0[..]],
        )
        .unwrap();
    }
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();
    let listed: Value = client
        .get(format!("{base}/pages/pg_old/anchors"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        page.to_string(),
        listed["page"],
        "the alias resolves to the new id"
    );
    assert_eq!(".hero", listed["anchors"][0]["selector"]);
    assert_eq!(json!([1.0, 2.0, 3.0, 4.0]), listed["anchors"][0]["rect"]);
    // Once replaced, the legacy rows do not come back.
    put(&client, &base, page, json!({"anchors": []})).await;
    let listed: Value = client
        .get(format!("{base}/pages/{page}/anchors"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(listed["anchors"].as_array().unwrap().is_empty());
}
