//! 45. Wasm-тесты rtc_peer.
//!
//! Проверяют логику, не требующую реального сетевого соединения.
//! Полноценное SDP+ICE-согласование тестируется через демо.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;

use hello_wasm02::rtc_peer::{self, IceState, PeerConfig, RtcState};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------- Feature detection ----------------

#[wasm_bindgen_test]
fn wasm_rtc_is_supported() {
    assert!(rtc_peer::is_supported());
}

// ---------------- Состояния ----------------

#[wasm_bindgen_test]
fn wasm_rtc_state_roundtrip_known() {
    let cases: [(&str, RtcState); 6] = [
        ("new", RtcState::New),
        ("connecting", RtcState::Connecting),
        ("connected", RtcState::Connected),
        ("disconnected", RtcState::Disconnected),
        ("failed", RtcState::Failed),
        ("closed", RtcState::Closed),
    ];
    for (s, k) in cases {
        assert_eq!(RtcState::from_str(s), k);
        assert_eq!(k.as_str(), s);
    }
}

#[wasm_bindgen_test]
fn wasm_rtc_state_unknown() {
    assert_eq!(RtcState::from_str("garbage"), RtcState::Unknown);
    assert_eq!(RtcState::Unknown.as_str(), "unknown");
}

#[wasm_bindgen_test]
fn wasm_rtc_state_terminal() {
    assert!(RtcState::Closed.is_terminal());
    assert!(RtcState::Failed.is_terminal());
    assert!(!RtcState::Connected.is_terminal());
    assert!(!RtcState::Connecting.is_terminal());
}

#[wasm_bindgen_test]
fn wasm_ice_state_roundtrip_known() {
    let cases: [(&str, IceState); 7] = [
        ("new", IceState::New),
        ("checking", IceState::Checking),
        ("connected", IceState::Connected),
        ("completed", IceState::Completed),
        ("disconnected", IceState::Disconnected),
        ("failed", IceState::Failed),
        ("closed", IceState::Closed),
    ];
    for (s, k) in cases {
        assert_eq!(IceState::from_str(s), k);
        assert_eq!(k.as_str(), s);
    }
}

#[wasm_bindgen_test]
fn wasm_ice_state_unknown() {
    assert_eq!(IceState::from_str("garbage"), IceState::Unknown);
}

// ---------------- PeerConfig ----------------

#[wasm_bindgen_test]
fn wasm_peer_config_loopback_empty() {
    let c = PeerConfig::loopback();
    assert!(c.ice_servers.is_empty());
}

#[wasm_bindgen_test]
fn wasm_peer_config_with_stun_has_google() {
    let c = PeerConfig::with_stun();
    assert_eq!(c.ice_servers.len(), 1);
    assert!(c.ice_servers[0].urls[0].contains("stun.l.google.com"));
}

// ---------------- RTCPeerConnection creation ----------------

#[wasm_bindgen_test]
fn wasm_create_loopback_peer() {
    let pc = rtc_peer::create_peer_connection(&PeerConfig::loopback())
        .expect("create pc");

    // Начальный state — "new". Читаем через Reflect,
    // потому что RtcPeerConnectionState — не подключённая фича.
    let state = js_sys::Reflect::get(
        pc.unchecked_ref::<wasm_bindgen::JsValue>(),
        &wasm_bindgen::JsValue::from_str("connectionState"),
    )
    .ok()
    .and_then(|v| v.as_string())
    .unwrap_or_default();
    assert_eq!(state, "new");

    rtc_peer::close_peer_connection(&pc);
}

#[wasm_bindgen_test]
fn wasm_create_peer_with_stun() {
    let pc = rtc_peer::create_peer_connection(&PeerConfig::with_stun())
        .expect("create pc");
    rtc_peer::close_peer_connection(&pc);
}

// ---------------- Data channel creation ----------------

#[wasm_bindgen_test]
fn wasm_create_data_channel() {
    let pc = rtc_peer::create_peer_connection(&PeerConfig::loopback())
        .expect("create pc");
    let dc = rtc_peer::create_data_channel(&pc, "test-channel")
        .expect("create dc");
    assert_eq!(rtc_peer::data_channel_label(&dc), "test-channel");
    // До соединения state — "connecting".
    assert_eq!(rtc_peer::data_channel_state(&dc), "connecting");
    rtc_peer::close_data_channel(&dc);
    rtc_peer::close_peer_connection(&pc);
}

// ---------------- Offer generation ----------------

#[wasm_bindgen_test]
async fn wasm_create_offer_produces_sdp() {
    let pc = rtc_peer::create_peer_connection(&PeerConfig::loopback())
        .expect("create pc");
    // Нужен data channel, чтобы SDP содержал application m-line.
    let _dc = rtc_peer::create_data_channel(&pc, "test").expect("dc");
    let offer = rtc_peer::create_offer(&pc).await.expect("offer");
    // Проверяем, что offer содержит SDP.
    let sdp = js_sys::Reflect::get(
        &offer,
        &wasm_bindgen::JsValue::from_str("sdp"),
    )
    .ok()
    .and_then(|v| v.as_string())
    .unwrap_or_default();
    assert!(!sdp.is_empty());
    assert!(sdp.contains("v=0"), "sdp = {sdp}");
    rtc_peer::close_peer_connection(&pc);
}

#[wasm_bindgen_test]
async fn wasm_offer_answer_roundtrip() {
    let a = rtc_peer::create_peer_connection(&PeerConfig::loopback())
        .expect("pc A");
    let b = rtc_peer::create_peer_connection(&PeerConfig::loopback())
        .expect("pc B");

    let _dc = rtc_peer::create_data_channel(&a, "chat").expect("dc");
    let offer = rtc_peer::create_offer(&a).await.expect("offer");
    rtc_peer::set_local_description(&a, &offer)
        .await
        .expect("set local A");

    let offer_full = rtc_peer::get_local_description(&a).expect("get local A");
    rtc_peer::set_remote_description(&b, &offer_full)
        .await
        .expect("set remote B");

    let answer = rtc_peer::create_answer(&b).await.expect("answer");
    rtc_peer::set_local_description(&b, &answer)
        .await
        .expect("set local B");

    let answer_full = rtc_peer::get_local_description(&b).expect("get local B");
    rtc_peer::set_remote_description(&a, &answer_full)
        .await
        .expect("set remote A");

    rtc_peer::close_peer_connection(&a);
    rtc_peer::close_peer_connection(&b);
}