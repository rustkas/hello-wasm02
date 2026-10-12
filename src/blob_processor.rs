//! 20.4. Blob → PNG + JSON: практический проект.
//!
//! Один вход (Blob), два выхода (PNG + JSON).
//! Показывает три вещи:
//! 1. Blob двусторонний (`arrayBuffer()` возвращает байты обратно).
//! 2. `Blob.slice()` — чтение по кускам, а не «всё в память».
//! 3. Yield через `TimeoutFuture::new(0)` — иначе UI обновится в конце.
//!
//! Размер куска **не задаётся константой**. Он вычисляется под размер
//! файла, чтобы число кусков было ~128 независимо от того, 1 КБ это или 1 ГБ.

use std::cell::Cell;
use std::rc::Rc;

use js_sys::{Array, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    Blob, BlobPropertyBag, CanvasRenderingContext2d,
    OffscreenCanvas, OffscreenCanvasRenderingContext2d,
};

/// Строковый маркер отмены. Возвращается из `process_blob` как `Err`,
/// когда пользователь нажал «Отменить». UI проверяет через `js_err_to_string`.
pub const CANCELLED: &str = "cancelled-by-user";

// ---------- Публичные типы ----------

pub struct ProcessedArtifacts {
    pub png: Blob,
    pub json: Blob,
    pub metadata: Metadata,
}

#[derive(Clone, Debug)]
pub struct Metadata {
    pub total_bytes: usize,
    pub chunks: usize,
    pub chunk_size: usize,
    pub sha256: String,
    pub unique_bytes: usize,
    pub sum: u64,
    pub min_byte: u8,
    pub max_byte: u8,
    pub histogram: [u32; 256],
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            total_bytes: 0,
            chunks: 0,
            chunk_size: 0,
            sha256: String::new(),
            unique_bytes: 0,
            sum: 0,
            min_byte: 0,
            max_byte: 0,
            histogram: [0; 256],
        }
    }
}

impl Metadata {
    pub fn to_json(&self) -> String {
        let mut hist = String::with_capacity(256 * 8);
        for (i, v) in self.histogram.iter().enumerate() {
            if i > 0 {
                hist.push(',');
            }
            hist.push_str(&v.to_string());
        }
        let s: String = format!(
            "{{\n\
             \"total_bytes\": {},\n\
             \"chunks\": {},\n\
             \"chunk_size\": {},\n\
             \"sha256\": \"{}\",\n\
             \"unique_bytes\": {},\n\
             \"sum\": {},\n\
             \"min_byte\": {},\n\
             \"max_byte\": {},\n\
             \"histogram\": [{}]\n\
             }}\n",
            self.total_bytes,
            self.chunks,
            self.chunk_size,
            self.sha256,
            self.unique_bytes,
            self.sum,
            self.min_byte,
            self.max_byte,
            hist,
        );
        s
    }
}

// ---------- Адаптивный размер куска ----------

/// Подбирает размер куска так, чтобы число кусков было около 128.
///
/// Границы:
/// - минимум 32 КБ — иначе накладные расходы на `arrayBuffer()` съедят выгоду;
/// - максимум 8 МБ — иначе прогресс становится «дёрганым» (один кусок
///   занимает сотни мс).
///
/// Примеры:
/// - 64 КБ → 32 КБ (минимум), 2 куска;
/// - 4 МБ → 32 КБ, 128 кусков;
/// - 64 МБ → 512 КБ, 128 кусков;
/// - 512 МБ → 4 МБ, 128 кусков;
/// - 4 ГБ → 8 МБ (максимум), 512 кусков.
pub fn choose_chunk_size(total_bytes: usize) -> usize {
    const TARGET_CHUNKS: usize = 128;
    const MIN_CHUNK: usize = 32 * 1024;
    const MAX_CHUNK: usize = 8 * 1024 * 1024;
    if total_bytes == 0 {
        return MIN_CHUNK;
    }
    let ideal = total_bytes / TARGET_CHUNKS;
    ideal.clamp(MIN_CHUNK, MAX_CHUNK)
}

// ---------- Чтение ----------

pub async fn read_blob_bytes(blob: &Blob) -> Result<Vec<u8>, JsValue> {
    let promise = blob.array_buffer();
    let value = wasm_bindgen_futures::JsFuture::from(promise).await?;
    let buffer: js_sys::ArrayBuffer = value.dyn_into()?;
    let u8arr = Uint8Array::new(&buffer);
    let out: Vec<u8> = u8arr.to_vec();
    Ok(out)
}

pub async fn read_slice_bytes(blob: &Blob, start: f64, end: f64) -> Result<Vec<u8>, JsValue> {
    let slice = blob.slice_with_f64_and_f64(start, end)?;
    read_blob_bytes(&slice).await
}

// ---------- Обработка ----------

/// Читает Blob кусками, считает метрики, отдаёт управление браузеру
/// каждые 16 мс (≈ 60 Гц) и вызывает `on_progress(done, total, &histogram)`.
///
/// **Контракт `on_progress`:** минимум два вызова — `(0, total, _)` в начале
/// и `(total, total, &полная_гистограмма)` в конце. Промежуточные вызовы
/// троттлятся до 60 Гц.
///
/// **Контракт `cancel`:** если `cancel.get() == true` на входе в новую
/// итерацию — функция возвращает `Err(JsValue::from_str(CANCELLED))`.
/// Идемпотентно: повторный вызов с уже установленным флагом сразу отменит.
pub async fn process_blob(
    blob: &Blob,
    chunk_size: usize,
    cancel: Rc<Cell<bool>>,
    mut on_progress: impl FnMut(usize, usize, &[u32; 256]),
) -> Result<Metadata, JsValue> {
    let total = blob.size() as usize;
    let mut meta = Metadata {
        total_bytes: total,
        chunk_size,
        min_byte: 255,
        max_byte: 0,
        ..Default::default()
    };

    on_progress(0, total, &meta.histogram);

    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let fnv_prime: u64 = 0x0000_0100_0000_01b3;
    let fnv_mix = |h: u64, b: u8| -> u64 { (h ^ (b as u64)).wrapping_mul(fnv_prime) };

    let mut last_yield_ms = now_ms();
    let mut offset = 0usize;

    while offset < total {
        if cancel.get() {
            return Err(JsValue::from_str(CANCELLED));
        }

        let end = (offset + chunk_size).min(total);
        let bytes = read_slice_bytes(blob, offset as f64, end as f64).await?;

        for &b in &bytes {
            meta.histogram[b as usize] = meta.histogram[b as usize].saturating_add(1);
            meta.sum = meta.sum.wrapping_add(b as u64);
            if b < meta.min_byte {
                meta.min_byte = b;
            }
            if b > meta.max_byte {
                meta.max_byte = b;
            }
            hash = fnv_mix(hash, b);
        }

        meta.chunks += 1;
        offset = end;

        if now_ms() - last_yield_ms >= 16.0 {
            gloo_timers::future::TimeoutFuture::new(0).await;
            last_yield_ms = now_ms();
            on_progress(offset, total, &meta.histogram);
        }
    }

    // Явный yield перед финальным callback'ом: дать браузеру отрисовать
    // последний промежуточный кадр и гарантированно показать 100%.
    gloo_timers::future::TimeoutFuture::new(0).await;
    on_progress(total, total, &meta.histogram);

    meta.unique_bytes = meta.histogram.iter().filter(|&&v| v > 0).count();
    if total == 0 {
        meta.min_byte = 0;
    }
    meta.sha256 = format!("{:016x}", hash);
    Ok(meta)
}

// ---------- Рисование гистограммы ----------

/// Общий «скелет» рисования. `CanvasRenderingContext2d` и
/// `OffscreenCanvasRenderingContext2d` — разные типы в web-sys, но их API
/// идентичен (Canvas 2D API). Макрос разворачивает одну и ту же логику
/// в два публичных/приватных биндера без дублирования кода.
macro_rules! histogram_draw_body {
    ($ctx:expr, $hist:expr, $w:expr, $h:expr) => {{
        $ctx.set_fill_style_str("#0d1117");
        $ctx.fill_rect(0.0, 0.0, $w, $h);

        $ctx.set_stroke_style_str("#21262d");
        $ctx.set_line_width(1.0);
        $ctx.begin_path();
        $ctx.move_to(0.0, $h - 0.5);
        $ctx.line_to($w, $h - 0.5);
        $ctx.stroke();

        let max = $hist.iter().copied().max().unwrap_or(1).max(1) as f64;
        let bar_w = $w / 256.0;
        $ctx.set_fill_style_str("#58a6ff");
        for (i, &count) in $hist.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let bh = (count as f64 / max) * ($h - 20.0);
            let x = (i as f64) * bar_w;
            let y = $h - 10.0 - bh;
            $ctx.fill_rect(x, y, bar_w.max(1.0), bh);
        }

        $ctx.set_fill_style_str("#8b949e");
        $ctx.set_font("11px monospace");
        $ctx.fill_text("byte histogram (0..255)", 8.0, 14.0)?;
    }};
}

/// Рисует гистограмму на обычном `CanvasRenderingContext2d`.
/// Используется в live-режиме — canvas живёт прямо в DOM.
pub fn draw_histogram(
    ctx: &CanvasRenderingContext2d,
    histogram: &[u32; 256],
    width: f64,
    height: f64,
) -> Result<(), JsValue> {
    histogram_draw_body!(ctx, histogram, width, height);
    Ok(())
}

/// Рисует гистограмму на `OffscreenCanvasRenderingContext2d`.
/// Используется при генерации PNG — canvas живёт только в памяти.
fn draw_histogram_offscreen(
    ctx: &OffscreenCanvasRenderingContext2d,
    histogram: &[u32; 256],
    width: f64,
    height: f64,
) -> Result<(), JsValue> {
    histogram_draw_body!(ctx, histogram, width, height);
    Ok(())
}

/// Генерирует PNG-гистограмму через `OffscreenCanvas`.
///
/// `convert_to_blob()` — Promise-версия (не callback). Именно она
/// экспортируется web-sys 0.3.105 под этим именем; у `HtmlCanvasElement`
/// `to_blob` — callback-API с другим контрактом.
pub async fn render_histogram_png(
    histogram: &[u32; 256],
    width: u32,
    height: u32,
) -> Result<Blob, JsValue> {
    let canvas = OffscreenCanvas::new(width, height)?;
    let ctx_js = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?;
    let ctx: OffscreenCanvasRenderingContext2d = ctx_js.dyn_into()?;

    draw_histogram_offscreen(&ctx, histogram, width as f64, height as f64)?;

    let promise = canvas.convert_to_blob()?;
    let value = wasm_bindgen_futures::JsFuture::from(promise).await?;
    value.dyn_into::<Blob>()
}

// ---------- Конвейер ----------

pub async fn run_pipeline(
    input: &Blob,
    chunk_size: usize,
    cancel: Rc<Cell<bool>>,
    mut on_progress: impl FnMut(usize, usize, &[u32; 256]),
) -> Result<ProcessedArtifacts, JsValue> {
    let metadata = process_blob(input, chunk_size, cancel, &mut on_progress).await?;
    let png = render_histogram_png(&metadata.histogram, 820, 200).await?;
    let json = make_json_blob(&metadata)?;
    Ok(ProcessedArtifacts { png, json, metadata })
}

pub fn make_json_blob(meta: &Metadata) -> Result<Blob, JsValue> {
    let text = meta.to_json();
    let bytes = Uint8Array::from(text.as_bytes());
    let parts = Array::new();
    parts.push(bytes.as_ref());
    let bag = BlobPropertyBag::new();
    bag.set_type("application/json");
    Blob::new_with_u8_array_sequence_and_options(&parts, &bag)
}

fn now_ms() -> f64 {
    js_sys::Date::now()
}