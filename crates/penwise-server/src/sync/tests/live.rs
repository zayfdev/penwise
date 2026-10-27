//! Live strokes: relayed to other sessions on the watched page, bounded, cancelled with their producer.

use penwise_proto::live::MAX_LIVE_STROKES_PER_DEVICE;
use penwise_proto::{
    ClientKind, ClientMsg, DeviceId, ItemRef, LiveSample, PageId, ServerMsg, StrokeCancel,
    StrokeDelta,
};

use super::fake::{FakeStore, device, item_op, page_id};
use super::{Rig, collect_catch_up, config};
use crate::sync::Hub;

fn stroke(n: u64) -> ItemRef {
    ItemRef::from_parts(1_700_000_000_000, u128::from(n))
}

fn delta(n: u64, page: PageId, start: u32) -> StrokeDelta {
    StrokeDelta {
        stroke: stroke(n),
        page,
        brush: vec![1, 2, 3],
        seed: u64::MAX,
        start,
        samples: vec![LiveSample {
            x: 1,
            y: 2,
            pressure: 3,
            tilt: 0,
            azimuth: 0,
            t_us: 0,
        }],
    }
}

async fn tablet(hub: &std::sync::Arc<Hub<FakeStore>>, dev: DeviceId, watch: Option<PageId>) -> Rig {
    let mut rig = Rig::on(hub.clone());
    rig.connect(dev, ClientKind::Tablet, 0).await;
    collect_catch_up(&mut rig).await;
    rig.send(&ClientMsg::Watch { page: watch }).await;
    rig
}

#[tokio::test]
async fn deltas_reach_watchers_of_the_page_but_not_the_producer_or_other_pages() {
    let store = FakeStore::new(16);
    let hub = Hub::new(store, config());
    let (p, q) = (page_id(1), page_id(2));
    let mut a = tablet(&hub, device(1), Some(p)).await;
    let mut b = tablet(&hub, device(2), Some(p)).await;
    let mut c = tablet(&hub, device(3), Some(q)).await;
    let mut d = tablet(&hub, device(4), None).await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    a.send(&ClientMsg::StrokeDelta(delta(1, p, 0))).await;
    assert_eq!(
        b.next().await,
        ServerMsg::StrokeDelta {
            device: device(1),
            delta: delta(1, p, 0)
        }
    );
    assert!(a.quiet().await && c.quiet().await && d.quiet().await);
    a.send(&ClientMsg::StrokeCancel(StrokeCancel {
        stroke: stroke(1),
        page: p,
    }))
    .await;
    assert_eq!(
        b.next().await,
        ServerMsg::StrokeCancel {
            device: device(1),
            cancel: StrokeCancel {
                stroke: stroke(1),
                page: p
            }
        }
    );
}

#[tokio::test]
async fn open_strokes_are_bounded_and_unknown_continuations_dropped() {
    let store = FakeStore::new(16);
    let hub = Hub::new(store, config());
    let p = page_id(1);
    let a = tablet(&hub, device(1), Some(p)).await;
    let mut b = tablet(&hub, device(2), Some(p)).await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let limit = MAX_LIVE_STROKES_PER_DEVICE as u64;
    for n in 1..=limit + 1 {
        a.send(&ClientMsg::StrokeDelta(delta(n, p, 0))).await;
    }
    a.send(&ClientMsg::StrokeDelta(delta(99, p, 5))).await;
    for n in 1..=limit {
        assert!(
            matches!(b.next().await, ServerMsg::StrokeDelta { delta, .. } if delta.stroke == stroke(n))
        );
    }
    assert!(
        b.quiet().await,
        "the fifth stroke and the orphan continuation are dropped"
    );
    // Continuing an open stroke still works.
    a.send(&ClientMsg::StrokeDelta(delta(1, p, 1))).await;
    assert!(matches!(b.next().await, ServerMsg::StrokeDelta { .. }));
}

#[tokio::test]
async fn over_limit_deltas_are_dropped_without_ending_the_session() {
    let store = FakeStore::new(16);
    let hub = Hub::new(store, config());
    let p = page_id(1);
    let mut a = tablet(&hub, device(1), Some(p)).await;
    let mut b = tablet(&hub, device(2), Some(p)).await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let mut empty = delta(1, p, 0);
    empty.samples.clear();
    let mut fat = delta(2, p, 0);
    fat.brush = vec![0; 4096];
    a.send(&ClientMsg::StrokeDelta(empty)).await;
    a.send(&ClientMsg::StrokeDelta(fat)).await;
    assert!(b.quiet().await);
    a.send(&ClientMsg::Ping { nonce: 3 }).await;
    assert_eq!(a.next().await, ServerMsg::Pong { nonce: 3 });
}

#[tokio::test]
async fn producer_disconnect_cancels_its_open_strokes_for_the_others() {
    let store = FakeStore::new(16);
    let hub = Hub::new(store, config());
    let p = page_id(1);
    let a = tablet(&hub, device(1), Some(p)).await;
    let mut b = tablet(&hub, device(2), Some(p)).await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    a.send(&ClientMsg::StrokeDelta(delta(1, p, 0))).await;
    a.send(&ClientMsg::StrokeDelta(delta(2, p, 0))).await;
    assert!(matches!(b.next().await, ServerMsg::StrokeDelta { .. }));
    assert!(matches!(b.next().await, ServerMsg::StrokeDelta { .. }));
    drop(a.tx);
    let mut cancelled = vec![];
    for _ in 0..2 {
        let ServerMsg::StrokeCancel { device: by, cancel } = b.next().await else {
            panic!("cancel")
        };
        assert_eq!(by, device(1));
        cancelled.push(cancel.stroke);
    }
    cancelled.sort();
    assert_eq!(cancelled, vec![stroke(1), stroke(2)]);
}

#[tokio::test]
async fn pushing_the_committed_item_frees_the_live_stroke_slot() {
    let store = FakeStore::new(16);
    let hub = Hub::new(store, config());
    let p = page_id(1);
    let mut a = tablet(&hub, device(1), Some(p)).await;
    let mut b = tablet(&hub, device(2), Some(p)).await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let limit = MAX_LIVE_STROKES_PER_DEVICE as u64;
    for n in 1..=limit {
        a.send(&ClientMsg::StrokeDelta(delta(n, p, 0))).await;
    }
    // `fake::item_op(.., n, ..)` adds the item whose id is `stroke(n)`.
    a.send(&ClientMsg::PushOps {
        ops: vec![item_op(1, 1, p, "x")],
    })
    .await;
    while !matches!(a.next().await, ServerMsg::Ack { .. }) {}
    a.send(&ClientMsg::StrokeDelta(delta(50, p, 0))).await;
    let mut seen = Vec::new();
    while seen.len() <= MAX_LIVE_STROKES_PER_DEVICE {
        if let ServerMsg::StrokeDelta { delta, .. } = b.next().await {
            seen.push(delta.stroke);
        }
    }
    assert_eq!(seen.last(), Some(&stroke(50)));
}

#[tokio::test]
async fn switch_page_is_relayed_to_everyone_else_and_remembered() {
    let store = FakeStore::new(16);
    let hub = Hub::new(store.clone(), config());
    let p = page_id(7);
    let mut a = tablet(&hub, device(1), None).await;
    let mut b = tablet(&hub, device(2), Some(page_id(1))).await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    a.send(&ClientMsg::SwitchPage { page: p }).await;
    assert_eq!(
        b.next().await,
        ServerMsg::SwitchPage {
            page: p,
            by: device(1)
        }
    );
    assert!(a.quiet().await);
    assert_eq!(store.current(), Some(p));
    let mut late = Rig::on(hub.clone());
    let ServerMsg::Welcome { current_page, .. } =
        late.connect(device(3), ClientKind::Tablet, 0).await
    else {
        panic!("Welcome")
    };
    assert_eq!(current_page, Some(p));
}

#[tokio::test]
async fn a_live_stroke_stays_on_the_page_of_its_first_delta() {
    let store = FakeStore::new(16);
    let hub = Hub::new(store, config());
    let (p, q) = (page_id(1), page_id(2));
    let a = tablet(&hub, device(1), Some(p)).await;
    let mut on_p = tablet(&hub, device(2), Some(p)).await;
    let mut on_q = tablet(&hub, device(3), Some(q)).await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    a.send(&ClientMsg::StrokeDelta(delta(1, p, 0))).await;
    assert!(matches!(on_p.next().await, ServerMsg::StrokeDelta { .. }));
    // A continuation naming another page must not leak the stroke onto it.
    a.send(&ClientMsg::StrokeDelta(delta(1, q, 1))).await;
    assert!(on_q.quiet().await, "the other page never sees the stroke");
    assert!(on_p.quiet().await, "the misaddressed delta is dropped");
    a.send(&ClientMsg::StrokeDelta(delta(1, p, 1))).await;
    assert!(matches!(on_p.next().await, ServerMsg::StrokeDelta { .. }));
}

#[tokio::test]
async fn the_delivery_cursor_is_recorded_as_the_ack_at_disconnect() {
    let store = FakeStore::new(16);
    store.commit(vec![
        item_op(2, 1, page_id(1), "a"),
        item_op(2, 2, page_id(1), "b"),
    ]);
    let hub = Hub::new(store.clone(), config());
    let mut rig = Rig::on(hub);
    rig.connect(device(1), ClientKind::Tablet, 0).await;
    collect_catch_up(&mut rig).await;
    assert_eq!(store.noted(), vec![(device(1), ClientKind::Tablet, 0)]);
    drop(rig.tx);
    for _ in 0..50 {
        if store.noted().len() == 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        store.noted(),
        vec![
            (device(1), ClientKind::Tablet, 0),
            (device(1), ClientKind::Tablet, 2)
        ],
        "the cursor the session reached is the ack"
    );
}
