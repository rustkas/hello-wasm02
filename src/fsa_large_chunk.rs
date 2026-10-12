//! Модель плана копирования: размер чанка, число итераций, live-скорость.
//!
//! Модуль не читает и не пишет файл (это делает `fsa_large_copy`) —
//! здесь только предметная модель:
//! * `ChunkPlan` — размер чанка под объём, ожидаемое число итераций,
//!   peak memory, экономия по сравнению с «прочитать целиком».
//! * `SpeedEma` — экспоненциальная средняя для сглаживания скачков.
//! * `CopyStats` — метрики копирования, раздельно read / write time,
//!   число yield'ов и число вызовов `on_progress`.
//!
//! `progress_calls` — educational-поле: с yield'ом оно десятки, без yield'а — 1.
//! Разница между двумя режимами видна в UI и в тестах.

use crate::fsa_choose_options::format_str;

/// Целевое число итераций при выборе размера чанка.
pub const TARGET_ITERATIONS: u32 = 200;
pub const MIN_CHUNK: usize = 64 * 1024;        // 64 КиБ
pub const MAX_CHUNK: usize = 4 * 1024 * 1024;  // 4 МиБ

/// План копирования под заданный размер файла.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChunkPlan {
    pub total_bytes: u64,
    pub chunk_size: usize,
    pub expected_chunks: u32,
    pub memory_bound_bytes: u64,
}

impl ChunkPlan {
    pub fn for_total(total: u64) -> Self {
        let total_usize: usize = usize::try_from(total).unwrap_or(usize::MAX);
        let raw: usize = total_usize / TARGET_ITERATIONS as usize;
        let chunk_size: usize = raw.clamp(MIN_CHUNK, MAX_CHUNK);
        let expected_chunks: u32 = if total == 0 {
            0
        } else {
            ((total + chunk_size as u64 - 1) / chunk_size as u64) as u32
        };
        return ChunkPlan {
            total_bytes: total,
            chunk_size,
            expected_chunks,
            memory_bound_bytes: chunk_size as u64,
        };
    }

    /// Сколько байт сэкономлено по сравнению с «прочитать файл целиком».
    pub fn memory_saved(&self) -> u64 {
        if self.total_bytes > self.memory_bound_bytes {
            return self.total_bytes - self.memory_bound_bytes;
        }
        return 0;
    }

    pub fn describe(&self) -> String {
        let total: String = format_bytes(self.total_bytes);
        let chunk: String = format_bytes(self.chunk_size as u64);
        let bound: String = format_bytes(self.memory_bound_bytes);
        let saved: String = format_bytes(self.memory_saved());
        let s: String = format_str(format_args!(
            "План: {total} · chunk {chunk} · ~{} итераций\n\
             Peak memory: {bound} (naive было бы {total}, экономия {saved})",
            self.expected_chunks,
        ));
        return s;
    }
}

/// Экспоненциальная скользящая средняя скорости.
///
/// Сырая скорость в первые 100 мс — шум: то 0 МБ/с, то 999 МБ/с.
/// EMA с `alpha ≈ 0.25` сглаживает и даёт стабильное число для UI.
#[derive(Debug, Clone)]
pub struct SpeedEma {
    alpha: f64,
    value: f64,
    initialised: bool,
}

impl SpeedEma {
    pub fn new(alpha: f64) -> Self {
        return SpeedEma { alpha, value: 0.0, initialised: false };
    }

    pub fn update(&mut self, bytes: u64, elapsed_ms: f64) {
        if elapsed_ms <= 0.0 {
            return;
        }
        let mbps: f64 = bytes as f64 / 1_048_576.0 / (elapsed_ms / 1000.0);
        if !self.initialised {
            self.value = mbps;
            self.initialised = true;
        } else {
            self.value = self.alpha * mbps + (1.0 - self.alpha) * self.value;
        }
    }

    pub fn value_mbps(&self) -> f64 {
        return self.value;
    }
}

/// Итоговые метрики копирования.
#[derive(Debug, Clone, PartialEq)]
pub struct CopyStats {
    pub bytes_copied: u64,
    pub chunks: u32,
    pub yields: u32,
    /// Сколько раз вызвался `on_progress` (с yield'ом — много, без — 1).
    /// Educational-поле: разница между режимами видна в UI и тестах.
    pub progress_calls: u32,
    pub read_ms: f64,
    pub write_ms: f64,
    pub total_ms: f64,
    pub chunk_size: usize,
    pub cancelled: bool,
    pub first_chunk_sizes: Vec<usize>,
}

impl CopyStats {
    pub fn avg_speed_mbps(&self) -> f64 {
        if self.total_ms <= 0.0 {
            return 0.0;
        }
        return self.bytes_copied as f64 / 1_048_576.0 / (self.total_ms / 1000.0);
    }

    /// Доля времени на чтение.
    pub fn read_share(&self) -> f64 {
        let sum: f64 = self.read_ms + self.write_ms;
        if sum <= 0.0 {
            return 0.0;
        }
        return self.read_ms / sum;
    }

    pub fn write_share(&self) -> f64 {
        return 1.0 - self.read_share();
    }

    /// Совпадает ли прочитанное с ожидаемым размером.
    pub fn matches(&self, expected: u64) -> bool {
        return !self.cancelled && self.bytes_copied == expected;
    }

    /// Узкое место: чтение, запись или баланс.
    pub fn bottleneck(&self) -> &'static str {
        let r: f64 = self.read_share();
        if r > 0.6 {
            return "чтение";
        }
        if r < 0.4 {
            return "запись";
        }
        return "баланс";
    }
}

// ---------------------------------------------------------------------------
// Форматтеры для UI
// ---------------------------------------------------------------------------

pub fn format_bytes(n: u64) -> String {
    if n < 1024 {
        return format_str(format_args!("{n} Б"));
    }
    if n < 1024 * 1024 {
        let k: f64 = n as f64 / 1024.0;
        return format_str(format_args!("{k:.1} КБ"));
    }
    if n < 1024 * 1024 * 1024 {
        let m: f64 = n as f64 / (1024.0 * 1024.0);
        return format_str(format_args!("{m:.2} МБ"));
    }
    let g: f64 = n as f64 / (1024.0 * 1024.0 * 1024.0);
    return format_str(format_args!("{g:.2} ГБ"));
}

pub fn format_chunk_sizes(sizes: &[usize]) -> String {
    if sizes.is_empty() {
        return "(нет)".to_string();
    }
    let mut parts: Vec<String> = Vec::with_capacity(sizes.len());
    for s in sizes {
        parts.push(format_str(format_args!("{}", format_bytes(*s as u64))));
    }
    return parts.join(", ");
}

/// Развёрнутый отчёт для UI: статус, объём, время, скорость, узкое место,
/// число обновлений прогресса и yield'ов.
pub fn format_stats(stats: &CopyStats, expected: u64) -> String {
    let status: String = if stats.cancelled {
        "⚠ Отменено — destination откатан".to_string()
    } else if stats.matches(expected) {
        "✓ Скопировано полностью, размер совпадает".to_string()
    } else {
        format_str(format_args!(
            "⚠ Скопировано {} из {} — расхождение",
            format_bytes(stats.bytes_copied),
            format_bytes(expected),
        ))
    };

    let read_pct: f64 = stats.read_share() * 100.0;
    let write_pct: f64 = stats.write_share() * 100.0;

    let mut lines: Vec<String> = Vec::with_capacity(11);
    lines.push(status);
    lines.push(format_str(format_args!(
        "Скопировано:  {} · чанков: {}",
        format_bytes(stats.bytes_copied), stats.chunks,
    )));
    lines.push(format_str(format_args!(
        "Progress:     {} обновлений · yield'ов: {}",
        stats.progress_calls, stats.yields,
    )));
    lines.push(format_str(format_args!(
        "Chunk size:   {} (adaptive)",
        format_bytes(stats.chunk_size as u64),
    )));
    lines.push(format_str(format_args!(
        "Время:        total {:.0} ms · read {:.0} ms ({:.0}%) · write {:.0} ms ({:.0}%)",
        stats.total_ms, stats.read_ms, read_pct,
        stats.write_ms, write_pct,
    )));
    lines.push(format_str(format_args!(
        "Скорость:     {:.2} МБ/с · узкое место: {}",
        stats.avg_speed_mbps(), stats.bottleneck(),
    )));
    lines.push(format_str(format_args!(
        "Размеры первых чанков: {}",
        format_chunk_sizes(&stats.first_chunk_sizes),
    )));
    return lines.join("\n");
}