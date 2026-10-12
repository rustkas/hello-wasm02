//! 39. Wasm-тесты rta_protocol: encode/decode, partial reads,
//! потоковая обработка.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::rta_protocol::{
    anatomy, encode, hex_preview, try_decode_one, DecodeError,
    Frame, FrameDecoder, FrameKind, HEADER_LEN, MAGIC, MAX_PAYLOAD,
    VERSION,
};
use hello_wasm02::rta_transport::{TransportKind, TransportState};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------- FrameKind ----------------

#[wasm_bindgen_test]
fn wasm_frame_kind_roundtrip() {
    let cases: [(&str, u8, FrameKind); 6] = [
        ("text", 0x01, FrameKind::Text),
        ("binary", 0x02, FrameKind::Binary),
        ("ping", 0x03, FrameKind::Ping),
        ("pong", 0x04, FrameKind::Pong),
        ("close", 0x05, FrameKind::Close),
        ("error", 0x06, FrameKind::Error),
    ];
    for (s, b, k) in cases {
        assert_eq!(FrameKind::from_byte(b), Some(k));
        assert_eq!(k.as_byte(), b);
        assert_eq!(k.as_str(), s);
    }
}

#[wasm_bindgen_test]
fn wasm_frame_kind_unknown() {
    assert_eq!(FrameKind::from_byte(0x00), None);
    assert_eq!(FrameKind::from_byte(0x99), None);
    assert_eq!(FrameKind::from_byte(0xff), None);
}

// ---------------- Encode ----------------

#[wasm_bindgen_test]
fn wasm_encode_text_frame_basic() {
    let f = Frame::text(42, "hello");
    let bytes = encode(&f).expect("encode");
    assert_eq!(bytes.len(), HEADER_LEN + 5);
    assert_eq!(&bytes[0..2], &MAGIC);
    assert_eq!(bytes[2], VERSION);
    assert_eq!(bytes[3], FrameKind::Text.as_byte());
    assert_eq!(&bytes[4..8], &42u32.to_be_bytes());
    assert_eq!(&bytes[8..12], &5u32.to_be_bytes());
    assert_eq!(&bytes[12..], b"hello");
}

#[wasm_bindgen_test]
fn wasm_encode_ping_frame_empty_payload() {
    let f = Frame::ping(1);
    let bytes = encode(&f).expect("encode");
    assert_eq!(bytes.len(), HEADER_LEN);
    assert_eq!(bytes[3], FrameKind::Ping.as_byte());
}

#[wasm_bindgen_test]
fn wasm_encode_rejects_oversized() {
    let big = vec![0u8; MAX_PAYLOAD + 1];
    let f = Frame::binary(1, &big);
    assert!(encode(&f).is_err());
}

// ---------------- Decode ----------------

#[wasm_bindgen_test]
fn wasm_decode_roundtrip() {
    let orig = Frame::text(123, "привет");
    let bytes = encode(&orig).expect("encode");
    let (parsed, consumed) = try_decode_one(&bytes).expect("decode");
    assert_eq!(consumed, bytes.len());
    assert_eq!(parsed.kind, FrameKind::Text);
    assert_eq!(parsed.seq, 123);
    assert_eq!(parsed.as_text(), "привет");
}

#[wasm_bindgen_test]
fn wasm_decode_bad_magic() {
    let mut bytes = encode(&Frame::text(1, "x")).expect("encode");
    bytes[0] = 0xff;
    let err = try_decode_one(&bytes).unwrap_err();
    match err {
        DecodeError::BadMagic { got } => assert_eq!(got, [0xff, 0x54]),
        _ => panic!("unexpected error: {err:?}"),
    }
}

#[wasm_bindgen_test]
fn wasm_decode_bad_version() {
    let mut bytes = encode(&Frame::text(1, "x")).expect("encode");
    bytes[2] = 99;
    let err = try_decode_one(&bytes).unwrap_err();
    match err {
        DecodeError::BadVersion { got } => assert_eq!(got, 99),
        _ => panic!("unexpected error: {err:?}"),
    }
}

#[wasm_bindgen_test]
fn wasm_decode_bad_kind() {
    let mut bytes = encode(&Frame::text(1, "x")).expect("encode");
    bytes[3] = 0x77;
    let err = try_decode_one(&bytes).unwrap_err();
    match err {
        DecodeError::BadKind { got } => assert_eq!(got, 0x77),
        _ => panic!("unexpected error: {err:?}"),
    }
}

#[wasm_bindgen_test]
fn wasm_decode_incomplete_header() {
    let bytes = encode(&Frame::text(1, "x")).expect("encode");
    let err = try_decode_one(&bytes[..5]).unwrap_err();
    match err {
        DecodeError::IncompleteHeader { have } => assert_eq!(have, 5),
        _ => panic!("unexpected error: {err:?}"),
    }
    assert!(err.is_incomplete());
}

#[wasm_bindgen_test]
fn wasm_decode_incomplete_payload() {
    let bytes = encode(&Frame::text(1, "hello")).expect("encode");
    // Обрезаем — payload целиком не влезает.
    let err = try_decode_one(&bytes[..HEADER_LEN + 2]).unwrap_err();
    match err {
        DecodeError::IncompletePayload { have, need } => {
            assert_eq!(have, 2);
            assert_eq!(need, 5);
        }
        _ => panic!("unexpected error: {err:?}"),
    }
    assert!(err.is_incomplete());
}

// ---------------- FrameDecoder ----------------

#[wasm_bindgen_test]
fn wasm_decoder_single_frame() {
    let mut d = FrameDecoder::new();
    let bytes = encode(&Frame::text(1, "hello")).expect("encode");
    let results = d.push(&bytes);
    assert_eq!(results.len(), 1);
    let frame = results[0].as_ref().unwrap();
    assert_eq!(frame.as_text(), "hello");
    assert_eq!(d.frames_emitted(), 1);
    assert_eq!(d.pending_bytes(), 0);
}

#[wasm_bindgen_test]
fn wasm_decoder_two_frames_in_one_chunk() {
    let mut d = FrameDecoder::new();
    let mut bytes = encode(&Frame::text(1, "a")).expect("encode");
    bytes.extend_from_slice(&encode(&Frame::text(2, "b")).expect("encode"));
    let results = d.push(&bytes);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].as_ref().unwrap().seq, 1);
    assert_eq!(results[1].as_ref().unwrap().seq, 2);
}

#[wasm_bindgen_test]
fn wasm_decoder_partial_header() {
    let mut d = FrameDecoder::new();
    let bytes = encode(&Frame::text(1, "hello")).expect("encode");
    // Разбиваем на 3 части: 5, 10, остаток.
    let r1 = d.push(&bytes[..5]);
    assert!(r1.is_empty());
    let r2 = d.push(&bytes[5..10]);
    assert!(r2.is_empty());
    let r3 = d.push(&bytes[10..]);
    assert_eq!(r3.len(), 1);
    assert_eq!(r3[0].as_ref().unwrap().as_text(), "hello");
}

#[wasm_bindgen_test]
fn wasm_decoder_byte_by_byte() {
    let mut d = FrameDecoder::new();
    let bytes = encode(&Frame::ping(7)).expect("encode");
    let mut total = Vec::new();
    for b in &bytes {
        let mut r = d.push(&[*b]);
        total.append(&mut r);
    }
    assert_eq!(total.len(), 1);
    assert_eq!(total[0].as_ref().unwrap().seq, 7);
}

#[wasm_bindgen_test]
fn wasm_decoder_fatal_error_clears_buffer() {
    let mut d = FrameDecoder::new();
    // Мусор, не начинающийся с MAGIC.
    let results = d.push(&[0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(results.len(), 1);
    assert!(results[0].is_err());
    assert_eq!(d.pending_bytes(), 0);
}

// ---------------- Anatomy ----------------

#[wasm_bindgen_test]
fn wasm_anatomy_basic() {
    let bytes = encode(&Frame::text(42, "abc")).expect("encode");
    let a = anatomy(&bytes).expect("anatomy");
    assert_eq!(a.magic, MAGIC);
    assert_eq!(a.version, VERSION);
    assert_eq!(a.kind_byte, FrameKind::Text.as_byte());
    assert_eq!(a.seq, 42);
    assert_eq!(a.len, 3);
    assert_eq!(a.payload_preview, "61 62 63");
}

#[wasm_bindgen_test]
fn wasm_anatomy_too_short() {
    assert!(anatomy(&[1, 2, 3]).is_none());
}

// ---------------- hex_preview ----------------

#[wasm_bindgen_test]
fn wasm_hex_preview_basic() {
    assert_eq!(hex_preview(&[0xde, 0xad, 0xbe, 0xef], 8), "de ad be ef");
}

#[wasm_bindgen_test]
fn wasm_hex_preview_truncates() {
    let bytes = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
    let s = hex_preview(&bytes, 3);
    assert!(s.starts_with("01 02 03"));
    assert!(s.ends_with('…'));
}

#[wasm_bindgen_test]
fn wasm_hex_preview_empty() {
    assert_eq!(hex_preview(&[], 8), "");
}

// ---------------- Transport meta ----------------

#[wasm_bindgen_test]
fn wasm_transport_kind_roundtrip() {
    let cases: [(&str, TransportKind); 3] = [
        ("websocket", TransportKind::WebSocket),
        ("webtransport", TransportKind::WebTransport),
        ("webrtc", TransportKind::WebRtc),
    ];
    for (s, k) in cases {
        assert_eq!(TransportKind::from_str(s), k);
        assert_eq!(k.as_str(), s);
    }
}

#[wasm_bindgen_test]
fn wasm_transport_kind_unknown_defaults_to_ws() {
    assert_eq!(TransportKind::from_str("garbage"), TransportKind::WebSocket);
}

#[wasm_bindgen_test]
fn wasm_transport_state_as_str() {
    assert_eq!(TransportState::Idle.as_str(), "idle");
    assert_eq!(TransportState::Ready.as_str(), "ready");
    assert_eq!(TransportState::Failed.as_str(), "failed");
}

#[wasm_bindgen_test]
fn wasm_transport_meta_non_empty() {
    for k in [
        TransportKind::WebSocket,
        TransportKind::WebTransport,
        TransportKind::WebRtc,
    ] {
        assert!(!k.reliability().is_empty());
        assert!(!k.multiplexing().is_empty());
        assert!(!k.latency().is_empty());
        assert!(!k.browser_support().is_empty());
        assert!(!k.setup().is_empty());
    }
}