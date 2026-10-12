//! Тесты для `js_sys`: Array, Map, Set, Date, Uint8Array.
use wasm_bindgen_test::*;
use wasm_bindgen::{JsCast, JsValue};
use js_sys::{Array, Map, Uint8Array};

use hello_wasm02::js_objects;

wasm_bindgen_test_configure!(run_in_browser);

// ============================================================
// Array
// ============================================================

#[wasm_bindgen_test]
fn test_array_of_numbers() {
    let arr = js_objects::array_of_numbers(&[1.0, 2.0, 3.0]);
    assert_eq!(arr.length(), 3);
    assert_eq!(arr.get(0).as_f64(), Some(1.0));
    assert_eq!(arr.get(2).as_f64(), Some(3.0));
}

#[wasm_bindgen_test]
fn test_array_concat() {
    let a = js_objects::array_of_numbers(&[1.0, 2.0]);
    let b = js_objects::array_of_numbers(&[3.0, 4.0]);
    let c = js_objects::array_concat(&a, &b);

    assert_eq!(c.length(), 4);
    assert_eq!(c.get(0).as_f64(), Some(1.0));
    assert_eq!(c.get(3).as_f64(), Some(4.0));
}

#[wasm_bindgen_test]
fn test_array_index_of() {
    let arr = js_objects::array_of_numbers(&[10.0, 20.0, 30.0]);

    assert_eq!(js_objects::array_index_of(&arr, &JsValue::from_f64(20.0)), 1);
    assert_eq!(js_objects::array_index_of(&arr, &JsValue::from_f64(99.0)), -1);
}

// ============================================================
// Map
// ============================================================

#[wasm_bindgen_test]
fn test_map_basic() {
    let map = js_objects::new_map();

    js_objects::map_set(&map, &JsValue::from_str("a"), &JsValue::from_f64(1.0));
    js_objects::map_set(&map, &JsValue::from_str("b"), &JsValue::from_f64(2.0));

    assert_eq!(js_objects::map_size(&map), 2);
    assert!(js_objects::map_has(&map, &JsValue::from_str("a")));
    assert!(!js_objects::map_has(&map, &JsValue::from_str("c")));

    let val = js_objects::map_get(&map, &JsValue::from_str("a")).unwrap();
    assert_eq!(val.as_f64(), Some(1.0));
}

#[wasm_bindgen_test]
fn test_map_delete() {
    let map = js_objects::new_map();
    js_objects::map_set(&map, &JsValue::from_str("x"), &JsValue::from_f64(1.0));

    assert!(js_objects::map_delete(&map, &JsValue::from_str("x")));
    assert_eq!(js_objects::map_size(&map), 0);
    assert!(!js_objects::map_has(&map, &JsValue::from_str("x")));
}

// ============================================================
// Set
// ============================================================

#[wasm_bindgen_test]
fn test_set_basic() {
    let set = js_objects::new_set();

    js_objects::set_add(&set, &JsValue::from_str("a"));
    js_objects::set_add(&set, &JsValue::from_str("b"));
    js_objects::set_add(&set, &JsValue::from_str("a")); // дубликат

    assert_eq!(js_objects::set_size(&set), 2);
    assert!(js_objects::set_has(&set, &JsValue::from_str("a")));
    assert!(js_objects::set_has(&set, &JsValue::from_str("b")));
    assert!(!js_objects::set_has(&set, &JsValue::from_str("c")));
}

#[wasm_bindgen_test]
fn test_set_delete() {
    let set = js_objects::new_set();
    js_objects::set_add(&set, &JsValue::from_str("x"));

    assert!(js_objects::set_delete(&set, &JsValue::from_str("x")));
    assert_eq!(js_objects::set_size(&set), 0);
}

// ============================================================
// Date
// ============================================================

#[wasm_bindgen_test]
fn test_date_now() {
    let now = js_objects::date_now();
    assert!(now > 1_700_000_000_000.0); // после 2023-11-14
}

#[wasm_bindgen_test]
fn test_date_from_string() {
    let d = js_objects::date_from_string("2024-01-15T12:00:00Z");

    assert_eq!(js_objects::date_full_year(&d), 2024.0);
    assert_eq!(js_objects::date_month(&d), 0.0); // январь = 0
    assert_eq!(js_objects::date_date(&d), 15.0);

    let iso = js_objects::date_to_iso(&d);
    assert!(iso.starts_with("2024-01-15"));
}

// ============================================================
// Uint8Array
// ============================================================

#[wasm_bindgen_test]
fn test_uint8_array_from_vec() {
    let data = vec![10u8, 20, 30, 40, 50];
    let arr = js_objects::uint8_array_from_vec(&data);

    assert_eq!(js_objects::uint8_array_length(&arr), 5);
    assert_eq!(js_objects::uint8_array_get(&arr, 0), 10);
    assert_eq!(js_objects::uint8_array_get(&arr, 4), 50);
}

#[wasm_bindgen_test]
fn test_uint8_array_to_vec() {
    let data = vec![1u8, 2, 3];
    let arr = js_objects::uint8_array_from_vec(&data);
    let back = js_objects::uint8_array_to_vec(&arr);

    assert_eq!(back, data);
}

#[wasm_bindgen_test]
fn test_uint8_array_slice() {
    let data = vec![1u8, 2, 3, 4, 5];
    let arr = js_objects::uint8_array_from_vec(&data);
    let slice = js_objects::uint8_array_slice(&arr, 1, 4);

    assert_eq!(js_objects::uint8_array_length(&slice), 3);
    assert_eq!(js_objects::uint8_array_to_vec(&slice), vec![2u8, 3, 4]);
}

#[wasm_bindgen_test]
fn test_uint8_array_zeroed() {
    let arr = js_objects::uint8_array_zeroed(4);
    assert_eq!(js_objects::uint8_array_length(&arr), 4);
    assert_eq!(js_objects::uint8_array_to_vec(&arr), vec![0u8, 0, 0, 0]);
}

#[wasm_bindgen_test]
fn test_uint8_array_set_get() {
    let arr = js_objects::uint8_array_zeroed(3);

    js_objects::uint8_array_set(&arr, 0, 100);
    js_objects::uint8_array_set(&arr, 1, 200);

    assert_eq!(js_objects::uint8_array_get(&arr, 0), 100);
    assert_eq!(js_objects::uint8_array_get(&arr, 1), 200);
    assert_eq!(js_objects::uint8_array_get(&arr, 2), 0);
}

// ============================================================
// Проверка типов через JsCast
// ============================================================

#[wasm_bindgen_test]
fn test_array_is_js_sys_type() {
    let arr = Array::new();
    arr.push(&JsValue::from_f64(1.0));

    let generic: &JsValue = arr.as_ref();
    assert!(generic.is_array());

    // Требует `use wasm_bindgen::JsCast;` в области видимости.
    let back: &Array = generic.unchecked_ref();
    assert_eq!(back.length(), 1);
}

#[wasm_bindgen_test]
fn test_map_is_object() {
    let map = Map::new();
    js_objects::map_set(&map, &JsValue::from_str("k"), &JsValue::from_f64(1.0));

    let generic: &JsValue = map.as_ref();
    assert!(generic.is_object());
    // Map не является массивом
    assert!(!generic.is_array());
}

#[wasm_bindgen_test]
fn test_uint8_array_is_object() {
    let arr = Uint8Array::new_with_length(4);
    let generic: &JsValue = arr.as_ref();

    assert!(generic.is_object());
    assert!(!generic.is_array());
}