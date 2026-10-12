//! Wasm-тесты: реальные замеры через Performance API.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::perf_core;
use hello_wasm02::perf_lab::{self, BenchResult};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn now_is_monotonic() {
    let t0 = perf_core::now_ms();
    let t1 = perf_core::now_ms();
    assert!(t1 >= t0);
}

#[wasm_bindgen_test]
fn span_measures_positive_duration() {
    let span = perf_core::Span::start("test");
    // Немного поработаем.
    let mut acc = 0u64;
    for i in 0..1000u64 {
        acc = acc.wrapping_add(i);
    }
    let _ = acc;
    let elapsed = span.finish();
    assert!(elapsed >= 0.0);
}

#[wasm_bindgen_test]
fn mark_and_measure() {
    perf_core::clear_marks().ok();
    perf_core::clear_measures().ok();
    perf_core::mark("start").expect("mark start");
    perf_core::mark("end").expect("mark end");
    let dur = perf_core::measure("span", "start", "end").expect("measure");
    assert!(dur >= 0.0);
}

#[wasm_bindgen_test]
fn entry_names_of_marks() {
    perf_core::clear_marks().ok();
    perf_core::mark("alpha").expect("mark alpha");
    perf_core::mark("beta").expect("mark beta");
    let names = perf_core::entry_names_of_type("mark").expect("names");
    assert!(names.iter().any(|n| n == "alpha"));
    assert!(names.iter().any(|n| n == "beta"));
}

#[wasm_bindgen_test]
fn benchmark_returns_samples() {
    let iterations = 5u32;
    let result = perf_lab::run_benchmark("work", iterations, || {
        let mut acc = 0u64;
        for i in 0..1000u64 {
            acc = acc.wrapping_add(i.wrapping_mul(3));
        }
        let _ = acc;
    });
    // Контракт: count() == iterations. Прогрев не входит в результат.
    assert_eq!(result.count(), iterations as usize);
    assert!(result.mean_ms() >= 0.0);
    assert!(result.median_ms() >= 0.0);
}

#[wasm_bindgen_test]
fn bottleneck_is_slowest() {
    let a = BenchResult::new("fast", vec![1.0, 1.2, 1.1]);
    let b = BenchResult::new("slow", vec![10.0, 11.0, 10.5]);
    let idx = perf_lab::find_bottleneck(&[a, b]).unwrap();
    assert_eq!(idx, 1, "должна быть медленная задача");
}