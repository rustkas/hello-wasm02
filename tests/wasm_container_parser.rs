#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;
use hello_wasm02::container_parser::{
    is_fast_start, parse_avcc, parse_mp4, parse_webm,
};

wasm_bindgen_test_configure!(run_in_browser);

// =============================================================================
// Вспомогательные конструкторы
// =============================================================================

/// Собирает MP4 box: size + type + body.
fn mp4_box(box_type: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let size = (8 + body.len()) as u32;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(box_type);
    out.extend_from_slice(body);
    out
}

/// Собирает контейнерный MP4 box из дочерних.
fn mp4_container(box_type: &[u8; 4], children: &[Vec<u8>]) -> Vec<u8> {
    let mut body = Vec::new();
    for c in children {
        body.extend_from_slice(c);
    }
    mp4_box(box_type, &body)
}

/// Склеивает список box-ов в один буфер.
fn concat(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in parts {
        out.extend_from_slice(p);
    }
    out
}

// =============================================================================
// MP4
// =============================================================================

#[wasm_bindgen_test]
fn mp4_parses_ftyp_in_first_32_bytes() {
    let ftyp = mp4_box(b"ftyp", b"isom\x00\x00\x02\x00isomiso2avc1mp41");
    let boxes = parse_mp4(&ftyp).expect("parse ftyp");
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].box_type, "ftyp");
    assert_eq!(boxes[0].offset, 0);
    assert!(boxes[0].size <= 32);
}

#[wasm_bindgen_test]
fn mp4_parses_mdat_size() {
    let payload = vec![0u8; 100];
    let mdat = mp4_box(b"mdat", &payload);
    let boxes = parse_mp4(&mdat).expect("parse mdat");
    assert_eq!(boxes[0].box_type, "mdat");
    assert_eq!(boxes[0].size, 108); // 8 + 100
}

#[wasm_bindgen_test]
fn mp4_parses_moov_tree() {
    let stsd = mp4_box(b"stsd", b"\x00\x00\x00\x00");
    let stbl = mp4_container(b"stbl", &[stsd]);
    let minf = mp4_container(b"minf", &[stbl]);
    let mdia = mp4_container(b"mdia", &[minf]);
    let trak = mp4_container(b"trak", &[mdia]);
    let moov = mp4_container(b"moov", &[trak]);

    let boxes = parse_mp4(&moov).expect("parse moov");
    assert_eq!(boxes[0].box_type, "moov");
    assert_eq!(boxes[0].children.len(), 1);
    assert_eq!(boxes[0].children[0].box_type, "trak");

    let stsd_found = boxes[0].find("stsd");
    assert!(stsd_found.is_some(), "stsd должен быть в дереве");
}

#[wasm_bindgen_test]
fn mp4_fast_start_detection() {
    let ftyp = mp4_box(b"ftyp", b"isom");
    let moov = mp4_box(b"moov", b"");
    let mdat = mp4_box(b"mdat", b"");

    let fast_bytes = concat(&[ftyp.clone(), moov.clone(), mdat.clone()]);
    let slow_bytes = concat(&[ftyp.clone(), mdat.clone(), moov.clone()]);

    let b1 = parse_mp4(&fast_bytes).unwrap();
    let b2 = parse_mp4(&slow_bytes).unwrap();

    assert!(is_fast_start(&b1));
    assert!(!is_fast_start(&b2));
}

#[wasm_bindgen_test]
fn mp4_avcc_extracts_sps_pps() {
    // Минимальный avcC: 1 SPS длиной 4, 1 PPS длиной 2.
    let avcc: &[u8] = &[
        0x01, 0x42, 0x00, 0x1E, // version, profile, compat, level
        0xFF,                   // lengthSizeMinusOne = 3
        0xE1,                   // numSPS = 1
        0x00, 0x04, 0x67, 0x42, 0x00, 0x1E, // SPS
        0x01,                   // numPPS = 1
        0x00, 0x02, 0x68, 0xCE, // PPS
    ];
    let (sps, pps) = parse_avcc(avcc).expect("parse avcC");
    assert_eq!(sps.len(), 4);
    assert_eq!(pps.len(), 2);
    assert_eq!(sps[0], 0x67); // NAL type 7 (SPS)
    assert_eq!(pps[0], 0x68); // NAL type 8 (PPS)
}

// =============================================================================
// WebM
// =============================================================================

#[wasm_bindgen_test]
fn webm_starts_with_ebml_header() {
    // EBML header начинается с 0x1A45DFA3.
    let data: &[u8] = &[
        0x1A, 0x45, 0xDF, 0xA3, // EBML id
        0x84,                   // size VINT (1 байт, 4)
        0x42, 0x86, 0x81, 0x01, // EBMLVersion
    ];
    let elems = parse_webm(data).expect("parse EBML");
    assert_eq!(elems.len(), 1);
    assert_eq!(elems[0].id, 0x1A45DFA3);
    assert_eq!(elems[0].name, "EBML");
}

#[wasm_bindgen_test]
fn webm_segment_and_tracks() {
    // Внимание: все size-VINT должны соответствовать реальной
    // длине содержимого элемента.
    //
    // Разберём байты по порядку:
    //   [0..4]  0x18538067      — Segment id
    //   [4]     0x8E            — Segment size VINT = 14
    //   [5..9]  0x1654AE6B      — Tracks id
    //   [9]     0x89            — Tracks size VINT = 9
    //   [10]    0xAE            — TrackEntry id
    //   [11]    0x87            — TrackEntry size VINT = 7
    //   [12]    0x86            — CodecID id
    //   [13]    0x85            — CodecID size VINT = 5
    //   [14..19]"V_VP8"          — payload
    //
    // Итого: 19 байт. Segment body = 14, Tracks body = 9,
    // TrackEntry body = 7, CodecID body = 5.
    let segment: &[u8] = &[
        0x18, 0x53, 0x80, 0x67, // Segment id
        0x8E,                   // Segment size = 14
        0x16, 0x54, 0xAE, 0x6B, // Tracks id
        0x89,                   // Tracks size = 9
        0xAE,                   // TrackEntry id
        0x87,                   // TrackEntry size = 7
        0x86,                   // CodecID id
        0x85,                   // CodecID size = 5
        b'V', b'_', b'V', b'P', b'8',
    ];
    assert_eq!(segment.len(), 19, "тестовые данные должны быть 19 байт");

    let elems = parse_webm(segment).expect("parse Segment");
    assert_eq!(elems.len(), 1);
    assert_eq!(elems[0].id, 0x18538067);
    assert_eq!(elems[0].name, "Segment");
    assert_eq!(elems[0].size, 14);

    let tracks_found = elems[0].find("Tracks");
    assert!(tracks_found.is_some(), "Tracks должен быть внутри Segment");

    let track_entry_found = elems[0].find("TrackEntry");
    assert!(track_entry_found.is_some(), "TrackEntry должен быть внутри Tracks");

    let codec_id = elems[0].find("CodecID");
    assert!(codec_id.is_some(), "CodecID должен быть внутри TrackEntry");
    assert_eq!(codec_id.unwrap().payload, b"V_VP8");
}

// =============================================================================
// VINT
// =============================================================================

#[wasm_bindgen_test]
fn webm_parses_multibyte_id() {
    // Cluster id = 0x1F43B675 (4 байта), size = 0.
    let data: &[u8] = &[
        0x1F, 0x43, 0xB6, 0x75,
        0x80,
    ];
    let elems = parse_webm(data).expect("parse Cluster");
    assert_eq!(elems.len(), 1);
    assert_eq!(elems[0].id, 0x1F43B675);
    assert_eq!(elems[0].name, "Cluster");
    assert_eq!(elems[0].size, 0);
}

#[wasm_bindgen_test]
fn webm_simple_block() {
    // SimpleBlock id = 0xA3 (1 байт), size = 4.
    let data: &[u8] = &[
        0xA3, 0x84, 0x81, 0x00, 0x00, 0x00,
    ];
    let elems = parse_webm(data).expect("parse SimpleBlock");
    assert_eq!(elems.len(), 1);
    assert_eq!(elems[0].id, 0xA3);
    assert_eq!(elems[0].name, "SimpleBlock");
    assert_eq!(elems[0].size, 4);
}

// =============================================================================
// Ошибки
// =============================================================================

#[wasm_bindgen_test]
fn webm_rejects_truncated_element() {
    // id=0xA3, size=10, но данных в буфере нет.
    let data: &[u8] = &[0xA3, 0x8A];
    assert!(parse_webm(data).is_err());
}

#[wasm_bindgen_test]
fn mp4_rejects_invalid_box_size() {
    // size = 0xFFFFFFFF, но данных в файле нет.
    let data: &[u8] = &[0xFF, 0xFF, 0xFF, 0xFF, b'f', b't', b'y', b'p'];
    assert!(parse_mp4(data).is_err());
}