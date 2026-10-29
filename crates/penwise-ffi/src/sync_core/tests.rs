use penwise_core::edit::Edit;
use penwise_core::item::{Affine, Item, ItemId, ItemKind, Layer};
use penwise_proto::{
    AnyOp, Background, ClientKind, ClientMsg, DeviceId, Hlc, Op, OpId, PageId, PageKind, SeqOp,
    ServerId, ServerMsg, decode_client, encode_server,
};

use super::legacy::parse_page_file;
use super::outcome::{Change, Outcome, decode_changes};
use super::{NewPage, SyncCore};

fn core(seed: u64) -> SyncCore {
    SyncCore::new(DeviceId::from_parts(1_000, u128::from(seed)), seed)
}

fn welcome(now: u64, head: u64) -> Vec<u8> {
    encode_server(&ServerMsg::Welcome {
        protocol_version: penwise_proto::PROTOCOL_VERSION,
        server: ServerId::from_parts(1, 9),
        device: DeviceId::from_parts(1_000, 1),
        head_seq: head,
        log_floor: 0,
        server_now_ms: now,
        current_page: None,
    })
    .expect("encode")
}

fn page_in(core: &mut SyncCore) -> PageId {
    let notebook = core
        .create_notebook(None, "nb", 0, 5_000)
        .expect("notebook");
    core.create_page(
        None,
        NewPage {
            notebook,
            title: "p".to_owned(),
            position: 0,
            kind: PageKind::Infinite,
            background: Background::Grid,
        },
        5_000,
    )
    .expect("page")
}

#[test]
fn hello_carries_the_replica_cursor() {
    let mut core = core(1);
    let frame = core.hello("tok", ClientKind::Tablet, 10).expect("hello");
    match decode_client(&frame).expect("decodes") {
        ClientMsg::Hello {
            last_seen_seq,
            client,
            device,
            ..
        } => {
            assert_eq!(last_seen_seq, 0);
            assert_eq!(client, ClientKind::Tablet);
            assert_eq!(device, core.device());
        }
        other => panic!("not a hello: {other:?}"),
    }
}

#[test]
fn pushes_wait_for_welcome_and_for_the_ack() {
    let mut core = core(2);
    let page = page_in(&mut core);
    assert!(
        core.poll_push().expect("poll").is_none(),
        "not welcomed yet"
    );
    core.hello("t", ClientKind::Tablet, 100).expect("hello");
    let out = core.receive(&welcome(100, 0), 100).expect("welcome");
    assert_eq!(out.kind, 0);
    assert!(core.poll_push().expect("poll").is_some());
    assert!(
        core.poll_push().expect("poll").is_none(),
        "one frame in flight"
    );
    let pending: Vec<OpId> = core
        .outbox()
        .pending_ops()
        .iter()
        .map(AnyOp::op_id)
        .collect();
    let ack = encode_server(&ServerMsg::Ack {
        op_ids: pending,
        seqs: vec![1, 2],
        rejected: Vec::new(),
    })
    .expect("encode");
    let out = core.receive(&ack, 101).expect("ack");
    assert_eq!(out.acked, 2);
    assert!(core.outbox().is_empty());
    assert!(core.is_settled());
    assert!(core.replica().page(page).is_some());
}

#[test]
fn welcome_calibrates_the_clock_for_new_stamps() {
    let mut core = core(3);
    page_in(&mut core);
    core.hello("t", ClientKind::Tablet, 1_000).expect("hello");
    // The server is an hour ahead of this device's clock.
    core.receive(&welcome(1_000 + 3_600_000, 0), 1_000)
        .expect("welcome");
    let page = core.page_ids()[0];
    core.commit_edits(page, &[Edit::Delete(ItemId([9; 16]))], 1_001)
        .expect_err("unknown item is refused by the replica");
    let nb = core
        .create_notebook(None, "later", 1, 1_001)
        .expect("notebook");
    let op = core
        .outbox()
        .pending_ops()
        .into_iter()
        .find(|op| matches!(op, AnyOp::Notebook(n) if n.notebook == nb))
        .expect("queued");
    assert!(
        op.hlc().wall_ms >= 1_000 + 3_600_000,
        "stamped on the server's clock"
    );
}

#[test]
fn ping_is_answered_and_garbage_is_an_error() {
    let mut core = core(4);
    let ping = encode_server(&ServerMsg::Ping { nonce: 77 }).expect("encode");
    let out = core.receive(&ping, 0).expect("ping");
    assert_eq!(out.replies.len(), 1);
    assert_eq!(
        decode_client(&out.replies[0]).expect("pong"),
        ClientMsg::Pong { nonce: 77 }
    );
    assert!(core.receive(&[1, 2, 3], 0).is_err());
    assert!(core.receive(&[], 0).is_err());
}

#[test]
fn remote_ops_report_item_changes_and_foreign_stamps_advance_the_clock() {
    let mut core = core(5);
    let page = page_in(&mut core);
    let remote = DeviceId::from_parts(2_000, 7);
    let item = Item {
        id: ItemId([4; 16]),
        layer: Layer::Agent,
        z: 65_536,
        transform: Affine::IDENTITY,
        kind: penwise_testkit_free_stroke(),
    };
    let hlc = Hlc {
        wall_ms: 9_000_000,
        counter: 0,
        device: remote,
    };
    let op = AnyOp::Item(Box::new(Op {
        op_id: OpId::from_parts(1, 1),
        device: remote,
        hlc,
        page,
        edit: Edit::Add(item),
    }));
    let frame = encode_server(&ServerMsg::OpsSince {
        from_seq: 0,
        upto_seq: 3,
        ops: vec![SeqOp { seq: 3, op }],
        head_seq: 3,
        more: false,
    })
    .expect("encode");
    let out = core.receive(&frame, 6_000).expect("ops");
    assert_eq!(out.applied, 1);
    assert_eq!(
        out.changes,
        vec![Change {
            page,
            item: Some(ItemId([4; 16]))
        }]
    );
    assert_eq!(core.head_seq(), 3);
    assert_eq!(core.item_ids(page), vec![ItemId([4; 16])]);
}

fn penwise_testkit_free_stroke() -> ItemKind {
    use penwise_core::brush::BrushParams;
    use penwise_core::item::StrokeItem;
    use penwise_core::sample::Sample;
    let samples = (0..4_u8)
        .map(|i| {
            Sample {
                x: f32::from(i) * 5.0,
                y: 1.0,
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: u32::from(i) * 8_000,
            }
            .quantize()
        })
        .collect();
    ItemKind::Stroke(StrokeItem {
        params: BrushParams::for_preset(0, 0xFF00_0000, 1.0),
        seed: 1,
        samples,
        trim: None,
        binding: None,
    })
}

#[test]
fn outcome_blob_round_trips_its_change_list() {
    let page = PageId::from_parts(5, 5);
    let out = Outcome {
        kind: 3,
        page: Some(page),
        replies: vec![vec![1, 2, 3], vec![]],
        changes: vec![
            Change {
                page,
                item: Some(ItemId([8; 16])),
            },
            Change {
                page: PageId::from_parts(6, 6),
                item: None,
            },
        ],
        ..Outcome::default()
    };
    assert_eq!(decode_changes(&out.encode()).expect("decodes"), out.changes);
    assert!(decode_changes(&out.encode()[..10]).is_err());
}

#[test]
fn whole_page_change_supersedes_item_changes() {
    let page = PageId::from_parts(5, 5);
    let mut out = Outcome::default();
    out.note(Change {
        page,
        item: Some(ItemId([1; 16])),
    });
    out.note(Change { page, item: None });
    out.note(Change {
        page,
        item: Some(ItemId([2; 16])),
    });
    assert_eq!(out.changes, vec![Change { page, item: None }]);
}

#[test]
fn exported_state_restores_the_same_core() {
    let mut a = core(6);
    let page = page_in(&mut a);
    a.commit_edits(
        page,
        &[Edit::Add(Item {
            id: ItemId([3; 16]),
            layer: Layer::Ink,
            z: 65_536,
            transform: Affine::IDENTITY,
            kind: penwise_testkit_free_stroke(),
        })],
        5_001,
    )
    .expect("commit");
    let state = a.export_state().expect("export");
    let b = SyncCore::restore(&state, 1).expect("restore");
    assert_eq!(b.device(), a.device());
    assert_eq!(b.outbox().len(), a.outbox().len());
    assert_eq!(b.item_ids(page), a.item_ids(page));
    assert_eq!(b.catalog_blob(), a.catalog_blob());
    assert_eq!(b.hlc.last(), a.hlc.last());
    assert!(SyncCore::restore(&state[..state.len() / 2], 1).is_err());
    assert!(SyncCore::restore(&[], 1).is_err());
    let mut wrong_version = state;
    wrong_version[0] = 99;
    assert!(SyncCore::restore(&wrong_version, 1).is_err());
}

#[test]
fn catalog_blob_lists_notebook_and_page() {
    let mut a = core(7);
    let page = page_in(&mut a);
    let blob = a.catalog_blob();
    // 1 notebook then 1 page.
    assert_eq!(u32::from_le_bytes(blob[..4].try_into().expect("count")), 1);
    assert!(blob.windows(16).any(|w| w == page.to_bytes()));
    assert!(blob.windows(2).any(|w| w == b"nb"));
}

#[test]
fn unknown_page_commits_are_refused() {
    let mut a = core(8);
    let err = a
        .commit_edits(
            PageId::from_parts(1, 1),
            &[Edit::Delete(ItemId([1; 16]))],
            1,
        )
        .expect_err("no such page");
    assert!(matches!(err, crate::error::SyncError::UnknownPage));
}

#[test]
fn page_files_parse_until_the_first_bad_record() {
    assert!(parse_page_file(b"").is_empty());
    assert!(parse_page_file(b"NOPE\x01").is_empty());
    assert!(parse_page_file(b"PWST\x02").is_empty());
    assert!(parse_page_file(b"PWST\x01").is_empty());
    let mut file = b"PWST\x01".to_vec();
    for seed in [7_i64, 8] {
        file.push(1);
        file.extend(0_i32.to_be_bytes());
        file.extend(0_i32.to_be_bytes());
        file.extend(1.0_f32.to_be_bytes());
        file.extend(seed.to_be_bytes());
        file.extend(1_i32.to_be_bytes());
        for v in [1.0_f32, 2.0, 0.5, 0.0, 0.0] {
            file.extend(v.to_be_bytes());
        }
        file.extend(0_i32.to_be_bytes());
    }
    assert_eq!(parse_page_file(&file).len(), 2);
    file.truncate(file.len() - 3);
    assert_eq!(
        parse_page_file(&file).len(),
        1,
        "torn tail keeps the earlier strokes"
    );
    // An absurd sample count stops the parse instead of allocating.
    let mut bad = b"PWST\x01\x01".to_vec();
    bad.extend([0; 12]);
    bad.extend(i32::MAX.to_be_bytes());
    assert!(parse_page_file(&bad).is_empty());
}
