use serde_json::json;

use super::content_of;

#[test]
fn text_comes_first_and_every_image_follows_as_a_png_block() {
    let reply = json!({
        "text": "Penwise: site",
        "images": [{ "label": "overview f1", "png": "iVBOR" }, { "label": "two", "png": "AAAA" }],
    });
    let content = content_of(&reply.to_string());
    let blocks = content["content"].as_array().expect("blocks");
    assert_eq!(3, blocks.len());
    assert_eq!("text", blocks[0]["type"]);
    assert_eq!("Penwise: site", blocks[0]["text"]);
    assert_eq!("image", blocks[1]["type"]);
    assert_eq!("image/png", blocks[1]["mimeType"]);
    assert_eq!("iVBOR", blocks[1]["data"]);
}

#[test]
fn an_answer_that_is_not_a_look_is_shown_as_text() {
    let content = content_of("not json");
    assert_eq!("not json", content["content"][0]["text"]);
}
