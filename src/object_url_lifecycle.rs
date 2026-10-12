//! Object URL: жизненный цикл и утечки.
//!
//! Обёртки над `URL.createObjectURL` / `URL.revokeObjectURL` + учёт живых
//! дескрипторов + чтение `performance.memory.*` (Chromium) + попытка
//! запросить GC, если браузер запущен с `--js-flags="--expose-gc"`.
//!
//! Ключевая идея: `Blob` в Rust — не владелец байтов. Пока жива строка
//! `blob:...`, реестр URL держит Blob. Утечка = «мы потеряли строку, но
//! браузер её не отпустил».

use js_sys::{Array, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use web_sys::{Blob, BlobPropertyBag, Url};

/// Создаёт Blob заданного размера из «шумных» байт — так GC не свернёт его.
pub fn make_payload_blob(size_kb: usize) -> Result<Blob, JsValue> {
    let len = size_kb.saturating_mul(1024).max(1) as u32;
    let bytes = Uint8Array::new_with_length(len);
    for i in 0..len {
        let v = ((i.wrapping_mul(31).wrapping_add(7)) & 0xFF) as u8;
        bytes.set_index(i, v);
    }
    let parts = Array::new();
    parts.push(bytes.as_ref());
    let bag = BlobPropertyBag::new();
    bag.set_type("application/octet-stream");
    Blob::new_with_u8_array_sequence_and_options(&parts, &bag)
}

/// `URL.createObjectURL(blob)` → строка `blob:...`.
pub fn create_object_url(blob: &Blob) -> Result<String, JsValue> {
    Url::create_object_url_with_blob(blob)
}

/// `URL.revokeObjectURL(url)`. Идемпотентна: повторный вызов — no-op.
pub fn revoke_object_url(url: &str) {
    let _ = Url::revoke_object_url(url);
}

/// `performance.memory.usedJSHeapSize` (байт). `None` — API недоступно.
pub fn read_used_heap_bytes() -> Option<f64> {
    read_memory_field("usedJSHeapSize")
}

/// `performance.memory.jsHeapSizeLimit` (байт). `None` — API недоступно.
pub fn read_heap_limit_bytes() -> Option<f64> {
    read_memory_field("jsHeapSizeLimit")
}

fn read_memory_field(name: &str) -> Option<f64> {
    let global = js_sys::global();
    let perf = Reflect::get(&global, &JsValue::from_str("performance")).ok()?;
    if perf.is_undefined() || perf.is_null() {
        return None;
    }
    let mem = Reflect::get(&perf, &JsValue::from_str("memory")).ok()?;
    if mem.is_undefined() || mem.is_null() {
        return None;
    }
    Reflect::get(&mem, &JsValue::from_str(name))
        .ok()?
        .as_f64()
}

/// Пытается вызвать `window.gc()` (доступно при `--js-flags=--expose-gc`).
/// Возвращает `true`, если вызов удался.
pub fn try_request_gc() -> bool {
    let global = js_sys::global();
    let Ok(candidate) = Reflect::get(&global, &JsValue::from_str("gc")) else {
        return false;
    };
    let Ok(func) = candidate.dyn_into::<js_sys::Function>() else {
        return false;
    };
    func.call0(&JsValue::UNDEFINED).is_ok()
}

/// Учёт blob-URL: живые / созданные / освобождённые / «потерянные».
#[derive(Default)]
pub struct ObjectUrlLab {
    live: Vec<String>,
    created: usize,
    revoked: usize,
    forgotten: usize,
}

impl ObjectUrlLab {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn live_count(&self) -> usize {
        self.live.len()
    }

    pub fn created_count(&self) -> usize {
        self.created
    }

    pub fn revoked_count(&self) -> usize {
        self.revoked
    }

    pub fn forgotten_count(&self) -> usize {
        self.forgotten
    }

    /// Создаёт `count` blob-URL по `size_kb` КБ и держит их как «живые».
    /// Отсутствие revoke и есть наблюдаемая утечка.
    pub fn leak(&mut self, count: usize, size_kb: usize) -> Result<usize, JsValue> {
        for _ in 0..count {
            let blob = make_payload_blob(size_kb)?;
            let url = create_object_url(&blob)?;
            self.live.push(url);
            self.created += 1;
        }
        Ok(count)
    }

    /// Освобождает один самый свежий URL. `false`, если живых не было.
    pub fn release_one(&mut self) -> bool {
        if let Some(url) = self.live.pop() {
            revoke_object_url(&url);
            self.revoked += 1;
            true
        } else {
            false
        }
    }

    /// Освобождает все живые URL. Возвращает число отозванных. Идемпотентно.
    pub fn release_all(&mut self) -> usize {
        let n = self.live.len();
        for url in self.live.drain(..) {
            revoke_object_url(&url);
            self.revoked += 1;
        }
        n
    }

    /// Настоящая утечка: Rust теряет строки, revoke не вызывается.
    /// Браузер продолжает держать Blob в URL-реестре до выгрузки документа.
    pub fn forget_all(&mut self) -> usize {
        let n = self.live.len();
        self.live.clear();
        self.forgotten += n;
        n
    }
}