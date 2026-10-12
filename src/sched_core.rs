//! Обёртки над браузерными API планирования.
//! Автономный модуль — не зависит от других модулей проекта.

use wasm_bindgen::prelude::*;
use web_sys::IdleDeadline;

/// Возвращает `performance.now()` — монотонные миллисекунды.
pub fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

/// Универсальный yield: `TimeoutFuture::new(0).await`.
/// Это macrotask — браузер между вызовами перерисовывает страницу.
pub async fn yield_to_ui() {
    gloo_timers::future::TimeoutFuture::new(0).await;
}

/// Планирует колбэк через `requestAnimationFrame`.
/// Возвращает `i32` — тип ID для rAF (в WebIDL это `long`).
pub fn schedule_raf<F>(f: F) -> Result<i32, JsValue>
where
    F: FnOnce(f64) + 'static,
{
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let cb = Closure::once_into_js(move |ts: f64| f(ts));
    window
        .request_animation_frame(cb.unchecked_ref())
        .map_err(|e| JsValue::from_str(&format!("rAF: {e:?}")))
}

/// Отменяет колбэк, запланированный через `schedule_raf`.
pub fn cancel_raf(id: i32) -> Result<(), JsValue> {
    web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .cancel_animation_frame(id)
        .map_err(|e| JsValue::from_str(&format!("cancel rAF: {e:?}")))
}

/// Планирует колбэк через `requestIdleCallback`.
///
/// Возвращает `u32`: в WebIDL `requestIdleCallback` возвращает
/// `unsigned long`, в отличие от `requestAnimationFrame` (`long` → `i32`).
pub fn schedule_idle<F>(f: F) -> Result<u32, JsValue>
where
    F: FnOnce(IdleDeadline) + 'static,
{
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let cb = Closure::once_into_js(move |deadline: IdleDeadline| f(deadline));
    window
        .request_idle_callback(cb.unchecked_ref())
        .map_err(|e| JsValue::from_str(&format!("rIC: {e:?}")))
}

/// Отменяет idle-колбэк. Принимает тот же `u32`, что вернул `schedule_idle`.
///
/// `cancel_idle_callback` возвращает `()` — не `Result`.
/// Ошибок здесь не бывает: невалидный ID просто игнорируется.
pub fn cancel_idle(id: u32) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    window.cancel_idle_callback(id);
    Ok(())
}

/// Планирует колбэк через `setTimeout`.
/// `setTimeout` — macrotask, браузер перерисовывает между вызовами.
pub fn schedule_timeout<F>(f: F, delay_ms: i32) -> Result<i32, JsValue>
where
    F: FnOnce() + 'static,
{
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let cb = Closure::once_into_js(move || f());
    window
        .set_timeout_with_callback_and_timeout_and_arguments_0(cb.unchecked_ref(), delay_ms)
        .map_err(|e| JsValue::from_str(&format!("setTimeout: {e:?}")))
}