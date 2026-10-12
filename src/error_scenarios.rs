//! 47. Обработка ошибок: сценарии-источники ошибок в браузере.
//!
//! ## Что здесь
//!
//! JS-функции, каждая из которых провоцирует определённый тип ошибки:
//! синхронный `throw` (`TypeError`, `RangeError`, `ReferenceError`,
//! `Error`, `DOMException`) и `Promise.reject` (`Error`, `DOMException`,
//! примитивы). Регистрируются на `window.__err_scenarios` через
//! `inline_js`, вызываются из Rust через `js_sys::Function::call0`.
//!
//! ## Почему не `extern "C" fn scenario()` напрямую
//!
//! Если объявить `extern "C" fn __err_throw_type_error() -> u32`,
//! то JS-throw приведёт к **WASM-trap**, а не к catchable `Err`.
//! Чтобы получить `Result<JsValue, JsValue>`, нужно вызывать через
//! `js_sys::Function::call0` — `wasm-bindgen` ловит JS-исключение
//! и возвращает его как `Err(JsValue)`.

use js_sys::{Function, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::video_frame::format_str;

// =============================================================================
// Установка сценариев
// =============================================================================

#[wasm_bindgen(inline_js = "
export function __err_install() {
    const scenarios = {
        'throw_type_error': function() {
            const x = undefined;
            return x.someDeepProperty;
        },
        'throw_range_error': function() {
            return new Array(-1);
        },
        'throw_reference_error': function() {
            return __err_nonExistentFunction12345();
        },
        'throw_custom_error': function() {
            throw new Error('Custom message from JS');
        },
        'throw_dom_exception': function() {
            throw new DOMException('Device not found', 'NotFoundError');
        },
        'reject_error': function() {
            return Promise.reject(new Error('Async operation failed'));
        },
        'reject_dom_exception': function() {
            return Promise.reject(new DOMException('Permission denied', 'NotAllowedError'));
        },
        'reject_string': function() {
            return Promise.reject('bare string rejection');
        },
        'reject_number': function() {
            return Promise.reject(42);
        },
        'success': function() {
            return Promise.resolve(42);
        },
    };
    window.__err_scenarios = scenarios;
}
")]
extern "C" {
    fn __err_install();
}

/// Устанавливает сценарии на `window`. Вызывается один раз при старте
/// приложения или тестов.
pub fn install() {
    __err_install();
}

// =============================================================================
// Получение функции сценария
// =============================================================================

pub fn get_scenario(name: &str) -> Result<Function, String> {
    let window = web_sys::window().ok_or_else(|| String::from("no window"))?;
    let scenarios = Reflect::get(window.as_ref(), &JsValue::from_str("__err_scenarios"))
        .map_err(|_| String::from("scenarios not installed — call install() first"))?;
    if scenarios.is_undefined() || scenarios.is_null() {
        return Err(String::from("scenarios not installed"));
    }
    let f = Reflect::get(&scenarios, &JsValue::from_str(name))
        .map_err(|_| format_str(format_args!("scenario '{name}' not found")))?;
    if f.is_undefined() {
        return Err(format_str(format_args!("scenario '{name}' is undefined")));
    }
    return f
        .dyn_into::<Function>()
        .map_err(|_| format_str(format_args!("scenario '{name}' is not a Function")));
}

// =============================================================================
// Запуск сценариев
// =============================================================================

/// Синхронный вызов. Если JS бросает — получаем `Err(JsValue)`.
pub fn run_sync(name: &str) -> Result<JsValue, JsValue> {
    let f = get_scenario(name).map_err(|e| JsValue::from_str(&e))?;
    return f.call0(&JsValue::UNDEFINED);
}

/// Асинхронный вызов. Сценарий обязан вернуть Promise; его
/// разрешение/отклонение превращается в `Ok`/`Err`.
pub async fn run_async(name: &str) -> Result<JsValue, JsValue> {
    let f = get_scenario(name).map_err(|e| JsValue::from_str(&e))?;
    let promise_value = f.call0(&JsValue::UNDEFINED)?;
    if promise_value.is_undefined() || promise_value.is_null() {
        return Err(JsValue::from_str("scenario returned no Promise"));
    }
    let promise: Promise = promise_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("scenario did not return a Promise"))?;
    return JsFuture::from(promise).await;
}
