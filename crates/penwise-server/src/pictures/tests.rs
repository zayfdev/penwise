use penwise_core::item::{ItemKind, Layer, LayerSet};
use penwise_proto::{AnyOp, ItemRef};
use serde_json::{Value, json};

use super::*;
use crate::storage::OpFilter;
use crate::testing::Rig;

/// A PNG header (IHDR only) padded with zeros to `total` bytes: enough for the upload route to accept.
fn padded_png(total: usize) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&64u32.to_be_bytes());
    bytes.extend_from_slice(&32u32.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes.resize(total, 0);
    bytes
}

#[tokio::test]
async fn an_upload_over_the_json_limit_is_accepted_up_to_24_mib() {
    let rig = Rig::new();
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();
    let ok = client
        .post(format!("{base}/assets"))
        .body(padded_png(3 * 1024 * 1024))
        .send()
        .await
        .unwrap();
    assert_eq!(200, ok.status().as_u16(), "3 MiB fits the 24 MiB limit");
    let too_big = client
        .post(format!("{base}/assets"))
        .body(padded_png(assets::MAX_BYTES + 1024))
        .send()
        .await
        .unwrap();
    assert_eq!(413, too_big.status().as_u16(), "above 24 MiB is refused");
    let not_png = client
        .post(format!("{base}/assets"))
        .body("GIF89a")
        .send()
        .await
        .unwrap();
    assert_eq!(400, not_png.status().as_u16());
}

#[tokio::test]
async fn an_uploaded_asset_is_served_back_by_digest() {
    let rig = Rig::new();
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();
    let body = padded_png(100);
    let stored: Value = client
        .post(format!("{base}/assets"))
        .body(body.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = stored["asset"].as_str().unwrap();
    let got = client
        .get(format!("{base}/assets/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(200, got.status().as_u16());
    assert_eq!(body, got.bytes().await.unwrap().to_vec());
    let missing = client
        .get(format!("{base}/assets/{}", "0".repeat(64)))
        .send()
        .await
        .unwrap();
    assert_eq!(404, missing.status().as_u16());
}

#[tokio::test]
async fn a_picture_round_trips_through_ops_place_list_remove() {
    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    let ink = rig
        .stroke(page, Layer::Ink, 1, &[[10.0, 10.0], [90.0, 90.0]])
        .await;
    let stored = rig.picture(8, 4, [0, 0, 255, 255]);
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();
    let url = format!("{base}/pages/{page}/images");

    let placed: Value = client
        .post(&url)
        .json(&json!({"asset": stored.asset, "x": 120.0, "y": 80.0, "w": 400.0, "h": 200.0, "label": "hero"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = placed["id"].as_str().unwrap().to_owned();
    assert_eq!(Some("hero"), placed["label"].as_str());

    // The server device logged an Add(Image) op, and the scene carries it beneath the ink.
    let log = rig.storage.ops_since(0, OpFilter::All, 100).await.unwrap();
    let added = log
        .ops
        .iter()
        .filter_map(|seq| match &seq.op {
            AnyOp::Item(op) if op.device == rig.storage.server_device() => Some(op),
            _ => None,
        })
        .count();
    assert_eq!(1, added, "one server-authored item op");
    let view = rig.view(page).await;
    let scene = view.replica.scene();
    let item_id: ItemId = id.parse::<ItemRef>().unwrap().into();
    let item = scene.get(item_id).expect("live image item");
    assert!(matches!(&item.kind, ItemKind::Image(i) if assets::id_of(&i.asset) == stored.asset));
    assert!(
        (item.transform.e - 120.0).abs() < f32::EPSILON
            && (item.transform.f - 80.0).abs() < f32::EPSILON
    );
    assert!(
        item.z < scene.get(ink).unwrap().z,
        "pictures sit under the ink"
    );
    assert_eq!(2, scene.render_order(LayerSet::ALL).len());

    // Listing shows id, geometry and label; re-placing the same id is a no-op.
    let again: Value = client
        .post(&url)
        .json(&json!({"id": id, "asset": stored.asset, "x": 0.0, "y": 0.0, "w": 5.0, "h": 5.0}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(placed, again, "same id returns the existing picture");
    let listed: Value = client.get(&url).send().await.unwrap().json().await.unwrap();
    let images = listed["images"].as_array().unwrap();
    assert_eq!(1, images.len());
    assert_eq!(placed, images[0]);
    assert_eq!(page.to_string(), listed["page"].as_str().unwrap());

    // Remove: the item is tombstoned, the ink stays, the list is empty.
    let removed: Value = client
        .delete(&url)
        .json(&json!({"ids": [id, "01ARZ3NDEKTSV4RRFFQ69G5FAV"]}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(1, removed["removed"], "unknown ids are not counted");
    let view = rig.view(page).await;
    assert!(view.replica.scene().get(item_id).is_none());
    assert!(
        view.replica
            .scene()
            .get_any(item_id)
            .is_some_and(|(_, deleted)| deleted)
    );
    assert!(view.replica.scene().get(ink).is_some());
    let listed: Value = client.get(&url).send().await.unwrap().json().await.unwrap();
    assert!(listed["images"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn bad_placements_are_refused() {
    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    let stored = rig.picture(2, 2, [0, 0, 0, 255]);
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();
    let url = format!("{base}/pages/{page}/images");
    let status = |body: Value| {
        let (client, url) = (client.clone(), url.clone());
        async move {
            client
                .post(url)
                .json(&body)
                .send()
                .await
                .unwrap()
                .status()
                .as_u16()
        }
    };
    let ok = json!({"asset": stored.asset, "x": 0, "y": 0, "w": 10, "h": 10});
    let mut zero = ok.clone();
    zero["w"] = json!(0);
    assert_eq!(400, status(zero).await);
    let mut unknown = ok.clone();
    unknown["asset"] = json!("f".repeat(64));
    assert_eq!(404, status(unknown).await);
    let mut layer = ok.clone();
    layer["layer"] = json!("text");
    assert_eq!(400, status(layer).await);
    let mut bad_id = ok.clone();
    bad_id["id"] = json!("not-a-ulid");
    assert_eq!(400, status(bad_id).await);
    let missing_page = client
        .post(format!("{base}/pages/01ARZ3NDEKTSV4RRFFQ69G5FAV/images"))
        .json(&ok)
        .send()
        .await
        .unwrap();
    assert_eq!(404, missing_page.status().as_u16());
    assert_eq!(
        0,
        rig.view(page)
            .await
            .replica
            .scene()
            .render_order(LayerSet::ALL)
            .len()
    );
}

#[tokio::test]
async fn placing_the_id_of_a_deleted_picture_is_a_clear_conflict() {
    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    let stored = rig.picture(8, 4, [0, 0, 255, 255]);
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();
    let url = format!("{base}/pages/{page}/images");
    let body = json!({"asset": stored.asset, "x": 1.0, "y": 1.0, "w": 40.0, "h": 20.0});
    let placed: Value = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = placed["id"].as_str().unwrap().to_owned();
    client
        .delete(&url)
        .json(&json!({"ids": [id]}))
        .send()
        .await
        .unwrap();
    let again = client
        .post(&url)
        .json(&json!({"id": id, "asset": stored.asset, "x": 1.0, "y": 1.0, "w": 40.0, "h": 20.0}))
        .send()
        .await
        .unwrap();
    assert_eq!(409, again.status().as_u16());
    assert!(again.text().await.unwrap().contains("deleted"));
}
