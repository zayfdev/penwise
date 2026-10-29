//! `Hub::announce_switch_page` (hunt #17): an HTTP page switch is broadcast to every session, watchers of other
//! pages included, and no session treats the server as itself.

use penwise_proto::{ClientKind, ServerMsg};

use super::fake::{FakeStore, device, page_id};
use super::{Rig, collect_catch_up, config};
use crate::sync::Hub;

#[tokio::test]
async fn an_announced_switch_page_reaches_every_session() {
    let store = FakeStore::new(8);
    let hub = Hub::new(store, config());
    let mut tablet = Rig::on(hub.clone());
    tablet.connect(device(1), ClientKind::Tablet, 0).await;
    collect_catch_up(&mut tablet).await;
    // An agent session (no page watch) must follow the switch too.
    let mut agent = Rig::on(hub.clone());
    agent.connect(device(2), ClientKind::Agent, 0).await;
    collect_catch_up(&mut agent).await;

    let page = page_id(4);
    let mut live = hub.subscribe_live();
    hub.announce_switch_page(page, device(9));
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(1), live.recv())
            .await
            .expect("hub published a live event")
            .is_ok(),
        "the frame is on the live channel"
    );

    let expected = ServerMsg::SwitchPage {
        page,
        by: device(9),
    };
    assert_eq!(tablet.next().await, expected);
    assert_eq!(agent.next().await, expected);
}
