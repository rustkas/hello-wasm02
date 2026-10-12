#![cfg(target_arch = "wasm32")]

//! Тесты 23.11 — MP4 muxer.
//!
//! ## Что проверяется
//!
//! - `build_mp4` создаёт валидный MP4 (ftyp + moov + mdat).
//! - Собранный файл читается нашим же demuxer-ом из 23.10.
//! - `stco[0]` указывает на первый байт mdat body.
//! - `stss` содержит только key indices.
//! - Sample sizes и timestamps совпадают с входными chunks.
//! - Пограничные случаи: пустой массив, невалидный avcC.

use wasm_bindgen_test::*;
use hello_wasm02::container_parser::{find_avcc_in_stsd, parse_mp4};
use hello_wasm02::mp4_demuxer::{
    build_samples, parse_mdhd_timescale, parse_sample_table,
};
use hello_wasm02::mp4_muxer::{build_mp4, MuxConfig, VideoChunk};

wasm_bindgen_test_configure!(run_in_browser);

// =============================================================================
// Хелперы
// =============================================================================

/// Тестовый avcC: baseline profile 0x42, level 0x1E, SPS+PPS.
fn test_avcc() -> Vec<u8> {
    return vec![
        0x01, 0x42, 0x00, 0x1E, // version, profile, compat, level
        0xFF, 0xE1, // lengthSizeMinusOne = 3, numSPS = 1
        0x00, 0x04, 0x67, 0x42, 0x00, 0x1E, // SPS
        0x01, // numPPS = 1
        0x00, 0x02, 0x68, 0xCE, // PPS
    ];
}

fn test_config() -> MuxConfig {
    return MuxConfig {
        width: 640,
        height: 480,
        avcc: test_avcc(),
    };
}

/// N chunks с разными размерами.
fn make_chunks(n: usize) -> Vec<VideoChunk> {
    let mut chunks = Vec::with_capacity(n);
    for i in 0..n {
        // Ключевые — #0 и #n/2.
        let is_key = i == 0 || (n > 2 && i == n / 2);
        chunks.push(VideoChunk {
            is_key,
            timestamp_us: (i as u64) * 33_333,
            duration_us: 33_333,
            data: vec![(i & 0xFF) as u8; 100 + i * 10],
        });
    }
    return chunks;
}

// =============================================================================
// Базовые свойства
// =============================================================================

#[wasm_bindgen_test]
fn mux_builds_valid_mp4_with_ftyp() {
    let chunks = make_chunks(5);
    let mp4 = build_mp4(&chunks, &test_config()).expect("build_mp4");

    // Первые 4 байта — size=32, следующие 4 — 'ftyp'.
    assert!(mp4.len() >= 8);
    assert_eq!(&mp4[4..8], b"ftyp");
}

#[wasm_bindgen_test]
fn mux_output_parses_with_container_parser() {
    let chunks = make_chunks(5);
    let mp4 = build_mp4(&chunks, &test_config()).expect("build_mp4");

    let boxes = parse_mp4(&mp4).expect("parse_mp4");
    let types: Vec<&str> = boxes.iter().map(|b| b.box_type.as_str()).collect();
    assert!(types.contains(&"ftyp"));
    assert!(types.contains(&"moov"));
    assert!(types.contains(&"mdat"));
}

#[wasm_bindgen_test]
fn mux_moov_contains_trak_and_stsd() {
    let chunks = make_chunks(5);
    let mp4 = build_mp4(&chunks, &test_config()).expect("build_mp4");
    let boxes = parse_mp4(&mp4).expect("parse_mp4");

    let moov = boxes.iter().find(|b| b.box_type == "moov").expect("moov");
    assert!(moov.find("trak").is_some(), "trak в moov");

    // stsd — не контейнер в parse_mp4: его payload содержит
    // VisualSampleEntry + avcC. Ищем avcC через find_avcc_in_stsd.
    let stsd = moov.find("stsd").expect("stsd в stbl");
    let avcc = find_avcc_in_stsd(stsd).expect("avcC в payload stsd");

    // avcC начинается с version=1, profile=0x42 (Baseline).
    assert_eq!(avcc[0], 0x01, "avcC.version");
    assert_eq!(avcc[1], 0x42, "avcC.profile (baseline)");
}

// =============================================================================
// Round-trip через demuxer 23.10
// =============================================================================

#[wasm_bindgen_test]
fn mux_sample_table_matches_input() {
    let chunks = make_chunks(7);
    let mp4 = build_mp4(&chunks, &test_config()).expect("build_mp4");
    let boxes = parse_mp4(&mp4).expect("parse_mp4");

    let moov = boxes.iter().find(|b| b.box_type == "moov").expect("moov");
    let mdhd = moov.find("mdhd").expect("mdhd");
    let timescale = parse_mdhd_timescale(mdhd).expect("timescale");
    let stbl = moov.find("stbl").expect("stbl");

    let table = parse_sample_table(stbl, timescale).expect("sample table");
    let samples = build_samples(&table).expect("build_samples");

    assert_eq!(samples.len(), 7);

    for (i, s) in samples.iter().enumerate() {
        assert_eq!(s.size, chunks[i].data.len() as u32);
        assert_eq!(s.timestamp_us, chunks[i].timestamp_us);
        assert_eq!(s.is_key, chunks[i].is_key);
    }
}

#[wasm_bindgen_test]
fn mux_offsets_point_into_mdat() {
    let chunks = make_chunks(4);
    let mp4 = build_mp4(&chunks, &test_config()).expect("build_mp4");
    let boxes = parse_mp4(&mp4).expect("parse_mp4");

    let mdat = boxes.iter().find(|b| b.box_type == "mdat").expect("mdat");
    let moov = boxes.iter().find(|b| b.box_type == "moov").expect("moov");
    let stbl = moov.find("stbl").expect("stbl");
    let timescale = parse_mdhd_timescale(moov.find("mdhd").unwrap()).unwrap();

    let table = parse_sample_table(stbl, timescale).unwrap();
    let samples = build_samples(&table).unwrap();

    // Все sample offset-ы должны быть внутри [mdat.offset+8, mdat.offset+mdat.size).
    let mdat_start = mdat.offset as u64 + 8;
    let mdat_end = mdat.offset as u64 + mdat.size;
    for s in &samples {
        assert!(
            s.offset >= mdat_start,
            "offset {} < mdat_start {}",
            s.offset,
            mdat_start,
        );
        assert!(
            s.offset + s.size as u64 <= mdat_end,
            "sample выходит за mdat",
        );
    }
}

#[wasm_bindgen_test]
fn mux_key_frames_match_input() {
    let chunks = make_chunks(10);
    let mp4 = build_mp4(&chunks, &test_config()).expect("build_mp4");
    let boxes = parse_mp4(&mp4).expect("parse_mp4");
    let moov = boxes.iter().find(|b| b.box_type == "moov").unwrap();
    let stbl = moov.find("stbl").unwrap();
    let timescale = parse_mdhd_timescale(moov.find("mdhd").unwrap()).unwrap();

    let table = parse_sample_table(stbl, timescale).unwrap();
    let samples = build_samples(&table).unwrap();

    let input_keys: Vec<bool> = chunks.iter().map(|c| c.is_key).collect();
    let output_keys: Vec<bool> = samples.iter().map(|s| s.is_key).collect();
    assert_eq!(input_keys, output_keys);
}

#[wasm_bindgen_test]
fn mux_total_size_matches_sum() {
    let chunks = make_chunks(5);
    let expected_mdat_body: usize = chunks.iter().map(|c| c.data.len()).sum();
    let mp4 = build_mp4(&chunks, &test_config()).expect("build_mp4");
    let boxes = parse_mp4(&mp4).expect("parse_mp4");

    let mdat = boxes.iter().find(|b| b.box_type == "mdat").expect("mdat");
    // mdat.size = 8 (header) + сумма chunks.
    assert_eq!(mdat.size as usize, 8 + expected_mdat_body);
}

// =============================================================================
// Пограничные случаи
// =============================================================================

#[wasm_bindgen_test]
fn mux_rejects_empty_chunks() {
    let result = build_mp4(&[], &test_config());
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn mux_rejects_zero_width() {
    let chunks = make_chunks(2);
    let mut cfg = test_config();
    cfg.width = 0;
    let result = build_mp4(&chunks, &cfg);
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn mux_rejects_short_avcc() {
    let chunks = make_chunks(2);
    let mut cfg = test_config();
    cfg.avcc = vec![0x01, 0x42];
    let result = build_mp4(&chunks, &cfg);
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn mux_single_chunk_works() {
    let chunks = make_chunks(1);
    let mp4 = build_mp4(&chunks, &test_config()).expect("build_mp4");
    let boxes = parse_mp4(&mp4).expect("parse_mp4");
    assert!(boxes.iter().any(|b| b.box_type == "moov"));
    assert!(boxes.iter().any(|b| b.box_type == "mdat"));
}