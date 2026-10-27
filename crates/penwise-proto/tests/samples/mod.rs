//! One canonical sample per wire variant (the v1 fixture set). Changing a sample changes its fixture bytes:
//! frozen formats must not change, so edit these only when appending new variants.
#![allow(dead_code, reason = "each test binary uses a subset")]

use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::item::Layer;
use penwise_proto::live::MAX_DELTA_SAMPLES;
use penwise_proto::message::{MAX_OPS_PER_FRAME, OpRejected};
use penwise_proto::{
    AnyOp, AuthToken, Background, ClientKind, ClientMsg, ErrorCode, HlcClock, ItemRef, ItemState,
    LiveSample, NotebookChange, NotebookPatch, NotebookState, OpRejection, PageChange, PageKind,
    PagePatch, PageState, RejectReason, Scope, SeqOp, ServerId, ServerMsg, SnapshotChunk,
    StrokeCancel, StrokeDelta,
};

use crate::common::{
    device, hlc, item_id, item_op, notebook_id, notebook_op, op_id, page_id, page_op, text_item,
};

fn clock() -> HlcClock {
    HlcClock::new(device(1))
}

pub fn op_item_add() -> AnyOp {
    item_op(
        &mut clock(),
        1_700_000_000_000,
        1,
        page_id(1),
        Edit::Add(text_item(item_id(1), 65_536, "hello")),
    )
}

pub fn op_item_update() -> AnyOp {
    let patch = ItemPatch {
        layer: Some(Layer::Agent),
        z: Some(-65_536),
        ..ItemPatch::default()
    };
    item_op(
        &mut clock(),
        1_700_000_000_100,
        2,
        page_id(1),
        Edit::Update {
            id: item_id(1),
            patch,
        },
    )
}

pub fn op_item_delete() -> AnyOp {
    item_op(
        &mut clock(),
        1_700_000_000_200,
        3,
        page_id(1),
        Edit::Delete(item_id(1)),
    )
}

pub fn op_page_create() -> AnyOp {
    page_op(
        &mut clock(),
        1_700_000_000_300,
        4,
        page_id(1),
        PageChange::Create {
            notebook: notebook_id(1),
            title: "Sketch".into(),
            position: 65_536,
            kind: PageKind::Fixed {
                width: 2560.0,
                height: 1600.0,
            },
            background: Background::Dot,
        },
    )
}

pub fn op_page_update() -> AnyOp {
    page_op(
        &mut clock(),
        1_700_000_000_400,
        5,
        page_id(1),
        PageChange::Update(PagePatch {
            title: Some("Renamed".into()),
            position: Some(-3),
            background: Some(Background::Lined),
            deleted: Some(true),
        }),
    )
}

pub fn op_notebook_create() -> AnyOp {
    notebook_op(
        &mut clock(),
        1_700_000_000_500,
        6,
        notebook_id(1),
        NotebookChange::Create {
            title: "Work".into(),
            position: 0,
        },
    )
}

pub fn op_notebook_update() -> AnyOp {
    notebook_op(
        &mut clock(),
        1_700_000_000_600,
        7,
        notebook_id(1),
        NotebookChange::Update(NotebookPatch {
            title: Some("Work 2".into()),
            position: Some(9),
            deleted: Some(false),
        }),
    )
}

/// `(fixture name, op)`: every `AnyOp` kind, and the three item edit shapes.
pub fn ops() -> Vec<(&'static str, AnyOp)> {
    vec![
        ("op_item_add", op_item_add()),
        ("op_item_update", op_item_update()),
        ("op_item_delete", op_item_delete()),
        ("op_page_create", op_page_create()),
        ("op_page_update", op_page_update()),
        ("op_notebook_create", op_notebook_create()),
        ("op_notebook_update", op_notebook_update()),
    ]
}

fn delta() -> StrokeDelta {
    StrokeDelta {
        stroke: ItemRef::from_parts(1_700_000_000_000, 77),
        page: page_id(1),
        brush: vec![1, 2, 3, 4],
        seed: u64::MAX - 1,
        start: 0,
        samples: vec![
            LiveSample {
                x: -16,
                y: 32,
                pressure: 2048,
                tilt: 100,
                azimuth: -5,
                t_us: 0,
            },
            LiveSample {
                x: 1 << 24,
                y: -(1 << 24),
                pressure: 4096,
                tilt: 1024,
                azimuth: 2048,
                t_us: 8_000,
            },
        ],
    }
}

fn cancel() -> StrokeCancel {
    StrokeCancel {
        stroke: ItemRef::from_parts(1_700_000_000_000, 77),
        page: page_id(1),
    }
}

pub fn client_msgs() -> Vec<(&'static str, ClientMsg)> {
    vec![
        (
            "client_hello",
            ClientMsg::Hello {
                protocol_version: 1,
                device: device(1),
                token: AuthToken("t".repeat(43)),
                last_seen_seq: 123_456,
                client: ClientKind::Tablet,
            },
        ),
        (
            "client_push_ops",
            ClientMsg::PushOps {
                ops: ops().into_iter().map(|(_, op)| op).collect(),
            },
        ),
        ("client_stroke_delta", ClientMsg::StrokeDelta(delta())),
        ("client_stroke_cancel", ClientMsg::StrokeCancel(cancel())),
        (
            "client_switch_page",
            ClientMsg::SwitchPage { page: page_id(2) },
        ),
        (
            "client_watch_page",
            ClientMsg::Watch {
                page: Some(page_id(2)),
            },
        ),
        ("client_watch_none", ClientMsg::Watch { page: None }),
        ("client_resync", ClientMsg::Resync { from_seq: 99 }),
        ("client_ping", ClientMsg::Ping { nonce: u64::MAX }),
        ("client_pong", ClientMsg::Pong { nonce: 1 }),
    ]
}

fn notebook_state() -> NotebookState {
    NotebookState {
        id: notebook_id(1),
        created: hlc(1_700_000_000_000, 0, 1),
        title: "Work".into(),
        title_hlc: hlc(1_700_000_000_050, 1, 2),
        position: 0,
        position_hlc: hlc(1_700_000_000_000, 0, 1),
        deleted: false,
        deleted_hlc: hlc(1_700_000_000_000, 0, 1),
    }
}

fn page_state() -> PageState {
    PageState {
        id: page_id(1),
        notebook: notebook_id(1),
        kind: PageKind::Infinite,
        created: hlc(1_700_000_000_010, 0, 1),
        title: String::new(),
        title_hlc: hlc(1_700_000_000_010, 0, 1),
        position: 65_536,
        position_hlc: hlc(1_700_000_000_010, 0, 1),
        background: Background::Grid,
        background_hlc: hlc(1_700_000_000_020, 3, 2),
        deleted: true,
        deleted_hlc: hlc(1_700_000_000_030, 0, 1),
    }
}

fn item_state() -> ItemState {
    ItemState {
        page: page_id(1),
        body: vec![0x50, 0x57, 0x49, 0x54, 9, 9],
        patch: Some(vec![0x50, 0x57, 0x45, 0x44]),
        stamps: vec![1, 2, 3],
        deleted: true,
    }
}

pub fn server_msgs() -> Vec<(&'static str, ServerMsg)> {
    vec![
        (
            "server_welcome",
            ServerMsg::Welcome {
                protocol_version: 1,
                server: ServerId::from_parts(1_700_000_000_000, 5),
                device: device(1),
                head_seq: 1_000,
                log_floor: 10,
                server_now_ms: 1_700_000_123_456,
                current_page: Some(page_id(1)),
            },
        ),
        (
            "server_reject_bad_token",
            ServerMsg::Reject {
                reason: RejectReason::BadToken,
            },
        ),
        (
            "server_reject_unsupported",
            ServerMsg::Reject {
                reason: RejectReason::UnsupportedProtocol { min: 1, max: 3 },
            },
        ),
        (
            "server_reject_malformed",
            ServerMsg::Reject {
                reason: RejectReason::Malformed,
            },
        ),
        (
            "server_reject_rate_limited",
            ServerMsg::Reject {
                reason: RejectReason::RateLimited,
            },
        ),
        (
            "server_ack",
            ServerMsg::Ack {
                op_ids: vec![op_id(1), op_id(2)],
                seqs: vec![11, 12],
                rejected: vec![
                    OpRejected {
                        op_id: op_id(3),
                        reason: OpRejection::UnknownItem,
                    },
                    OpRejected {
                        op_id: op_id(4),
                        reason: OpRejection::WrongPage,
                    },
                ],
            },
        ),
        (
            "server_ops_since",
            ServerMsg::OpsSince {
                from_seq: 10,
                upto_seq: 40,
                ops: vec![
                    SeqOp {
                        seq: 11,
                        op: op_item_add(),
                    },
                    SeqOp {
                        seq: 12,
                        op: op_page_update(),
                    },
                    SeqOp {
                        seq: 40,
                        op: op_notebook_create(),
                    },
                ],
                head_seq: 41,
                more: true,
            },
        ),
        (
            "server_snapshot_begin",
            ServerMsg::SnapshotBegin {
                head_seq: 77,
                scope: Scope::Page(page_id(1)),
            },
        ),
        (
            "server_snapshot_begin_all",
            ServerMsg::SnapshotBegin {
                head_seq: 77,
                scope: Scope::All,
            },
        ),
        (
            "server_snapshot_chunk",
            ServerMsg::SnapshotChunk(SnapshotChunk {
                notebooks: vec![notebook_state()],
                pages: vec![page_state()],
                items: vec![item_state()],
            }),
        ),
        (
            "server_snapshot_end",
            ServerMsg::SnapshotEnd { head_seq: 77 },
        ),
        (
            "server_stroke_delta",
            ServerMsg::StrokeDelta {
                device: device(2),
                delta: delta(),
            },
        ),
        (
            "server_stroke_cancel",
            ServerMsg::StrokeCancel {
                device: device(2),
                cancel: cancel(),
            },
        ),
        (
            "server_switch_page",
            ServerMsg::SwitchPage {
                page: page_id(3),
                by: device(2),
            },
        ),
        ("server_resync", ServerMsg::Resync { from_seq: 5 }),
        (
            "server_error",
            ServerMsg::Error {
                code: ErrorCode::TooLarge,
                detail: "frame of 5 MiB".into(),
            },
        ),
        ("server_ping", ServerMsg::Ping { nonce: 7 }),
        ("server_pong", ServerMsg::Pong { nonce: 8 }),
    ]
}

/// A `PushOps` at the frame limit (used by limit tests; not a fixture).
pub fn full_push() -> ClientMsg {
    ClientMsg::PushOps {
        ops: (0..MAX_OPS_PER_FRAME).map(|_| op_item_delete()).collect(),
    }
}

/// A delta at the sample limit (not a fixture).
pub fn full_delta() -> StrokeDelta {
    let mut d = delta();
    d.samples = vec![d.samples[0]; MAX_DELTA_SAMPLES];
    d
}
