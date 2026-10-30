//! Every committed v1 wire fixture (and the envelope-v2 `Focus` ones) decodes through the binding layer (natively and under
//! `wasm-pack test --node --release crates/penwise-wasm`).
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test harness: failures are reported by panicking"
)]

use penwise_wasm::decode::{client_kind, op_kind, server_kind};
use penwise_wasm::viewer::Viewer;

macro_rules! fixtures {
    ($($name:literal),* $(,)?) => {
        /// `(file name, bytes)` of every fixture, listed explicitly (wasm has no file system).
        const FIXTURES: &[(&str, &[u8])] = &[
            $(($name, include_bytes!(concat!(
                "../../penwise-proto/tests/fixtures/v1/", $name
            )))),*
        ];
    };
}

fixtures!(
    "client_hello.bin",
    "client_ping.bin",
    "client_pong.bin",
    "client_push_ops.bin",
    "client_resync.bin",
    "client_stroke_cancel.bin",
    "client_stroke_delta.bin",
    "client_switch_page.bin",
    "client_watch_none.bin",
    "client_watch_page.bin",
    "op_item_add.bin",
    "op_item_delete.bin",
    "op_item_update.bin",
    "op_notebook_create.bin",
    "op_notebook_update.bin",
    "op_page_create.bin",
    "op_page_update.bin",
    "server_ack.bin",
    "server_error.bin",
    "server_ops_since.bin",
    "server_ping.bin",
    "server_pong.bin",
    "server_reject_bad_token.bin",
    "server_reject_malformed.bin",
    "server_reject_rate_limited.bin",
    "server_reject_unsupported.bin",
    "server_resync.bin",
    "server_snapshot_begin.bin",
    "server_snapshot_begin_all.bin",
    "server_snapshot_chunk.bin",
    "server_snapshot_end.bin",
    "server_stroke_cancel.bin",
    "server_stroke_delta.bin",
    "server_switch_page.bin",
    "server_welcome.bin",
);

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn every_fixture_decodes() {
    for (name, bytes) in FIXTURES {
        if name.starts_with("client_") {
            client_kind(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        } else if name.starts_with("op_") {
            op_kind(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        } else if name.starts_with("server_") {
            let kind = server_kind(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            let received = Viewer::new().receive(bytes);
            assert!(received.is_ok(), "{name}: receive failed");
            assert_eq!(received.unwrap().kind, kind, "{name}");
        } else {
            panic!("{name}: unknown fixture prefix");
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn kind_names_match_the_file_names() {
    let name_of = |file: &str| {
        let (_, bytes) = FIXTURES.iter().find(|(n, _)| *n == file).unwrap();
        client_kind(bytes).unwrap()
    };
    assert_eq!(name_of("client_hello.bin"), "Hello");
    assert_eq!(name_of("client_push_ops.bin"), "PushOps");
    assert_eq!(name_of("client_watch_none.bin"), "Watch");
    let (_, bytes) = FIXTURES
        .iter()
        .find(|(n, _)| *n == "op_page_create.bin")
        .unwrap();
    assert_eq!(op_kind(bytes).unwrap(), "Page");
    let (_, bytes) = FIXTURES
        .iter()
        .find(|(n, _)| *n == "op_item_add.bin")
        .unwrap();
    assert_eq!(op_kind(bytes).unwrap(), "Item");
}

/// The envelope-v2 `Focus` frames decode through the binding layer too.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn focus_fixtures_decode() {
    let client: [&[u8]; 2] = [
        include_bytes!("../../penwise-proto/tests/fixtures/v2/client_focus_frame.bin"),
        include_bytes!("../../penwise-proto/tests/fixtures/v2/client_focus_page.bin"),
    ];
    for bytes in client {
        assert_eq!(client_kind(bytes).unwrap(), "Focus");
    }
    let server: [&[u8]; 2] = [
        include_bytes!("../../penwise-proto/tests/fixtures/v2/server_focus_frame.bin"),
        include_bytes!("../../penwise-proto/tests/fixtures/v2/server_focus_page.bin"),
    ];
    for bytes in server {
        let kind = server_kind(bytes).unwrap();
        let received = Viewer::new().receive(bytes).unwrap();
        assert_eq!(
            (received.kind, kind),
            (penwise_wasm::received::MessageKind::Focus, kind)
        );
        assert!(received.page.is_some());
    }
}

/// A new fixture file must be added to the list above.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn fixture_list_matches_the_directory() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../penwise-proto/tests/fixtures/v1");
    let mut on_disk: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    on_disk.sort();
    let mut listed: Vec<String> = FIXTURES.iter().map(|(n, _)| (*n).to_owned()).collect();
    listed.sort();
    assert_eq!(listed, on_disk);
}

/// `JsError` only exists on wasm: the exported wrappers map decode failures to it there.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn exported_wrappers_report_garbage_as_js_errors() {
    assert!(Viewer::new().receive(&[]).is_err());
    assert!(Viewer::new().receive(&[1, 0xFF, 0xFF]).is_err());
    assert!(penwise_wasm::decode::decode_server_kind(&[9]).is_err());
    assert!(penwise_wasm::decode::decode_client_kind(&[]).is_err());
    assert!(penwise_wasm::decode::decode_op_kind(&[2, 2]).is_err());
    assert!(penwise_wasm::encode::new_device_id(1.0, &[0; 3]).is_err());
    assert!(penwise_wasm::encode::encode_switch_page("nope").is_err());
}
