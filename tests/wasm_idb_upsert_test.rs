//! Wasm-тесты модуля `idb_upsert`: семантика add/put, диагностика,
//! атомарность пачки, классификация ошибок.
//!
//! ПРАВИЛО: `format!` в tail-позиции — через `format_str(format_args!(...))`
//! или явный `return`. rust-analyzer теряет тип в этой позиции.
//!
//! ПРАВИЛО: каждое имя базы содержит уникальный суффикс `uid()` —
//! тесты не пересекаются между запусками.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::*;

use hello_wasm02::idb_upsert::{
    self, SaveOutcome, WriteError,
};

wasm_bindgen_test_configure!(run_in_browser);

fn uid() -> String {
    return format!("{}", js_sys::Date::now() as u64);
}

fn test_db_name() -> String {
    return format!("test-upsert-{}", uid());
}

fn make_record(id: &str, name: &str) -> JsValue {
    let o = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&o, &"id".into(), &JsValue::from_str(id));
    let _ = js_sys::Reflect::set(&o, &"name".into(), &JsValue::from_str(name));
    let _ = js_sys::Reflect::set(&o, &"updated_at".into(), &JsValue::from_f64(0.0));
    return o.into();
}

async fn fresh_db() -> (String, JsValue) {
    let name = test_db_name();
    let db = idb_upsert::open_demo_db(&name).await.expect("open_demo_db");
    return (name, db);
}

async fn cleanup(name: &str, db: &JsValue) {
    if let Ok(close_fn) = js_sys::Reflect::get(db, &"close".into()) {
        if let Ok(f) = close_fn.dyn_into::<js_sys::Function>() {
            let _ = f.call0(db);
        }
    }
    idb_upsert::delete_demo_db(name).await.expect("cleanup");
}

// ---------------- classify_write_error ----------------

#[wasm_bindgen_test]
fn classify_constraint_error() {
    let e = web_sys::DomException::new_with_message_and_name(
        "duplicate",
        "ConstraintError",
    )
    .expect("make DomException");
    let js: JsValue = e.into();
    assert_eq!(
        idb_upsert::classify_write_error(&js),
        WriteError::Constraint,
    );
    assert!(idb_upsert::is_constraint_error(&js));
}

#[wasm_bindgen_test]
fn classify_unknown_error() {
    let e = JsValue::from_str("some strange error");
    let kind = idb_upsert::classify_write_error(&e);
    assert_eq!(kind, WriteError::Other("some strange error".to_string()));
}

#[wasm_bindgen_test]
fn write_error_names() {
    assert_eq!(WriteError::Constraint.name(), "ConstraintError");
    assert_eq!(WriteError::Data.name(), "DataError");
    assert_eq!(WriteError::Quota.name(), "QuotaExceededError");
    assert_eq!(WriteError::Abort.name(), "AbortError");
}

// ---------------- add_one / put_one ----------------

#[wasm_bindgen_test]
async fn add_creates_then_conflicts() {
    let (name, db) = fresh_db().await;

    idb_upsert::add_one(&db, "contacts", &make_record("a", "Alice"))
        .await
        .expect("first add");

    let r = idb_upsert::add_one(&db, "contacts", &make_record("a", "Alice2")).await;
    assert!(r.is_err(), "повторный add с тем же ключом должен упасть");
    let kind = idb_upsert::classify_write_error(&r.unwrap_err());
    assert_eq!(kind, WriteError::Constraint);

    let count = idb_upsert::count_all(&db, "contacts").await.unwrap();
    assert_eq!(count, 1, "вторая запись не должна была записаться");

    cleanup(&name, &db).await;
}

#[wasm_bindgen_test]
async fn put_is_idempotent() {
    let (name, db) = fresh_db().await;

    idb_upsert::put_one(&db, "contacts", &make_record("a", "Alice"))
        .await
        .expect("first put");
    idb_upsert::put_one(&db, "contacts", &make_record("a", "Alice2"))
        .await
        .expect("second put");

    let count = idb_upsert::count_all(&db, "contacts").await.unwrap();
    assert_eq!(count, 1);

    let rec = idb_upsert::get_one(&db, "contacts", &JsValue::from_str("a"))
        .await
        .unwrap()
        .expect("present");
    let new_name = js_sys::Reflect::get(&rec, &"name".into())
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    assert_eq!(new_name, "Alice2", "put перезаписывает значение");

    cleanup(&name, &db).await;
}

// ---------------- save_with_diagnosis ----------------

#[wasm_bindgen_test]
async fn diagnosis_created_on_fresh_key() {
    let (name, db) = fresh_db().await;

    let report = idb_upsert::save_with_diagnosis(
        &db,
        "contacts",
        &make_record("a", "Alice"),
        &JsValue::from_str("a"),
    )
    .await
    .expect("save");

    assert_eq!(report.outcome, SaveOutcome::Created);
    assert!(report.previous.is_none());

    cleanup(&name, &db).await;
}

#[wasm_bindgen_test]
async fn diagnosis_updated_with_previous() {
    let (name, db) = fresh_db().await;

    idb_upsert::put_one(&db, "contacts", &make_record("a", "Alice"))
        .await
        .unwrap();

    let report = idb_upsert::save_with_diagnosis(
        &db,
        "contacts",
        &make_record("a", "Alice2"),
        &JsValue::from_str("a"),
    )
    .await
    .expect("save");

    assert_eq!(report.outcome, SaveOutcome::Updated);
    let prev = report.previous.expect("previous present");
    let old_name = js_sys::Reflect::get(&prev, &"name".into())
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    assert_eq!(old_name, "Alice", "diagnosis вернула старое значение");

    cleanup(&name, &db).await;
}

// ---------------- batch_add_atomic ----------------

#[wasm_bindgen_test]
async fn batch_add_succeeds_when_all_new() {
    let (name, db) = fresh_db().await;

    let values = vec![
        make_record("a", "A"),
        make_record("b", "B"),
        make_record("c", "C"),
    ];
    let out = idb_upsert::batch_add_atomic(&db, "contacts", &values)
        .await
        .expect("batch");

    assert_eq!(out.attempted, 3);
    assert_eq!(out.written, 3);
    assert!(out.aborted_at.is_none());
    assert!(out.abort_reason.is_none());

    let count = idb_upsert::count_all(&db, "contacts").await.unwrap();
    assert_eq!(count, 3);

    cleanup(&name, &db).await;
}

#[wasm_bindgen_test]
async fn batch_add_aborts_on_conflict_and_rolls_back() {
    let (name, db) = fresh_db().await;

    // Одна запись уже есть.
    idb_upsert::put_one(&db, "contacts", &make_record("a", "A"))
        .await
        .unwrap();

    // Пачка: d, e — новые, a — конфликт на 3-й позиции.
    let values = vec![
        make_record("d", "D"),
        make_record("e", "E"),
        make_record("a", "A2"),
    ];
    let out = idb_upsert::batch_add_atomic(&db, "contacts", &values)
        .await
        .expect("batch");

    assert_eq!(out.attempted, 3);
    assert_eq!(out.written, 2, "два запроса успели до конфликта");
    assert_eq!(out.aborted_at, Some(2), "пачка упала на 3-й записи");
    assert_eq!(out.abort_reason, Some(WriteError::Constraint));

    // Ключевое: несмотря на written=2, в store осталась только исходная
    // запись `a`. Транзакция откатила d и e.
    let count = idb_upsert::count_all(&db, "contacts").await.unwrap();
    assert_eq!(count, 1, "транзакция откатила всю пачку");

    cleanup(&name, &db).await;
}

#[wasm_bindgen_test]
async fn batch_add_aborts_on_first_conflict() {
    let (name, db) = fresh_db().await;

    idb_upsert::put_one(&db, "contacts", &make_record("a", "A"))
        .await
        .unwrap();

    let values = vec![
        make_record("a", "A2"), // конфликт сразу
        make_record("b", "B"),
    ];
    let out = idb_upsert::batch_add_atomic(&db, "contacts", &values)
        .await
        .expect("batch");

    assert_eq!(out.written, 0, "первый же запрос упал");
    assert_eq!(out.aborted_at, Some(0));
    assert_eq!(out.abort_reason, Some(WriteError::Constraint));

    cleanup(&name, &db).await;
}

// ---------------- batch_put_idempotent ----------------

#[wasm_bindgen_test]
async fn batch_put_never_aborts_on_existing_keys() {
    let (name, db) = fresh_db().await;

    // Три существующие записи.
    idb_upsert::put_one(&db, "contacts", &make_record("a", "A"))
        .await
        .unwrap();
    idb_upsert::put_one(&db, "contacts", &make_record("b", "B"))
        .await
        .unwrap();
    idb_upsert::put_one(&db, "contacts", &make_record("c", "C"))
        .await
        .unwrap();

    // put поверх существующих + один новый — всё должно пройти.
    let values = vec![
        make_record("a", "A2"),
        make_record("b", "B2"),
        make_record("d", "D"),
    ];
    let out = idb_upsert::batch_put_idempotent(&db, "contacts", &values)
        .await
        .expect("batch put");

    assert_eq!(out.attempted, 3);
    assert_eq!(out.written, 3);
    assert!(out.aborted_at.is_none());

    let count = idb_upsert::count_all(&db, "contacts").await.unwrap();
    assert_eq!(count, 4, "a,b,c,d — четыре записи");

    cleanup(&name, &db).await;
}

#[wasm_bindgen_test]
async fn batch_put_idempotent_on_repeat() {
    let (name, db) = fresh_db().await;

    let values = vec![
        make_record("a", "A"),
        make_record("b", "B"),
    ];

    let out1 = idb_upsert::batch_put_idempotent(&db, "contacts", &values)
        .await
        .unwrap();
    assert_eq!(out1.written, 2);

    // Повторный прогон — тот же результат.
    let out2 = idb_upsert::batch_put_idempotent(&db, "contacts", &values)
        .await
        .unwrap();
    assert_eq!(out2.written, 2);
    assert!(out2.aborted_at.is_none());

    let count = idb_upsert::count_all(&db, "contacts").await.unwrap();
    assert_eq!(count, 2);

    cleanup(&name, &db).await;
}

// ---------------- empty batch ----------------

#[wasm_bindgen_test]
async fn batch_empty_is_ok() {
    let (name, db) = fresh_db().await;

    let out = idb_upsert::batch_add_atomic(&db, "contacts", &[])
        .await
        .expect("empty batch");
    assert_eq!(out.attempted, 0);
    assert_eq!(out.written, 0);
    assert!(out.aborted_at.is_none());

    cleanup(&name, &db).await;
}