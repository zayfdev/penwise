//! Design harness M2 on the tablet path: `SceneHandle` with frame layouts, bound strokes, marks, status and
//! show-done — no socket, exactly what `Native.scene*` calls do.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test harness")]

use penwise_core::item::{ItemId, LayerSet};
use penwise_ffi::scene_handle::{SceneHandle, StrokeSpec};
use penwise_ffi::sync_core::{NewPage, SyncCore};
use penwise_proto::{Background, DeviceId, PageKind};

const NOW: u64 = 1_700_000_000_000;

fn device() -> DeviceId {
    DeviceId::from_parts(NOW, 7)
}

/// A page with one phone frame at (100, 50).
fn handle() -> (SyncCore, SceneHandle, String) {
    let mut core = SyncCore::new(device(), 7);
    let notebook = core.create_notebook(None, "Notes", 0, NOW).unwrap();
    let page = core
        .create_page(
            None,
            NewPage {
                notebook,
                title: "p1".to_owned(),
                position: 0,
                kind: PageKind::Fixed {
                    width: 1200.0,
                    height: 1200.0,
                },
                background: Background::Blank,
            },
            NOW,
        )
        .unwrap();
    let mut scene = SceneHandle::open(&core, page, 7).unwrap();
    let json = r#"{"project":"site","route":"/","preset":"phone","x":100.0,"y":50.0}"#;
    scene.add_frame(json, NOW).unwrap();
    let frames: serde_json::Value = serde_json::from_str(&scene.frames_json()).unwrap();
    let frame_id = frames[0]["id"].as_str().unwrap().to_owned();
    (core, scene, frame_id)
}

/// A short horizontal stroke in page coords at `y`, samples interleaved as `SAMPLE_FLOATS`.
fn samples(x: f32, y: f32) -> (Vec<f32>, Vec<i32>) {
    let mut flat = Vec::new();
    let mut times = Vec::new();
    for i in 0..10_u8 {
        flat.extend_from_slice(&[x + f32::from(i) * 3.0, y, 0.5, 0.0, 0.0]);
        times.push(i32::from(i) * 2000);
    }
    (flat, times)
}

fn layout(rect: &serde_json::Value, state: &str) -> String {
    format!(
        r#"{{"preset":"phone","scroll":[0,0],"doc":[390,844],"viewport":[390,844],"state":"{state}","rects":{{"k":{rect}}}}}"#
    )
}

fn draw_bound(scene: &mut SceneHandle, binding: &str, at: (f32, f32), now: u64) -> Vec<u8> {
    let (flat, times) = samples(at.0, at.1);
    let spec = StrokeSpec {
        id: scene.new_item_id(now),
        preset: 0,
        argb: 0xFF00_0000,
        scale: 1.0,
        seed: 5,
        samples: &flat,
        times: &times,
    };
    scene.add_bound_stroke(&spec, binding, now).unwrap();
    spec.id.0.to_vec()
}

fn frame_id(hex: &str) -> ItemId {
    ItemId::from_hex(hex).unwrap()
}

#[test]
fn a_bound_stroke_follows_its_element_and_hides_when_it_is_gone() {
    let (_core, mut scene, frame) = handle();
    let frame = frame_id(&frame);
    assert!(
        scene
            .set_frame_layout(frame, &layout(&serde_json::json!([20, 300, 80, 40]), ""))
            .unwrap()
    );
    let binding = format!(
        r#"{{"frame":"{frame}","mode":"bound","key":"k"}}"#,
        frame = frame.to_hex()
    );
    let stroke = draw_bound(&mut scene, &binding, (130.0, 320.0), NOW);
    let stroke = ItemId(stroke.try_into().unwrap());

    assert_eq!(scene.frame_keys(frame), r#"["k"]"#);
    let marks: serde_json::Value = serde_json::from_str(&scene.marks_json()).unwrap();
    assert_eq!(marks.as_array().unwrap().len(), 1);
    assert_eq!(marks[0]["status"], "new");
    assert_eq!(marks[0]["preset"], "phone");
    assert!(marks[0]["rect"].is_array(), "visible rect");
    // Effective position: frame (100, 50) + rect (20, 300) + local (10, 270)…, i.e. near (130, 320).
    assert!(
        scene
            .hit_test([135.0, 322.0], 6.0, LayerSet::ALL)
            .contains(&stroke),
        "hit at the effective page position"
    );
    assert!(
        scene
            .hit_test([35.0, 322.0], 5.0, LayerSet::ALL)
            .iter()
            .all(|id| *id != stroke),
        "not at the binding-local position"
    );
    // The element is gone after the edit: null hides the stroke and the mark.
    assert!(
        scene
            .set_frame_layout(frame, &layout(&serde_json::Value::Null, ""))
            .unwrap()
    );
    let marks: serde_json::Value = serde_json::from_str(&scene.marks_json()).unwrap();
    assert!(marks[0]["rect"].is_null());
    assert!(
        scene
            .render_order(LayerSet::ALL)
            .iter()
            .all(|id| *id != stroke)
    );
}

#[test]
fn marks_carry_status_and_show_done_is_local() {
    let (_core, mut scene, frame) = handle();
    let frame = frame_id(&frame);
    scene
        .set_frame_layout(frame, &layout(&serde_json::json!([20, 300, 80, 40]), ""))
        .unwrap();
    let binding = format!(r#"{{"frame":"{}","mode":"area"}}"#, frame.to_hex());
    let stroke = draw_bound(&mut scene, &binding, (130.0, 320.0), NOW);
    let mark = {
        let marks: serde_json::Value = serde_json::from_str(&scene.marks_json()).unwrap();
        ItemId::from_hex(marks[0]["id"].as_str().unwrap()).unwrap()
    };
    assert!(scene.set_mark_status(mark, "seen").unwrap().len() == 1);
    let marks: serde_json::Value = serde_json::from_str(&scene.marks_json()).unwrap();
    assert_eq!(marks[0]["status"], "seen");
    assert!(scene.set_mark_status(mark, "done").unwrap().len() == 1);
    assert!(scene.marks_json().contains("\"rect\":null"));
    assert!(!scene.set_show_done(false), "already off");
    assert!(scene.set_show_done(true), "shows done marks");
    let marks: serde_json::Value = serde_json::from_str(&scene.marks_json()).unwrap();
    assert!(marks[0]["rect"].is_array(), "shown");
    assert!(
        scene
            .render_order(LayerSet::ALL)
            .contains(&ItemId(stroke.try_into().unwrap()))
    );
    // Reopen: visible again with show-done off.
    assert!(scene.set_show_done(false));
    assert!(scene.set_mark_status(mark, "reopened").unwrap().len() == 1);
    assert!(scene.marks_json().contains("\"rect\":["));
}

#[test]
fn a_state_mismatch_dims_and_the_preset_switch_hides() {
    let (_core, mut scene, frame) = handle();
    let frame = frame_id(&frame);
    // Drawn with the menu open, so the stroke remembers that state.
    scene
        .set_frame_layout(
            frame,
            &layout(&serde_json::json!([20, 300, 80, 40]), "click:menu"),
        )
        .unwrap();
    let binding = format!(
        r#"{{"frame":"{}","mode":"bound","key":"k"}}"#,
        frame.to_hex()
    );
    let stroke = ItemId(
        draw_bound(&mut scene, &binding, (130.0, 320.0), NOW)
            .try_into()
            .unwrap(),
    );
    // The page is back in its default state: the stroke dims (30% opacity), still in the render order.
    assert!(
        scene
            .set_frame_layout(frame, &layout(&serde_json::json!([20, 300, 80, 40]), ""))
            .unwrap()
    );
    assert!(scene.render_order(LayerSet::ALL).contains(&stroke));
    // `[multiply, partCount, kind, len, r, g, b, a, opacity, …]`.
    let floats = scene.item_floats(stroke).unwrap();
    assert!(
        (floats[8] - 0.3).abs() < 1.0e-6,
        "dimmed opacity, got {floats:?}"
    );
    // Back to the recorded state: full opacity.
    assert!(
        scene
            .set_frame_layout(
                frame,
                &layout(&serde_json::json!([20, 300, 80, 40]), "click:menu"),
            )
            .unwrap()
    );
    let floats = scene.item_floats(stroke).unwrap();
    assert!(floats[8] > 0.9);
    // Another preset: hidden.
    scene.set_frame_preset(frame, "desktop", 1440, 900).unwrap();
    let marks: serde_json::Value = serde_json::from_str(&scene.marks_json()).unwrap();
    assert!(marks[0]["rect"].is_null(), "phone marks hide at desktop");
    assert!(
        scene
            .render_order(LayerSet::ALL)
            .iter()
            .all(|id| *id != stroke)
    );
}
