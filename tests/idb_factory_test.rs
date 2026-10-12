//! Wasm-тесты модуля `idb_factory`: открытие, удаление, список баз,
//! сравнение ключей.
//!
//! Компилируются только под wasm32 — на host файл пуст.
//!
//! ПРАВИЛО: тесты не пересекаются между запусками — каждое имя базы
//! содержит уникальный суффикс `uid()`. После каждого теста база
//! удаляется, соединение закрывается.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::*;

use hello_wasm02::idb_factory;

wasm_bindgen_test_configure!(run_in_browser);

/// Уникальный суффикс, чтобы тесты не пересекались в одном origin
/// при последовательных запусках.
fn uid() -> String {
    return format!("{}", js_sys::Date::now() as u64);
}

/// Уникальное имя тестовой базы.
fn test_db_name() -> String {
    return format!("test-idb-{}", uid());
}

/// Закрывает открытое соединение с базой.
///
/// `IDBDatabase` не имеет типизированной обёртки в web-sys, но метод
/// `close()` доступен через Reflect. Без закрытия следующий
/// `deleteDatabase` заблокируется.
fn close_db(db: &JsValue) {
    if let Ok(close_fn) = js_sys::Reflect::get(db, &JsValue::from_str("close")) {
        if let Ok(f) = close_fn.dyn_into::<js_sys::Function>() {
            let _ = f.call0(db);
        }
    }
}

/// Читает поле `name` из `IDBDatabase`.
fn db_name(db: &JsValue) -> String {
    js_sys::Reflect::get(db, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// Читает поле `version` из `IDBDatabase`.
fn db_version(db: &JsValue) -> u32 {
    js_sys::Reflect::get(db, &JsValue::from_str("version"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u32
}

// ---------------- open_by_name / open_demo ----------------

#[wasm_bindgen_test]
async fn open_returns_object_with_name_and_version() {
    let name = test_db_name();
    let db = idb_factory::open_by_name(&name, 1)
        .await
        .expect("open_by_name");

    assert_eq!(db_name(&db), name, "имя открытой базы должно совпасть");
    assert_eq!(db_version(&db), 1, "версия должна быть 1");

    // Уборка: закрыть + удалить.
    close_db(&db);
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn open_with_higher_version_bumps_schema() {
    let name = test_db_name();

    // Открываем v1.
    let db = idb_factory::open_by_name(&name, 1).await.expect("open v1");
    assert_eq!(db_version(&db), 1);
    close_db(&db);

    // Открываем v2 — версия должна обновиться.
    let db = idb_factory::open_by_name(&name, 2).await.expect("open v2");
    assert_eq!(db_version(&db), 2);
    close_db(&db);

    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn open_demo_creates_demo_database() {
    // Убираем прошлое состояние.
    let _ = idb_factory::delete_demo().await;

    let db = idb_factory::open_demo().await.expect("open_demo");
    assert_eq!(db_name(&db), idb_factory::DEMO_DB);
    assert_eq!(db_version(&db), idb_factory::DEMO_VERSION);
    close_db(&db);

    idb_factory::delete_demo().await.expect("cleanup");
}

// ---------------- delete_by_name / delete_demo ----------------

#[wasm_bindgen_test]
async fn delete_by_name_removes_database() {
    let name = test_db_name();

    // Открываем, чтобы база появилась.
    let db = idb_factory::open_by_name(&name, 1).await.expect("open");
    close_db(&db);

    // Проверяем, что база в списке.
    let before = idb_factory::list_databases().await.expect("list");
    assert!(
        before.iter().any(|d| d.name == name),
        "база {name} должна быть в списке до удаления"
    );

    // Удаляем.
    idb_factory::delete_by_name(&name).await.expect("delete");

    // Проверяем, что её больше нет.
    let after = idb_factory::list_databases().await.expect("list after");
    assert!(
        !after.iter().any(|d| d.name == name),
        "база {name} не должна быть в списке после удаления"
    );
}

#[wasm_bindgen_test]
async fn delete_by_name_is_idempotent() {
    let name = test_db_name();

    // Первый вызов — база есть или нет, главное результат Ok.
    idb_factory::delete_by_name(&name).await.expect("first delete");

    // Второй вызов — база точно отсутствует. Тоже Ok.
    idb_factory::delete_by_name(&name).await.expect("second delete");

    // Третий — на всякий случай.
    idb_factory::delete_by_name(&name).await.expect("third delete");
}

#[wasm_bindgen_test]
async fn delete_demo_is_idempotent() {
    let _ = idb_factory::delete_demo().await;
    let _ = idb_factory::delete_demo().await;
    let _ = idb_factory::delete_demo().await;
    // Все три вызова не паникуют.
}

// ---------------- list_databases ----------------

#[wasm_bindgen_test]
async fn list_databases_returns_array_with_opened() {
    let name = test_db_name();

    let db = idb_factory::open_by_name(&name, 1).await.expect("open");
    close_db(&db);

    let list = idb_factory::list_databases().await.expect("list");
    let found = list.iter().find(|d| d.name == name).expect("in list");
    assert_eq!(found.version, 1, "версия в списке должна быть 1");

    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn list_databases_reports_version() {
    let name = test_db_name();

    let db = idb_factory::open_by_name(&name, 3).await.expect("open v3");
    close_db(&db);

    let list = idb_factory::list_databases().await.expect("list");
    let found = list.iter().find(|d| d.name == name).expect("in list");
    assert_eq!(found.version, 3);

    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn list_databases_does_not_contain_deleted() {
    let name = test_db_name();

    // Сначала открыть и удалить.
    let db = idb_factory::open_by_name(&name, 1).await.expect("open");
    close_db(&db);
    idb_factory::delete_by_name(&name).await.expect("delete");

    // Теперь проверить, что нет.
    let list = idb_factory::list_databases().await.expect("list");
    assert!(!list.iter().any(|d| d.name == name));
}

// ---------------- compare_keys: правило типов ----------------
//
// IndexedDB сортирует ключи по типу: number < date < string < binary < array.
// Внутри типа — естественный порядок.

#[wasm_bindgen_test]
fn compare_keys_numbers() {
    let a = JsValue::from_f64(1.0);
    let b = JsValue::from_f64(2.0);
    assert_eq!(idb_factory::compare_keys(&a, &b).unwrap(), -1);
    assert_eq!(idb_factory::compare_keys(&b, &a).unwrap(), 1);
    assert_eq!(idb_factory::compare_keys(&a, &a).unwrap(), 0);
}

#[wasm_bindgen_test]
fn compare_keys_number_less_than_date() {
    // По правилам IndexedDB: любой number < любой date.
    let num = JsValue::from_f64(1e9); // миллиард — большое число
    let date = js_sys::Date::new(&JsValue::from_f64(0.0)); // 1970-01-01
    let date_val: JsValue = date.into();

    assert_eq!(
        idb_factory::compare_keys(&num, &date_val).unwrap(),
        -1,
        "number всегда меньше date, даже если численно больше"
    );
    assert_eq!(idb_factory::compare_keys(&date_val, &num).unwrap(), 1);
}

#[wasm_bindgen_test]
fn compare_keys_date_less_than_string() {
    // По правилам IndexedDB: любая date < любой string.
    let date = js_sys::Date::new(&JsValue::from_f64(0.0));
    let date_val: JsValue = date.into();
    let s = JsValue::from_str(""); // даже пустая строка

    assert_eq!(idb_factory::compare_keys(&date_val, &s).unwrap(), -1);
    assert_eq!(idb_factory::compare_keys(&s, &date_val).unwrap(), 1);
}

#[wasm_bindgen_test]
fn compare_keys_string_less_than_binary() {
    // По правилам IndexedDB: любая string < любой binary (ArrayBuffer).
    let s = JsValue::from_str("zzz");
    let buf = js_sys::ArrayBuffer::new(0); // даже пустой
    let buf_val: JsValue = buf.into();

    assert_eq!(
        idb_factory::compare_keys(&s, &buf_val).unwrap(),
        -1,
        "string всегда меньше binary"
    );
    assert_eq!(idb_factory::compare_keys(&buf_val, &s).unwrap(), 1);
}

#[wasm_bindgen_test]
fn compare_keys_binary_less_than_array() {
    // По правилам IndexedDB: любой binary < любой array.
    let buf = js_sys::ArrayBuffer::new(100);
    let buf_val: JsValue = buf.into();
    let arr = js_sys::Array::new();
    let arr_val: JsValue = arr.into();

    assert_eq!(idb_factory::compare_keys(&buf_val, &arr_val).unwrap(), -1);
    assert_eq!(idb_factory::compare_keys(&arr_val, &buf_val).unwrap(), 1);
}

// ---------------- compare_keys: внутри одного типа ----------------

#[wasm_bindgen_test]
fn compare_keys_strings_lexicographic() {
    let a = JsValue::from_str("apple");
    let b = JsValue::from_str("banana");
    assert_eq!(idb_factory::compare_keys(&a, &b).unwrap(), -1);
    assert_eq!(idb_factory::compare_keys(&b, &a).unwrap(), 1);

    // Совпадающие строки.
    let a2 = JsValue::from_str("apple");
    assert_eq!(idb_factory::compare_keys(&a, &a2).unwrap(), 0);
}

#[wasm_bindgen_test]
fn compare_keys_dates_chronological() {
    let early = js_sys::Date::new(&JsValue::from_f64(0.0));
    let late = js_sys::Date::new(&JsValue::from_f64(1_000_000.0));
    let early_val: JsValue = early.into();
    let late_val: JsValue = late.into();

    assert_eq!(idb_factory::compare_keys(&early_val, &late_val).unwrap(), -1);
    assert_eq!(idb_factory::compare_keys(&late_val, &early_val).unwrap(), 1);
}

#[wasm_bindgen_test]
fn compare_keys_negative_numbers() {
    let a = JsValue::from_f64(-10.0);
    let b = JsValue::from_f64(5.0);
    assert_eq!(idb_factory::compare_keys(&a, &b).unwrap(), -1);
    assert_eq!(idb_factory::compare_keys(&b, &a).unwrap(), 1);
}

// ---------------- compare_keys: full chain ----------------
//
// Полная цепочка: number < date < string < binary < array.

#[wasm_bindgen_test]
fn compare_keys_full_type_order() {
    let number = JsValue::from_f64(1.0);
    let date: JsValue = js_sys::Date::new(&JsValue::from_f64(0.0)).into();
    let string = JsValue::from_str("");
    let binary: JsValue = js_sys::ArrayBuffer::new(0).into();
    let array: JsValue = js_sys::Array::new().into();

    let chain = [&number, &date, &string, &binary, &array];

    for i in 0..chain.len() {
        for j in (i + 1)..chain.len() {
            assert_eq!(
                idb_factory::compare_keys(chain[i], chain[j]).unwrap(),
                -1,
                "элемент {i} должен быть меньше {j} по типу"
            );
            assert_eq!(
                idb_factory::compare_keys(chain[j], chain[i]).unwrap(),
                1,
                "элемент {j} должен быть больше {i} по типу"
            );
        }
    }
}