//! Full-length frames on the tablet path (design harness v2): the frame JSON's new fields, the live window and
//! board ↔ document points under a bridge layout, and `Focus` through the sync core. No socket.
#![allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)]

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
