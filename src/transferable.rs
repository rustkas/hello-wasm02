//! 22.3. Transferable objects: zero-copy между потоками.
//!
//! Обёртки вокруг `ArrayBuffer`, `MessageChannel` и `MessagePort`:
//!
//! - **feature detection** — поддерживает ли браузер `MessageChannel`
//!   и `ArrayBuffer`;
//! - **создание буферов** — `Uint8Array::new_with_length`;
//! - **отправка** — `postMessage` с transfer-флагом и без;
//! - **диагностика** — `byte_length` показывает `0` для нейтрированного
//!   буфера;
//! - **MessageChannel** — создание, передача порта, обмен по каналу.
//!
//! ## Утилита `bool_of`
//!
//! rust-analyzer теряет тип `matches!` в tail-позиции функции и выдаёт
//! `expected bool, found ()`. Обёртка `bool_of` с явной сигнатурой
//! возвращает типу конкретность.

use js_sys::{Array, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use web_sys::{MessageChannel, MessagePort, Worker};

// =============================================================================
// Утилита для обхода бага rust-analyzer в tail-позиции
// =============================================================================

/// Обёртка над `matches!` для tail-позиций.
#[inline]
fn bool_of(value: bool) -> bool {
    value
}

// =============================================================================
// Feature detection
// =============================================================================

/// `MessageChannel` доступен в этом браузере.
pub fn is_message_channel_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("MessageChannel")),
        Ok(v) if v.is_function()
    ))
}

/// `ArrayBuffer` доступен. Фактически проверка бессмысленна — он есть
/// везде, где есть typed arrays, — но оставляем как шаблон.
pub fn is_array_buffer_supported() -> bool {
    let global = js_sys::global();
    bool_of(matches!(
        Reflect::get(&global, &JsValue::from_str("ArrayBuffer")),
        Ok(v) if v.is_function()
    ))
}

// =============================================================================
// Создание буферов
// =============================================================================

/// Создаёт `ArrayBuffer` заданного размера, заполняет первые байты
/// паттерном. Возвращает `(buffer, byte_length)`.
pub fn make_buffer(size: usize) -> (JsValue, f64) {
    let u8 = Uint8Array::new_with_length(size as u32);
    for i in 0..size.min(1024) {
        u8.set_index(i as u32, (i & 0xFF) as u8);
    }
    let buffer: JsValue = u8.buffer().into();
    let len = byte_length(&buffer);
    (buffer, len)
}

/// `byteLength` значения. Для нейтрированного буфера — `0`.
/// Для не-буфера возвращает `-1`.
pub fn byte_length(value: &JsValue) -> f64 {
    Reflect::get(value, &JsValue::from_str("byteLength"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0)
}

// =============================================================================
// Transfer vs Copy
// =============================================================================

/// Отправляет буфер Worker'у как **transferable** — O(1).
///
/// Возвращает `t0` (время отправки в мс).
/// После вызова `buffer` в отправителе нейтрирован:
/// `byte_length(&buffer) == 0`.
pub fn send_transfer(worker: &Worker, buffer: &JsValue, size: usize) -> Result<f64, JsValue> {
    let msg = Object::new();
    Reflect::set(&msg, &JsValue::from_str("type"), &JsValue::from_str("transfer_test"))?;
    Reflect::set(&msg, &JsValue::from_str("buffer"), buffer)?;
    Reflect::set(&msg, &JsValue::from_str("size"), &JsValue::from_f64(size as f64))?;
    let t0 = js_sys::Date::now();
    Reflect::set(&msg, &JsValue::from_str("t0"), &JsValue::from_f64(t0))?;

    let transfer = Array::new();
    transfer.push(buffer);
    worker.post_message_with_transfer(&msg, &transfer)?;
    Ok(t0)
}

/// Отправляет буфер Worker'у как **copy** — O(n).
///
/// Возвращает `t0` (время отправки в мс).
/// После вызова `buffer` в отправителе не изменён:
/// `byte_length(&buffer) == size`.
pub fn send_copy(worker: &Worker, buffer: &JsValue, size: usize) -> Result<f64, JsValue> {
    let msg = Object::new();
    Reflect::set(&msg, &JsValue::from_str("type"), &JsValue::from_str("transfer_test"))?;
    Reflect::set(&msg, &JsValue::from_str("buffer"), buffer)?;
    Reflect::set(&msg, &JsValue::from_str("size"), &JsValue::from_f64(size as f64))?;
    let t0 = js_sys::Date::now();
    Reflect::set(&msg, &JsValue::from_str("t0"), &JsValue::from_f64(t0))?;

    // Пустой transfer-список — копирование.
    worker.post_message(&msg)?;
    Ok(t0)
}

// =============================================================================
// MessageChannel
// =============================================================================

/// Создаёт пару связанных портов.
pub fn create_message_channel() -> Result<MessageChannel, JsValue> {
    MessageChannel::new()
}

/// Порт №1 канала (остаётся в main thread).
pub fn port1(channel: &MessageChannel) -> MessagePort {
    channel.port1()
}

/// Порт №2 канала (обычно передаётся Worker'у).
pub fn port2(channel: &MessageChannel) -> MessagePort {
    channel.port2()
}

/// Передаёт порт Worker'у. Порт становится недоступен в отправителе.
///
/// Сообщение — `{ type: "setup_port", port: <MessagePort> }`,
/// массив transferable содержит только сам порт.
pub fn transfer_port(worker: &Worker, port: &MessagePort) -> Result<(), JsValue> {
    let msg = Object::new();
    Reflect::set(
        &msg,
        &JsValue::from_str("type"),
        &JsValue::from_str("setup_port"),
    )?;
    Reflect::set(&msg, &JsValue::from_str("port"), port.as_ref())?;

    let transfer = Array::new();
    transfer.push(port.as_ref());
    worker.post_message_with_transfer(&msg, &transfer)
}

/// Отправляет `{ type: "ping", seq, t0 }` по каналу.
pub fn send_ping(port: &MessagePort, seq: u32) -> Result<f64, JsValue> {
    let msg = Object::new();
    Reflect::set(&msg, &JsValue::from_str("type"), &JsValue::from_str("ping"))?;
    Reflect::set(&msg, &JsValue::from_str("seq"), &JsValue::from_f64(seq as f64))?;
    let t0 = js_sys::Date::now();
    Reflect::set(&msg, &JsValue::from_str("t0"), &JsValue::from_f64(t0))?;
    port.post_message(&msg)?;
    Ok(t0)
}

/// Активирует порт — без этого `onmessage` не сработает после передачи.
pub fn start_port(port: &MessagePort) {
    port.start();
}

/// Закрывает порт.
pub fn close_port(port: &MessagePort) {
    port.close();
}