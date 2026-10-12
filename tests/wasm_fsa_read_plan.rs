//! Тесты `fsa_read_*` — план, лимиты, magic-байты, peek, text/bytes/slice,
//! поток и три инкрементальные задачи.

use wasm_bindgen_test::*;
use web_sys::File;

use hello_wasm02::fsa_read_ops::{
    count_lines_stream, find_marker_stream, format_chunk_sizes, format_hexdump,
    format_stream_stats, hash_stream, peek_bytes, read_bytes_safe,
    read_slice_safe, read_stream_stats, read_text_safe, StreamStats,
};
use hello_wasm02::fsa_read_plan::{
    detect_magic_format, explain_error, explain_plan, format_magic_line,
    format_size, suggest_plan, validate_plan, ReadError, ReadLimits, ReadPlan,
};

wasm_bindgen_test_configure!(run_in_browser);

fn make_file(content: &[u8], name: &str, mime: &str) -> File {
    // Blob создаём без типа — важно только содержимое.
    let u8arr = js_sys::Uint8Array::from(content);
    let blob = web_sys::Blob::new_with_u8_array_sequence(
        &js_sys::Array::of1(&u8arr),
    )
    .unwrap();

    // MIME задаётся через FilePropertyBag, а не через Blob:
    // File constructor берёт type только из options.type.
    let file_opts = web_sys::FilePropertyBag::new();
    file_opts.set_type(mime);

    web_sys::File::new_with_blob_sequence_and_options(
        &js_sys::Array::of1(&blob),
        name,
        &file_opts,
    )
    .unwrap()
}
fn small_limits() -> ReadLimits {
    return ReadLimits {
        max_buffer_bytes: 1024,
        max_text_bytes: 512,
        peek_default: 8,
    };
}

// ---------------------------------------------------------------------------
// detect_magic_format
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn magic_detects_png() {
    let bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    assert_eq!(detect_magic_format(&bytes), Some("PNG (изображение)"));
}

#[wasm_bindgen_test]
fn magic_detects_jpeg() {
    let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00];
    assert_eq!(detect_magic_format(&bytes), Some("JPEG (изображение)"));
}

#[wasm_bindgen_test]
fn magic_detects_pdf() {
    let bytes = b"%PDF-1.4";
    assert_eq!(detect_magic_format(bytes), Some("PDF (документ)"));
}

#[wasm_bindgen_test]
fn magic_detects_zip() {
    let bytes = [0x50, 0x4B, 0x03, 0x04, 0x14];
    assert_eq!(detect_magic_format(&bytes), Some("ZIP (архив)"));
}

#[wasm_bindgen_test]
fn magic_detects_webp() {
    let mut bytes: Vec<u8> = Vec::with_capacity(12);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(b"WEBP");
    assert_eq!(detect_magic_format(&bytes), Some("WEBP (изображение)"));
}

#[wasm_bindgen_test]
fn magic_detects_gzip() {
    let bytes = [0x1F, 0x8B, 0x08];
    assert_eq!(detect_magic_format(&bytes), Some("GZIP (архив)"));
}

#[wasm_bindgen_test]
fn magic_unknown_returns_none() {
    let bytes = b"hello world";
    assert_eq!(detect_magic_format(bytes), None);
}

#[wasm_bindgen_test]
fn magic_empty_returns_none() {
    assert_eq!(detect_magic_format(&[]), None);
}

#[wasm_bindgen_test]
fn magic_line_hex() {
    let bytes = [0x89, 0x50, 0x4E, 0x47];
    let line = format_magic_line(&bytes);
    assert_eq!(line, "89 50 4E 47");
}

// ---------------------------------------------------------------------------
// suggest_plan / validate_plan
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn suggest_text_small_is_text() {
    let plan = suggest_plan(100, "text/plain", &ReadLimits::demo());
    assert_eq!(plan, ReadPlan::Text);
}

#[wasm_bindgen_test]
fn suggest_binary_small_is_bytes() {
    let plan = suggest_plan(100, "image/png", &ReadLimits::demo());
    assert_eq!(plan, ReadPlan::Bytes);
}

#[wasm_bindgen_test]
fn suggest_huge_binary_is_stream() {
    let plan = suggest_plan(500 * 1024 * 1024, "video/mp4", &ReadLimits::demo());
    assert_eq!(plan, ReadPlan::Stream);
}

#[wasm_bindgen_test]
fn validate_text_rejects_binary_mime() {
    let limits = ReadLimits::demo();
    let res = validate_plan(&ReadPlan::Text, 100, "image/png", &limits);
    assert!(matches!(res, Err(ReadError::NotApplicable { .. })));
}

#[wasm_bindgen_test]
fn validate_bytes_rejects_too_large() {
    let limits = small_limits();
    let size = limits.max_buffer_bytes + 1;
    let res = validate_plan(&ReadPlan::Bytes, size, "application/octet-stream", &limits);
    assert!(matches!(res, Err(ReadError::TooLarge { .. })));
}

#[wasm_bindgen_test]
fn validate_slice_rejects_end_above_size() {
    let limits = small_limits();
    let res = validate_plan(
        &ReadPlan::Slice { start: 0, end: 200 },
        100,
        "application/octet-stream",
        &limits,
    );
    assert!(matches!(res, Err(ReadError::Io(_))));
}

// ---------------------------------------------------------------------------
// peek_bytes
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn peek_returns_first_bytes() {
    let data: Vec<u8> = (0u8..=255).collect();
    let file = make_file(&data, "peek.bin", "application/octet-stream");
    let peeked = peek_bytes(&file, 8).await.expect("peek");
    assert_eq!(peeked, &data[..8]);
}

#[wasm_bindgen_test]
async fn peek_short_file_returns_all() {
    let data = b"abc";
    let file = make_file(data, "short.bin", "application/octet-stream");
    let peeked = peek_bytes(&file, 100).await.expect("peek");
    assert_eq!(peeked, data);
}

// ---------------------------------------------------------------------------
// format_hexdump
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn hexdump_empty() {
    assert_eq!(format_hexdump(&[], 0), "(нет данных)");
}

#[wasm_bindgen_test]
fn hexdump_single_line() {
    let bytes = &[0x89, 0x50, 0x4E, 0x47];
    let out = format_hexdump(bytes, 0);
    assert!(out.contains("00000000:"));
    assert!(out.contains("89 50 4E 47"));
}

// ---------------------------------------------------------------------------
// read_text / read_bytes / read_slice
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn read_text_roundtrip() {
    let file = make_file(b"Hello, 19.4!", "a.txt", "text/plain");
    let text = read_text_safe(&file, &ReadLimits::demo()).await.expect("text");
    assert_eq!(text, "Hello, 19.4!");
}

#[wasm_bindgen_test]
async fn read_text_rejects_binary() {
    let file = make_file(b"\x89PNG...", "a.bin", "image/png");
    let res = read_text_safe(&file, &ReadLimits::demo()).await;
    assert!(matches!(res, Err(ReadError::NotApplicable { .. })));
}

#[wasm_bindgen_test]
async fn read_bytes_roundtrip() {
    let data: Vec<u8> = (0u8..=255).collect();
    let file = make_file(&data, "a.bin", "application/octet-stream");
    let bytes = read_bytes_safe(&file, &ReadLimits::demo()).await.expect("bytes");
    assert_eq!(bytes, data);
}

#[wasm_bindgen_test]
async fn read_slice_reads_range() {
    let data: Vec<u8> = (0u8..=255).collect();
    let file = make_file(&data, "slice.bin", "application/octet-stream");
    let bytes = read_slice_safe(&file, 10, 30, &ReadLimits::demo())
        .await
        .expect("slice");
    assert_eq!(bytes, &data[10..30]);
}

// ---------------------------------------------------------------------------
// Stream
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn stream_reads_full_file() {
    let data = vec![0xABu8; 128 * 1024];
    let file = make_file(&data, "s.bin", "application/octet-stream");
    let stats = read_stream_stats(&file, |_| {}, || false).await.expect("stream");
    assert_eq!(stats.bytes_read, data.len() as u64);
    assert!(stats.matches_size(data.len() as u64));
}

#[wasm_bindgen_test]
async fn stream_cancel_immediately() {
    let data = vec![0u8; 64 * 1024];
    let file = make_file(&data, "c.bin", "application/octet-stream");
    let stats = read_stream_stats(&file, |_| {}, || true).await.expect("stream");
    assert!(stats.cancelled);
}

#[wasm_bindgen_test]
fn stream_stats_format() {
    let stats = StreamStats {
        bytes_read: 1024 * 1024,
        chunks: 16,
        yields: 2,
        elapsed_ms: 10.0,
        cancelled: false,
        first_chunk_sizes: vec![65536, 65536, 65536],
    };
    let text = format_stream_stats(&stats, 1024 * 1024);
    assert!(text.contains("✓"));
    assert!(text.contains("64.0 КБ"));
}

#[wasm_bindgen_test]
fn chunk_sizes_format() {
    assert_eq!(format_chunk_sizes(&[]), "(нет)");
    let s = format_chunk_sizes(&[1024, 2048]);
    assert!(s.contains("1.0 КБ"));
    assert!(s.contains("2.0 КБ"));
}

// ---------------------------------------------------------------------------
// hash_stream
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn hash_stream_matches_full() {
    let data: Vec<u8> = (0u8..=255).cycle().take(100_000).collect();
    let file = make_file(&data, "h.bin", "application/octet-stream");

    // Синхронный хеш того же буфера.
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(&data);
    let expected = hex::encode(h.finalize());

    let result = hash_stream(&file, |_| {}, || false).await.expect("hash");
    assert_eq!(result.hex, expected);
    assert_eq!(result.stats.bytes_read, data.len() as u64);
}

// ---------------------------------------------------------------------------
// count_lines_stream
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn count_lines_basic() {
    let data = b"line1\nline2\nline3\n";
    let file = make_file(data, "a.txt", "text/plain");
    let r = count_lines_stream(&file, |_| {}, || false).await.expect("lines");
    assert_eq!(r.count, 3);
}

#[wasm_bindgen_test]
async fn count_lines_no_trailing_newline() {
    let data = b"a\nb\nc";
    let file = make_file(data, "a.txt", "text/plain");
    let r = count_lines_stream(&file, |_| {}, || false).await.expect("lines");
    assert_eq!(r.count, 3);
}

#[wasm_bindgen_test]
async fn count_lines_empty() {
    let file = make_file(b"", "a.txt", "text/plain");
    let r = count_lines_stream(&file, |_| {}, || false).await.expect("lines");
    assert_eq!(r.count, 0);
}

// ---------------------------------------------------------------------------
// find_marker_stream
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn find_marker_at_offset() {
    let mut data = vec![b'A'; 1000];
    data.extend_from_slice(b"MARKER");
    data.extend_from_slice(&[b'B'; 500]);
    let file = make_file(&data, "m.bin", "application/octet-stream");
    let r = find_marker_stream(&file, b"MARKER", |_| {}, || false)
        .await
        .expect("marker");
    assert!(r.found);
    assert_eq!(r.offset, Some(1000));
}

#[wasm_bindgen_test]
async fn find_marker_across_chunks() {
    // Большой файл: маркер стоит примерно в середине.
    let mut data = vec![b'X'; 200_000];
    data.extend_from_slice(b"NEEDLE");
    data.extend_from_slice(&[b'Y'; 200_000]);
    let file = make_file(&data, "m.bin", "application/octet-stream");
    let r = find_marker_stream(&file, b"NEEDLE", |_| {}, || false)
        .await
        .expect("marker");
    assert!(r.found);
    assert_eq!(r.offset, Some(200_000));
}

#[wasm_bindgen_test]
async fn find_marker_not_found() {
    let data = vec![b'X'; 10_000];
    let file = make_file(&data, "m.bin", "application/octet-stream");
    let r = find_marker_stream(&file, b"NEEDLE", |_| {}, || false)
        .await
        .expect("marker");
    assert!(!r.found);
    assert_eq!(r.offset, None);
}

#[wasm_bindgen_test]
async fn find_marker_empty_is_error() {
    let data = vec![0u8; 100];
    let file = make_file(&data, "m.bin", "application/octet-stream");
    let res = find_marker_stream(&file, b"", |_| {}, || false).await;
    assert!(matches!(res, Err(ReadError::Io(_))));
}

// ---------------------------------------------------------------------------
// format_* / explain_*
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn format_size_units() {
    assert_eq!(format_size(0), "0 Б");
    assert_eq!(format_size(512), "512 Б");
    assert_eq!(format_size(2048), "2.0 КБ");
}

#[wasm_bindgen_test]
fn explain_plan_nonempty() {
    for plan in [
        ReadPlan::Text,
        ReadPlan::Bytes,
        ReadPlan::Slice { start: 0, end: 16 },
        ReadPlan::Stream,
    ] {
        let text = explain_plan(&plan, 100, "application/octet-stream");
        assert!(!text.is_empty());
    }
}

#[wasm_bindgen_test]
fn explain_error_mentions_stream() {
    let e = ReadError::TooLarge {
        size: 500_000_000,
        limit: 100_000_000,
        plan_name: "Bytes",
    };
    let s = explain_error(&e);
    assert!(s.contains("Stream"));
}