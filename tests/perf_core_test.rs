//! Host-тесты perf_lab: чистая математика (статистика).
//! Performance API недоступно на host — здесь только BenchResult.

use hello_wasm02::perf_lab::BenchResult;

#[test]
fn min_max_mean_median() {
    let r = BenchResult::new("test", vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(r.min_ms(), 1.0);
    assert_eq!(r.max_ms(), 5.0);
    assert_eq!(r.mean_ms(), 3.0);
    assert_eq!(r.median_ms(), 3.0);
    assert_eq!(r.count(), 5);
}

#[test]
fn median_even_length_is_average_of_middle() {
    let r = BenchResult::new("even", vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(r.median_ms(), 2.5);
}

#[test]
fn std_dev_zero_for_single_sample() {
    let r = BenchResult::new("single", vec![42.0]);
    assert_eq!(r.std_dev_ms(), 0.0);
}

#[test]
fn std_dev_matches_formula() {
    // Известный пример: [2, 4, 4, 4, 5, 5, 7, 9] → σ_pop = 2, σ_sample ≈ 2.138
    let r = BenchResult::new("sample", vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
    let sd = r.std_dev_ms();
    assert!((sd - 2.138).abs() < 0.01, "получили {sd}");
}

#[test]
fn empty_result_handles_gracefully() {
    let r = BenchResult::new("empty", vec![]);
    assert_eq!(r.count(), 0);
    assert_eq!(r.mean_ms(), 0.0);
    assert_eq!(r.median_ms(), 0.0);
    assert_eq!(r.std_dev_ms(), 0.0);
}