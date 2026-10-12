use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen_test::*;
use hello_wasm02::hashing::*;

wasm_bindgen_test_configure!(run_in_browser);

// ===============================================================
// Известные векторы SHA-256 из NIST
// ===============================================================

#[wasm_bindgen_test]
fn test_sha2_empty() {
    // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    let hex = sha256_via_sha2(b"");
    assert_eq!(
        hex,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[wasm_bindgen_test]
fn test_sha2_abc() {
    // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
    let hex = sha256_via_sha2(b"abc");
    assert_eq!(
        hex,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[wasm_bindgen_test]
fn test_sha2_hello() {
    // SHA-256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
    let hex = sha256_via_sha2(b"hello");
    assert_eq!(
        hex,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
}

// ===============================================================
// Совпадение реализаций: sha2 crate и SubtleCrypto
// ===============================================================

#[wasm_bindgen_test]
async fn test_subtle_matches_sha2() {
    let data = b"The quick brown fox jumps over the lazy dog";
    let rust_hex = sha256_via_sha2(data);
    let subtle_hex = sha256_via_subtle(data)
        .await
        .expect("sha256_via_subtle");
    assert_eq!(rust_hex, subtle_hex);
}

#[wasm_bindgen_test]
async fn test_subtle_matches_sha2_empty() {
    let data: &[u8] = &[];
    let rust_hex = sha256_via_sha2(data);
    let subtle_hex = sha256_via_subtle(data)
        .await
        .expect("sha256_via_subtle");
    assert_eq!(rust_hex, subtle_hex);
}

#[wasm_bindgen_test]
async fn test_subtle_matches_sha2_binary() {
    let data: Vec<u8> = (0..=255u8).collect();
    let rust_hex = sha256_via_sha2(&data);
    let subtle_hex = sha256_via_subtle(&data)
        .await
        .expect("sha256_via_subtle");
    assert_eq!(rust_hex, subtle_hex);
}

// ===============================================================
// Стриминговый вариант без callback
// ===============================================================

fn make_blob(bytes: &[u8]) -> web_sys::Blob {
    let uint8 = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::of1(&uint8.buffer());
    return web_sys::Blob::new_with_u8_array_sequence(&parts)
        .expect("Blob::new_with_u8_array_sequence");
}

#[wasm_bindgen_test]
async fn test_stream_matches_in_memory_small() {
    let data: Vec<u8> = b"hello world".to_vec();
    let blob = make_blob(&data);

    let memory_hex = sha256_via_sha2(&data);
    let stream_hex = sha256_stream_with_progress(&blob, 1024, |_, _| {})
        .await
        .expect("sha256_stream_with_progress");
    assert_eq!(memory_hex, stream_hex);
}

#[wasm_bindgen_test]
async fn test_stream_matches_in_memory_multi_chunk() {
    let data: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&data);

    let memory_hex = sha256_via_sha2(&data);
    let stream_hex = sha256_stream_with_progress(&blob, 16 * 1024, |_, _| {})
        .await
        .expect("sha256_stream_with_progress");
    assert_eq!(memory_hex, stream_hex);
}

#[wasm_bindgen_test]
async fn test_stream_matches_subtle() {
    let data: Vec<u8> = (0..50_000).map(|i| (i % 251) as u8).collect();
    let blob = make_blob(&data);

    let stream_hex = sha256_stream_with_progress(&blob, 8 * 1024, |_, _| {})
        .await
        .expect("sha256_stream_with_progress");
    let subtle_hex = sha256_via_subtle(&data)
        .await
        .expect("sha256_via_subtle");
    assert_eq!(stream_hex, subtle_hex);
}

#[wasm_bindgen_test]
async fn test_stream_empty_blob() {
    let blob = make_blob(&[]);
    let stream_hex = sha256_stream_with_progress(&blob, 1024, |_, _| {})
        .await
        .expect("sha256_stream_with_progress");
    assert_eq!(
        stream_hex,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

// ===============================================================
// Стриминговый вариант с callback прогресса
// ===============================================================

/// Собирает все вызовы callback в вектор.
///
/// Возвращает `(Rc<RefCell<Vec<(offset, total)>>>, замыкание-обёртка)`.
/// Замыкание клонирует `Rc`, поэтому его можно передать в
/// `sha256_stream_with_progress`.
fn collect_progress() -> (
    Rc<RefCell<Vec<(f64, f64)>>>,
    impl FnMut(f64, f64),
) {
    let calls: Rc<RefCell<Vec<(f64, f64)>>> = Rc::new(RefCell::new(Vec::new()));
    let calls_c = calls.clone();
    let callback = move |offset: f64, total: f64| {
        calls_c.borrow_mut().push((offset, total));
    };
    return (calls, callback);
}

#[wasm_bindgen_test]
async fn test_progress_first_call_is_zero() {
    // Первый вызов callback — (0.0, total) до цикла чтения.
    let data: Vec<u8> = (0..50_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&data);

    let (calls, callback) = collect_progress();
    let _hex = sha256_stream_with_progress(&blob, 8 * 1024, callback)
        .await
        .expect("sha256_stream_with_progress");

    let log = calls.borrow();
    assert!(!log.is_empty(), "callback должен вызываться хотя бы один раз");
    assert_eq!(log[0].0, 0.0, "первый вызов: offset = 0");
    assert_eq!(log[0].1, 50_000.0, "первый вызов: total = размер");
}

#[wasm_bindgen_test]
async fn test_progress_last_call_is_total() {
    // Последний вызов — (total, total), файл прочитан до конца.
    let data: Vec<u8> = (0..50_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&data);

    let (calls, callback) = collect_progress();
    let _hex = sha256_stream_with_progress(&blob, 8 * 1024, callback)
        .await
        .expect("sha256_stream_with_progress");

    let log = calls.borrow();
    let last = log.last().expect("хотя бы один вызов");
    assert_eq!(last.0, 50_000.0, "последний вызов: offset = размер");
    assert_eq!(last.1, 50_000.0, "последний вызов: total = размер");
}

#[wasm_bindgen_test]
async fn test_progress_monotonic() {
    // offset монотонно возрастает: 0 → 8192 → 16384 → ... → 50000.
    let data: Vec<u8> = (0..50_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&data);

    let (calls, callback) = collect_progress();
    let _hex = sha256_stream_with_progress(&blob, 8 * 1024, callback)
        .await
        .expect("sha256_stream_with_progress");

    let log = calls.borrow();
    assert!(log.len() >= 2, "минимум два вызова: старт и после первого чанка");

    for i in 1..log.len() {
        assert!(
            log[i].0 >= log[i - 1].0,
            "offset должен монотонно расти: log[{}].0={} < log[{}].0={}",
            i,
            log[i].0,
            i - 1,
            log[i - 1].0,
        );
    }
}

#[wasm_bindgen_test]
async fn test_progress_total_is_constant() {
    // Поле `total` во всех вызовах одинаково и равно размеру файла.
    let data: Vec<u8> = (0..40_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&data);

    let (calls, callback) = collect_progress();
    let _hex = sha256_stream_with_progress(&blob, 8 * 1024, callback)
        .await
        .expect("sha256_stream_with_progress");

    let log = calls.borrow();
    for (i, (_, total)) in log.iter().enumerate() {
        assert_eq!(
            *total, 40_000.0,
            "log[{}].total = {}, ожидалось 40000",
            i, total,
        );
    }
}

#[wasm_bindgen_test]
async fn test_progress_call_count_matches_chunks() {
    // 50 000 байт / chunk_size 8 192 → 7 чанков (последний неполный).
    // Ожидаем 1 стартовый вызов + 7 = 8 вызовов callback.
    let data: Vec<u8> = (0..50_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&data);

    let (calls, callback) = collect_progress();
    let _hex = sha256_stream_with_progress(&blob, 8 * 1024, callback)
        .await
        .expect("sha256_stream_with_progress");

    let log = calls.borrow();
    let expected_chunks = 50_000usize.div_ceil(8 * 1024);
    assert_eq!(
        log.len(),
        expected_chunks + 1,
        "ожидалось {} вызовов (1 старт + {} чанков), получено {}",
        expected_chunks + 1,
        expected_chunks,
        log.len(),
    );
}

#[wasm_bindgen_test]
async fn test_progress_empty_blob() {
    // Для пустого Blob цикл while не выполняется, но стартовый
    // вызов on_progress(0.0, 0.0) всё равно происходит.
    let blob = make_blob(&[]);

    let (calls, callback) = collect_progress();
    let _hex = sha256_stream_with_progress(&blob, 1024, callback)
        .await
        .expect("sha256_stream_with_progress");

    let log = calls.borrow();
    assert_eq!(log.len(), 1, "для пустого Blob — ровно один вызов");
    assert_eq!(log[0].0, 0.0);
    assert_eq!(log[0].1, 0.0);
}

#[wasm_bindgen_test]
async fn test_progress_single_chunk() {
    // Файл 5000 байт, chunk_size 8192 — попадает в один чанк.
    // Ожидаем 2 вызова: (0, 5000) и (5000, 5000).
    let data: Vec<u8> = (0..5_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&data);

    let (calls, callback) = collect_progress();
    let _hex = sha256_stream_with_progress(&blob, 8 * 1024, callback)
        .await
        .expect("sha256_stream_with_progress");

    let log = calls.borrow();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0], (0.0, 5_000.0));
    assert_eq!(log[1], (5_000.0, 5_000.0));
}

#[wasm_bindgen_test]
async fn test_progress_result_matches_no_progress() {
    // Результат с callback должен совпадать с результатом без callback.
    let data: Vec<u8> = (0..75_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&data);

    let without = sha256_stream_with_progress(&blob, 16 * 1024, |_, _| {})
        .await
        .expect("sha256_stream_with_progress");

    let (calls, callback) = collect_progress();
    let with_cb = sha256_stream_with_progress(&blob, 16 * 1024, callback)
        .await
        .expect("sha256_stream_with_progress");

    assert_eq!(without, with_cb);
    // И финальный offset совпадает с размером.
    let log = calls.borrow();
    let last = log.last().expect("хотя бы один вызов");
    assert_eq!(last.0, 75_000.0);
}

// ===============================================================
// Быстрый отпечаток
// ===============================================================

#[wasm_bindgen_test]
fn test_quick_fingerprint_format() {
    let file = web_sys::File::new_with_str_sequence(
        &js_sys::Array::of1(&"abc".into()),
        "sample.txt",
    )
    .expect("File::new_with_str_sequence");

    let fp = quick_fingerprint(&file);
    assert!(fp.contains("байт"));
    assert!(fp.contains("изменён"));
    assert!(fp.starts_with("3 байт"));
}