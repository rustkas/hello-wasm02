use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen_test::*;
use hello_wasm02::blob_stream::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Создаёт Blob из последовательности байт.
fn make_blob(bytes: &[u8]) -> web_sys::Blob {
    let uint8 = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::of1(&uint8.buffer());
    return web_sys::Blob::new_with_u8_array_sequence(&parts)
        .expect("Blob::new_with_u8_array_sequence");
}

#[wasm_bindgen_test]
async fn test_stream_chunks_small() {
    let original: Vec<u8> = (0..100u8).collect();
    let blob = make_blob(&original);

    let total: f64 = stream_chunks(&blob, |_c| {})
        .await
        .expect("stream_chunks");
    assert_eq!(total, original.len() as f64);
}

#[wasm_bindgen_test]
async fn test_stream_chunks_large() {
    let original: Vec<u8> = (0..200_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&original);

    let total: f64 = stream_chunks(&blob, |_c| {})
        .await
        .expect("stream_chunks");
    assert_eq!(total, original.len() as f64);
}

#[wasm_bindgen_test]
async fn test_stream_stats_matches_size() {
    let original: Vec<u8> = (0..150_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&original);

    let stats = stream_stats(&blob).await.expect("stream_stats");
    assert_eq!(stats.total_bytes, original.len() as f64);
    assert!(stats.chunk_count >= 1);
    assert!(stats.min_chunk > 0);
    assert!(stats.max_chunk >= stats.min_chunk);
    assert!(stats.avg_chunk > 0.0);
    assert!(stats.avg_chunk <= stats.max_chunk as f64);
}

#[wasm_bindgen_test]
async fn test_stream_empty_blob() {
    let blob = make_blob(&[]);
    let stats = stream_stats(&blob).await.expect("stream_stats");
    assert_eq!(stats.total_bytes, 0.0);
    assert_eq!(stats.chunk_count, 0);
    assert_eq!(stats.min_chunk, 0);
    assert_eq!(stats.max_chunk, 0);
    assert_eq!(stats.avg_chunk, 0.0);
}

#[wasm_bindgen_test]
async fn test_stream_chunks_callback() {
    let original: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&original);

    let chunks: Rc<RefCell<Vec<Chunk>>> = Rc::new(RefCell::new(Vec::new()));
    let chunks_c = chunks.clone();

    let total: f64 = stream_chunks(&blob, move |c| {
        chunks_c.borrow_mut().push(c.clone());
    })
    .await
    .expect("stream_chunks");

    assert_eq!(total, original.len() as f64);

    let collected = chunks.borrow();
    assert!(!collected.is_empty());

    // Первый чанк имеет index == 0.
    assert_eq!(collected[0].index, 0);

    // Индексы идут подряд: 0, 1, 2, ...
    for (i, c) in collected.iter().enumerate() {
        assert_eq!(c.index, i as u32);
    }

    // Суммарный размер всех чанков совпадает с полным.
    let sum: u64 = collected.iter().map(|c| c.size as u64).sum();
    assert_eq!(sum, original.len() as u64);

    // total_so_far последнего чанка == полный размер.
    let last = collected.last().expect("last chunk");
    assert_eq!(last.total_so_far, original.len() as f64);
}

#[wasm_bindgen_test]
async fn test_total_so_far_monotonic() {
    let original: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&original);

    let totals: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
    let totals_c = totals.clone();

    let _: f64 = stream_chunks(&blob, move |c| {
        totals_c.borrow_mut().push(c.total_so_far);
    })
    .await
    .expect("stream_chunks");

    let list = totals.borrow();
    assert!(!list.is_empty());

    for i in 1..list.len() {
        assert!(
            list[i] >= list[i - 1],
            "total_so_far должен монотонно расти: list[{}]={} < list[{}]={}",
            i,
            list[i],
            i - 1,
            list[i - 1],
        );
    }

    let last = *list.last().expect("last total");
    assert_eq!(last, original.len() as f64);
}

#[wasm_bindgen_test]
async fn test_total_so_far_is_f64() {
    let original: Vec<u8> = vec![0u8; 100];
    let blob = make_blob(&original);

    let captured: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
    let captured_c = captured.clone();

    let _: f64 = stream_chunks(&blob, move |c| {
        *captured_c.borrow_mut() = Some(c.total_so_far);
    })
    .await
    .expect("stream_chunks");

    let value: f64 = captured.borrow().expect("captured value");
    assert_eq!(value, 100.0);
}

#[wasm_bindgen_test]
async fn test_stream_chunks_returns_f64() {
    let original: Vec<u8> = vec![1u8; 50];
    let blob = make_blob(&original);

    let total: f64 = stream_chunks(&blob, |_c| {})
        .await
        .expect("stream_chunks");
    assert_eq!(total, 50.0);
}

#[wasm_bindgen_test]
async fn test_total_bytes_is_f64() {
    let original: Vec<u8> = vec![2u8; 50];
    let blob = make_blob(&original);

    let stats = stream_stats(&blob).await.expect("stream_stats");
    let total: f64 = stats.total_bytes;
    assert_eq!(total, 50.0);
}

#[wasm_bindgen_test]
async fn test_f64_represents_large_sizes() {
    let big: f64 = 4_422_163_697.0;
    assert!(big > u32::MAX as f64, "значение должно быть > u32::MAX");

    let recovered: u64 = big as u64;
    assert_eq!(recovered, 4_422_163_697u64);

    let wrapped_mod: u64 = (big as u64) % (u32::MAX as u64 + 1);
    assert_eq!(wrapped_mod, 127_196_401u64);
}

#[wasm_bindgen_test]
async fn test_stream_stats_avg_chunk_bounds() {
    let original: Vec<u8> = (0..200_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&original);

    let stats = stream_stats(&blob).await.expect("stream_stats");
    assert!(stats.avg_chunk >= stats.min_chunk as f64);
    assert!(stats.avg_chunk <= stats.max_chunk as f64);
}

#[wasm_bindgen_test]
async fn test_stream_total_bytes_is_deterministic() {
    // Единственный стабильный инвариант двух проходов — `total_bytes`.
    //
    // `Blob.stream()` НЕ гарантирует одинаковое разбиение на чанки
    // между вызовами: браузер может отдать 1 чанк из 100 000 байт
    // в одном проходе и 2 чанка (65 536 + 34 464) в другом.
    // Поэтому `chunk_count`, `min_chunk`, `max_chunk` и `avg_chunk`
    // сравнивать между проходами нельзя.
    let original: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
    let blob = make_blob(&original);

    let stats1 = stream_stats(&blob).await.expect("stats1");
    let stats2 = stream_stats(&blob).await.expect("stats2");

    assert_eq!(stats1.total_bytes, stats2.total_bytes);
    assert_eq!(stats1.total_bytes, original.len() as f64);

    // Проверяем только, что оба прохода прошли успешно.
    assert!(stats1.chunk_count >= 1);
    assert!(stats2.chunk_count >= 1);
}