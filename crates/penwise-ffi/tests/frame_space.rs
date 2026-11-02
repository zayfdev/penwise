//! Full-length frames on the tablet path (design harness v2): the frame JSON's new fields, the live window and
//! board ↔ document points under a bridge layout, and `Focus` through the sync core. No socket.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

use penwise_core::item::ItemId;
use penwise_ffi::scene_handle::SceneHandle;
use penwise_ffi::sync_core::{NewPage, SyncCore, decode_changes};
use penwise_proto::{
    Background, ClientMsg, DeviceId, ItemRef, PageKind, ServerId, ServerMsg, decode_client,
    encode_server,
};
use serde_json::Value;

const NOW: u64 = 1_700_000_000_000;

fn setup() -> (SyncCore, SceneHandle, penwise_proto::PageId) {
    let mut core = SyncCore::new(DeviceId::from_parts(NOW, 7), 7);
    let notebook = core.create_notebook(None, "Notes", 0, NOW).unwrap();
    let page = core
        .create_page(
            None,
            NewPage {
                notebook,
                title: "p1".to_owned(),
                position: 0,
                kind: PageKind::Infinite,
                background: Background::Blank,
            },
            NOW,
        )
        .unwrap();
    let scene = SceneHandle::open(&core, page, 7).unwrap();
    (core, scene, page)
}

fn frames(scene: &SceneHandle) -> Vec<Value> {
    serde_json::from_str(&scene.frames_json()).unwrap()
}

fn frame_id(frame: &Value) -> ItemId {
    ItemId::from_hex(frame["id"].as_str().unwrap()).unwrap()
}

#[test]
fn new_frames_are_full_length_and_the_json_carries_extent_doc_height_and_box() {
    let (_core, mut scene, _page) = setup();
    scene
        .add_frame(
            r#"{"project":"site","route":"/","preset":"desktop","x":10,"y":20}"#,
            NOW,
        )
        .unwrap();
    scene
        .add_frame(
            r#"{"project":"site","route":"/a","preset":"phone","extent":"viewport","x":2000,"y":20}"#,
            NOW + 1,
        )
        .unwrap();
    assert!(
        scene
            .add_frame(
                r#"{"project":"s","route":"/","preset":"phone","extent":"tall","x":0,"y":0}"#,
                NOW,
            )
            .is_err()
    );
    let listed = frames(&scene);
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0]["extent"], "full");
    assert_eq!(listed[0]["docH"], 0);
    assert_eq!(
        listed[0]["boxH"], 900.0,
        "unknown document height: the viewport"
    );
    assert_eq!(listed[1]["extent"], "viewport");
    assert_eq!(listed[1]["boxH"], 844.0);
}

#[test]
fn live_window_and_document_points_follow_the_reported_scroll() {
    let (_core, mut scene, _page) = setup();
    scene
        .add_frame(
            r#"{"project":"site","route":"/","preset":"desktop","x":500,"y":100}"#,
            NOW,
        )
        .unwrap();
    scene
        .add_frame(
            r#"{"project":"site","route":"/","preset":"desktop","extent":"viewport","x":3000,"y":100}"#,
            NOW + 1,
        )
        .unwrap();
    let listed = frames(&scene);
    let (full, viewport) = (frame_id(&listed[0]), frame_id(&listed[1]));
    let layout = r#"{"preset":"desktop","scroll":[0,1200],"viewport":[1440,900]}"#;
    scene.set_frame_layout(full, layout).unwrap();
    scene.set_frame_layout(viewport, layout).unwrap();

    assert_eq!(
        scene.frame_live_window(full),
        Some([500.0, 1300.0, 1440.0, 900.0])
    );
    assert_eq!(
        scene.frame_live_window(viewport),
        Some([3000.0, 100.0, 1440.0, 900.0])
    );
    assert_eq!(
        scene.frame_doc_point(full, [520.0, 3100.0]),
        Some([20.0, 3000.0])
    );
    assert_eq!(
        scene.frame_board_point(full, [20.0, 3000.0]),
        Some([520.0, 3100.0])
    );
    assert_eq!(
        scene.frame_doc_point(viewport, [3020.0, 150.0]),
        Some([20.0, 1250.0])
    );
    assert_eq!(scene.frame_live_window(ItemId([9; 16])), None);
}

#[test]
fn focus_encodes_for_the_server_and_decodes_into_the_outcome() {
    let (mut core, _scene, page) = setup();
    let item = ItemId([5; 16]);
    let welcome = |protocol_version: u16| {
        encode_server(&ServerMsg::Welcome {
            protocol_version,
            server: ServerId::from_parts(NOW, 3),
            device: DeviceId::from_parts(NOW, 7),
            head_seq: 0,
            log_floor: 0,
            server_now_ms: NOW,
            current_page: None,
        })
        .unwrap()
    };
    // Before a Welcome, and after one from a protocol-1 server, Focus is not sent.
    assert!(!core.server_supports_focus());
    assert_eq!(core.focus(page, Some(item)).unwrap(), None);
    let old = core.receive(&welcome(1), NOW).unwrap();
    assert!(!old.focus_supported);
    assert_eq!(old.encode()[2] & 0b100_0000, 0);
    assert_eq!(core.focus(page, None).unwrap(), None);
    // A protocol-2 Welcome reports it in the outcome (bit 6) and unlocks the frame.
    core.hello("t", penwise_proto::ClientKind::Tablet, NOW)
        .unwrap();
    let new = core.receive(&welcome(2), NOW).unwrap();
    assert!(new.focus_supported && core.server_supports_focus());
    assert_eq!(new.encode()[2] & 0b100_0000, 0b100_0000);
    // A new connection starts unsure again.
    core.hello("t", penwise_proto::ClientKind::Tablet, NOW)
        .unwrap();
    assert!(!core.server_supports_focus());
    core.receive(&welcome(2), NOW).unwrap();

    let frame = core.focus(page, Some(item)).unwrap().unwrap();
    assert_eq!(
        decode_client(&frame).unwrap(),
        ClientMsg::Focus {
            page,
            item: Some(ItemRef::from(item))
        }
    );
    let page_only = core.focus(page, None).unwrap().unwrap();
    assert_eq!(
        decode_client(&page_only).unwrap(),
        ClientMsg::Focus { page, item: None }
    );

    let from = DeviceId::from_parts(NOW, 9);
    let fit_frame = encode_server(&ServerMsg::Focus {
        page,
        item: Some(ItemRef::from(item)),
        by: from,
    })
    .unwrap();
    let outcome = core.receive(&fit_frame, NOW).unwrap();
    assert_eq!(outcome.kind, 14);
    assert_eq!((outcome.page, outcome.focus_item), (Some(page), Some(item)));
    let blob = outcome.encode();
    assert_eq!(
        blob[2] & 0b10_1000,
        0b10_1000,
        "page (bit3) and focus item (bit5)"
    );
    assert_eq!(
        &blob[blob.len() - 16..],
        &item.0,
        "the item trails the change list"
    );
    assert!(
        decode_changes(&blob).unwrap().is_empty(),
        "scene refresh ignores the tail"
    );

    let fit_page = encode_server(&ServerMsg::Focus {
        page,
        item: None,
        by: from,
    })
    .unwrap();
    let outcome = core.receive(&fit_page, NOW).unwrap();
    assert_eq!((outcome.page, outcome.focus_item), (Some(page), None));
    assert_eq!(outcome.encode()[2] & 0b10_0000, 0);
}

fn box_h(scene: &SceneHandle, frame: ItemId) -> f64 {
    let listed = frames(scene);
    let entry = listed
        .iter()
        .find(|f| frame_id(f) == frame)
        .expect("listed frame");
    entry["boxH"].as_f64().unwrap()
}

#[test]
fn box_height_is_computed_by_core_from_the_layout_the_hint_and_the_limit() {
    let (mut core, mut scene, page) = setup();
    let added = scene
        .add_frame(
            r#"{"project":"site","route":"/","preset":"desktop","x":0,"y":0}"#,
            NOW,
        )
        .unwrap();
    scene.commit_into(&mut core, &added, NOW).unwrap();
    let frame = frame_id(&frames(&scene)[0]);
    assert_eq!(
        box_h(&scene, frame),
        900.0,
        "unknown everywhere: the viewport"
    );

    // The server's hint (a synced field) applies while the layout says nothing.
    let hint = penwise_core::item::set_doc_height(scene.scene(), frame, 4000).unwrap();
    core.commit_edits(page, &hint, NOW).unwrap();
    scene.reload(&core).unwrap();
    assert_eq!(box_h(&scene, frame), 4000.0);

    // The live layout's document height wins, even when it is shorter than the hint.
    scene
        .set_frame_layout(frame, r#"{"doc":[1440,2500]}"#)
        .unwrap();
    assert_eq!(box_h(&scene, frame), 2500.0);
    // A layout height of 0 means unknown: back to the hint. A hostile one is clamped.
    scene
        .set_frame_layout(frame, r#"{"doc":[1440,0]}"#)
        .unwrap();
    assert_eq!(box_h(&scene, frame), 4000.0);
    scene
        .set_frame_layout(frame, r#"{"doc":[1440,1e30]}"#)
        .unwrap();
    assert_eq!(box_h(&scene, frame), 1_048_576.0);
}

#[test]
fn hex_is_the_id_the_http_routes_take() {
    let (_core, mut scene, _page) = setup();
    scene
        .add_frame(
            r#"{"project":"site","route":"/","preset":"phone","x":0,"y":0}"#,
            NOW,
        )
        .unwrap();
    let listed = frames(&scene);
    let id = frame_id(&listed[0]);
    let hex = listed[0]["hex"].as_str().unwrap();
    assert_eq!(hex, id.to_hex());
    assert_eq!(hex.len(), 32);
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
}

#[test]
fn clearing_a_layout_replaces_it_entirely_for_the_next_one() {
    let (_core, mut scene, _page) = setup();
    scene
        .add_frame(
            r#"{"project":"site","route":"/","preset":"desktop","x":500,"y":100}"#,
            NOW,
        )
        .unwrap();
    let frame = frame_id(&frames(&scene)[0]);
    let live = r#"{"preset":"desktop","scroll":[0,1200],"doc":[1440,6000],"viewport":[1440,900],"state":"click:menu","rects":{"k":[10,20,30,40]}}"#;
    assert!(scene.set_frame_layout(frame, live).unwrap());
    assert_eq!(box_h(&scene, frame), 6000.0);
    assert_eq!(
        scene.frame_live_window(frame),
        Some([500.0, 1300.0, 1440.0, 900.0])
    );

    // Demote to the still: its layout replaces the live one, doc included (a merge would keep 6000 / 1200).
    assert!(scene.clear_frame_layout(frame));
    assert!(!scene.clear_frame_layout(frame), "nothing left to clear");
    assert_eq!(
        box_h(&scene, frame),
        900.0,
        "no layout: back to the viewport"
    );
    assert_eq!(
        scene.frame_live_window(frame),
        Some([500.0, 100.0, 1440.0, 900.0])
    );
    scene
        .set_frame_layout(frame, r#"{"preset":"desktop","doc":[1440,3000]}"#)
        .unwrap();
    assert_eq!(box_h(&scene, frame), 3000.0);
    assert_eq!(
        scene.frame_live_window(frame),
        Some([500.0, 100.0, 1440.0, 900.0]),
        "the live scroll did not survive"
    );
    assert_eq!(scene.frame_keys(frame), "[]");
}

#[test]
fn replacing_a_layout_is_one_step_and_a_bad_one_keeps_the_old() {
    let (_core, mut scene, _page) = setup();
    scene
        .add_frame(
            r#"{"project":"site","route":"/","preset":"desktop","x":500,"y":100}"#,
            NOW,
        )
        .unwrap();
    let frame = frame_id(&frames(&scene)[0]);
    let live = r#"{"preset":"desktop","scroll":[0,1200],"doc":[1440,6000],"viewport":[1440,900],"state":"click:menu","rects":{"k":[10,20,30,40]}}"#;
    scene.set_frame_layout(frame, live).unwrap();
    assert!(
        scene
            .replace_frame_layout(frame, r#"{"preset":"desktop","doc":[1440,3000]}"#)
            .unwrap()
    );
    assert_eq!(
        box_h(&scene, frame),
        3000.0,
        "the still's document, not the live one's 6000"
    );
    assert_eq!(
        scene.frame_live_window(frame),
        Some([500.0, 100.0, 1440.0, 900.0]),
        "the live scroll did not survive"
    );
    assert!(scene.frame_keys(frame).len() <= 2, "no stale rect keys");
    // Unreadable input leaves the layout as it was.
    assert!(scene.replace_frame_layout(frame, "nope").is_err());
    assert_eq!(box_h(&scene, frame), 3000.0);
}

#[test]
fn content_bounds_are_the_viewers_rule_for_the_whole_page() {
    use penwise_core::item::LayerSet;
    let (_core, mut scene, _page) = setup();
    assert_eq!(scene.content_bounds(LayerSet::ALL), None, "an empty page");
    scene
        .add_frame(
            r#"{"project":"site","route":"/","preset":"desktop","x":500,"y":100}"#,
            NOW,
        )
        .unwrap();
    let frame = frame_id(&frames(&scene)[0]);
    scene
        .set_frame_layout(
            frame,
            r#"{"preset":"desktop","doc":[1440,6000],"viewport":[1440,900]}"#,
        )
        .unwrap();
    // Item bounds use the synced hint (no `doc_h` yet: the 900 px viewport), the same as the viewer's `content_bounds`
    // that the web fits with, so both clients fit the same rectangle.
    assert_eq!(
        scene.content_bounds(LayerSet::ALL),
        Some([500.0, 100.0, 1940.0, 1000.0])
    );
}

/// A full-length frame's marks sit at document points, so the page scrolling moves none of them: the layout's scroll
/// alone must not ask the ink layer for a full repaint. It did, for every scroll frame the bridge reported, and every
/// background render the tablet started during a finger scroll was then thrown away as overtaken: strokes that came
/// into view mid-scroll stayed cut off or missing until the page stopped (tablet, 2026-11-02). A viewport frame's
/// marks do move with the scroll, and any other change (rects, document, state) still repaints.
#[test]
fn a_scroll_alone_repaints_a_viewport_frame_but_not_a_full_length_one() {
    let (_core, mut scene, _page) = setup();
    scene
        .add_frame(
            r#"{"project":"site","route":"/","preset":"desktop","x":0,"y":0}"#,
            NOW,
        )
        .unwrap();
    scene
        .add_frame(
            r#"{"project":"site","route":"/v","preset":"desktop","extent":"viewport","x":3000,"y":0}"#,
            NOW,
        )
        .unwrap();
    let all = frames(&scene);
    let (full, viewport) = (frame_id(&all[0]), frame_id(&all[1]));
    let at = |y: u32, rect: &str| {
        format!(
            r#"{{"preset":"desktop","scroll":[0,{y}],"doc":[1440,6000],"viewport":[1440,900],"state":"","rects":{{"k":{rect}}}}}"#
        )
    };
    let all_dirty = |scene: &mut SceneHandle| scene.take_dirty().first() == Some(&1);
    for frame in [full, viewport] {
        assert!(
            scene
                .set_frame_layout(frame, &at(0, "[10,20,30,40]"))
                .unwrap()
        );
    }
    scene.take_dirty();

    assert!(
        !scene
            .set_frame_layout(full, &at(400, "[10,-380,30,40]"))
            .unwrap()
    );
    assert!(
        !all_dirty(&mut scene),
        "a full frame's scroll moves no mark"
    );
    assert_eq!(
        scene.frame_live_window(full),
        Some([0.0, 400.0, 1440.0, 900.0]),
        "the window still follows"
    );

    assert!(
        scene
            .set_frame_layout(full, &at(400, "[10,-300,30,40]"))
            .unwrap()
    );
    assert!(all_dirty(&mut scene), "a moved element repaints");

    assert!(
        scene
            .set_frame_layout(viewport, &at(400, "[10,-380,30,40]"))
            .unwrap()
    );
    assert!(
        all_dirty(&mut scene),
        "a viewport frame's marks scroll with the page"
    );
}
