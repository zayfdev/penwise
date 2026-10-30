//! Design harness M2 in the browser: frame layouts, bound stroke commits, mark JSON, status, show-done and the
//! presentation the draw list returns.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test harness")]

mod common;

use common::{device, notebook_create, notebook_id, page_create, page_id};
use penwise_proto::{
    ClientMsg, HlcClock, SeqOp, ServerId, ServerMsg, decode_client, encode_server,
};
use penwise_wasm::pointer::SAMPLE_STRIDE;
use penwise_wasm::viewer::Viewer;

const NOW: u64 = 1_700_000_000_000;
const NOW_F: f64 = 1_700_000_000_000.0;
const ALL: u8 = 0b111;

/// An identified viewer with page 2, welcomed (see `authoring::ready`).
fn ready() -> (Viewer, String) {
    let mut viewer = Viewer::new();
    viewer.identify(&device(7).to_string(), 99).unwrap();
    viewer.begin_session("token", NOW_F).unwrap();
    let mut clock = HlcClock::new(device(1));
    let ops: Vec<SeqOp> = vec![
        notebook_create(&mut clock, NOW, notebook_id(1), "Notes", 0),
        page_create(&mut clock, NOW, page_id(2), notebook_id(1), "p", 0),
    ]
    .into_iter()
    .zip(1..)
    .map(|(op, seq)| SeqOp { seq, op })
    .collect();
    let welcome = ServerMsg::Welcome {
        protocol_version: 1,
        server: ServerId::from_parts(1, 9),
        device: device(7),
        head_seq: 2,
        log_floor: 0,
        server_now_ms: NOW,
        current_page: None,
    };
    assert!(viewer.receive(&encode_server(&welcome).unwrap()).is_ok());
    let since = ServerMsg::OpsSince {
        from_seq: 0,
        upto_seq: 2,
        ops,
        head_seq: 2,
        more: false,
    };
    let got = viewer.receive(&encode_server(&since).unwrap()).unwrap();
    assert_eq!(got.apply_errors, 0);
    (viewer, page_id(2).to_string())
}

fn layout(rect: &str, state: &str) -> String {
    format!(
        r#"{{"preset":"phone","scroll":[0,0],"doc":[390,844],"viewport":[390,844],"state":"{state}","rects":{{"k":{rect}}}}}"#
    )
}

/// Ten samples along a 27 px horizontal line at `y`, page coords.
fn samples(x: f32, y: f32) -> (Vec<f32>, Vec<u32>) {
    let mut flat = Vec::new();
    let mut times = Vec::new();
    for i in 0..10_u8 {
        flat.extend_from_slice(&[x + f32::from(i) * 3.0, y, 0.5, 0.0, 0.0]);
        times.push(u32::from(i) * 2000);
    }
    assert_eq!(flat.len(), 10 * SAMPLE_STRIDE);
    (flat, times)
}

/// Commits one bound stroke on `frame` at page (130, 320): returns its id text.
fn draw_bound(viewer: &mut Viewer, page: &str, frame: &str) -> String {
    let id = viewer
        .stroke_begin(page, 0, 0xFF00_0000, 1.0, NOW_F + 50.0)
        .unwrap();
    let (flat, times) = samples(130.0, 320.0);
    viewer.stroke_push(&flat, &times).unwrap();
    let binding = format!(r#"{{"frame":"{frame}","mode":"bound","key":"k"}}"#);
    viewer.stroke_end_bound(NOW_F + 100.0, &binding).unwrap();
    id
}

#[test]
fn a_bound_stroke_lands_on_its_element_and_shows_as_a_mark() {
    let (mut viewer, page) = ready();
    let frame = viewer
        .add_frame(
            &page,
            r#"{"project":"site","route":"/","preset":"phone","x":100,"y":50}"#,
            NOW_F,
        )
        .unwrap();
    assert!(
        viewer
            .set_frame_layout(&frame, &layout("[20, 300, 80, 40]", ""))
            .unwrap()
    );
    draw_bound(&mut viewer, &page, &frame);

    assert_eq!(viewer.frame_keys(&frame).unwrap(), r#"["k"]"#);
    let marks: serde_json::Value = serde_json::from_str(&viewer.marks(&page).unwrap()).unwrap();
    assert_eq!(marks.as_array().unwrap().len(), 1);
    assert_eq!(marks[0]["status"], "new");
    assert_eq!(marks[0]["preset"], "phone");
    assert_eq!(
        marks[0]["strokes"].as_array().unwrap().len(),
        1,
        "grouped into one mark"
    );
    assert!(marks[0]["rect"].is_array(), "visible rect");

    // The draw list is the effective placement: frame (100,50) + rect (20,300) + local.
    let list = viewer.render_page_paths(&page, ALL).unwrap();
    let points = list.points();
    let xs: Vec<f32> = points.chunks_exact(2).map(|p| p[0]).collect();
    assert!(
        xs.iter().any(|x| (x - 130.0).abs() < 20.0),
        "placed near the element, got {xs:?}"
    );
    assert!(
        xs.iter().all(|x| *x > 100.0),
        "not at the binding-local position"
    );

    // The element is gone: hidden from the draw list.
    assert!(
        viewer
            .set_frame_layout(&frame, &layout("null", ""))
            .unwrap()
    );
    assert!(
        viewer
            .render_page_paths(&page, ALL)
            .unwrap()
            .points()
            .is_empty()
    );
    assert!(viewer.marks(&page).unwrap().contains("\"rect\":null"));
}

#[test]
fn status_and_show_done_drive_visibility() {
    let (mut viewer, page) = ready();
    let frame = viewer
        .add_frame(
            &page,
            r#"{"project":"site","route":"/","preset":"phone","x":0,"y":0}"#,
            NOW_F,
        )
        .unwrap();
    viewer
        .set_frame_layout(&frame, &layout("[20, 300, 80, 40]", ""))
        .unwrap();
    draw_bound(&mut viewer, &page, &frame);
    let marks: serde_json::Value = serde_json::from_str(&viewer.marks(&page).unwrap()).unwrap();
    let mark = marks[0]["id"].as_str().unwrap().to_owned();

    viewer
        .set_mark_status(&mark, "seen", NOW_F + 200.0)
        .unwrap();
    assert!(viewer.marks(&page).unwrap().contains("\"status\":\"seen\""));
    viewer
        .set_mark_status(&mark, "done", NOW_F + 300.0)
        .unwrap();
    assert!(viewer.marks(&page).unwrap().contains("\"rect\":null"));
    assert!(
        viewer
            .render_page_paths(&page, ALL)
            .unwrap()
            .points()
            .is_empty()
    );

    assert!(viewer.set_show_done(true), "visible output changed");
    assert!(!viewer.set_show_done(true), "already on");
    assert!(viewer.marks(&page).unwrap().contains("\"rect\":["));
    assert!(
        !viewer
            .render_page_paths(&page, ALL)
            .unwrap()
            .points()
            .is_empty()
    );

    // Reopened shows with show-done off again.
    assert!(viewer.set_show_done(false));
    viewer
        .set_mark_status(&mark, "reopened", NOW_F + 400.0)
        .unwrap();
    assert!(viewer.marks(&page).unwrap().contains("\"rect\":["));
}

#[test]
fn a_preset_switch_hides_marks_and_a_state_mismatch_dims() {
    let (mut viewer, page) = ready();
    let frame = viewer
        .add_frame(
            &page,
            r#"{"project":"site","route":"/","preset":"phone","x":100,"y":50}"#,
            NOW_F,
        )
        .unwrap();
    // Drawn with the menu open.
    viewer
        .set_frame_layout(&frame, &layout("[20, 300, 80, 40]", "click:menu"))
        .unwrap();
    draw_bound(&mut viewer, &page, &frame);

    // Back to the default state: dimmed to 30%, still drawn.
    assert!(
        viewer
            .set_frame_layout(&frame, &layout("[20, 300, 80, 40]", ""))
            .unwrap()
    );
    let list = viewer.render_page_paths(&page, ALL).unwrap();
    assert!(!list.points().is_empty());
    let styles = list.styles();
    assert!(
        styles.chunks_exact(8).any(|s| (s[4] - 0.3).abs() < 1.0e-6),
        "dimmed opacity, got {styles:?}"
    );

    // Desktop: phone marks hide.
    viewer
        .set_frame_preset(&frame, "desktop", 1440, 900, NOW_F + 200.0)
        .unwrap();
    assert!(viewer.marks(&page).unwrap().contains("\"rect\":null"));
    assert!(
        viewer
            .render_page_paths(&page, ALL)
            .unwrap()
            .points()
            .is_empty()
    );
}

#[test]
fn the_bound_commit_queues_one_op_pair_and_one_undo_step() {
    let (mut viewer, page) = ready();
    let frame = viewer
        .add_frame(
            &page,
            r#"{"project":"site","route":"/","preset":"phone","x":100,"y":50}"#,
            NOW_F,
        )
        .unwrap();
    viewer
        .set_frame_layout(&frame, &layout("[20, 300, 80, 40]", ""))
        .unwrap();
    draw_bound(&mut viewer, &page, &frame);
    let push = viewer.poll_push().unwrap().expect("queued ops");
    assert!(matches!(
        decode_client(&push).unwrap(),
        ClientMsg::PushOps { .. }
    ));
    // Stroke + mark are one undo step.
    let undone = viewer.undo(&page, NOW_F + 500.0).unwrap();
    assert_eq!(undone, 2, "the stroke and its mark undo together");
    assert!(viewer.marks(&page).unwrap().contains("[]"));
}

#[test]
fn full_length_frames_report_extent_box_and_a_live_window_that_follows_the_scroll() {
    let (mut viewer, page) = ready();
    let full = viewer
        .add_frame(
            &page,
            r#"{"project":"site","route":"/","preset":"desktop","x":500,"y":100}"#,
            NOW_F,
        )
        .unwrap();
    let viewport = viewer
        .add_frame(
            &page,
            r#"{"project":"site","route":"/","preset":"desktop","extent":"viewport","x":3000,"y":100}"#,
            NOW_F + 1.0,
        )
        .unwrap();
    let listed: serde_json::Value = serde_json::from_str(&viewer.frames(&page).unwrap()).unwrap();
    assert_eq!(listed[0]["extent"], "full");
    assert_eq!(listed[0]["docH"], 0);
    assert_eq!(listed[0]["boxH"], 900.0);
    assert_eq!(listed[1]["extent"], "viewport");

    let layout = r#"{"preset":"desktop","scroll":[0,1200],"viewport":[1440,900]}"#;
    viewer.set_frame_layout(&full, layout).unwrap();
    viewer.set_frame_layout(&viewport, layout).unwrap();
    assert_eq!(
        viewer.frame_live_window(&full).unwrap(),
        vec![500.0, 1300.0, 1440.0, 900.0]
    );
    assert_eq!(
        viewer.frame_live_window(&viewport).unwrap(),
        vec![3000.0, 100.0, 1440.0, 900.0]
    );
    assert_eq!(
        viewer.frame_doc_point(&full, 520.0, 3100.0).unwrap(),
        vec![20.0, 3000.0]
    );
    assert_eq!(
        viewer.frame_board_point(&full, 20.0, 3000.0).unwrap(),
        vec![520.0, 3100.0]
    );
    let unknown = penwise_proto::ItemRef::from_parts(1, 1).to_string();
    assert!(viewer.frame_live_window(&unknown).unwrap().is_empty());
}
