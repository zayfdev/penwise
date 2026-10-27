use penwise_core::brush::Color;
use penwise_core::edit::Edit;
use penwise_core::item::{Affine, Item, ItemKind, Layer, Paint, TextItem};
use penwise_proto::{Background, PageChange, PageKind, PagePatch};
use serde_json::Value;

use super::*;
use crate::testing::Rig;

#[test]
fn a_match_comes_back_with_context() {
    let text = "Shopping list\nmilk\neggs\nbread";
    let snippet = snippet_of(text, "EGGS").unwrap();
    assert!(snippet.contains("eggs"), "{snippet}");
    assert!(!snippet.contains('\n'), "collapsed to one line");
}

#[test]
fn a_missing_needle_has_no_snippet() {
    assert!(snippet_of("milk and eggs", "butter").is_none());
    assert!(snippet_of("", "milk").is_none());
}

#[test]
fn snippets_never_split_a_character() {
    let text = "héllo wörld with ümlauts and more ümlauts";
    let snippet = snippet_of(text, "wörld").unwrap();
    assert!(snippet.contains("wörld"));
}

#[test]
fn a_match_near_the_edges_is_clamped() {
    let text = "start of a page with the word apples at the very beginning";
    assert!(snippet_of(text, "apples").unwrap().contains("apples"));
    let tail = "the end says bananas";
    assert!(snippet_of(tail, "bananas").unwrap().ends_with("bananas"));
}

fn text_item(rig: &mut Rig, layer: Layer, at: [f32; 2], content: &str) -> Item {
    Item {
        id: rig.item_id(),
        layer,
        z: 1,
        transform: Affine::translate(at[0], at[1]),
        kind: ItemKind::Text(TextItem {
            content: content.to_owned(),
            size: [200.0, 40.0],
            font_size: 20.0,
            paint: Paint {
                color: Color::INK,
                opacity: 1.0,
            },
            sources: Vec::new(),
        }),
    }
}

async fn search(base: &str, q: &str) -> Value {
    reqwest::Client::new()
        .get(format!("{base}/search"))
        .query(&[("q", q)])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn search_reads_text_items_across_pages_and_follows_edits() {
    let mut rig = Rig::new();
    let kind = PageKind::Fixed {
        width: 500.0,
        height: 500.0,
    };
    let shopping = rig.page(kind, Background::Blank).await;
    let other = rig.page(kind, Background::Blank).await;
    let deleted = rig.page(kind, Background::Blank).await;
    let milk = text_item(
        &mut rig,
        Layer::Text,
        [10.0, 50.0],
        "Shopping list\nmilk\neggs",
    );
    let milk_id = milk.id;
    rig.edit(shopping, Edit::Add(milk)).await;
    let typed = text_item(&mut rig, Layer::Ink, [10.0, 10.0], "typed Header");
    rig.edit(shopping, Edit::Add(typed)).await;
    let note = text_item(&mut rig, Layer::Text, [0.0, 0.0], "call the plumber");
    rig.edit(other, Edit::Add(note)).await;
    let gone = text_item(&mut rig, Layer::Text, [0.0, 0.0], "milk on a dead page");
    rig.edit(deleted, Edit::Add(gone)).await;
    let mut author = crate::api::Author::new(&rig.storage);
    author
        .page(
            deleted,
            PageChange::Update(PagePatch {
                deleted: Some(true),
                ..PagePatch::default()
            }),
        )
        .await
        .unwrap();
    author.commit().await.unwrap();
    let base = rig.serve(routes()).await;

    let hits = search(&base, "MILK").await;
    let found = hits["hits"].as_array().unwrap();
    assert_eq!(1, found.len(), "the deleted page is not searched: {hits}");
    assert_eq!(shopping.to_string(), found[0]["page"].as_str().unwrap());
    assert_eq!("p", found[0]["title"]);
    assert!(found[0]["snippet"].as_str().unwrap().contains("milk"));
    assert_eq!(
        1,
        search(&base, "plumber").await["hits"]
            .as_array()
            .unwrap()
            .len()
    );
    assert_eq!(
        1,
        search(&base, "typed header").await["hits"]
            .as_array()
            .unwrap()
            .len(),
        "typed text counts too"
    );
    assert!(
        search(&base, "butter").await["hits"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        search(&base, "   ").await["hits"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // Deleting the text item removes the page from the results: no cache to go stale.
    rig.edit(shopping, Edit::Delete(milk_id)).await;
    assert!(
        search(&base, "milk").await["hits"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
