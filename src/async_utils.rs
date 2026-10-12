//! Помощники для асинхронной работы на Wasm.
//!
//! ## Как это работает
//!
//! JavaScript-мир асинхронный: `setTimeout`, `fetch`, `Promise` — всё
//! это работает через event loop браузера. Rust-мир тоже асинхронный:
//! `Future`, `async`, `await`. Задача этого модуля — построить мост
//! между ними.
//!
//! Мост — `wasm_bindgen_futures::JsFuture`. Он принимает `js_sys::Promise`
//! и превращает его в Rust `Future`. После этого `promise.await`
//! работает как обычно.
//!
//! `spawn_local` — способ запустить `Future` в фоне. Он регистрирует
//! задачу в event loop браузера; когда `Future` готов, она продолжает
//! выполнение с того же места.
#![allow(dead_code)]

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use js_sys::{Array, Promise};

// ============================================================
// Задержки
// ============================================================

/// Создаёт `Promise`, который разрешается через `ms` миллисекунд.
///
/// Внутри использует `setTimeout`. `Promise` не несёт значения —
/// разрешается с `undefined`.
pub fn delay_ms(ms: i32) -> Promise {
    Promise::new(&mut |resolve, _reject| {
        let window = web_sys::window().expect("no window");
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .expect("setTimeout failed");
    })
}

/// Ждёт `ms` миллисекунд.
///
/// Возвращает `Result<(), JsValue>` для единообразия с другими
/// асинхронными функциями. `delay_ms` не может провалиться, но
/// сигнатура `async fn` с `Result` удобнее для `?`.
pub async fn sleep(ms: i32) -> Result<(), JsValue> {
    JsFuture::from(delay_ms(ms)).await?;
    Ok(())
}

// ============================================================
// Promise ↔ Future
// ============================================================

/// Ждёт разрешения `Promise` и возвращает результат.
///
/// Если `Promise` разрешается значением `v`, возвращает `Ok(v)`.
/// Если `Promise` отклоняется причиной `e`, возвращает `Err(e)`.
pub async fn await_promise(promise: Promise) -> Result<JsValue, JsValue> {
    JsFuture::from(promise).await
}

/// Обёртка над `Promise.resolve(value)`.
pub fn resolved(value: &JsValue) -> Promise {
    Promise::resolve(value)
}

/// Обёртка над `Promise.reject(reason)`.
pub fn rejected(reason: &JsValue) -> Promise {
    Promise::reject(reason)
}

// ============================================================
// Promise.race и таймауты
// ============================================================

/// Обёртка над `Promise.race(promises)`.
///
/// Возвращает `Promise`, который разрешается или отклоняется
/// значением первого завершившегося `Promise` из массива.
///
/// Использует готовую обёртку `js_sys::Promise::race`, которая
/// корректно вызывает JavaScript-метод с нужным `this`.
pub fn promise_race(promises: &Array) -> Promise {
    Promise::race(promises)
}

/// Ждёт `Promise` с таймаутом.
///
/// Если `Promise` разрешится за `timeout_ms`, возвращает его значение.
/// Если таймаут сработает раньше, возвращает `Err` с сообщением
/// `"timeout"`.
///
/// **Ограничение:** если `Promise` отклоняется, `Promise.race` вернёт
/// ошибку `Promise`, но определить, что именно произошло — таймаут
/// или отклонение, — не получится. Для точного различения нужен
/// `AbortController` (глава 16).
pub async fn with_timeout(
    promise: Promise,
    timeout_ms: i32,
) -> Result<JsValue, JsValue> {
    // Promise, который reject-ится по таймауту.
    let timeout_promise = Promise::new(&mut |_resolve, reject| {
        let window = web_sys::window().expect("no window");
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                &reject,
                timeout_ms,
            )
            .expect("setTimeout failed");
    });

    let promises = Array::new();
    promises.push(&promise);
    promises.push(&timeout_promise);

    let race = promise_race(&promises);
    JsFuture::from(race).await
}

// ============================================================
// spawn_local
// ============================================================

/// Запускает `Future` в фоне.
///
/// `Future` выполняется через event loop браузера. Функция возвращает
/// управление сразу, не дожидаясь завершения `Future`.
///
/// **Важно:** `Future` не должна паниковать. Если паника произойдёт,
/// `spawn_local` проглотит её, и вы увидите только сообщение в консоли
/// (если установлен `console_error_panic_hook`).
pub fn spawn<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    spawn_local(future);
}

/// Запускает `Future`, которая возвращает `Result`.
///
/// Если `Future` завершается с `Err`, ошибка логируется в консоль.
pub fn spawn_logged<F, E>(future: F)
where
    F: std::future::Future<Output = Result<(), E>> + 'static,
    E: std::fmt::Debug,
{
    spawn_local(async move {
        if let Err(e) = future.await {
            web_sys::console::error_1(
                &format!("spawn_logged error: {:?}", e).into(),
            );
        }
    });
}