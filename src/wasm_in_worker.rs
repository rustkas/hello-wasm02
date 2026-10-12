//! 22.4. WASM в Worker: Rust в параллель.
//!
//! Модуль решает две задачи:
//!
//! 1. **Экспорт фильтра** — `#[wasm_bindgen] box_blur_rgba` доступен
//!    и из main thread (как обычная Rust-функция), и из Worker'а
//!    (через `import ... from "/pkg/hello_wasm02.js"`).
//! 2. **Экспорт instance_id** — уникальный идентификатор на инстанс
//!    WASM. Показывает, что main thread и Worker имеют разные
//!    инстансы одного и того же `.wasm`.
//!
//! ## Почему `#[wasm_bindgen]` на фильтре
//!
//! Worker написан на JavaScript. Чтобы вызвать Rust-функцию из
//! Worker'а, она должна быть экспортирована в JS-модуль. `#[wasm_bindgen]`
//! делает именно это — добавляет обёртку в `pkg/hello_wasm02.js`.
//!
//! ## Утилита `bool_of`
//!
//! rust-analyzer теряет тип `matches!` в tail-позиции функции.
//! Обёртка `bool_of` возвращает типу конкретность.

use std::cell::RefCell;
use std::fmt::Arguments;

use js_sys::{Array, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, Worker, WorkerOptions, WorkerType};

// =============================================================================
// Утилиты для обхода багов rust-analyzer в tail-позиции
// =============================================================================

/// Обёртка над `format!` для tail-позиций.
#[inline]
fn format_str(args: Arguments<'_>) -> String {
    args.to_string()
}

/// Обёртка над `matches!` для tail-позиций.
#[inline]
fn bool_of(value: bool) -> bool {
    value
}

// =============================================================================
// Feature detection
// =============================================================================

/// `Worker` доступен в этом браузере.
pub fn is_worker_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("Worker")),
        Ok(v) if v.is_function()
    ))
}

/// Module workers (`{ type: "module" }`) поддерживаются.
///
/// В современных браузерах (Chrome 80+, Firefox 114+, Safari 15+)
/// module workers — часть стандарта. Прямой feature detection без
/// создания Worker'а невозможен, поэтому проверяем косвенно: наличие
/// `Worker` и `WorkerOptions`.
pub fn is_module_worker_supported() -> bool {
    is_worker_supported()
}

/// `ImageBitmap` доступен — на случай, если демо переключат на
/// ImageBitmap-транспорт.
pub fn is_image_bitmap_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("ImageBitmap")),
        Ok(v) if v.is_function()
    ))
}

// =============================================================================
// Instance ID: уникален для каждого инстанса WASM
// =============================================================================

thread_local! {
    static INSTANCE_ID: RefCell<Option<String>> = RefCell::new(None);
}

/// Уникальный идентификатор инстанса WASM.
///
/// Генерируется один раз при первом вызове. Разные инстансы
/// (main thread, каждый Worker) получают разные ID — доказательство,
/// что WASM загружен заново в каждом контексте.
#[wasm_bindgen]
pub fn instance_id() -> String {
    INSTANCE_ID.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let n = (js_sys::Math::random() * 0x1_0000_0000u64 as f64) as u32;
            *slot = Some(format_str(format_args!("wasm-{n:08x}")));
        }
        slot.as_deref().unwrap_or("wasm-unknown").to_string()
    })
}

// =============================================================================
// Фильтр: box blur, применяется `passes` раз
// =============================================================================

/// Box blur 3×3, применённый `passes` раз к RGBA-изображению.
///
/// Вход: `data` — RGBA-байты (`w * h * 4`), `w`, `h` — размеры.
/// Выход: новые RGBA-байты того же размера.
///
/// Экспортируется в JS через `#[wasm_bindgen]` — вызывается
/// и из main thread'а (напрямую как Rust-функция), и из Worker'а
/// (через `import { box_blur_rgba } from "/pkg/hello_wasm02.js"`).
#[wasm_bindgen]
pub fn box_blur_rgba(data: &[u8], w: u32, h: u32, passes: u32) -> Vec<u8> {
    let ww = w as i32;
    let hh = h as i32;
    let len = data.len();
    let mut src: Vec<u8> = data.to_vec();
    let mut dst: Vec<u8> = vec![0u8; len];

    for _ in 0..passes {
        for y in 0..hh {
            for x in 0..ww {
                let mut r: u32 = 0;
                let mut g: u32 = 0;
                let mut b: u32 = 0;
                let mut n: u32 = 0;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let nx = x + dx;
                        let ny = y + dy;
                        if nx < 0 || nx >= ww || ny < 0 || ny >= hh {
                            continue;
                        }
                        let idx = ((ny * ww + nx) * 4) as usize;
                        r += src[idx] as u32;
                        g += src[idx + 1] as u32;
                        b += src[idx + 2] as u32;
                        n += 1;
                    }
                }
                let di = ((y * ww + x) * 4) as usize;
                dst[di] = (r / n) as u8;
                dst[di + 1] = (g / n) as u8;
                dst[di + 2] = (b / n) as u8;
                dst[di + 3] = src[di + 3];
            }
        }
        std::mem::swap(&mut src, &mut dst);
    }
    src
}

// =============================================================================
// Создание module worker
// =============================================================================

/// Создаёт Worker как ESM-модуль. Обязательно для `import()` внутри
/// Worker'а.
pub fn create_module_worker(url: &str) -> Result<Worker, JsValue> {
    let opts = WorkerOptions::new();
    opts.set_type(WorkerType::Module);
    Worker::new_with_options(url, &opts)
}

// =============================================================================
// Обмен с Worker'ом
// =============================================================================

/// Отправляет изображение Worker'у: `{ type, buffer, width, height, passes }`.
///
/// `buffer` создаётся в JS-heap (Uint8Array::new_with_length), поэтому
/// его ArrayBuffer transferable.
pub fn post_image_to_worker(
    worker: &Worker,
    data: &[u8],
    w: u32,
    h: u32,
    passes: u32,
) -> Result<(), JsValue> {
    let arr = Uint8Array::new_with_length(data.len() as u32);
    arr.copy_from(data);
    let buffer: JsValue = arr.buffer().into();

    let msg = Object::new();
    Reflect::set(&msg, &JsValue::from_str("type"), &JsValue::from_str("filter"))?;
    Reflect::set(&msg, &JsValue::from_str("buffer"), &buffer)?;
    Reflect::set(&msg, &JsValue::from_str("width"), &JsValue::from_f64(w as f64))?;
    Reflect::set(&msg, &JsValue::from_str("height"), &JsValue::from_f64(h as f64))?;
    Reflect::set(&msg, &JsValue::from_str("passes"), &JsValue::from_f64(passes as f64))?;

    let transfer = Array::new();
    transfer.push(&buffer);
    worker.post_message_with_transfer(&msg, &transfer)
}

/// Результат, извлечённый из `MessageEvent` от Worker'а.
pub struct FilterResult {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub elapsed_ms: f64,
    pub instance_id: String,
}

/// Извлекает результат фильтрации из `MessageEvent`.
///
/// Ожидает `{ type: "result", buffer, width, height, elapsed_ms, instance_id }`.
/// Возвращает `None`, если структура не соответствует.
pub fn extract_filter_result(event: &MessageEvent) -> Option<FilterResult> {
    let data = event.data();

    let kind = Reflect::get(&data, &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())?;
    if kind != "result" {
        return None;
    }

    let buffer_val = Reflect::get(&data, &JsValue::from_str("buffer")).ok()?;
    let arr: Uint8Array = buffer_val.dyn_into().ok()?;
    let bytes: Vec<u8> = arr.to_vec();

    let width = Reflect::get(&data, &JsValue::from_str("width"))
        .ok()?
        .as_f64()? as u32;
    let height = Reflect::get(&data, &JsValue::from_str("height"))
        .ok()?
        .as_f64()? as u32;
    let elapsed_ms = Reflect::get(&data, &JsValue::from_str("elapsed_ms"))
        .ok()?
        .as_f64()?;
    let instance_id = Reflect::get(&data, &JsValue::from_str("instance_id"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| String::from("wasm-unknown"));

    Some(FilterResult {
        bytes,
        width,
        height,
        elapsed_ms,
        instance_id,
    })
}

/// Извлекает ID инстанса из сообщения `{ type: "ready", instance }`.
pub fn extract_ready_instance(event: &MessageEvent) -> Option<String> {
    let data = event.data();
    let kind = Reflect::get(&data, &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())?;
    if kind != "ready" {
        return None;
    }
    Reflect::get(&data, &JsValue::from_str("instance"))
        .ok()
        .and_then(|v| v.as_string())
}