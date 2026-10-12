//! Помощники для работы с встроенными объектами JavaScript.
//!
//! `js_sys` предоставляет Rust-обёртки для всех глобальных объектов,
//! гарантированных стандартом ECMAScript: `Array`, `Object`, `Function`,
//! `Map`, `Set`, `Promise`, `Date`, типизированные массивы.
//!
//! Этот модуль — переиспользуемая библиотека. Часть функций пока
//! не используется в `app.rs`, но понадобится в следующих главах.
#![allow(dead_code)]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use js_sys::{Array, Date, Function, Map, Object, Promise, Reflect, Set, Uint8Array};

// ============================================================
// Array
// ============================================================

/// Создаёт массив из слайса `JsValue`.
pub fn array_from_values(values: &[JsValue]) -> Array {
    let arr = Array::new();
    for v in values {
        arr.push(v);
    }
    arr
}

/// Создаёт массив чисел.
pub fn array_of_numbers(values: &[f64]) -> Array {
    let arr = Array::new();
    for n in values {
        arr.push(&JsValue::from_f64(*n));
    }
    arr
}

/// Создаёт массив из `Vec<T>` через `JsValue::from`.
pub fn array_from_vec<T: Into<JsValue> + Clone>(items: &[T]) -> Array {
    let arr = Array::new();
    for item in items {
        arr.push(&item.clone().into());
    }
    arr
}

/// Сортирует массив.
pub fn array_sort(arr: &Array) -> Array {
    arr.sort()
}

/// Объединяет массивы.
pub fn array_concat(a: &Array, b: &Array) -> Array {
    a.concat(b)
}

/// Ищет индекс элемента.
pub fn array_index_of(arr: &Array, value: &JsValue) -> i32 {
    arr.index_of(value, 0)
}

// ============================================================
// Object
// ============================================================

/// Создаёт объект из итератора пар ключ-значение.
pub fn object_from_entries(entries: &[(&str, JsValue)]) -> Result<Object, JsValue> {
    let obj = Object::new();
    for (key, value) in entries {
        Reflect::set(obj.as_ref(), &JsValue::from_str(key), value)?;
    }
    Ok(obj)
}

/// Возвращает ключи объекта.
pub fn object_keys(obj: &Object) -> Array {
    Object::keys(obj)
}

/// Возвращает значения объекта.
pub fn object_values(obj: &Object) -> Array {
    Object::values(obj)
}

// ============================================================
// Function
// ============================================================

/// Создаёт `Function` из `js_sys::eval`.
pub fn eval_function(code: &str) -> Result<Function, JsValue> {
    js_sys::eval(code)?.dyn_into::<Function>()
}

/// Создаёт `Function` из строки аргументов и тела.
///
/// `args` — список имён аргументов, `body` — тело функции.
/// Возвращает `Function` напрямую: `Function::new_with_args`
/// не может провалиться.
pub fn function_with_args(args: &[&str], body: &str) -> Function {
    Function::new_with_args(&args.join(","), body)
}

// ============================================================
// Map
// ============================================================

/// Создаёт `Map`.
pub fn new_map() -> Map {
    Map::new()
}

/// Устанавливает пару ключ-значение.
pub fn map_set(map: &Map, key: &JsValue, value: &JsValue) -> Map {
    map.set(key, value)
}

/// Читает значение по ключу.
pub fn map_get(map: &Map, key: &JsValue) -> Option<JsValue> {
    let value = map.get(key);
    if value.is_undefined() {
        None
    } else {
        Some(value)
    }
}

/// Проверяет наличие ключа.
pub fn map_has(map: &Map, key: &JsValue) -> bool {
    map.has(key)
}

/// Удаляет пару.
pub fn map_delete(map: &Map, key: &JsValue) -> bool {
    map.delete(key)
}

/// Возвращает размер.
pub fn map_size(map: &Map) -> u32 {
    map.size()
}

// ============================================================
// Set
// ============================================================

/// Создаёт `Set`.
pub fn new_set() -> Set {
    Set::new(&Array::new().into())
}

/// Добавляет значение.
pub fn set_add(set: &Set, value: &JsValue) -> Set {
    set.add(value)
}

/// Проверяет наличие значения.
pub fn set_has(set: &Set, value: &JsValue) -> bool {
    set.has(value)
}

/// Удаляет значение.
pub fn set_delete(set: &Set, value: &JsValue) -> bool {
    set.delete(value)
}

/// Возвращает размер.
pub fn set_size(set: &Set) -> u32 {
    set.size()
}

// ============================================================
// Promise
// ============================================================

/// Создаёт зарезолвленный `Promise`.
pub fn promise_resolve(value: &JsValue) -> Promise {
    Promise::resolve(value)
}

/// Создаёт зареджекченный `Promise`.
pub fn promise_reject(reason: &JsValue) -> Promise {
    Promise::reject(reason)
}

/// Создаёт `Promise` через `Promise::new`.
///
/// `executor` — замыкание, принимающее `resolve` и `reject`.
/// Rust коэрцирует `&mut F` в `&mut dyn FnMut(Function, Function)`
/// автоматически, без `.into()`.
pub fn promise_new<F>(mut executor: F) -> Promise
where
    F: FnMut(Function, Function) + 'static,
{
    Promise::new(&mut executor)
}

// ============================================================
// Date
// ============================================================

/// Текущее время в миллисекундах (как `Date.now()`).
pub fn date_now() -> f64 {
    Date::now()
}

/// Создаёт `Date` из строки.
pub fn date_from_string(s: &str) -> Date {
    Date::new(&JsValue::from_str(s))
}

/// Возвращает год.
pub fn date_full_year(d: &Date) -> f64 {
    d.get_full_year() as f64
}

/// Возвращает месяц (0–11).
pub fn date_month(d: &Date) -> f64 {
    d.get_month() as f64
}

/// Возвращает день месяца.
pub fn date_date(d: &Date) -> f64 {
    d.get_date() as f64
}

/// Форматирует `Date` в ISO-строку.
pub fn date_to_iso(d: &Date) -> String {
    d.to_iso_string().as_string().unwrap_or_default()
}

// ============================================================
// Typed Arrays
// ============================================================

/// Создаёт `Uint8Array` из `Vec<u8>`.
pub fn uint8_array_from_vec(data: &[u8]) -> Uint8Array {
    Uint8Array::from(data)
}

/// Возвращает длину `Uint8Array`.
pub fn uint8_array_length(arr: &Uint8Array) -> u32 {
    arr.length()
}

/// Возвращает `Vec<u8>` из `Uint8Array`.
pub fn uint8_array_to_vec(arr: &Uint8Array) -> Vec<u8> {
    arr.to_vec()
}

/// Создаёт `Uint8Array` заданной длины, заполненный нулями.
pub fn uint8_array_zeroed(len: u32) -> Uint8Array {
    Uint8Array::new_with_length(len)
}

/// Читает элемент по индексу.
pub fn uint8_array_get(arr: &Uint8Array, index: u32) -> u8 {
    arr.get_index(index)
}

/// Устанавливает элемент по индексу.
pub fn uint8_array_set(arr: &Uint8Array, index: u32, value: u8) {
    arr.set_index(index, value);
}

/// Возвращает срез `Uint8Array`.
pub fn uint8_array_slice(arr: &Uint8Array, start: u32, end: u32) -> Uint8Array {
    arr.slice(start, end)
}