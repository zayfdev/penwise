//! Browser drawing: pointer conversion, live deltas, the committed `Add`, the `PushOps` gate and own undo/redo.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test harness: failures are reported by panicking"
)]

mod common;

use common::{device, hlc, notebook_create, notebook_id, page_create, page_id};
use penwise_core::edit::Edit;
use penwise_core::sample::{ANGLE_STEP, Sample};
use penwise_proto::message::OpRejected;
use penwise_proto::{
    AnyOp, ClientMsg, HlcClock, OpRejection, SeqOp, ServerId, ServerMsg, decode_client,
    encode_server,
};
use penwise_wasm::draw_list::DrawKind;
use penwise_wasm::pointer::{pointer_sample, pointer_samples};
use penwise_wasm::received::MessageKind;
use penwise_wasm::viewer::Viewer;

const NOW: u64 = 1_700_000_000_000;
/// Exact in f64 (below 2^53).
const NOW_F: f64 = 1_700_000_000_000.0;
const ALL: u8 = 0b111;

fn send(viewer: &mut Viewer, msg: &ServerMsg) -> penwise_wasm::received::Received {
    viewer.receive(&encode_server(msg).unwrap()).unwrap()
}

/// An identified viewer that has seen page 2 and been welcomed (server clock = `NOW`).
fn ready() -> (Viewer, String) {
    let mut viewer = Viewer::new();
    viewer.identify(&device(7).to_string(), 99).unwrap();
    let hello = viewer.begin_session("token", NOW_F).unwrap();
    assert!(matches!(
        decode_client(&hello).unwrap(),
        ClientMsg::Hello { device: d, .. } if d == device(7)
    ));
    let mut clock = HlcClock::new(device(1));
    let ops: Vec<SeqOp> = vec![
        notebook_create(&mut clock, NOW, notebook_id(1), "Notes", 0),
        page_create(&mut clock, NOW, page_id(2), notebook_id(1), "p", 0),
    ]
    .into_iter()
    .zip(1..)
    .map(|(op, seq)| SeqOp { seq, op })
    .collect();
    send(
        &mut viewer,
        &ServerMsg::Welcome {
            protocol_version: 1,
            server: ServerId::from_parts(1, 9),
            device: device(7),
            head_seq: 2,
            log_floor: 0,
            server_now_ms: NOW,
            current_page: None,
        },
    );
    let got = send(
        &mut viewer,
        &ServerMsg::OpsSince {
            from_seq: 0,
            upto_seq: 2,
            ops,
            head_seq: 2,
            more: false,
        },
    );
    assert_eq!(got.apply_errors, 0);
    (viewer, page_id(2).to_string())
}

fn draw(viewer: &mut Viewer, page: &str) -> String {
    let id = viewer
        .stroke_begin(page, 0, 0xFF00_0000, 1.0, NOW_F)
        .unwrap();
    let raw = [
        10.0, 10.0, 0.5, 0.0, 0.0, 60.0, 40.0, 0.7, 10.0, 5.0, 120.0, 20.0, 0.6, 0.0, 0.0,
    ];
    let samples = pointer_samples(&raw);
    let delta = viewer.stroke_push(&samples, &[0, 8_000, 16_000]).unwrap();
    assert!(matches!(
        decode_client(&delta).unwrap(),
        ClientMsg::StrokeDelta(d) if d.start == 0 && d.samples.len() == 3
    ));
    id
}

fn pushed_ops(frame: &[u8]) -> Vec<AnyOp> {
    match decode_client(frame).unwrap() {
        ClientMsg::PushOps { ops } => ops,
        other => panic!("expected PushOps, got {other:?}"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn pointer_tilt_matches_the_digitizer_mapping() {
    let upright = pointer_sample(1.0, 2.0, 0.5, 0.0, 0.0, 5);
    assert_eq!(
        (upright.x, upright.y, upright.tilt, upright.azimuth),
        (1.0, 2.0, 0.0, 0.0)
    );
    assert!((upright.pressure - 0.5).abs() < 1.0 / 4095.0);
    assert_eq!(upright.t_us, 5);
    // Leaning right 30° (tiltX): tilt 30°, barrel pointing right = azimuth π/2.
    let right = pointer_sample(0.0, 0.0, 1.0, 30.0, 0.0, 0);
    assert!((right.tilt - 30.0_f32.to_radians()).abs() < ANGLE_STEP);
    assert!((right.azimuth - core::f32::consts::FRAC_PI_2).abs() < ANGLE_STEP);
    // Leaning toward the user (tiltY +): barrel points down the page = azimuth π.
    let down = pointer_sample(0.0, 0.0, 1.0, 0.0, 45.0, 0);
    assert!((down.tilt - core::f32::consts::FRAC_PI_4).abs() < ANGLE_STEP);
    assert!((down.azimuth.abs() - core::f32::consts::PI).abs() < ANGLE_STEP);
    // Garbage in → clamped, never NaN out.
    let junk = pointer_sample(f32::NAN, 0.0, f32::NAN, f32::NAN, 500.0, 0);
    assert!(junk.pressure == 0.0 && junk.tilt.is_finite() && junk.azimuth.is_finite());
    let _: Sample = junk;
    assert_eq!(
        pointer_samples(&[1.0, 2.0, 0.5, 0.0, 0.0, 9.0]).len(),
        5,
        "trailing partial event dropped"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn stroke_previews_then_commits_and_pushes() {
    let (mut viewer, page) = ready();
    assert!(!viewer.stroke_open());
    draw(&mut viewer, &page);
    assert!(viewer.stroke_open());
    assert!(
        !viewer.render_live_paths(&page).unwrap().is_empty(),
        "own preview renders"
    );

    viewer.stroke_end(NOW_F).unwrap();
    assert!(!viewer.stroke_open());
    assert!(viewer.render_live_paths(&page).unwrap().is_empty());
    let list = viewer.render_page_paths(&page, ALL).unwrap();
    assert_eq!(list.commands()[0], DrawKind::PathNonZero as u32);
    assert_eq!(viewer.pending_ops(), 1);

    let frame = viewer
        .poll_push()
        .unwrap()
        .expect("welcomed, outbox non-empty");
    let ops = pushed_ops(&frame);
    assert_eq!(ops.len(), 1);
    let AnyOp::Item(op) = &ops[0] else {
        panic!("item op")
    };
    assert_eq!(op.device, device(7));
    assert!(
        matches!(&op.edit, Edit::Add(item) if matches!(&item.kind, penwise_core::item::ItemKind::Stroke(s) if s.samples.len() == 3))
    );
    assert!(
        viewer.poll_push().unwrap().is_none(),
        "one PushOps in flight"
    );

    let ack = send(
        &mut viewer,
        &ServerMsg::Ack {
            op_ids: vec![op.op_id],
            seqs: vec![3],
            rejected: vec![],
        },
    );
    assert_eq!(
        (ack.kind, ack.acked, ack.rejected),
        (MessageKind::Ack, 1, 0)
    );
    assert_eq!(viewer.pending_ops(), 0);
    assert!(viewer.poll_push().unwrap().is_none());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn rejected_ops_leave_the_outbox() {
    let (mut viewer, page) = ready();
    draw(&mut viewer, &page);
    viewer.stroke_end(NOW_F).unwrap();
    let ops = pushed_ops(&viewer.poll_push().unwrap().unwrap());
    let AnyOp::Item(op) = &ops[0] else { panic!() };
    let ack = send(
        &mut viewer,
        &ServerMsg::Ack {
            op_ids: vec![],
            seqs: vec![],
            rejected: vec![OpRejected {
                op_id: op.op_id,
                reason: OpRejection::Invalid,
            }],
        },
    );
    assert_eq!(ack.rejected, 1);
    assert_eq!(viewer.pending_ops(), 0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn pushes_wait_for_welcome_and_snapshots() {
    let mut viewer = Viewer::new();
    viewer.identify(&device(7).to_string(), 1).unwrap();
    assert!(viewer.poll_push().unwrap().is_none());
    let (mut viewer, page) = ready();
    draw(&mut viewer, &page);
    viewer.stroke_end(NOW_F).unwrap();
    viewer.end_session();
    assert!(
        viewer.poll_push().unwrap().is_none(),
        "offline: queued, not sent"
    );
    assert_eq!(viewer.pending_ops(), 1);

    // Reconnect: a page snapshot replaces the page, the unacknowledged stroke is put back on top.
    viewer.begin_session("token", NOW_F).unwrap();
    send(
        &mut viewer,
        &ServerMsg::Welcome {
            protocol_version: 1,
            server: ServerId::from_parts(1, 9),
            device: device(7),
            head_seq: 2,
            log_floor: 0,
            server_now_ms: NOW,
            current_page: None,
        },
    );
    let begin = send(
        &mut viewer,
        &ServerMsg::SnapshotBegin {
            head_seq: 2,
            scope: penwise_proto::Scope::Page(page_id(2)),
        },
    );
    assert_eq!(begin.scope.as_deref(), Some(page.as_str()));
    assert!(
        viewer.poll_push().unwrap().is_none(),
        "paused during a snapshot"
    );
    send(&mut viewer, &ServerMsg::SnapshotEnd { head_seq: 2 });
    assert!(!viewer.render_page_paths(&page, ALL).unwrap().is_empty());
    assert!(viewer.poll_push().unwrap().is_some());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn undo_and_redo_emit_ops_for_own_edits_only() {
    let (mut viewer, page) = ready();
    assert!(!viewer.can_undo(&page));
    assert_eq!(viewer.undo(&page, NOW_F).unwrap(), 0);
    draw(&mut viewer, &page);
    viewer.stroke_end(NOW_F).unwrap();
    assert!(viewer.can_undo(&page) && !viewer.can_redo(&page));

    assert_eq!(viewer.undo(&page, NOW_F + 1.0).unwrap(), 1);
    assert!(viewer.render_page_paths(&page, ALL).unwrap().is_empty());
    assert!(viewer.can_redo(&page));
    assert_eq!(viewer.pending_ops(), 2, "the undo is an ordinary op");

    assert_eq!(viewer.redo(&page, NOW_F + 2.0).unwrap(), 1);
    assert!(!viewer.render_page_paths(&page, ALL).unwrap().is_empty());
    assert_eq!(viewer.pending_ops(), 3);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn cancel_drops_the_preview_and_misuse_is_an_error() {
    let (mut viewer, page) = ready();
    assert!(viewer.cancel_stroke().is_err());
    assert!(viewer.end_stroke(NOW).is_err());
    draw(&mut viewer, &page);
    assert!(
        viewer.begin_stroke(page_id(2), 0, 0, 1.0, 0).is_err(),
        "already open"
    );
    let frame = viewer.stroke_cancel().unwrap();
    assert!(matches!(
        decode_client(&frame).unwrap(),
        ClientMsg::StrokeCancel(_)
    ));
    assert!(viewer.render_live_paths(&page).unwrap().is_empty());
    assert_eq!(viewer.pending_ops(), 0);
    assert!(
        viewer.begin_stroke(page_id(9), 0, 0, 1.0, 0).is_err(),
        "unknown page"
    );
    assert!(viewer.push_stroke(&[0.0; 5], &[]).is_err());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn drawing_needs_an_identity() {
    let mut viewer = Viewer::new();
    assert!(viewer.begin_stroke(page_id(2), 0, 0, 1.0, 0).is_err());
    assert_eq!(viewer.pending_ops(), 0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn preset_names_follow_the_core_order() {
    let names = penwise_wasm::preset_names();
    assert_eq!(names.len(), penwise_core::brush::Preset::ALL.len());
    assert_eq!(names.first().map(String::as_str), Some("ballpoint"));
}

/// Catalog ops 1–2 (notebook 1 with page 2) as an `OpsSince` frame from the start of the log.
fn catalog_frame() -> ServerMsg {
    let mut clock = HlcClock::new(device(1));
    let ops = vec![
        notebook_create(&mut clock, NOW, notebook_id(1), "Notes", 0),
        page_create(&mut clock, NOW, page_id(2), notebook_id(1), "p", 0),
    ]
    .into_iter()
    .zip(1..)
    .map(|(op, seq)| SeqOp { seq, op })
    .collect();
    ServerMsg::OpsSince {
        from_seq: 0,
        upto_seq: 2,
        ops,
        head_seq: 2,
        more: false,
    }
}

fn welcome() -> ServerMsg {
    ServerMsg::Welcome {
        protocol_version: 1,
        server: ServerId::from_parts(1, 9),
        device: device(7),
        head_seq: 2,
        log_floor: 0,
        server_now_ms: NOW,
        current_page: None,
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn unsent_ops_survive_a_reload() {
    let (mut viewer, page) = ready();
    draw(&mut viewer, &page);
    viewer.stroke_end(NOW_F).unwrap();
    let first = pushed_ops(&viewer.poll_push().unwrap().unwrap());
    let state = viewer.export_state().unwrap();
    assert!(viewer.state_revision() > 0.0);

    // New page load: empty replica, same device, the outbox restored.
    let mut reloaded = Viewer::new();
    reloaded.restore_state(&state, 12345).unwrap();
    assert_eq!(reloaded.pending_ops(), 1);
    assert_eq!(reloaded.head_seq(), 0);
    let hello = reloaded.begin_session("token", NOW_F).unwrap();
    assert!(matches!(
        decode_client(&hello).unwrap(),
        ClientMsg::Hello { device: d, last_seen_seq: 0, .. } if d == device(7)
    ));
    send(&mut reloaded, &welcome());
    send(&mut reloaded, &catalog_frame());
    // The unsent stroke is on the page again, before the server has seen it.
    assert!(
        !reloaded
            .render_page_paths(&page, ALL)
            .unwrap()
            .commands()
            .is_empty()
    );
    let again = pushed_ops(&reloaded.poll_push().unwrap().unwrap());
    assert_eq!(again[0].op_id(), first[0].op_id());

    // Stamps after the reload still sort after the ones before it (same wall clock, fresh counter).
    draw(&mut reloaded, &page);
    reloaded.stroke_end(NOW_F).unwrap();
    let newest = reloaded.pending_ops();
    assert_eq!(newest, 2);
    let blob = reloaded.export_state().unwrap();
    let mut third = Viewer::new();
    third.restore_state(&blob, 1).unwrap();
    assert_eq!(third.pending_ops(), 2);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn restored_clock_orders_new_ops_after_old_ones() {
    let (mut viewer, page) = ready();
    draw(&mut viewer, &page);
    viewer.stroke_end(NOW_F).unwrap();
    let old = pushed_ops(&viewer.poll_push().unwrap().unwrap())[0].hlc();
    let mut reloaded = Viewer::new();
    reloaded
        .restore_state(&viewer.export_state().unwrap(), 4)
        .unwrap();
    reloaded.begin_session("t", NOW_F).unwrap();
    send(&mut reloaded, &welcome());
    send(&mut reloaded, &catalog_frame());
    draw(&mut reloaded, &page);
    reloaded.stroke_end(NOW_F).unwrap();
    let ops = pushed_ops(&reloaded.poll_push().unwrap().unwrap());
    let newest = ops.iter().map(AnyOp::hlc).max().unwrap();
    assert!(newest > old);
    assert!(ops.iter().filter(|o| o.hlc() > old).count() >= 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn refused_ops_become_dead_letters_and_leave_the_replica() {
    let (mut viewer, page) = ready();
    draw(&mut viewer, &page);
    viewer.stroke_end(NOW_F).unwrap();
    let ops = pushed_ops(&viewer.poll_push().unwrap().unwrap());
    let revision = viewer.state_revision();
    let ack = send(
        &mut viewer,
        &ServerMsg::Ack {
            op_ids: vec![],
            seqs: vec![],
            rejected: vec![OpRejected {
                op_id: ops[0].op_id(),
                reason: OpRejection::ClockSkew,
            }],
        },
    );
    assert!(
        ack.resync,
        "the caller must reconnect to refill the replica"
    );
    assert!(viewer.state_revision() > revision);
    assert_eq!(viewer.head_seq(), 0);
    assert!(
        viewer.render_page_paths(&page, ALL).unwrap().is_empty(),
        "the refused stroke is gone from the replica"
    );
    let letters = viewer.dead_letters();
    assert_eq!(letters.len(), 1);
    assert_eq!(letters[0].op_id, ops[0].op_id().to_string());
    assert_eq!(letters[0].reason, "device clock too far ahead");
    assert!(letters[0].summary.contains("item"));
    let bytes = viewer
        .dead_letter_bytes(&letters[0].op_id)
        .unwrap()
        .unwrap();
    assert!(!bytes.is_empty());

    // Dead letters are persisted with the outbox, and can be dismissed.
    let mut reloaded = Viewer::new();
    reloaded
        .restore_state(&viewer.export_state().unwrap(), 3)
        .unwrap();
    assert_eq!(reloaded.dead_letters().len(), 1);
    assert_eq!(reloaded.pending_ops(), 0);
    assert!(reloaded.dismiss_dead_letter(&letters[0].op_id).unwrap());
    assert!(reloaded.dead_letters().is_empty());
    assert!(!reloaded.dismiss_dead_letter(&letters[0].op_id).unwrap());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_log_stamp_far_ahead_is_reported_but_still_ordered_before_new_ops() {
    let (mut viewer, page) = ready();
    // A logged op stamped 10 minutes ahead of the calibrated clock.
    let mut ahead = HlcClock::resume(hlc(NOW + 600_000, 0));
    let op = page_create(
        &mut ahead,
        NOW + 600_000,
        page_id(3),
        notebook_id(1),
        "f",
        5,
    );
    let far = op.hlc();
    send(
        &mut viewer,
        &ServerMsg::OpsSince {
            from_seq: 2,
            upto_seq: 3,
            ops: vec![SeqOp { seq: 3, op }],
            head_seq: 3,
            more: false,
        },
    );
    draw(&mut viewer, &page);
    viewer.stroke_end(NOW_F).unwrap();
    let skew = viewer.take_clock_skew_ms();
    assert!((599_000.0..=601_000.0).contains(&skew), "skew {skew}");
    assert!(viewer.take_clock_skew_ms() < 1.0, "reading resets it");
    let ops = pushed_ops(&viewer.poll_push().unwrap().unwrap());
    assert!(
        ops[0].hlc() > far,
        "the new op orders after the far-ahead stamp"
    );
}
