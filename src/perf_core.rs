//! Обёртка над Performance API: mark, measure, Span.
//! Автономный модуль — не зависит от других модулей проекта.
//!
//! ВАЖНО (web-sys 0.3.105):
//!   • Performance::mark возвращает Result<PerformanceMark, JsValue>.
//!   • Performance::clear_marks / clear_measures — возвращают () без Result.
//!   • Performance::get_entries_by_type — js_sys::Array напрямую.
//!   • measure(name, start, end) вызывается через Reflect:
//!     типизированного метода с тремя строками в web-sys нет.

use wasm_bindgen::prelude::*;
use web_sys::Performance;

/// Возвращает `window.performance`.
pub fn performance() -> Result<Performance, JsValue> {
    web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .performance()
        .ok_or_else(|| JsValue::from_str("no performance"))
}

/// `performance.now()` — монотонные миллисекунды.
pub fn now_ms() -> f64 {
    performance().map(|p| p.now()).unwrap_or(0.0)
}

/// Ставит именованную отметку. Возвращает `Ok(())`.
pub fn mark(name: &str) -> Result<(), JsValue> {
    // Performance::mark возвращает PerformanceMark — отбрасываем его.
    performance()?
        .mark(name)
        .map(|_| ())
        .map_err(|e| JsValue::from_str(&format!("mark {name}: {e:?}")))
}

/// Измеряет промежуток между двумя отметками.
///
/// В web-sys 0.3.105 типизированного метода с тремя строками нет,
/// поэтому вызываем `performance.measure(name, start, end)` через Reflect.
/// Возвращает `duration` созданной записи.
pub fn measure(name: &str, start: &str, end: &str) -> Result<f64, JsValue> {
    let perf = performance()?;

    let f = js_sys::Reflect::get(&perf, &JsValue::from_str("measure"))
        .map_err(|e| JsValue::from_str(&format!("Reflect measure: {e:?}")))?;
    let func: js_sys::Function = f
        .dyn_into()
        .map_err(|_| JsValue::from_str("measure не функция"))?;

    func.call3(
        &perf,
        &JsValue::from_str(name),
        &JsValue::from_str(start),
        &JsValue::from_str(end),
    )
    .map_err(|e| JsValue::from_str(&format!("measure {name}: {e:?}")))?;

    // Читаем duration только что созданной записи.
    let entries: js_sys::Array = perf.get_entries_by_name(name);
    if entries.length() == 0 {
        return Ok(0.0);
    }
    let entry: web_sys::PerformanceEntry =
        entries.get(entries.length() - 1).unchecked_into();
    Ok(entry.duration())
}

/// Очищает все mark'и. Возвращает `Ok(())` для единообразия API.
pub fn clear_marks() -> Result<(), JsValue> {
    performance()?.clear_marks();
    Ok(())
}

/// Очищает все measure'ы.
pub fn clear_measures() -> Result<(), JsValue> {
    performance()?.clear_measures();
    Ok(())
}

/// Возвращает список имён всех записей указанного типа.
///
/// `get_entries_by_type` возвращает `js_sys::Array` — без Result.
pub fn entry_names_of_type(entry_type: &str) -> Result<Vec<String>, JsValue> {
    let perf = performance()?;
    let entries: js_sys::Array = perf.get_entries_by_type(entry_type);
    let mut out = Vec::with_capacity(entries.length() as usize);
    for i in 0..entries.length() {
        let entry: web_sys::PerformanceEntry = entries.get(i).unchecked_into();
        out.push(entry.name());
    }
    Ok(out)
}

/// RAII-обёртка: засекает старт при создании, возвращает длительность
/// при `finish()`.
pub struct Span {
    start: f64,
    name: String,
}

impl Span {
    pub fn start(name: &str) -> Self {
        Self {
            start: now_ms(),
            name: name.to_string(),
        }
    }

    pub fn elapsed(&self) -> f64 {
        now_ms() - self.start
    }

    /// Возвращает длительность. Отметки не создаёт.
    pub fn finish(self) -> f64 {
        let end = now_ms();
        let elapsed = end - self.start;
        let _ = &self.name;
        elapsed
    }
}