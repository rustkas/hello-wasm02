//! Тесты для `JsValue`: примитивы, массивы, объекты, функции.
use wasm_bindgen_test::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use js_sys::{Array, Function, Object};

use hello_wasm02::value;

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================
// Примитивы
// ============================================================

#[wasm_bindgen_test]
fn test_string_value() {
    let v = value::from_str("hello");

    assert!(value::is_string(&v));
    assert!(!value::is_number(&v));
    assert!(!value::is_bool(&v));
    assert!(!value::is_null(&v));
    assert!(!value::is_undefined(&v));
    assert!(!value::is_array(&v));
    assert!(!value::is_object(&v));
    assert!(!value::is_function(&v));
    assert!(value::is_primitive(&v));

    assert_eq!(value::as_string(&v), Some("hello".into()));
    assert_eq!(value::type_name(&v), "string");
}

#[wasm_bindgen_test]
fn test_number_value() {
    let v = value::from_f64(42.5);

    assert!(value::is_number(&v));
    assert!(!value::is_string(&v));
    assert!(!value::is_bool(&v));
    assert!(value::is_primitive(&v));

    assert_eq!(value::as_f64(&v), Some(42.5));
    assert_eq!(value::type_name(&v), "number");
}

#[wasm_bindgen_test]
fn test_bool_value() {
    let t = value::from_bool(true);
    let f = value::from_bool(false);

    assert!(value::is_bool(&t));
    assert!(value::is_bool(&f));
    assert!(value::is_primitive(&t));

    assert_eq!(value::as_bool(&t), Some(true));
    assert_eq!(value::as_bool(&f), Some(false));
    assert_eq!(value::type_name(&t), "boolean");
}

#[wasm_bindgen_test]
fn test_null_value() {
    let v = value::null();

    assert!(value::is_null(&v));
    assert!(!value::is_undefined(&v));
    assert!(!value::is_object(&v)); // важное отличие от typeof
    assert!(value::is_primitive(&v));

    assert_eq!(value::as_string(&v), None);
    assert_eq!(value::as_f64(&v), None);
}

#[wasm_bindgen_test]
fn test_undefined_value() {
    let v = value::undefined();

    assert!(value::is_undefined(&v));
    assert!(!value::is_null(&v));
    assert!(value::is_primitive(&v));

    assert_eq!(value::as_string(&v), None);
}

// ============================================================
// Массивы
// ============================================================

#[wasm_bindgen_test]
fn test_empty_array() {
    let arr = value::new_array();

    assert_eq!(arr.length(), 0);

    // Явная аннотация, чтобы `as_ref()` вывел целевой тип.
    let js: &JsValue = arr.as_ref();
    assert!(js.is_array());
    assert!(js.is_object()); // ← массивы — объекты в JavaScript
}

#[wasm_bindgen_test]
fn test_array_of_strings() {
    let arr = value::array_of_strings(&["один", "два", "три"]);

    assert_eq!(arr.length(), 3);

    let first = value::array_get(&arr, 0).unwrap();
    assert_eq!(value::as_string(&first), Some("один".into()));

    let second = value::array_get(&arr, 1).unwrap();
    assert_eq!(value::as_string(&second), Some("два".into()));

    let out_of_range = value::array_get(&arr, 99);
    assert!(out_of_range.is_none());
}

// ============================================================
// Объекты
// ============================================================

#[wasm_bindgen_test]
fn test_empty_object() {
    let obj = value::new_object();

    // Явная аннотация, чтобы `as_ref()` вывел целевой тип.
    let js: &JsValue = obj.as_ref();
    assert!(js.is_object());
    assert!(!js.is_array());
    assert!(!js.is_function());
}

#[wasm_bindgen_test]
fn test_object_set_get() {
    let obj = value::new_object();

    value::object_set(&obj, "name", &value::from_str("Rust")).unwrap();
    value::object_set(&obj, "version", &value::from_f64(1.75)).unwrap();

    let name = value::object_get(&obj, "name").unwrap();
    assert_eq!(value::as_string(&name), Some("Rust".into()));

    let version = value::object_get(&obj, "version").unwrap();
    assert_eq!(value::as_f64(&version), Some(1.75));

    assert!(value::object_has(&obj, "name"));
    assert!(value::object_has(&obj, "version"));
    assert!(!value::object_has(&obj, "missing"));
}

#[wasm_bindgen_test]
fn test_object_from_pairs() {
    let obj = value::object_from_pairs(&[
        ("language", "Rust"),
        ("target", "wasm"),
    ])
    .unwrap();

    let lang = value::object_get(&obj, "language").unwrap();
    assert_eq!(value::as_string(&lang), Some("Rust".into()));

    let target = value::object_get(&obj, "target").unwrap();
    assert_eq!(value::as_string(&target), Some("wasm".into()));
}

// ============================================================
// Функции
// ============================================================

#[wasm_bindgen_test]
fn test_function_detection() {
    let f: Function = js_sys::eval("(function() { return 1; })")
        .unwrap()
        .dyn_into()
        .unwrap();

    let v: &JsValue = f.as_ref();

    assert!(value::is_function(v));
    assert!(!value::is_object(v)); // функции ≠ object по is_object
    assert!(!value::is_array(v));
    assert!(!value::is_primitive(v));
    assert_eq!(value::type_name(v), "function");
}

#[wasm_bindgen_test]
fn test_function_call() {
    let f: Function = js_sys::eval("(function(x) { return x * 2; })")
        .unwrap()
        .dyn_into()
        .unwrap();

    let arg = value::from_f64(21.0);
    let result = value::call_function_1(&f, &arg).unwrap();

    assert_eq!(value::as_f64(&result), Some(42.0));
}

// ============================================================
// describe
// ============================================================

#[wasm_bindgen_test]
fn test_describe() {
    assert_eq!(value::describe(&value::null()), "null");
    assert_eq!(value::describe(&value::undefined()), "undefined");
    assert!(value::describe(&value::from_str("x")).starts_with("string:"));
    assert!(value::describe(&value::from_f64(1.0)).starts_with("number:"));
    assert!(value::describe(&value::from_bool(true)).starts_with("boolean:"));

    // Привязка с явной аннотацией — иначе `as_ref()` неоднозначен.
    let arr = value::new_array();
    let arr_js: &JsValue = arr.as_ref();
    assert_eq!(value::describe(arr_js), "array: len=0");

    let obj = value::new_object();
    let obj_js: &JsValue = obj.as_ref();
    assert_eq!(value::describe(obj_js), "object");
}

// ============================================================
// JsCast: unchecked_ref и dyn_into
// ============================================================

#[wasm_bindgen_test]
fn test_unchecked_ref_to_array() {
    let arr = Array::new();
    arr.push(&JsValue::from_str("a"));

    let generic: &JsValue = arr.as_ref();
    assert!(generic.is_array());

    // После проверки типа — unchecked_ref безопасен
    let back: &Array = generic.unchecked_ref();
    assert_eq!(back.length(), 1);
}

#[wasm_bindgen_test]
fn test_dyn_into_object() {
    let obj = Object::new();
    value::object_set(&obj, "k", &JsValue::from_str("v")).unwrap();

    let generic: JsValue = obj.clone().into();
    assert!(generic.is_object());

    let back: Object = generic.dyn_into().unwrap();
    let v = value::object_get(&back, "k").unwrap();
    assert_eq!(value::as_string(&v), Some("v".into()));
}