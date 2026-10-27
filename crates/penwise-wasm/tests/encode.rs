//! Client frame encoders and device ids.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test harness: failures are reported by panicking"
)]

mod common;

use common::page_id;
use penwise_proto::{AuthToken, ClientKind, ClientMsg, DeviceId, PROTOCOL_VERSION, decode_client};
use penwise_wasm::encode::{
    device_id, encode_ping, encode_pong, encode_resync, hello_frame, protocol_version,
    switch_page_frame, watch_frame,
};
use penwise_wasm::error::WasmError;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn device_id_packs_clock_and_random_bits() {
    let random = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let id = device_id(1_700_000_000_000.9, &random).unwrap();
    let bits = (1_u128 << 72)
        | (2 << 64)
        | (3 << 56)
        | (4 << 48)
        | (5 << 40)
        | (6 << 32)
        | (7 << 24)
        | (8 << 16)
        | (9 << 8)
        | 10;
    assert_eq!(id, DeviceId::from_parts(1_700_000_000_000, bits));
    assert_eq!(id.to_string().len(), 26);
    assert_eq!(device_id(0.0, &random).unwrap().timestamp_ms(), 0);
    assert_eq!(
        device_id(1.0, &random[..9]),
        Err(WasmError::RandomLength(9))
    );
    assert_eq!(device_id(f64::NAN, &random), Err(WasmError::BadClock));
    assert_eq!(device_id(-1.0, &random), Err(WasmError::BadClock));
    assert_eq!(device_id(f64::INFINITY, &random), Err(WasmError::BadClock));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn hello_is_a_browser_hello() {
    let device = DeviceId::from_parts(1_700_000_000_000, 5);
    let frame = hello_frame(&device.to_string(), "secret", 12).unwrap();
    assert_eq!(
        decode_client(&frame).unwrap(),
        ClientMsg::Hello {
            protocol_version: PROTOCOL_VERSION,
            device,
            token: AuthToken("secret".into()),
            last_seen_seq: 12,
            client: ClientKind::Browser,
        }
    );
    assert_eq!(protocol_version(), PROTOCOL_VERSION);
    assert!(matches!(hello_frame("bad", "t", 0), Err(WasmError::Id(_))));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn small_frames_round_trip() {
    let page = page_id(3);
    let decode = |frame: Vec<u8>| decode_client(&frame).unwrap();
    assert_eq!(
        decode(watch_frame(Some(&page.to_string())).unwrap()),
        ClientMsg::Watch { page: Some(page) }
    );
    assert_eq!(
        decode(watch_frame(None).unwrap()),
        ClientMsg::Watch { page: None }
    );
    assert_eq!(
        decode(switch_page_frame(&page.to_string()).unwrap()),
        ClientMsg::SwitchPage { page }
    );
    assert!(matches!(watch_frame(Some("x")), Err(WasmError::Id(_))));
    assert!(matches!(switch_page_frame(""), Err(WasmError::Id(_))));
    assert_eq!(
        decode(encode_resync(7).unwrap()),
        ClientMsg::Resync { from_seq: 7 }
    );
    assert_eq!(
        decode(encode_ping(8).unwrap()),
        ClientMsg::Ping { nonce: 8 }
    );
    assert_eq!(
        decode(encode_pong(9).unwrap()),
        ClientMsg::Pong { nonce: 9 }
    );
}
