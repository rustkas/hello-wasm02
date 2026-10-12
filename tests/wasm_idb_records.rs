//! Wasm-тесты модуля `idb_records`: CRUD, autoIncrement, cursor,
//! диапазоны ключей, направления.
//!
//! Компилируются только под wasm32 — на host файл пуст.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::*;

use hello_wasm02::idb_db::Database;
use hello_wasm02::idb_factory;
use hello_wasm02::idb_records::{self, Direction, KeyRangeSpec, RecordStore};
use hello_wasm02::idb_schema::{self, SchemaPlan, StoreSpec};

wasm_bindgen_test_configure!(run_in_browser);

fn uid() -> String {
    return format!("{}", js_sys::Date::now() as u64);
}

fn test_db() -> String {
    return format!("test-records-{}", uid());
}

fn err_name(e: &JsValue) -> String {
    if let Some(dx) = e.dyn_ref::<web_sys::DomException>() {
        return dx.name();
    }
    return e.as_string().unwrap_or_else(|| format!("{e:?}"));
}

async fn setup(name: &str) -> Database {
    let plan = SchemaPlan::new()
        .add(StoreSpec::with_key("books", "isbn"))
        .add(StoreSpec::with_auto_key("logs"))
        .add(StoreSpec::with_auto_key("metrics"));
    let outcome = idb_schema::open_with_plan(name, 1, &plan)
        .await
        .expect("open_with_plan");
    return outcome.db;
}

fn make_book(isbn: &str, title: &str, year: u32) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&obj, &"isbn".into(), &JsValue::from_str(isbn));
    let _ = js_sys::Reflect::set(&obj, &"title".into(), &JsValue::from_str(title));
    let _ = js_sys::Reflect::set(&obj, &"year".into(), &JsValue::from_f64(year as f64));
    return obj.into();
}

fn make_metric(id: u32, value: f64) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&obj, &"metric_id".into(), &JsValue::from_f64(id as f64));
    let _ = js_sys::Reflect::set(&obj, &"value".into(), &JsValue::from_f64(value));
    return obj.into();
}

async fn cleanup(name: &str) {
    idb_factory::delete_by_name(name).await.expect("cleanup");
}

// ---------------- add / put ----------------

#[wasm_bindgen_test]
async fn add_inserts_and_returns_key() {
    let name = test_db();
    let db = setup(&name).await;

    let s = RecordStore::open(db.raw(), "books", "readwrite").expect("open");
    let key = idb_records::add(&s, &make_book("978-1", "Book", 2020))
        .await
        .expect("add");
    assert_eq!(key.as_string().as_deref(), Some("978-1"));

    db.close();
    cleanup(&name).await;
}

#[wasm_bindgen_test]
async fn add_duplicate_throws_constraint() {
    let name = test_db();
    let db = setup(&name).await;

    let s1 = RecordStore::open(db.raw(), "books", "readwrite").expect("open 1");
    idb_records::add(&s1, &make_book("978-1", "A", 2000)).await.unwrap();

    let s2 = RecordStore::open(db.raw(), "books", "readwrite").expect("open 2");
    let r = idb_records::add(&s2, &make_book("978-1", "B", 2001)).await;
    assert!(r.is_err(), "дубликат должен упасть");
    assert_eq!(err_name(&r.unwrap_err()), "ConstraintError");

    db.close();
    cleanup(&name).await;
}

#[wasm_bindgen_test]
async fn put_overwrites_existing() {
    let name = test_db();
    let db = setup(&name).await;

    let s1 = RecordStore::open(db.raw(), "books", "readwrite").expect("open 1");
    idb_records::put(&s1, &make_book("978-1", "Old", 2000)).await.unwrap();

    let s2 = RecordStore::open(db.raw(), "books", "readwrite").expect("open 2");
    idb_records::put(&s2, &make_book("978-1", "New", 2000)).await.unwrap();

    let s3 = RecordStore::open(db.raw(), "books", "readonly").expect("open 3");
    let rec = idb_records::get(&s3, &JsValue::from_str("978-1"))
        .await
        .expect("get")
        .expect("present");
    let title = js_sys::Reflect::get(&rec, &"title".into())
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    assert_eq!(title, "New");

    db.close();
    cleanup(&name).await;
}

#[wasm_bindgen_test]
async fn auto_increment_generates_numeric_key() {
    let name = test_db();
    let db = setup(&name).await;

    let s = RecordStore::open(db.raw(), "logs", "readwrite").expect("open");
    let rec = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&rec, &"msg".into(), &JsValue::from_str("hello"));

    let key = idb_records::put(&s, &rec.into()).await.expect("put");
    assert!(key.as_f64().is_some(), "ожидали number key");
    assert!(key.as_f64().unwrap() >= 1.0);

    db.close();
    cleanup(&name).await;
}

#[wasm_bindgen_test]
async fn count_and_get_all() {
    let name = test_db();
    let db = setup(&name).await;

    let s1 = RecordStore::open(db.raw(), "books", "readwrite").expect("open");
    for i in 0..5 {
        idb_records::add(&s1, &make_book(&format!("978-{i}"), "T", 2000 + i))
            .await
            .unwrap();
    }

    let s2 = RecordStore::open(db.raw(), "books", "readonly").expect("open 2");
    let n = idb_records::count(&s2).await.expect("count");
    assert_eq!(n, 5);

    let s3 = RecordStore::open(db.raw(), "books", "readonly").expect("open 3");
    let all = idb_records::get_all(&s3).await.expect("get_all");
    assert_eq!(all.len(), 5);

    db.close();
    cleanup(&name).await;
}

#[wasm_bindgen_test]
async fn delete_idempotent() {
    let name = test_db();
    let db = setup(&name).await;

    let s1 = RecordStore::open(db.raw(), "books", "readwrite").expect("open");
    idb_records::add(&s1, &make_book("978-1", "T", 2020)).await.unwrap();

    let s2 = RecordStore::open(db.raw(), "books", "readwrite").expect("open 2");
    idb_records::delete(&s2, &JsValue::from_str("978-1")).await.unwrap();

    let s3 = RecordStore::open(db.raw(), "books", "readwrite").expect("open 3");
    idb_records::delete(&s3, &JsValue::from_str("978-1")).await.unwrap();

    let s4 = RecordStore::open(db.raw(), "books", "readonly").expect("open 4");
    let n = idb_records::count(&s4).await.expect("count");
    assert_eq!(n, 0);

    db.close();
    cleanup(&name).await;
}

// ---------------- cursor ----------------

#[wasm_bindgen_test]
async fn cursor_next_matches_get_all() {
    let name = test_db();
    let db = setup(&name).await;

    let s1 = RecordStore::open(db.raw(), "metrics", "readwrite").expect("open");
    let records: Vec<JsValue> = (1..=100).map(|i| make_metric(i, i as f64 * 1.5)).collect();
    idb_records::seed_bulk(&s1, &records).await.expect("seed");

    let s2 = RecordStore::open(db.raw(), "metrics", "readonly").expect("open 2");
    let all = idb_records::get_all(&s2).await.expect("get_all");

    let s3 = RecordStore::open(db.raw(), "metrics", "readonly").expect("open 3");
    let cur = idb_records::cursor_visit(&s3, None, Direction::Next, None, |_| {})
        .await
        .expect("cursor");

    assert_eq!(all.len(), 100);
    assert_eq!(cur.scanned, 100);
    assert!(!cur.truncated);

    // Порядок совпадает.
    for i in 0..cur.keys.len() {
        let k = cur.keys[i].as_f64().unwrap_or(0.0) as u32;
        assert_eq!(k, (i as u32) + 1);
    }

    db.close();
    cleanup(&name).await;
}

#[wasm_bindgen_test]
async fn cursor_prev_reverses_order() {
    let name = test_db();
    let db = setup(&name).await;

    let s1 = RecordStore::open(db.raw(), "metrics", "readwrite").expect("open");
    let records: Vec<JsValue> = (1..=10).map(|i| make_metric(i, i as f64)).collect();
    idb_records::seed_bulk(&s1, &records).await.expect("seed");

    let s2 = RecordStore::open(db.raw(), "metrics", "readonly").expect("open 2");
    let cur = idb_records::cursor_visit(&s2, None, Direction::Prev, None, |_| {})
        .await
        .expect("cursor");

    assert_eq!(cur.scanned, 10);
    let first = cur.keys[0].as_f64().unwrap_or(0.0) as u32;
    let last = cur.keys[9].as_f64().unwrap_or(0.0) as u32;
    assert_eq!(first, 10, "Prev начинается с последнего");
    assert_eq!(last, 1, "Prev заканчивается первым");

    db.close();
    cleanup(&name).await;
}

#[wasm_bindgen_test]
async fn cursor_lower_bound() {
    let name = test_db();
    let db = setup(&name).await;

    let s1 = RecordStore::open(db.raw(), "metrics", "readwrite").expect("open");
    let records: Vec<JsValue> = (1..=100).map(|i| make_metric(i, i as f64)).collect();
    idb_records::seed_bulk(&s1, &records).await.expect("seed");

    let s2 = RecordStore::open(db.raw(), "metrics", "readonly").expect("open 2");
    let range = KeyRangeSpec::LowerBound {
        value: JsValue::from_f64(95.0),
        open: false,
    };
    let cur = idb_records::cursor_visit(&s2, Some(range), Direction::Next, None, |_| {})
        .await
        .expect("cursor");

    assert_eq!(cur.scanned, 6, "записи 95..100 — всего 6");

    db.close();
    cleanup(&name).await;
}

#[wasm_bindgen_test]
async fn cursor_bound_range() {
    let name = test_db();
    let db = setup(&name).await;

    let s1 = RecordStore::open(db.raw(), "metrics", "readwrite").expect("open");
    let records: Vec<JsValue> = (1..=100).map(|i| make_metric(i, i as f64)).collect();
    idb_records::seed_bulk(&s1, &records).await.expect("seed");

    let s2 = RecordStore::open(db.raw(), "metrics", "readonly").expect("open 2");
    let range = KeyRangeSpec::Bound {
        lower: JsValue::from_f64(10.0),
        upper: JsValue::from_f64(19.0),
        lower_open: false,
        upper_open: false,
    };
    let cur = idb_records::cursor_visit(&s2, Some(range), Direction::Next, None, |_| {})
        .await
        .expect("cursor");

    assert_eq!(cur.scanned, 10, "10..19 — всего 10");

    db.close();
    cleanup(&name).await;
}

#[wasm_bindgen_test]
async fn cursor_limit_truncates() {
    let name = test_db();
    let db = setup(&name).await;

    let s1 = RecordStore::open(db.raw(), "metrics", "readwrite").expect("open");
    let records: Vec<JsValue> = (1..=1000).map(|i| make_metric(i, i as f64)).collect();
    idb_records::seed_bulk(&s1, &records).await.expect("seed");

    let s2 = RecordStore::open(db.raw(), "metrics", "readonly").expect("open 2");
    let cur = idb_records::cursor_visit(&s2, None, Direction::Next, Some(50), |_| {})
        .await
        .expect("cursor");

    assert_eq!(cur.scanned, 50, "limit 50");
    assert!(cur.truncated, "итерация прервана по limit");

    db.close();
    cleanup(&name).await;
}