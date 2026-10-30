//! `Viewer` fed with real server frames: catalog, snapshot, op batches, live ink and its commit.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::cloned_ref_to_slice_refs,
    reason = "test harness: failures are reported by panicking"
)]

mod common;

use common::{
    BALLPOINT, WAVE, device, hlc, item_id, item_op, notebook_create, notebook_id, page_create,
    page_id, stroke,
};
use penwise_core::brush::BrushParams;
use penwise_core::codec::stroke::encode_params;
use penwise_core::edit::Edit;
use penwise_core::sample::Sample;
use penwise_proto::{
    Background, ErrorCode, HlcClock, ItemRef, ItemState, LiveSample, MaterializedItem,
    NotebookState, PageKind, PageState, RejectReason, Scope, ServerId, ServerMsg, SnapshotChunk,
    StrokeCancel, StrokeDelta, encode_server,
};
use penwise_wasm::draw_list::DrawKind;
use penwise_wasm::error::WasmError;
use penwise_wasm::received::{MessageKind, Received};
use penwise_wasm::viewer::{Viewer, parse_id};

const ALL_LAYERS: u8 = 0b111;

fn send(viewer: &mut Viewer, msg: &ServerMsg) -> Received {
    viewer.receive(&encode_server(msg).unwrap()).unwrap()
}

fn delta(stroke_id: u64, page: u64, start: u32, points: &[[f32; 2]]) -> StrokeDelta {
    let mut brush = Vec::new();
    encode_params(&BrushParams::for_preset(0, 0xFF00_0000, 1.0), &mut brush);
    let samples = points
        .iter()
        .zip(start..)
        .map(|(p, i)| {
            LiveSample::from_grid(
                Sample {
                    x: p[0],
                    y: p[1],
                    pressure: 0.5,
                    tilt: 0.0,
                    azimuth: 0.0,
                    t_us: i * 8_333,
                }
                .to_grid(),
            )
        })
        .collect();
    StrokeDelta {
        stroke: ItemRef::from_parts(1_700_000_000_000, u128::from(stroke_id)),
        page: page_id(page),
        brush,
        seed: 7,
        start,
        samples,
    }
}

/// Notebook 1 with pages 2 (position 200) and 3 (position 100) through an `OpsSince`.
fn with_catalog() -> Viewer {
    let mut viewer = Viewer::new();
    let mut clock = HlcClock::new(device(1));
    let ops = vec![
        notebook_create(&mut clock, 1, notebook_id(1), "Notes", 0),
        page_create(&mut clock, 2, page_id(2), notebook_id(1), "second", 200),
        page_create(&mut clock, 3, page_id(3), notebook_id(1), "first", 100),
    ];
    let ops = ops
        .into_iter()
        .zip(1..)
        .map(|(op, seq)| penwise_proto::SeqOp { seq, op })
        .collect();
    let got = send(
        &mut viewer,
        &ServerMsg::OpsSince {
            from_seq: 0,
            upto_seq: 3,
            ops,
            head_seq: 3,
            more: false,
        },
    );
    assert_eq!(got.apply_errors, 0);
    viewer
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn welcome_summarises_server_state() {
    let mut viewer = Viewer::new();
    let got = send(
        &mut viewer,
        &ServerMsg::Welcome {
            protocol_version: 1,
            server: ServerId::from_parts(1, 9),
            device: device(1),
            head_seq: 42,
            log_floor: 1,
            server_now_ms: 1_700_000_000_000,
            current_page: Some(page_id(5)),
        },
    );
    assert_eq!(got.kind, MessageKind::Welcome);
    assert_eq!(got.head_seq, 42);
    assert_eq!(got.server, Some(ServerId::from_parts(1, 9).to_string()));
    assert_eq!(got.page, Some(page_id(5).to_string()));
    assert_eq!(viewer.head_seq(), 0, "Welcome does not move the replica");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn small_messages_fill_the_summary() {
    let mut viewer = Viewer::new();
    let reject = send(
        &mut viewer,
        &ServerMsg::Reject {
            reason: RejectReason::RateLimited,
        },
    );
    assert_eq!(
        (reject.kind, reject.reject.as_deref()),
        (MessageKind::Reject, Some("rate_limited"))
    );
    let bad = send(
        &mut viewer,
        &ServerMsg::Reject {
            reason: RejectReason::UnsupportedProtocol { min: 1, max: 2 },
        },
    );
    assert_eq!(bad.reject.as_deref(), Some("unsupported_protocol"));
    let error = send(
        &mut viewer,
        &ServerMsg::Error {
            code: ErrorCode::ShuttingDown,
            detail: "bye".into(),
        },
    );
    assert_eq!(error.error.as_deref(), Some("shutting_down"));
    assert_eq!(error.detail.as_deref(), Some("bye"));
    let ping = send(&mut viewer, &ServerMsg::Ping { nonce: 77 });
    assert_eq!((ping.kind, ping.nonce), (MessageKind::Ping, 77));
    assert_eq!(send(&mut viewer, &ServerMsg::Pong { nonce: 5 }).nonce, 5);
    let switch = send(
        &mut viewer,
        &ServerMsg::SwitchPage {
            page: page_id(4),
            by: device(2),
        },
    );
    assert_eq!(switch.page, Some(page_id(4).to_string()));
    assert_eq!(switch.device, Some(device(2).to_string()));
    let item = ItemRef::from_parts(1_700_000_000_000, 5);
    let focus = send(
        &mut viewer,
        &ServerMsg::Focus {
            page: page_id(4),
            item: Some(item),
            by: device(2),
        },
    );
    assert_eq!(focus.kind, MessageKind::Focus);
    assert_eq!(
        (focus.page, focus.item, focus.device),
        (
            Some(page_id(4).to_string()),
            Some(item.to_string()),
            Some(device(2).to_string())
        )
    );
    let fit_page = send(
        &mut viewer,
        &ServerMsg::Focus {
            page: page_id(4),
            item: None,
            by: device(2),
        },
    );
    assert_eq!(fit_page.item, None);
    let ack = send(
        &mut viewer,
        &ServerMsg::Ack {
            op_ids: vec![],
            seqs: vec![],
            rejected: vec![],
        },
    );
    assert_eq!(ack.kind, MessageKind::Ack);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn catalog_getters_list_live_rows_in_position_order() {
    let viewer = with_catalog();
    assert_eq!(viewer.head_seq(), 3);
    let notebooks = viewer.notebooks();
    assert_eq!(notebooks.len(), 1);
    assert_eq!(notebooks[0].id, notebook_id(1).to_string());
    assert_eq!(
        (notebooks[0].title.as_str(), notebooks[0].page_count),
        ("Notes", 2)
    );

    let pages = viewer.pages(&notebook_id(1).to_string()).unwrap();
    let titles: Vec<&str> = pages.iter().map(|p| p.title.as_str()).collect();
    assert_eq!(titles, ["first", "second"]);
    assert!((pages[0].position - 100.0).abs() < f64::EPSILON);
    assert!(pages[0].fixed);
    assert_eq!(
        (pages[0].width, pages[0].height, pages[0].background),
        (1000.0, 800.0, 1)
    );

    let one = viewer.page(&page_id(2).to_string()).unwrap().unwrap();
    assert_eq!(one.notebook, notebook_id(1).to_string());
    assert!(viewer.page(&page_id(9).to_string()).unwrap().is_none());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn malformed_ids_are_errors() {
    assert!(matches!(
        parse_id::<penwise_proto::PageId>("nope"),
        Err(WasmError::Id(_))
    ));
    assert!(parse_id::<penwise_proto::PageId>(&page_id(1).to_string()).is_ok());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn snapshot_sequence_builds_catalog_and_page() {
    let mut viewer = Viewer::new();
    let created = hlc(1_700_000_000_000, 0);
    let page = page_id(2);
    let item = MaterializedItem::from_add(stroke(item_id(1), 1, BALLPOINT, &WAVE), created);
    let begin = send(
        &mut viewer,
        &ServerMsg::SnapshotBegin {
            head_seq: 9,
            scope: Scope::All,
        },
    );
    assert!(begin.catalog_changed);
    let chunk = send(
        &mut viewer,
        &ServerMsg::SnapshotChunk(SnapshotChunk {
            notebooks: vec![NotebookState {
                id: notebook_id(1),
                created,
                title: "Notes".into(),
                title_hlc: created,
                position: 0,
                position_hlc: created,
                deleted: false,
                deleted_hlc: created,
            }],
            pages: vec![PageState {
                id: page,
                notebook: notebook_id(1),
                kind: PageKind::Infinite,
                created,
                title: "p".into(),
                title_hlc: created,
                position: 0,
                position_hlc: created,
                background: Background::Dot,
                background_hlc: created,
                deleted: false,
                deleted_hlc: created,
            }],
            items: vec![ItemState::from_item(page, &item)],
        }),
    );
    assert!(chunk.catalog_changed);
    assert_eq!(chunk.changed_pages, [page.to_string()]);
    assert_eq!(chunk.apply_errors, 0);
    let end = send(&mut viewer, &ServerMsg::SnapshotEnd { head_seq: 9 });
    assert_eq!((end.kind, end.head_seq), (MessageKind::SnapshotEnd, 9));
    assert_eq!(viewer.head_seq(), 9);

    let list = viewer
        .render_page_paths(&page.to_string(), ALL_LAYERS)
        .unwrap();
    assert_eq!(list.commands()[0], DrawKind::PathNonZero as u32);
    let bounds = viewer
        .content_bounds(&page.to_string(), ALL_LAYERS)
        .unwrap()
        .unwrap();
    assert!(
        bounds[0] < 10.0 && bounds[2] > 160.0,
        "stroke width widens the bounds: {bounds:?}"
    );
    assert!(
        viewer
            .content_bounds(&page.to_string(), 0)
            .unwrap()
            .is_none()
    );
    assert!(
        viewer
            .content_bounds(&page_id(8).to_string(), ALL_LAYERS)
            .unwrap()
            .is_none()
    );
    assert!(
        viewer
            .render_page_paths(&page_id(8).to_string(), ALL_LAYERS)
            .unwrap()
            .is_empty()
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ops_since_adds_items_and_counts_refusals() {
    let mut viewer = with_catalog();
    let page = page_id(2);
    let mut clock = HlcClock::new(device(1));
    let add = item_op(
        &mut clock,
        1_700_000_000_001,
        10,
        page,
        Edit::Add(stroke(item_id(1), 1, BALLPOINT, &WAVE)),
    );
    let unknown_page = item_op(
        &mut clock,
        1_700_000_000_002,
        11,
        page_id(99),
        Edit::Add(stroke(item_id(2), 2, BALLPOINT, &WAVE)),
    );
    let got = send(
        &mut viewer,
        &ServerMsg::OpsSince {
            from_seq: 3,
            upto_seq: 20,
            ops: vec![
                penwise_proto::SeqOp { seq: 4, op: add },
                penwise_proto::SeqOp {
                    seq: 5,
                    op: unknown_page,
                },
            ],
            head_seq: 25,
            more: true,
        },
    );
    assert_eq!(
        (got.kind, got.head_seq, got.more),
        (MessageKind::OpsSince, 25, true)
    );
    assert_eq!(got.apply_errors, 1);
    assert_eq!(got.changed_pages, [page.to_string()]);
    assert!(!got.catalog_changed);
    assert_eq!(viewer.head_seq(), 20, "cursor advances to upto_seq");
    assert!(
        !viewer
            .render_page_paths(&page.to_string(), ALL_LAYERS)
            .unwrap()
            .is_empty()
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn live_preview_is_replaced_by_the_committed_add() {
    let mut viewer = with_catalog();
    let page = page_id(2);
    let page_text = page.to_string();
    let first = send(
        &mut viewer,
        &ServerMsg::StrokeDelta {
            device: device(2),
            delta: delta(1, 2, 0, &WAVE[..3]),
        },
    );
    assert_eq!(first.live_pages, [page_text.clone()]);
    assert_eq!(first.device, Some(device(2).to_string()));
    send(
        &mut viewer,
        &ServerMsg::StrokeDelta {
            device: device(2),
            delta: delta(1, 2, 3, &WAVE[3..]),
        },
    );
    let live = viewer.render_live_paths(&page_text).unwrap();
    assert!(!live.is_empty());
    assert_eq!(live.commands()[0], DrawKind::PathNonZero as u32);
    assert!(
        viewer
            .render_page_paths(&page_text, ALL_LAYERS)
            .unwrap()
            .is_empty()
    );

    let mut clock = HlcClock::new(device(2));
    let add = item_op(
        &mut clock,
        1_700_000_000_005,
        20,
        page,
        Edit::Add(stroke(
            penwise_core::item::ItemId(ItemRef::from_parts(1_700_000_000_000, 1).0),
            1,
            BALLPOINT,
            &WAVE,
        )),
    );
    let got = send(
        &mut viewer,
        &ServerMsg::OpsSince {
            from_seq: 3,
            upto_seq: 4,
            ops: vec![penwise_proto::SeqOp { seq: 4, op: add }],
            head_seq: 4,
            more: false,
        },
    );
    assert_eq!(
        got.live_pages,
        [page_text.clone()],
        "commit removed the preview"
    );
    assert_eq!(got.changed_pages, [page_text.clone()]);
    assert!(viewer.render_live_paths(&page_text).unwrap().is_empty());
    assert!(
        !viewer
            .render_page_paths(&page_text, ALL_LAYERS)
            .unwrap()
            .is_empty()
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn cancel_resync_and_clear_drop_previews() {
    let mut viewer = with_catalog();
    let page_text = page_id(2).to_string();
    let live = |viewer: &mut Viewer, id: u64| {
        send(
            viewer,
            &ServerMsg::StrokeDelta {
                device: device(2),
                delta: delta(id, 2, 0, &WAVE[..3]),
            },
        );
    };
    live(&mut viewer, 1);
    let cancel = send(
        &mut viewer,
        &ServerMsg::StrokeCancel {
            device: device(2),
            cancel: StrokeCancel {
                stroke: ItemRef::from_parts(1_700_000_000_000, 1),
                page: page_id(2),
            },
        },
    );
    assert_eq!(cancel.live_pages, [page_text.clone()]);
    assert!(viewer.render_live_paths(&page_text).unwrap().is_empty());

    live(&mut viewer, 2);
    let resync = send(&mut viewer, &ServerMsg::Resync { from_seq: 3 });
    assert_eq!(
        (resync.kind, resync.live_pages),
        (MessageKind::Resync, vec![page_text.clone()])
    );
    assert!(viewer.render_live_paths(&page_text).unwrap().is_empty());

    live(&mut viewer, 3);
    viewer.clear_live();
    assert!(viewer.render_live_paths(&page_text).unwrap().is_empty());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn undecodable_frames_are_errors_and_change_nothing() {
    let mut viewer = with_catalog();
    assert!(matches!(viewer.apply_frame(&[]), Err(WasmError::Proto(_))));
    assert!(matches!(
        viewer.apply_frame(&[1, 0xFF]),
        Err(WasmError::Proto(_))
    ));
    assert_eq!(viewer.head_seq(), 3);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn focus_is_sent_only_after_a_welcome_that_reports_protocol_2() {
    let welcome = |protocol_version: u16| ServerMsg::Welcome {
        protocol_version,
        server: ServerId::from_parts(1, 9),
        device: device(1),
        head_seq: 0,
        log_floor: 0,
        server_now_ms: 1_700_000_000_000,
        current_page: None,
    };
    let page = page_id(2).to_string();
    let mut viewer = Viewer::new();
    assert!(!viewer.supports_focus());
    assert_eq!(viewer.focus(&page, None).unwrap(), None);

    let old = send(&mut viewer, &welcome(1));
    assert!(!old.focus_supported && !viewer.supports_focus());
    assert_eq!(viewer.focus(&page, None).unwrap(), None);

    let new = send(&mut viewer, &welcome(2));
    assert!(new.focus_supported && viewer.supports_focus());
    let frame = viewer.focus(&page, None).unwrap().expect("focus frame");
    assert_eq!(
        penwise_proto::decode_client(&frame).unwrap(),
        penwise_proto::ClientMsg::Focus {
            page: page_id(2),
            item: None
        }
    );
}
