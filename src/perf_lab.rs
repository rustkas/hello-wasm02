//! Лаборатория производительности: реестр замеров, статистика, bottleneck.

use crate::format_str::format_str;
use crate::perf_core;

/// Сколько первых итераций отбрасывать как «прогрев».
pub const WARMUP_ITERATIONS: u32 = 3;

#[derive(Debug, Clone)]
pub struct BenchResult {
    pub name: String,
    pub samples_ms: Vec<f64>,
}

impl BenchResult {
    pub fn new(name: &str, samples_ms: Vec<f64>) -> Self {
        Self {
            name: name.into(),
            samples_ms,
        }
    }

    pub fn count(&self) -> usize {
        self.samples_ms.len()
    }

    pub fn min_ms(&self) -> f64 {
        self.samples_ms.iter().cloned().fold(f64::INFINITY, f64::min)
    }

    pub fn max_ms(&self) -> f64 {
        self.samples_ms
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    pub fn mean_ms(&self) -> f64 {
        if self.samples_ms.is_empty() {
            return 0.0;
        }
        self.samples_ms.iter().sum::<f64>() / self.samples_ms.len() as f64
    }

    pub fn median_ms(&self) -> f64 {
        if self.samples_ms.is_empty() {
            return 0.0;
        }
        let mut v = self.samples_ms.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = v.len() / 2;
        if v.len() % 2 == 0 {
            (v[mid - 1] + v[mid]) / 2.0
        } else {
            v[mid]
        }
    }

    pub fn std_dev_ms(&self) -> f64 {
        if self.samples_ms.len() < 2 {
            return 0.0;
        }
        let m = self.mean_ms();
        let var = self
            .samples_ms
            .iter()
            .map(|&x| (x - m).powi(2))
            .sum::<f64>()
            / (self.samples_ms.len() as f64 - 1.0);
        var.sqrt()
    }
}

/// Прогоняет задачу: сначала `WARMUP_ITERATIONS` прогревных вызовов
/// (не замеряются), затем `iterations` измеренных.
///
/// Контракт: `iterations` — именно число сэмплов в результате.
/// `BenchResult::count() == iterations` (при `iterations >= 1`).
pub fn run_benchmark<F: FnMut()>(name: &str, iterations: u32, mut task: F) -> BenchResult {
    // Прогрев: JIT, первый GC, пустой кэш — не должны попасть в статистику.
    for _ in 0..WARMUP_ITERATIONS {
        task();
    }

    let mut samples = Vec::with_capacity(iterations as usize);
    for _ in 0..iterations {
        let t0 = perf_core::now_ms();
        task();
        let dt = perf_core::now_ms() - t0;
        samples.push(dt);
    }
    BenchResult::new(name, samples)
}

/// Индекс узкого места — задача с наибольшим средним временем.
pub fn find_bottleneck(results: &[BenchResult]) -> Option<usize> {
    results
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            a.mean_ms()
                .partial_cmp(&b.mean_ms())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
}

/// Индекс самой стабильной задачи (наименьший stddev).
pub fn find_most_stable(results: &[BenchResult]) -> Option<usize> {
    results
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            a.std_dev_ms()
                .partial_cmp(&b.std_dev_ms())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
}

/// Форматирование в читаемую строку.
///
/// `format!` в tail-позиции обёрнут через `format_str(format_args!(...))` —
/// правило проекта: rust-analyzer теряет тип в этой позиции.
pub fn format_result(r: &BenchResult) -> String {
    format_str(format_args!(
        "{}: mean {:.3} ms, median {:.3}, min {:.3}, max {:.3}, stddev {:.3} (n={})",
        r.name,
        r.mean_ms(),
        r.median_ms(),
        r.min_ms(),
        r.max_ms(),
        r.std_dev_ms(),
        r.count()
    ))
}