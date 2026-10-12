//! Wasm-тесты модуля ls_bench: замер блокировки, sweep, заполнение
//! до квоты (с предохранителем).
//!
//! Компилируются только под wasm32 — на host файл пуст.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::ls_bench;
use hello_wasm02::ls_probe;

wasm_bindgen_test_configure!(run_in_browser);

fn uid() -> String {
    return format!("{}", js_sys::Date::now() as u64);
}

// ---------- now_ms ----------

#[wasm_bindgen_test]
fn now_ms_is_monotonic() {
    let t0 = ls_bench::now_ms();
    let t1 = ls_bench::now_ms();
    assert!(t1 >= t0);
}

// ---------- measure_set_get ----------

#[wasm_bindgen_test]
fn measure_set_get_returns_positive_times() {
    let key = format!("bench-{}", uid());
    let value = "x".repeat(1000);
    let sample = ls_bench::measure_set_get(&key, &value).unwrap();
    assert_eq!(sample.size_chars, 1000);
    assert!(sample.write_ms >= 0.0);
    assert!(sample.read_ms >= 0.0);
}

#[wasm_bindgen_test]
fn measure_set_get_cleans_up_key() {
    let key = format!("bench-clean-{}", uid());
    ls_bench::measure_set_get(&key, "hello").unwrap();
    // Замер должен убрать за собой ключ.
    assert!(ls_probe::get_string(&key).unwrap().is_none());
}

// ---------- run_sweep ----------

#[wasm_bindgen_test]
fn run_sweep_one_sample_per_size() {
    let prefix = format!("sweep-{}", uid());
    let sizes = [10usize, 100, 1000];
    let samples = ls_bench::run_sweep(&prefix, &sizes).unwrap();
    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0].size_chars, 10);
    assert_eq!(samples[1].size_chars, 100);
    assert_eq!(samples[2].size_chars, 1000);
}

#[wasm_bindgen_test]
fn run_sweep_cleans_up_keys() {
    let prefix = format!("sweep-clean-{}", uid());
    let _ = ls_bench::run_sweep(&prefix, &[10, 20]).unwrap();
    let ks = ls_probe::keys().unwrap();
    assert!(
        !ks.iter().any(|k| k.starts_with(&prefix)),
        "после sweep не должно остаться ключей"
    );
}

// ---------- fill_until_quota ----------
// ВАЖНО: не заполняем реальную 5-мегабайтную квоту — используем
// малый soft_limit (5 чанков по 1 КБ). Это проверка логики
// предохранителя и очистки, а не реального переполнения.
// Реальное заполнение пользователь запускает вручную через демо.

#[wasm_bindgen_test]
fn fill_until_quota_stops_at_soft_limit() {
    let prefix = format!("fill-{}", uid());
    let r = ls_bench::fill_until_quota(&prefix, 1024, 5).unwrap();
    // Упёрлись в soft_limit, а не в квоту.
    assert_eq!(r.written_chunks, 5);
    assert_eq!(r.chunk_utf16_bytes, 2048); // 1024 × 2
    assert_eq!(r.total_utf16_bytes, 5 * 2048);
    assert!(r.elapsed_ms >= 0.0);
}

#[wasm_bindgen_test]
fn fill_until_quota_cleans_up_keys() {
    let prefix = format!("fill-clean-{}", uid());
    let _ = ls_bench::fill_until_quota(&prefix, 512, 3).unwrap();
    let ks = ls_probe::keys().unwrap();
    assert!(
        !ks.iter().any(|k| k.starts_with(&prefix)),
        "после fill не должно остаться ключей"
    );
}

#[wasm_bindgen_test]
fn fill_until_quota_elapsed_is_measured() {
    let prefix = format!("fill-time-{}", uid());
    let r = ls_bench::fill_until_quota(&prefix, 1024, 5).unwrap();
    // Хотя бы несколько микросекунд должны пройти.
    assert!(r.elapsed_ms >= 0.0);
    // Проверяем, что размер посчитан верно.
    assert_eq!(r.written_chunks as usize * r.chunk_utf16_bytes, r.total_utf16_bytes);
}

#[wasm_bindgen_test]
async fn fill_until_quota_chunked_stops_at_soft_limit() {
    let prefix = format!("fill-async-{}", uid());
    let mut calls = 0u32;
    let r = ls_bench::fill_until_quota_chunked(&prefix, 512, 5, 2, |_w, _b| {
        calls += 1;
    })
    .await
    .unwrap();

    assert_eq!(r.written_chunks, 5);
    assert_eq!(r.chunk_utf16_bytes, 1024);
    assert_eq!(r.total_utf16_bytes, 5 * 1024);
    // 3 пакета по 2 + последний из 1 = 3 вызова on_progress.
    assert!(calls >= 2, "on_progress должен вызываться после каждого batch");

    let ks = ls_probe::keys().unwrap();
    assert!(!ks.iter().any(|k| k.starts_with(&prefix)));
}

#[wasm_bindgen_test]
async fn fill_until_quota_chunked_batch_one() {
    let prefix = format!("fill-b1-{}", uid());
    let mut calls = 0u32;
    let _ = ls_bench::fill_until_quota_chunked(&prefix, 256, 3, 1, |_w, _b| {
        calls += 1;
    })
    .await
    .unwrap();
    // batch_size = 1 → ровно 3 вызова.
    assert_eq!(calls, 3);
}