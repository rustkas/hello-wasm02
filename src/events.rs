//! Помощники для работы с событиями.
//!
//! Этот модуль — переиспользуемая библиотека. В текущем `app.rs`
//! используется только часть функций; остальные понадобятся
//! в следующих главах. Подавляем `dead_code`, чтобы предупреждения
//! не засоряли вывод сборки.
//!
//! ## Замечание о `return`
//!
//! В функциях, где возвращаемое значение — результат макроса
//! (`matches!`) или цепочки методов, используется явный `return`.
//! Это защищает от редких случаев, когда `rust-analyzer` неверно
//! интерпретирует tail-выражение, и делает намерение однозначным.
#![allow(dead_code)]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Event, EventTarget, KeyboardEvent, MouseEvent, PointerEvent};

// ============================================================
// Добавление обработчиков
// ============================================================

/// Добавляет обработчик события к `EventTarget`.
///
/// Closure перемещается внутрь и сохраняется через `forget()`.
/// Это простой, но одноразовый способ: обработчик живёт
/// до конца работы приложения.
///
/// Для production-кода предпочтительнее хранить `Closure`
/// явно (см. `add_event_listener_keep` ниже).
pub fn add_event_listener<F>(
    target: &EventTarget,
    event_type: &str,
    mut f: F,
) -> Result<(), JsValue>
where
    F: FnMut(Event) + 'static,
{
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        f(event);
    });
    target.add_event_listener_with_callback(event_type, closure.as_ref().unchecked_ref())?;
    closure.forget();
    Ok(())
}

/// Добавляет обработчик и возвращает `Closure` для явного управления временем жизни.
///
/// Вызывающий код должен сохранить `Closure` в структуре или переменной.
/// Когда `Closure` будет удалён, обработчик перестанет работать.
pub fn add_event_listener_keep<F>(
    target: &EventTarget,
    event_type: &str,
    f: F,
) -> Result<Closure<dyn FnMut(Event)>, JsValue>
where
    F: FnMut(Event) + 'static,
{
    let closure = Closure::<dyn FnMut(Event)>::new(f);
    target.add_event_listener_with_callback(event_type, closure.as_ref().unchecked_ref())?;
    Ok(closure)
}

/// Добавляет обработчик с конкретным типом события.
///
/// `T` должен реализовать `JsCast`. Для большинства типов событий
/// (`MouseEvent`, `KeyboardEvent`, `InputEvent`) приведение через
/// `dyn_into` работает автоматически.
pub fn add_typed_listener<T, F>(
    target: &EventTarget,
    event_type: &str,
    mut f: F,
) -> Result<(), JsValue>
where
    T: JsCast + 'static,
    F: FnMut(T) + 'static,
{
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        if let Ok(typed) = event.dyn_into::<T>() {
            f(typed);
        }
    });
    target.add_event_listener_with_callback(event_type, closure.as_ref().unchecked_ref())?;
    closure.forget();
    Ok(())
}

// ============================================================
// Приведение событий к конкретным типам
// ============================================================

/// Приводит `Event` к `MouseEvent`.
pub fn as_mouse_event(event: &Event) -> Result<MouseEvent, JsValue> {
    event
        .clone()
        .dyn_into::<MouseEvent>()
        .map_err(|_| JsValue::from_str("event is not a MouseEvent"))
}

/// Приводит `Event` к `KeyboardEvent`.
pub fn as_keyboard_event(event: &Event) -> Result<KeyboardEvent, JsValue> {
    event
        .clone()
        .dyn_into::<KeyboardEvent>()
        .map_err(|_| JsValue::from_str("event is not a KeyboardEvent"))
}

/// Приводит `Event` к `PointerEvent`.
pub fn as_pointer_event(event: &Event) -> Result<PointerEvent, JsValue> {
    event
        .clone()
        .dyn_into::<PointerEvent>()
        .map_err(|_| JsValue::from_str("event is not a PointerEvent"))
}

// ============================================================
// Управление распространением
// ============================================================

/// Останавливает дальнейшее распространение события (bubbling).
///
/// Событие всё ещё будет обработано текущим обработчиком,
/// но не поднимется к родительским элементам.
pub fn stop_propagation(event: &Event) {
    event.stop_propagation();
}

/// Останавливает распространение и предотвращает вызов
/// других обработчиков на том же элементе.
pub fn stop_immediate_propagation(event: &Event) {
    event.stop_immediate_propagation();
}

/// Предотвращает действие браузера по умолчанию.
///
/// Примеры: перезагрузка формы при `submit`,
/// переход по ссылке при `click` на `<a>`.
pub fn prevent_default(event: &Event) {
    event.prevent_default();
}

// ============================================================
// Вспомогательные проверки
// ============================================================

/// Проверяет, нажата ли клавиша Enter.
pub fn is_enter(event: &KeyboardEvent) -> bool {
    return event.key() == "Enter";
}

/// Проверяет, нажата ли клавиша Escape.
pub fn is_escape(event: &KeyboardEvent) -> bool {
    return event.key() == "Escape";
}

/// Проверяет, нажата ли клавиша-стрелка.
///
/// Возвращает `true` для `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`.
pub fn is_arrow(event: &KeyboardEvent) -> bool {
    return matches!(
        event.key().as_str(),
        "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight"
    );
}

/// Проверяет, нажата ли клавиша-стрелка вверх.
pub fn is_arrow_up(event: &KeyboardEvent) -> bool {
    return event.key() == "ArrowUp";
}

/// Проверяет, нажата ли клавиша-стрелка вниз.
pub fn is_arrow_down(event: &KeyboardEvent) -> bool {
    return event.key() == "ArrowDown";
}

/// Проверяет, нажата ли клавиша-стрелка влево.
pub fn is_arrow_left(event: &KeyboardEvent) -> bool {
    return event.key() == "ArrowLeft";
}

/// Проверяет, нажата ли клавиша-стрелка вправо.
pub fn is_arrow_right(event: &KeyboardEvent) -> bool {
    return event.key() == "ArrowRight";
}

/// Проверяет, что событие мыши вызвано левой кнопкой.
pub fn is_left_button(event: &MouseEvent) -> bool {
    return event.button() == 0;
}

/// Проверяет, что событие мыши вызвано правой кнопкой.
pub fn is_right_button(event: &MouseEvent) -> bool {
    return event.button() == 2;
}

/// Проверяет, что событие мыши вызвано средней кнопкой.
pub fn is_middle_button(event: &MouseEvent) -> bool {
    return event.button() == 1;
}

// ============================================================
// Модификаторы клавиатуры
// ============================================================

/// Проверяет, зажат ли Shift.
pub fn has_shift(event: &KeyboardEvent) -> bool {
    return event.shift_key();
}

/// Проверяет, зажат ли Ctrl.
pub fn has_ctrl(event: &KeyboardEvent) -> bool {
    return event.ctrl_key();
}

/// Проверяет, зажат ли Alt.
pub fn has_alt(event: &KeyboardEvent) -> bool {
    return event.alt_key();
}

/// Проверяет, зажата ли Meta (Cmd на macOS, Win на Windows).
pub fn has_meta(event: &KeyboardEvent) -> bool {
    return event.meta_key();
}