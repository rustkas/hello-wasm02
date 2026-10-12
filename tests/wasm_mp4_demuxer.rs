#![cfg(target_arch = "wasm32")]

//! Тесты 23.10 — MP4 demuxer.
//!
//! ## Что проверяется
//!
//! - Парсинг пяти таблиц `stbl`: `stco`, `stsc`, `stsz`, `stts`, `stss`.
//! - Преобразование таблиц в плоский список `VideoSample`:
//!   offset, size, timestamp, duration, is_key.
//! - Накопление offset-ов внутри chunk-а и переход между chunk-ами.
//! - Multi-entry `stsc` (chunk-ы разной ёмкости).
//! - Fixed-size `stsz` (`sample_size != 0`).
//! - Multi-entry `stts` (VFR — variable frame rate).
//! - Проверка границ `verify_offsets` против диапазона `mdat`.
//! - Codec-string из `avcC` и timescale из `mdhd` (v0 и v1).
//! - Негативные случаи: усечённые payload-ы, отсутствующие таблицы.
//! - Порядок таблиц внутри `stbl` не важен.

use wasm_bindgen_test::*;
use hello_wasm02::container_parser::{parse_mp4, BoxInfo};
use hello_wasm02::mp4_demuxer::{
    avc_codec_string, build_samples, parse_mdhd_timescale, parse_sample_table,
    verify_offsets,
};

wasm_bindgen_test_configure!(run_in_browser);

// =============================================================================
// Сборка тестовых данных
// =============================================================================

/// Собирает MP4 box: size + type + body.
fn mp4_box(box_type: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let size = (8 + body.len()) as u32;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(box_type);
    out.extend_from_slice(body);
    return out;
}

/// Общий заголовок `version=0, flags=0` для всех таблиц stbl.
fn v0_flags() -> Vec<u8> {
    return vec![0u8; 4];
}

fn make_stco(offsets: &[u32]) -> Vec<u8> {
    let mut body = v0_flags();
    body.extend_from_slice(&(offsets.len() as u32).to_be_bytes());
    for &o in offsets {
        body.extend_from_slice(&o.to_be_bytes());
    }
    return body;
}

fn make_stsc(entries: &[(u32, u32)]) -> Vec<u8> {
    let mut body = v0_flags();
    body.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for &(first_chunk, samples_per_chunk) in entries {
        body.extend_from_slice(&first_chunk.to_be_bytes());
        body.extend_from_slice(&samples_per_chunk.to_be_bytes());
        body.extend_from_slice(&1u32.to_be_bytes()); // sample_description_index = 1
    }
    return body;
}

/// `stsz` с явным списком размеров: `sample_size = 0`.
fn make_stsz_per_sample(sizes: &[u32]) -> Vec<u8> {
    let mut body = v0_flags();
    body.extend_from_slice(&0u32.to_be_bytes()); // sample_size = 0
    body.extend_from_slice(&(sizes.len() as u32).to_be_bytes());
    for &s in sizes {
        body.extend_from_slice(&s.to_be_bytes());
    }
    return body;
}

/// `stsz` с общим размером: `sample_size != 0`.
fn make_stsz_fixed(sample_size: u32, sample_count: u32) -> Vec<u8> {
    let mut body = v0_flags();
    body.extend_from_slice(&sample_size.to_be_bytes());
    body.extend_from_slice(&sample_count.to_be_bytes());
    return body;
}

fn make_stts(entries: &[(u32, u32)]) -> Vec<u8> {
    let mut body = v0_flags();
    body.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for &(sample_count, sample_delta) in entries {
        body.extend_from_slice(&sample_count.to_be_bytes());
        body.extend_from_slice(&sample_delta.to_be_bytes());
    }
    return body;
}

fn make_stss(sample_numbers: &[u32]) -> Vec<u8> {
    let mut body = v0_flags();
    body.extend_from_slice(&(sample_numbers.len() as u32).to_be_bytes());
    for &s in sample_numbers {
        body.extend_from_slice(&s.to_be_bytes());
    }
    return body;
}

/// Собирает `stbl` из готовых payload-ов таблиц.
///
/// Универсальный helper: покрывает и per-sample, и fixed-size `stsz`,
/// и случаи с отсутствующей `stss`.
fn build_stbl_with(
    stco: &[u8],
    stsc: &[u8],
    stsz: &[u8],
    stts: &[u8],
    stss: Option<&[u8]>,
) -> BoxInfo {
    let mut body = Vec::new();
    body.extend_from_slice(&mp4_box(b"stco", stco));
    body.extend_from_slice(&mp4_box(b"stsc", stsc));
    body.extend_from_slice(&mp4_box(b"stsz", stsz));
    body.extend_from_slice(&mp4_box(b"stts", stts));
    if let Some(stss_body) = stss {
        body.extend_from_slice(&mp4_box(b"stss", stss_body));
    }
    let bytes = mp4_box(b"stbl", &body);
    return parse_mp4(&bytes)
        .expect("stbl parse")
        .into_iter()
        .next()
        .expect("stbl должен быть одним верхнеуровневым box-ом");
}

/// `stbl` с per-sample `stsz` — самый частый вариант.
fn build_stbl(
    stco: &[u32],
    stsc: &[(u32, u32)],
    stsz: &[u32],
    stts: &[(u32, u32)],
    stss: &[u32],
) -> BoxInfo {
    return build_stbl_with(
        &make_stco(stco),
        &make_stsc(stsc),
        &make_stsz_per_sample(stsz),
        &make_stts(stts),
        Some(&make_stss(stss)),
    );
}

/// `stbl` с fixed-size `stsz`.
fn build_stbl_fixed(
    stco: &[u32],
    stsc: &[(u32, u32)],
    sample_size: u32,
    sample_count: u32,
    stts: &[(u32, u32)],
) -> BoxInfo {
    return build_stbl_with(
        &make_stco(stco),
        &make_stsc(stsc),
        &make_stsz_fixed(sample_size, sample_count),
        &make_stts(stts),
        None,
    );
}

/// Timescale, используемый в большинстве тестов: 30000.
/// При delta=1000 даёт 33.333 мс на кадр (30 fps).
const TS_30K: u32 = 30_000;

// =============================================================================
// Базовые свойства
// =============================================================================

#[wasm_bindgen_test]
fn sample_count_matches_stsz_length() {
    let stbl = build_stbl(
        &[100, 200, 300],
        &[(1, 1)],
        &[10, 20, 30],
        &[(3, 1000)],
        &[],
    );
    let table = parse_sample_table(&stbl, TS_30K).expect("parse table");
    assert_eq!(table.total_samples(), 3);

    let samples = build_samples(&table).expect("build samples");
    assert_eq!(samples.len(), 3);
}

// =============================================================================
// Арифметика offset-ов
// =============================================================================

#[wasm_bindgen_test]
fn offsets_accumulate_within_chunk() {
    // 1 chunk, 4 сэмпла. Внутри chunk-а offset растёт на размер
    // предыдущего сэмпла.
    let stbl = build_stbl(
        &[1000],
        &[(1, 4)],
        &[10, 20, 30, 40],
        &[(4, 1000)],
        &[],
    );
    let table = parse_sample_table(&stbl, TS_30K).unwrap();
    let samples = build_samples(&table).unwrap();

    assert_eq!(samples[0].offset, 1000);
    assert_eq!(samples[1].offset, 1010);
    assert_eq!(samples[2].offset, 1030);
    assert_eq!(samples[3].offset, 1060);
}

#[wasm_bindgen_test]
fn offsets_reset_between_chunks() {
    // 2 chunk-а по 2 сэмпла. Второй chunk начинается по своему
    // offset-у из stco, а не продолжает первый.
    let stbl = build_stbl(
        &[1000, 5000],
        &[(1, 2)],
        &[10, 20, 30, 40],
        &[(4, 1000)],
        &[],
    );
    let table = parse_sample_table(&stbl, TS_30K).unwrap();
    let samples = build_samples(&table).unwrap();

    assert_eq!(samples[0].offset, 1000);
    assert_eq!(samples[1].offset, 1010);
    assert_eq!(samples[2].offset, 5000); // новый chunk
    assert_eq!(samples[3].offset, 5030);
}

#[wasm_bindgen_test]
fn multi_entry_stsc_supports_variable_chunk_capacity() {
    // stsc = [(1, 4), (3, 1)] означает:
    //   chunk #1 — 4 samples  (правило с first_chunk = 1)
    //   chunk #2 — 4 samples  (то же правило)
    //   chunk #3 — 1 sample   (правило с first_chunk = 3 перекрывает)
    //   chunk #4 — 1 sample
    // Итого: 4 + 4 + 1 + 1 = 10 samples.
    let sizes = [10u32; 10];
    let stbl = build_stbl(
        &[100, 200, 300, 400],
        &[(1, 4), (3, 1)],
        &sizes,
        &[(10, 1000)],
        &[],
    );
    let table = parse_sample_table(&stbl, TS_30K).unwrap();
    assert_eq!(table.total_samples(), 10);

    let samples = build_samples(&table).unwrap();
    assert_eq!(samples.len(), 10);

    // chunk #1: samples 0..3, стартовый offset 100.
    assert_eq!(samples[0].offset, 100);
    assert_eq!(samples[3].offset, 130);
    // chunk #2: samples 4..7, стартовый offset 200.
    assert_eq!(samples[4].offset, 200);
    assert_eq!(samples[7].offset, 230);
    // chunk #3: 1 sample, offset 300.
    assert_eq!(samples[8].offset, 300);
    // chunk #4: 1 sample, offset 400.
    assert_eq!(samples[9].offset, 400);
}

// =============================================================================
// Fixed-size stsz
// =============================================================================

#[wasm_bindgen_test]
fn fixed_size_stsz_uses_constant_sample_size() {
    // Все 4 сэмпла одного размера = 100. stsz.sample_size != 0,
    // массив размеров пустой.
    let stbl = build_stbl_fixed(
        &[1000, 2000],
        &[(1, 2)],
        100, 4,
        &[(4, 1000)],
    );
    let table = parse_sample_table(&stbl, TS_30K).unwrap();
    assert_eq!(table.stsz_fixed, Some(100));
    assert!(table.stsz.is_empty(), "при fixed-size массив размеров пуст");
    assert_eq!(table.total_samples(), 4);

    let samples = build_samples(&table).unwrap();
    assert_eq!(samples.len(), 4);
    for s in &samples {
        assert_eq!(s.size, 100);
    }
    assert_eq!(samples[0].offset, 1000);
    assert_eq!(samples[1].offset, 1100);
    assert_eq!(samples[2].offset, 2000);
    assert_eq!(samples[3].offset, 2100);
}

// =============================================================================
// Key frames
// =============================================================================

#[wasm_bindgen_test]
fn stss_marks_only_listed_samples_as_key() {
    // stss = [1, 5] — key frames #1 и #5 (1-based), всего 6 samples.
    let stbl = build_stbl(
        &[0, 100, 200, 300, 400, 500],
        &[(1, 1)],
        &[10; 6],
        &[(6, 1000)],
        &[1, 5],
    );
    let table = parse_sample_table(&stbl, TS_30K).unwrap();
    let samples = build_samples(&table).unwrap();

    assert_eq!(samples.len(), 6);
    assert!(samples[0].is_key, "sample #0 (1-based 1) — key");
    assert!(!samples[1].is_key);
    assert!(!samples[2].is_key);
    assert!(!samples[3].is_key);
    assert!(samples[4].is_key, "sample #4 (1-based 5) — key");
    assert!(!samples[5].is_key);
}

#[wasm_bindgen_test]
fn empty_stss_means_all_frames_are_key() {
    // Пустой stss = все сэмплы ключевые (например, для анимаций).
    let stbl = build_stbl(
        &[0, 100, 200],
        &[(1, 1)],
        &[10, 20, 30],
        &[(3, 1000)],
        &[],
    );
    let table = parse_sample_table(&stbl, TS_30K).unwrap();
    let samples = build_samples(&table).unwrap();

    assert!(samples.iter().all(|s| s.is_key),
        "при пустом stss все frames должны быть key");
}

// =============================================================================
// Timestamps и durations
// =============================================================================

#[wasm_bindgen_test]
fn timestamps_accumulate_with_constant_delta() {
    // 3 сэмпла, delta = 1000, timescale = 30000 → 33.333 мс.
    let stbl = build_stbl(
        &[0, 100, 200],
        &[(1, 1)],
        &[10, 20, 30],
        &[(3, 1000)],
        &[],
    );
    let table = parse_sample_table(&stbl, TS_30K).unwrap();
    let samples = build_samples(&table).unwrap();

    assert_eq!(samples[0].timestamp_us, 0);
    assert_eq!(samples[0].duration_us, 33_333);
    assert_eq!(samples[1].timestamp_us, 33_333);
    assert_eq!(samples[2].timestamp_us, 66_666);
}

#[wasm_bindgen_test]
fn multi_entry_stts_supports_vfr() {
    // 2 кадра по 33.3 мс (delta = 1000) + 2 кадра по 50 мс (delta = 1500).
    // Timestamp s3 = 2×33.333 + 1×50.000 = 116.666 мс.
    // (Второй 50-мс интервал относится к s4, которого нет.)
    let stbl = build_stbl(
        &[0, 100, 200, 300],
        &[(1, 1)],
        &[10; 4],
        &[(2, 1000), (2, 1500)],
        &[],
    );
    let table = parse_sample_table(&stbl, TS_30K).unwrap();
    let samples = build_samples(&table).unwrap();

    assert_eq!(samples[0].duration_us, 33_333);
    assert_eq!(samples[1].duration_us, 33_333);
    assert_eq!(samples[2].duration_us, 50_000);
    assert_eq!(samples[3].duration_us, 50_000);

    assert_eq!(samples[0].timestamp_us, 0);
    assert_eq!(samples[1].timestamp_us, 33_333);
    assert_eq!(samples[2].timestamp_us, 66_666);
    assert_eq!(samples[3].timestamp_us, 116_666);
}

#[wasm_bindgen_test]
fn timestamps_scale_with_timescale() {
    // Тот же delta = 1000, но timescale = 1000 → ровно 1 секунда.
    let stbl = build_stbl(
        &[0, 100],
        &[(1, 1)],
        &[10, 10],
        &[(2, 1000)],
        &[],
    );
    let table = parse_sample_table(&stbl, 1000).unwrap();
    let samples = build_samples(&table).unwrap();

    assert_eq!(samples[0].duration_us, 1_000_000);
    assert_eq!(samples[1].timestamp_us, 1_000_000);
}

// =============================================================================
// verify_offsets
// =============================================================================

#[wasm_bindgen_test]
fn verify_offsets_accepts_valid_range() {
    let stbl = build_stbl(
        &[1000, 2000],
        &[(1, 2)],
        &[10, 20, 30, 40],
        &[(4, 1000)],
        &[],
    );
    let table = parse_sample_table(&stbl, TS_30K).unwrap();
    let samples = build_samples(&table).unwrap();

    // mdat: [1000, 2070). Последний sample кончается ровно на 2070.
    assert!(verify_offsets(&samples, 1000, 2070).is_ok());
}

#[wasm_bindgen_test]
fn verify_offsets_rejects_sample_after_mdat_end() {
    let stbl = build_stbl(
        &[1000, 2000],
        &[(1, 2)],
        &[10, 20, 30, 40],
        &[(4, 1000)],
        &[],
    );
    let table = parse_sample_table(&stbl, TS_30K).unwrap();
    let samples = build_samples(&table).unwrap();

    // mdat: [1000, 1500). Sample #2 (offset 2000) вне диапазона.
    let result = verify_offsets(&samples, 1000, 1500);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("sample #2"),
        "сообщение должно указывать на проблемный sample, получено: {msg}");
}

#[wasm_bindgen_test]
fn verify_offsets_rejects_sample_before_mdat_start() {
    let stbl = build_stbl(
        &[500, 2000],
        &[(1, 2)],
        &[10, 20, 30, 40],
        &[(4, 1000)],
        &[],
    );
    let table = parse_sample_table(&stbl, TS_30K).unwrap();
    let samples = build_samples(&table).unwrap();

    // mdat: [1000, 3000). Sample #0 (offset 500) до mdat.
    let result = verify_offsets(&samples, 1000, 3000);
    assert!(result.is_err(), "sample до mdat должен отвергаться");
    let msg = result.unwrap_err();
    assert!(msg.contains("sample #0"), "получено: {msg}");
}

// =============================================================================
// Codec string
// =============================================================================

#[wasm_bindgen_test]
fn codec_string_from_avcc_baseline() {
    // avcC[1..4] = 42 00 1E — Baseline Profile, level 3.0.
    let avcc = &[
        0x01, 0x42, 0x00, 0x1E,
        0xFF, 0xE1,
        0x00, 0x04, 0x67, 0x42, 0x00, 0x1E,
        0x01,
        0x00, 0x02, 0x68, 0xCE,
    ];
    assert_eq!(
        avc_codec_string(avcc).as_deref(),
        Some("avc1.42001E"),
    );
}

#[wasm_bindgen_test]
fn codec_string_from_avcc_high_profile() {
    // avcC[1..4] = 64 00 28 — High Profile, level 4.0.
    let avcc = &[0x01, 0x64, 0x00, 0x28, 0xFF, 0xE1];
    assert_eq!(
        avc_codec_string(avcc).as_deref(),
        Some("avc1.640028"),
    );
}

#[wasm_bindgen_test]
fn codec_string_rejects_too_short_avcc() {
    // Меньше 4 байт — профиль/level прочитать нельзя.
    assert!(avc_codec_string(&[0x01, 0x42]).is_none());
}

// =============================================================================
// mdhd timescale
// =============================================================================

fn make_mdhd_v0(timescale: u32) -> BoxInfo {
    // version 0: [ver+flags: 4][ctime: 4][mtime: 4][timescale: 4][duration: 4]
    let mut payload = vec![0u8; 12];
    payload.extend_from_slice(&timescale.to_be_bytes());
    payload.extend_from_slice(&0u32.to_be_bytes());
    return BoxInfo {
        box_type: String::from("mdhd"),
        offset: 0,
        size: 8 + payload.len() as u64,
        header_size: 8,
        children: Vec::new(),
        payload,
    };
}

fn make_mdhd_v1(timescale: u32) -> BoxInfo {
    // version 1: 64-битные ctime и mtime — timescale на offset 20.
    let mut payload = vec![1u8, 0, 0, 0];
    payload.extend_from_slice(&[0u8; 8]); // creation_time (u64)
    payload.extend_from_slice(&[0u8; 8]); // modification_time (u64)
    payload.extend_from_slice(&timescale.to_be_bytes());
    payload.extend_from_slice(&[0u8; 8]); // duration (u64)
    return BoxInfo {
        box_type: String::from("mdhd"),
        offset: 0,
        size: 8 + payload.len() as u64,
        header_size: 8,
        children: Vec::new(),
        payload,
    };
}

#[wasm_bindgen_test]
fn mdhd_timescale_v0_reads_offset_12() {
    let mdhd = make_mdhd_v0(30_000);
    assert_eq!(parse_mdhd_timescale(&mdhd), Some(30_000));
}

#[wasm_bindgen_test]
fn mdhd_timescale_v1_reads_offset_20() {
    let mdhd = make_mdhd_v1(90_000);
    assert_eq!(parse_mdhd_timescale(&mdhd), Some(90_000));
}

#[wasm_bindgen_test]
fn mdhd_rejects_zero_timescale() {
    // timescale = 0 делает все timestamp-ы бессмысленными.
    let mdhd = make_mdhd_v0(0);
    assert!(parse_mdhd_timescale(&mdhd).is_none());
}

#[wasm_bindgen_test]
fn mdhd_rejects_truncated_payload() {
    let mdhd = BoxInfo {
        box_type: String::from("mdhd"),
        offset: 0,
        size: 16,
        header_size: 8,
        children: Vec::new(),
        payload: vec![0u8; 8], // меньше 12 байт version 0
    };
    assert!(parse_mdhd_timescale(&mdhd).is_none());
}

// =============================================================================
// Негативные случаи
// =============================================================================

#[wasm_bindgen_test]
fn missing_stco_returns_error() {
    // stbl без stco — обязательная таблица отсутствует.
    let body = mp4_box(b"stsc", &make_stsc(&[(1, 1)]));
    let bytes = mp4_box(b"stbl", &body);
    let stbl = parse_mp4(&bytes)
        .expect("stbl parse")
        .into_iter()
        .next()
        .expect("one box");

    let result = parse_sample_table(&stbl, TS_30K);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("stco"));
}

#[wasm_bindgen_test]
fn truncated_stsz_returns_error() {
    // stsz объявляет 5 samples, но массива размеров нет.
    let mut stsz_body = v0_flags();
    stsz_body.extend_from_slice(&0u32.to_be_bytes()); // sample_size = 0
    stsz_body.extend_from_slice(&5u32.to_be_bytes()); // entry_count = 5
    // массив размеров отсутствует — payload обрезан.

    let stbl = build_stbl_with(
        &make_stco(&[0, 100]),
        &make_stsc(&[(1, 1)]),
        &stsz_body,
        &make_stts(&[(5, 1000)]),
        None,
    );

    let result = parse_sample_table(&stbl, TS_30K);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("stsz"));
}

#[wasm_bindgen_test]
fn truncated_stco_returns_error() {
    // stco объявляет 3 offsets, но содержит только один.
    let mut stco_body = v0_flags();
    stco_body.extend_from_slice(&3u32.to_be_bytes());
    stco_body.extend_from_slice(&100u32.to_be_bytes()); // 1 из 3

    let stbl = build_stbl_with(
        &stco_body,
        &make_stsc(&[(1, 1)]),
        &make_stsz_per_sample(&[10, 20, 30]),
        &make_stts(&[(3, 1000)]),
        None,
    );

    let result = parse_sample_table(&stbl, TS_30K);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("stco"));
}

// =============================================================================
// Порядок таблиц внутри stbl
// =============================================================================

#[wasm_bindgen_test]
fn table_order_inside_stbl_does_not_matter() {
    // MP4-спецификация не фиксирует порядок таблиц внутри stbl.
    // Парсер должен находить их независимо от порядка.
    let mut body = Vec::new();
    // Обратный порядок: stss, stts, stsz, stsc, stco.
    body.extend_from_slice(&mp4_box(b"stss", &make_stss(&[1])));
    body.extend_from_slice(&mp4_box(b"stts", &make_stts(&[(2, 1000)])));
    body.extend_from_slice(&mp4_box(b"stsz", &make_stsz_per_sample(&[10, 20])));
    body.extend_from_slice(&mp4_box(b"stsc", &make_stsc(&[(1, 1)])));
    body.extend_from_slice(&mp4_box(b"stco", &make_stco(&[100, 200])));
    let bytes = mp4_box(b"stbl", &body);
    let stbl = parse_mp4(&bytes)
        .expect("stbl parse")
        .into_iter()
        .next()
        .expect("one box");

    let table = parse_sample_table(&stbl, TS_30K).expect("parse table");
    let samples = build_samples(&table).expect("build samples");

    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].offset, 100);
    assert_eq!(samples[1].offset, 200);
    assert!(samples[0].is_key);
    assert!(!samples[1].is_key);
}