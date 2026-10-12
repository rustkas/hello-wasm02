//! Помощники для работы с `JsValue`.
//!
//! `JsValue` — универсальный тип на границе Rust ↔ JavaScript.
//! Он может представлять любое JavaScript-значение: примитив,
//! массив, объект, функцию. Этот модуль содержит функции для
//! определения типа `JsValue` и безопасного преобразования
//! в Rust-типы.
//!
//! Модуль — переиспользуемая библиотека. Часть функций пока
//! не используется в `app.rs`, но понадобится в следующих главах.
#![allow(dead_code)]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use js_sys::{Array, Function, Object, Reflect};

// ============================================================
// Создание JsValue из Rust-значений
// ============================================================

/// Создаёт `JsValue` из строки.
pub fn from_str(s: &str) -> JsValue {
    JsValue::from_str(s)
}

/// Создаёт `JsValue` из числа.
pub fn from_f64(n: f64) -> JsValue {
    JsValue::from_f64(n)
}

/// Создаёт `JsValue` из булева значения.
pub fn from_bool(b: bool) -> JsValue {
    JsValue::from_bool(b)
}

/// Возвращает `JsValue`, представляющий `null`.
pub fn null() -> JsValue {
    JsValue::NULL
}

/// Возвращает `JsValue`, представляющий `undefined`.
pub fn undefined() -> JsValue {
    JsValue::UNDEFINED
}

// ============================================================
// Определение типа
// ============================================================

/// Возвращает строковое имя типа `JsValue`.
///
/// Использует JavaScript-оператор `typeof`. Для `null` возвращает
/// `"object"` — это особенность JS, которую нужно помнить.
pub fn type_name(value: &JsValue) -> String {
    value
        .js_typeof()
        .as_string()
        .unwrap_or_else(|| "unknown".into())
}

/// Проверяет, является ли значение строкой.
pub fn is_string(value: &JsValue) -> bool {
    value.is_string()
}

/// Проверяет, является ли значение числом.
pub fn is_number(value: &JsValue) -> bool {
    value.as_f64().is_some()
}

/// Проверяет, является ли значение булевым.
pub fn is_bool(value: &JsValue) -> bool {
    value.as_bool().is_some()
}

/// Проверяет, является ли значение `null`.
pub fn is_null(value: &JsValue) -> bool {
    value.is_null()
}

/// Проверяет, является ли значение `undefined`.
pub fn is_undefined(value: &JsValue) -> bool {
    value.is_undefined()
}

/// Проверяет, является ли значение массивом.
pub fn is_array(value: &JsValue) -> bool {
    value.is_array()
}

/// Проверяет, является ли значение объектом.
///
/// В JavaScript `[] instanceof Object === true` — массив наследуется
/// от `Object`, поэтому `is_object()` возвращает `true` и для массивов,
/// и для объектов. Функции возвращают `false`: у них `typeof === "function"`.
///
/// Для точного определения массива используйте `is_array()`,
/// для функции — `is_function()`.
pub fn is_object(value: &JsValue) -> bool {
    value.is_object()
}

/// Проверяет, является ли значение функцией.
pub fn is_function(value: &JsValue) -> bool {
    value.is_function()
}

// ============================================================
// Безопасное преобразование
// ============================================================

/// Пытается получить `String` из `JsValue`.
pub fn as_string(value: &JsValue) -> Option<String> {
    value.as_string()
}

/// Пытается получить `f64` из `JsValue`.
pub fn as_f64(value: &JsValue) -> Option<f64> {
    value.as_f64()
}

/// Пытается получить `bool` из `JsValue`.
pub fn as_bool(value: &JsValue) -> Option<bool> {
    value.as_bool()
}

/// Описывает `JsValue` в читаемом виде для логирования.
pub fn describe(value: &JsValue) -> String {
    if value.is_null() {
        return "null".into();
    }
    if value.is_undefined() {
        return "undefined".into();
    }
    if let Some(s) = value.as_string() {
        return format!("string: {:?}", s);
    }
    if let Some(n) = value.as_f64() {
        return format!("number: {}", n);
    }
    if let Some(b) = value.as_bool() {
        return format!("boolean: {}", b);
    }
    if value.is_array() {
        let arr: &Array = value.unchecked_ref();
        return format!("array: len={}", arr.length());
    }
    if value.is_function() {
        return "function".into();
    }
    if value.is_object() {
        return "object".into();
    }
    return format!("unknown ({})", type_name(value))
}

// ============================================================
// Работа с массивами
// ============================================================

/// Создаёт пустой массив.
pub fn new_array() -> Array {
    Array::new()
}

/// Создаёт массив из Rust-слайса строк.
pub fn array_of_strings(items: &[&str]) -> Array {
    let arr = Array::new();
    for s in items {
        arr.push(&JsValue::from_str(s));
    }
    arr
}

/// Читает элемент массива по индексу.
pub fn array_get(arr: &Array, index: u32) -> Option<JsValue> {
    let value = arr.get(index);
    if value.is_undefined() {
        None
    } else {
        Some(value)
    }
}

// ============================================================
// Работа с объектами
// ============================================================

/// Создаёт пустой объект.
pub fn new_object() -> Object {
    Object::new()
}

/// Устанавливает свойство объекта.
pub fn object_set(obj: &Object, key: &str, value: &JsValue) -> Result<(), JsValue> {
    Reflect::set(obj.as_ref(), &JsValue::from_str(key), value)
        .map(|_| ())
        .map_err(|e| JsValue::from_str(&format!("Reflect::set failed: {:?}", e)))
}

/// Читает свойство объекта.
pub fn object_get(obj: &Object, key: &str) -> Result<JsValue, JsValue> {
    Reflect::get(obj.as_ref(), &JsValue::from_str(key))
}

/// Проверяет наличие свойства в объекте.
pub fn object_has(obj: &Object, key: &str) -> bool {
    Reflect::has(obj.as_ref(), &JsValue::from_str(key)).unwrap_or(false)
}

/// Создаёт объект из пар ключ-значение.
pub fn object_from_pairs(pairs: &[(&str, &str)]) -> Result<Object, JsValue> {
    let obj = Object::new();
    for (key, value) in pairs {
        object_set(&obj, key, &JsValue::from_str(value))?;
    }
    Ok(obj)
}

// ============================================================
// Работа с функциями
// ============================================================

/// Вызывает `Function` без аргументов.
pub fn call_function(f: &Function) -> Result<JsValue, JsValue> {
    f.call0(&JsValue::NULL)
}

/// Вызывает `Function` с одним аргументом.
pub fn call_function_1(f: &Function, arg: &JsValue) -> Result<JsValue, JsValue> {
    f.call1(&JsValue::NULL, arg)
}

// ============================================================
// Проверка на вложенность
// ============================================================

/// Проверяет, является ли `JsValue` «примитивом»:
/// string, number, boolean, null или undefined.
pub fn is_primitive(value: &JsValue) -> bool {
    is_string(value)
        || is_number(value)
        || is_bool(value)
        || is_null(value)
        || is_undefined(value)
}