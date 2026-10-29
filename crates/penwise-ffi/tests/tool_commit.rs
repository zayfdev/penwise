//! Scene tools commit to the core in the same step (`SceneHandle::commit_into`), and a failing commit never
//! leaves an edit on the screen that the replica does not hold.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use penwise_core::item::LayerSet;
use penwise_ffi::error::SyncError;
use penwise_ffi::scene_handle::{SAMPLE_FLOATS, SceneHandle, StrokeSpec};
use penwise_ffi::sync_core::{NewPage, SyncCore};
use penwise_proto::{Background, DeviceId, PageId, PageKind};

fn core_with_page(seed: u64) -> (SyncCore, PageId) {
    let mut core = SyncCore::new(DeviceId::from_parts(1_000, u128::from(seed)), seed);
    let notebook = core.create_notebook(None, "nb", 0, 5_000).unwrap();
    let page = core
        .create_page(
            None,
            NewPage {
                notebook,
                title: "p".to_owned(),
                position: 0,
                kind: PageKind::Infinite,
                background: Background::Blank,
            },
            5_000,
        )
        .unwrap();
    (core, page)
}

fn stroke(
    scene: &mut SceneHandle,
    points: usize,
) -> Result<Vec<penwise_core::edit::Edit>, SyncError> {
    let id = scene.new_item_id(5_000);
    let mut samples = Vec::new();
    let mut times = Vec::new();
    for i in 0..points {
        samples.extend([i as f32 * 0.5, 3.0, 0.5, 0.0, 0.0]);
        times.push(i32::try_from(i % 100_000).unwrap() * 8);
    }
    assert_eq!(samples.len(), times.len() * SAMPLE_FLOATS);
    scene.add_stroke(&StrokeSpec {
        id,
        preset: 0,
        argb: 0xFF00_0000,
        scale: 1.0,
        seed: 1,
        samples: &samples,
        times: &times,
    })
}

#[test]
fn a_tool_result_commits_to_the_replica_in_one_step() {
    let (mut core, page) = core_with_page(1);
    let mut scene = SceneHandle::open(&core, page, 3).unwrap();
    let edits = stroke(&mut scene, 20).unwrap();
    scene.commit_into(&mut core, &edits, 5_001).unwrap();
    assert_eq!(scene.render_order(LayerSet::ALL).len(), 1);
    assert_eq!(core.item_ids(page).len(), 1);
    assert_eq!(core.outbox().len(), 3);
}

#[test]
fn an_oversized_stroke_never_reaches_the_scene() {
    let (core, page) = core_with_page(2);
    let mut scene = SceneHandle::open(&core, page, 3).unwrap();
    let err = stroke(&mut scene, 400_000).unwrap_err();
    assert!(matches!(err, SyncError::OpTooLarge(_)), "{err}");
    assert!(scene.render_order(LayerSet::ALL).is_empty());
}

#[test]
fn a_failed_commit_reloads_the_scene_from_the_replica() {
    let (core, page) = core_with_page(3);
    let (mut other, _) = core_with_page(4); // does not know `page`
    let mut scene = SceneHandle::open(&core, page, 3).unwrap();
    let edits = stroke(&mut scene, 20).unwrap();
    assert_eq!(scene.render_order(LayerSet::ALL).len(), 1);
    let err = scene.commit_into(&mut other, &edits, 5_001).unwrap_err();
    assert!(matches!(err, SyncError::UnknownPage), "{err}");
    assert!(
        scene.render_order(LayerSet::ALL).is_empty(),
        "the scene shows what the replica holds, not the refused edit"
    );
    assert!(
        other.outbox().len() == 2,
        "nothing queued by the failed commit"
    );
}

/// C1 frames end to end through the headless tools: add from JSON, list as JSON, preset/route switches, undo,
/// and the frame present in the render order (so Clear page deletes it).
#[test]
fn frame_tools_commit_list_and_undo() {
    use penwise_core::item::ItemId;
    use serde_json::{Value, json};

    let (mut core, page) = core_with_page(5);
    let queued = core.outbox().len(); // the notebook + page creation ops
    let mut scene = SceneHandle::open(&core, page, 3).unwrap();
    let edits = scene
        .add_frame(
            r#"{"project":"portfolio","route":"/","preset":"phone","x":120.5,"y":-40}"#,
            5_000,
        )
        .unwrap();
    assert_eq!(edits.len(), 1);
    scene.commit_into(&mut core, &edits, 5_001).unwrap();
    assert_eq!(core.item_ids(page).len(), 1);

    let listed = |scene: &SceneHandle| -> Vec<Value> {
        serde_json::from_str(&scene.frames_json()).expect("frames json")
    };
    let frames = listed(&scene);
    assert_eq!(frames.len(), 1, "the frame is committed, not just applied");
    let frame = &frames[0];
    assert_eq!(frame["project"], "portfolio");
    assert_eq!(frame["route"], "/");
    assert_eq!(frame["preset"], "phone");
    assert_eq!(frame["cssW"], 390);
    assert_eq!(frame["cssH"], 844);
    assert_eq!(frame["rect"], json!([120.5, -40.0, 390.0, 844.0]));
    let id = ItemId::from_hex(frame["id"].as_str().expect("hex id")).expect("32 hex chars");
    assert_eq!(scene.render_order(LayerSet::ALL), vec![id]);

    let edits = scene.set_frame_preset(id, "desktop", 0, 0).unwrap();
    assert_eq!(edits.len(), 1);
    scene.commit_into(&mut core, &edits, 5_002).unwrap();
    let frame = listed(&scene).remove(0);
    assert_eq!(frame["preset"], "desktop");
    assert_eq!(frame["cssW"], 1440);
    assert_eq!(frame["cssH"], 900);

    let edits = scene.set_frame_route(id, "/work").unwrap();
    assert_eq!(edits.len(), 1);
    scene.commit_into(&mut core, &edits, 5_003).unwrap();
    assert_eq!(listed(&scene)[0]["route"], "/work");
    assert_eq!(
        core.outbox().len(),
        queued + 3,
        "each tool is one committed op"
    );

    // Same value, unknown id: a no-op that commits nothing.
    assert!(scene.set_frame_route(id, "/work").unwrap().is_empty());
    assert!(
        scene
            .set_frame_preset(ItemId([9; 16]), "tablet", 0, 0)
            .unwrap()
            .is_empty()
    );
    assert_eq!(core.outbox().len(), queued + 3);

    // Bad input is an error, never a silent edit.
    assert!(matches!(
        scene.add_frame(
            r#"{"project":"p","route":"/","preset":"watch","x":0,"y":0}"#,
            5_004
        ),
        Err(SyncError::BadArgument("frame preset"))
    ));
    assert!(matches!(
        scene.add_frame("{", 5_004),
        Err(SyncError::BadArgument("frame json"))
    ));
    assert!(matches!(
        scene.set_frame_preset(id, "custom", 0, 100),
        Err(SyncError::Edit(
            penwise_core::edit::EditError::InvalidValue {
                field: "frame_size"
            }
        ))
    ));
    assert!(matches!(
        scene.set_frame_route(id, "no-slash"),
        Err(SyncError::Edit(
            penwise_core::edit::EditError::InvalidValue {
                field: "frame_route"
            }
        ))
    ));

    // Undo walks back route, then preset; the frame itself stays.
    let undone = scene.undo().unwrap();
    scene.commit_into(&mut core, &undone, 5_005).unwrap();
    assert_eq!(listed(&scene)[0]["route"], "/");
    let undone = scene.undo().unwrap();
    scene.commit_into(&mut core, &undone, 5_006).unwrap();
    let frame = listed(&scene).remove(0);
    assert_eq!(frame["preset"], "phone");
    assert_eq!(frame["cssW"], 390);
}
