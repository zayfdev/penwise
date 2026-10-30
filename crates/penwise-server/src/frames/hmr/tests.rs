//! Tap tests: what counts as an HMR change.

use super::*;

fn tap() -> (Tap, broadcast::Receiver<()>) {
    let (feed, receiver) = broadcast::channel(8);
    (Tap::new(feed), receiver)
}

#[test]
fn vite_update_and_reload_messages_fire_once_each() {
    let (mut tap, mut receiver) = tap();
    tap.see(br#"{"type":"connected"}"#);
    assert!(receiver.try_recv().is_err(), "connect is not a change");
    tap.see(br#"{"type":"update","updates":[{"type":"js-update"}]}"#);
    assert!(receiver.try_recv().is_ok());
    tap.see(br#"{"type":"full-reload","path":"*"}"#);
    assert!(receiver.try_recv().is_ok());
    assert!(receiver.try_recv().is_err(), "one event per message");
}

#[test]
fn a_marker_split_over_two_reads_is_found() {
    let (mut tap, mut receiver) = tap();
    tap.see(b"\x81\x20{\"type\":\"full-rel");
    assert!(receiver.try_recv().is_err());
    tap.see(br#"oad","path":"*"}"#);
    assert!(receiver.try_recv().is_ok());
}

#[test]
fn next_messages_count_and_unrelated_traffic_does_not() {
    let (mut tap, mut receiver) = tap();
    tap.see(br#"{"action":"building"}"#);
    tap.see(br#"{"event":"ping"}"#);
    assert!(receiver.try_recv().is_err());
    tap.see(br#"{"action":"built","hash":"abc"}"#);
    assert!(receiver.try_recv().is_ok());
}
