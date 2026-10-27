#![allow(clippy::cast_precision_loss, reason = "test fixture coordinates")]

use penwise_core::item::{ItemKind, Layer};
use penwise_proto::ItemRef;
use serde_json::Value;

use super::*;
use crate::testing::{FakeModel, Rig};

#[test]
fn read_scale_is_bounded() {
    assert!((1.0 - read_scale(None)).abs() < f32::EPSILON);
    assert!((0.5 - read_scale(Some(0.01))).abs() < f32::EPSILON);
    assert!((2.0 - read_scale(Some(9.0))).abs() < f32::EPSILON);
    assert!((1.0 - read_scale(Some(f32::NAN))).abs() < f32::EPSILON);
}

fn wave(x: f32) -> Vec<[f32; 2]> {
    (0..30)
        .map(|i| [x + i as f32 * 5.0, 100.0 + (i as f32 * 0.4).sin() * 10.0])
        .collect()
}

async fn read(
    client: &reqwest::Client,
    base: &str,
    page: PageId,
    extra: &str,
) -> reqwest::Response {
    client
        .get(format!("{base}/text?page={page}{extra}"))
        .send()
        .await
        .unwrap()
}

fn live_texts(view: &PageView) -> Vec<(Layer, String, Vec<ItemId>)> {
    view.replica
        .items()
        .filter(|state| !state.deleted)
        .filter_map(|state| match &state.item.kind {
            ItemKind::Text(text) => {
                Some((state.item.layer, text.content.clone(), text.sources.clone()))
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn recognition_is_stored_as_a_text_layer_item_and_cached_by_its_sources() {
    let mut rig = Rig::new();
    let model = FakeModel::start("buy milk").await;
    rig.ocr_url = model.url.clone();
    let page = rig.fixed_page().await;
    let first = rig.stroke(page, Layer::Ink, 1, &wave(200.0)).await;
    let second = rig.stroke(page, Layer::Ink, 2, &wave(260.0)).await;
    rig.stroke(page, Layer::Agent, 3, &wave(500.0)).await;
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();

    let body: Value = read(&client, &base, page, "").await.json().await.unwrap();
    assert_eq!("buy milk", body["text"]);
    assert_eq!(false, body["cached"]);
    assert_eq!(1, model.calls());

    // The text layer item: server-authored, on the text layer, sourced from exactly the ink strokes.
    let view = rig.view(page).await;
    let texts = live_texts(&view);
    assert_eq!(1, texts.len());
    let mut sources = vec![first, second];
    sources.sort();
    assert_eq!((Layer::Text, "buy milk".to_owned(), sources), texts[0]);

    // Unchanged ink: answered from the item, no model call.
    let again: Value = read(&client, &base, page, "").await.json().await.unwrap();
    assert_eq!(
        (&Value::from("buy milk"), &Value::from(true)),
        (&again["text"], &again["cached"])
    );
    assert_eq!(1, model.calls());
    // An agent mark does not make the handwriting stale; refresh does.
    rig.stroke(page, Layer::Agent, 4, &wave(520.0)).await;
    read(&client, &base, page, "").await;
    assert_eq!(1, model.calls());
    read(&client, &base, page, "&refresh=true").await;
    assert_eq!(2, model.calls());
    assert_eq!(
        1,
        live_texts(&rig.view(page).await).len(),
        "a refresh replaces, never stacks"
    );
}

#[tokio::test]
async fn new_or_erased_ink_makes_the_stored_text_stale() {
    let mut rig = Rig::new();
    let model = FakeModel::start("one").await;
    rig.ocr_url = model.url.clone();
    let page = rig.fixed_page().await;
    let first = rig.stroke(page, Layer::Ink, 1, &wave(200.0)).await;
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();
    read(&client, &base, page, "").await;
    assert_eq!(1, model.calls());

    model.set("one two");
    let more = rig.stroke(page, Layer::Ink, 2, &wave(300.0)).await;
    let body: Value = read(&client, &base, page, "").await.json().await.unwrap();
    assert_eq!(
        (&Value::from("one two"), &Value::from(false)),
        (&body["text"], &body["cached"])
    );
    assert_eq!(2, model.calls(), "a new stroke is re-read");
    let texts = live_texts(&rig.view(page).await);
    assert_eq!(1, texts.len(), "the old recognition is tombstoned");
    assert_eq!(2, texts[0].2.len());

    rig.edit(page, Edit::Delete(first)).await;
    rig.edit(page, Edit::Delete(more)).await;
    let body: Value = read(&client, &base, page, "").await.json().await.unwrap();
    assert_eq!("", body["text"], "no ink, no text");
    assert_eq!(2, model.calls(), "an empty page needs no model");
    assert!(live_texts(&rig.view(page).await).is_empty());
}

#[tokio::test]
async fn typed_text_is_part_of_the_page_text_and_an_unreachable_model_is_a_503() {
    let mut rig = Rig::new();
    rig.ocr_url = "http://127.0.0.1:1".to_owned();
    let page = rig.fixed_page().await;
    let typed = Item {
        id: rig.item_id(),
        layer: Layer::Ink,
        z: 5,
        transform: Affine::translate(10.0, 10.0),
        kind: ItemKind::Text(TextItem {
            content: "typed note".into(),
            size: [100.0, 20.0],
            font_size: 16.0,
            paint: Paint {
                color: Color::INK,
                opacity: 1.0,
            },
            sources: Vec::new(),
        }),
    };
    rig.edit(page, Edit::Add(typed)).await;
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();
    let body: Value = read(&client, &base, page, "").await.json().await.unwrap();
    assert_eq!("typed note", body["text"], "no ink: no model needed");

    rig.stroke(page, Layer::Ink, 1, &wave(200.0)).await;
    let down = read(&client, &base, page, "").await;
    assert_eq!(503, down.status().as_u16());
    assert!(
        down.text()
            .await
            .unwrap()
            .contains("handwriting model unavailable")
    );
    assert!(
        live_texts(&rig.view(page).await)
            .iter()
            .all(|(layer, _, _)| *layer != Layer::Text),
        "nothing is stored when the model is down"
    );
    let unknown = client
        .get(format!("{base}/text?page={}", ItemRef::from_parts(1, 1)))
        .send()
        .await
        .unwrap();
    assert_eq!(404, unknown.status().as_u16());
}
