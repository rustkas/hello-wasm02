//! Потоковое копирование `File` → `FileSystemFileHandle` без полной
//! буферизации.
//!
//! Схема:
//! 1. Открыть writable у destination.
//! 2. Получить reader у source.
//! 3. Цикл: read один чанк → write его → следующий.
//! 4. Yield раз в `opts.yield_interval_ms` мс, чтобы UI успевал отрисовать.
//! 5. `commit()` фиксирует; `abort()` откатывает при отмене.
//!
//! **Backpressure реальна.** `writer.write(chunk).await` блокирует,
//! пока диск не готов принять следующий блок. Мы не читаем вперёд —
//! читаем ровно столько, сколько записали. Это даёт memory bound
//! в один `chunk_size`.
//!
//! **Параллелить read и write нельзя.** `join!(read, write)` превратит
//! pipeline в два независимых потока данных: буферы растут, memory
//! bound рушится, а выигрыш в throughput нулевой — диск всё равно один.
//!
//! **Yield управляем параметром.** `yield_interval_ms = 16.0` — плавный
//! UI, `0.0` — синхронный цикл (UI зависает до конца). Оба варианта
//! оставлены намеренно: демо показывает разницу.
//!
//! **Проверка — не догадка.** `sha256_file` даёт потоковый SHA-256
//! файла с memory O(1); `format_hash_verdict` сравнивает два хеша
//! и возвращает человекочитаемый вердикт.

use gloo_timers::future::TimeoutFuture;
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, FileSystemFileHandle, ReadableStreamDefaultReader};

use crate::fsa_choose_options::format_str;
use crate::fsa_large_chunk::{ChunkPlan, CopyStats, SpeedEma};
use crate::fsa_write_ops::WriteSession;

/// Опции копирования.
#[derive(Debug, Clone, Copy)]
pub struct CopyOptions {
    /// Интервал yield'а в мс. `16.0` — плавный UI, `0.0` — без yield.
    pub yield_interval_ms: f64,
}

impl CopyOptions {
    pub fn with_yield() -> Self {
        return CopyOptions { yield_interval_ms: 16.0 };
    }
    pub fn without_yield() -> Self {
        return CopyOptions { yield_interval_ms: 0.0 };
    }
}

/// Итог копирования.
pub enum CopyOutcome {
    Done(CopyStats),
    Cancelled(CopyStats),
}

/// Копирование с дефолтным yield'ом (16 мс).
///
/// `on_progress(frac, speed_mbps)` вызывается не чаще 16 мс.
/// `is_cancelled()` проверяется на каждой итерации и перед записью.
pub async fn copy_stream(
    src: &File,
    dst_handle: &FileSystemFileHandle,
    on_progress: impl FnMut(f64, f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<CopyOutcome, String> {
    return copy_stream_with_options(
        src,
        dst_handle,
        CopyOptions::with_yield(),
        on_progress,
        is_cancelled,
    )
    .await;
}

/// Копирование с явными опциями.
///
/// `opts.yield_interval_ms = 0.0` отключает yield: цикл становится
/// полностью синхронным, `on_progress` вызовется ровно один раз —
/// в самом конце. Это counterfactual-режим для демонстрации
/// необходимости yield'а.
pub async fn copy_stream_with_options(
    src: &File,
    dst_handle: &FileSystemFileHandle,
    opts: CopyOptions,
    mut on_progress: impl FnMut(f64, f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<CopyOutcome, String> {
    let plan: ChunkPlan = ChunkPlan::for_total(src.size() as u64);

    let stream = src.stream();
    let reader = ReadableStreamDefaultReader::new(&stream)
        .map_err(|e| format_str(format_args!("reader: {e:?}")))?;

    // Открываем writable. keepExistingData=false: destination создаётся
    // с нуля — при отмене ничего не останется.
    let mut session = WriteSession::open(dst_handle, false)
        .await
        .map_err(|e| e.explain())?;

    let total: u64 = src.size() as u64;
    let mut bytes_copied: u64 = 0;
    let mut chunks: u32 = 0;
    let mut yields: u32 = 0;
    let mut progress_calls: u32 = 0;
    let mut read_ms: f64 = 0.0;
    let mut write_ms: f64 = 0.0;
    let mut first_chunk_sizes: Vec<usize> = Vec::with_capacity(8);
    let mut last_yield: f64 = 0.0;
    let t_total: f64 = js_sys::Date::now();
    let mut speed = SpeedEma::new(0.25);
    let mut last_speed_t: f64 = t_total;
    let mut last_speed_b: u64 = 0;
    let mut cancelled_flag: bool = false;
    let do_yield: bool = opts.yield_interval_ms > 0.0;

    loop {
        if is_cancelled() {
            cancelled_flag = true;
            break;
        }

        // --- Read ---
        let t_read: f64 = js_sys::Date::now();
        let result = JsFuture::from(reader.read())
            .await
            .map_err(|e| format_str(format_args!("read: {e:?}")))?;
        read_ms += js_sys::Date::now() - t_read;

        let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if done {
            break;
        }

        let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))
            .map_err(|e| format_str(format_args!("value: {e:?}")))?;
        let chunk = Uint8Array::new(&value);
        let bytes: Vec<u8> = chunk.to_vec();
        let len: usize = bytes.len();

        if first_chunk_sizes.len() < 8 {
            first_chunk_sizes.push(len);
        }

        // --- Write (with backpressure) ---
        let t_write: f64 = js_sys::Date::now();
        session
            .write(&bytes)
            .await
            .map_err(|e| e.explain())?;
        write_ms += js_sys::Date::now() - t_write;

        bytes_copied += len as u64;
        chunks += 1;

        // --- Yield раз в opts.yield_interval_ms + обновление прогресса ---
        if do_yield {
            let now: f64 = js_sys::Date::now();
            if now - last_yield >= opts.yield_interval_ms {
                let frac: f64 = if total > 0 {
                    bytes_copied as f64 / total as f64
                } else {
                    1.0
                };

                let dt: f64 = now - last_speed_t;
                let db: u64 = bytes_copied - last_speed_b;
                speed.update(db, dt);
                last_speed_t = now;
                last_speed_b = bytes_copied;

                on_progress(frac, speed.value_mbps());
                progress_calls += 1;
                TimeoutFuture::new(0).await;
                last_yield = now;
                yields += 1;
            }
        }
    }

    // --- Отмена: abort, destination не остаётся с частичными данными ---
    if cancelled_flag {
        let _ = session.abort().await;
        let total_ms: f64 = js_sys::Date::now() - t_total;
        let stats = CopyStats {
            bytes_copied, chunks, yields, progress_calls,
            read_ms, write_ms, total_ms,
            chunk_size: plan.chunk_size,
            cancelled: true,
            first_chunk_sizes,
        };
        return Ok(CopyOutcome::Cancelled(stats));
    }

    // --- Commit: сливаем буфер, закрываем поток ---
    let t_commit: f64 = js_sys::Date::now();
    let _ = session.commit().await.map_err(|e| e.explain())?;
    write_ms += js_sys::Date::now() - t_commit;

    on_progress(1.0, speed.value_mbps());
    progress_calls += 1;
    let total_ms: f64 = js_sys::Date::now() - t_total;
    let stats = CopyStats {
        bytes_copied, chunks, yields, progress_calls,
        read_ms, write_ms, total_ms,
        chunk_size: plan.chunk_size,
        cancelled: false,
        first_chunk_sizes,
    };
    return Ok(CopyOutcome::Done(stats));
}

// ---------------------------------------------------------------------------
// SHA-256 файла
// ---------------------------------------------------------------------------

/// Потоковый SHA-256 файла. Память O(1) — не зависит от размера.
///
/// Совпадение размеров источника и приёмника — необходимое, но не
/// достаточное условие корректности копии. SHA-256 даёт достаточное.
pub async fn sha256_file(handle: &FileSystemFileHandle) -> Result<String, String> {
    use sha2::{Digest, Sha256};

    let file = crate::fsa_api_file::get_file(handle)
        .await
        .map_err(|e| format_str(format_args!("get_file: {e:?}")))?;

    let stream = file.stream();
    let reader = ReadableStreamDefaultReader::new(&stream)
        .map_err(|e| format_str(format_args!("reader: {e:?}")))?;

    let mut hasher = Sha256::new();

    loop {
        let result = JsFuture::from(reader.read())
            .await
            .map_err(|e| format_str(format_args!("read: {e:?}")))?;
        let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if done {
            break;
        }
        let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))
            .map_err(|e| format_str(format_args!("value: {e:?}")))?;
        let chunk = Uint8Array::new(&value);
        hasher.update(&chunk.to_vec());
    }

    return Ok(hex::encode(hasher.finalize()));
}

/// Сравнить хеши источника и приёмника, вернуть человекочитаемый вердикт.
pub fn format_hash_verdict(src_hash: &str, dst_hash: &str) -> String {
    let match_line: String = if src_hash == dst_hash {
        "✓ Хеши совпадают — копия побитово идентична источнику.".to_string()
    } else {
        "✗ Хеши различаются — копия повреждена.".to_string()
    };
    let src_short: String = if src_hash.len() >= 16 { src_hash[..16].to_string() } else { src_hash.to_string() };
    let dst_short: String = if dst_hash.len() >= 16 { dst_hash[..16].to_string() } else { dst_hash.to_string() };
    let s: String = format_str(format_args!(
        "{match_line}\n\
         Источник:  {src_short}…\n\
         Приёмник:  {dst_short}…",
    ));
    return s;
}