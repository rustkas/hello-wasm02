//! Wasm-тесты модуля `idb_schema`: планировщик схемы, createObjectStore,
//! deleteObjectStore, version-монотонность.
//!
//! Компилируются только под wasm32 — на host файл пуст.
//!
//! ПРАВИЛО: `format!` в tail-позиции функции оборачивается в явный `return`.
//! rust-analyzer стабильно теряет тип `String` в этой позиции и рапортует
//! `expected String, found ()` — ошибка потом «проливается» на вызывающих.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::*;

use hello_wasm02::idb_factory;
use hello_wasm02::idb_schema::{self, SchemaPlan, StoreSpec};

wasm_bindgen_test_configure!(run_in_browser);

fn uid() -> String {
    return format!("{}", js_sys::Date::now() as u64);
}

fn test_db() -> String {
    return format!("test-schema-{}", uid());
}

fn err_name(e: &JsValue) -> String {
    if let Some(dx) = e.dyn_ref::<web_sys::DomException>() {
        return dx.name();
    }
    return e.as_string().unwrap_or_else(|| format!("{e:?}"));
}

// ---------------- создание схемы ----------------

#[wasm_bindgen_test]
async fn open_creates_planned_stores() {
    let name = test_db();
    let plan = SchemaPlan::new()
        .add(StoreSpec::with_key("authors", "id"))
        .add(StoreSpec::with_key("books", "isbn"));

    let outcome = idb_schema::open_with_plan(&name, 1, &plan)
        .await
        .expect("open v1");

    assert!(outcome.diff.upgraded, "должен был быть upgrade");
    assert_eq!(outcome.diff.old_version, 0);
    assert_eq!(outcome.diff.new_version, 1);
    assert_eq!(outcome.diff.created.len(), 2);
    assert!(outcome.diff.created.contains(&"authors".to_string()));
    assert!(outcome.diff.created.contains(&"books".to_string()));
    assert!(outcome.diff.deleted.is_empty());
    assert!(outcome.diff.errors.is_empty());

    let mut names = outcome.db.store_names();
    names.sort();
    assert_eq!(names, vec!["authors", "books"]);
    assert_eq!(outcome.db.version(), 1);

    outcome.db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn auto_increment_store_creation() {
    let name = test_db();
    let plan = SchemaPlan::new().add(StoreSpec::with_auto_key("logs"));

    let outcome = idb_schema::open_with_plan(&name, 1, &plan)
        .await
        .expect("open");
    assert_eq!(outcome.diff.created, vec!["logs"]);
    assert_eq!(outcome.db.store_names(), vec!["logs"]);

    outcome.db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

// ---------------- идемпотентность плана ----------------

#[wasm_bindgen_test]
async fn open_is_idempotent_with_same_plan() {
    let name = test_db();
    let plan = SchemaPlan::new().add(StoreSpec::with_key("authors", "id"));

    let o1 = idb_schema::open_with_plan(&name, 1, &plan)
        .await
        .expect("open v1");
    assert_eq!(o1.diff.created, vec!["authors"]);
    o1.db.close();

    // Тот же план, но версия выше — upgrade запускается, но менять нечего.
    let o2 = idb_schema::open_with_plan(&name, 2, &plan)
        .await
        .expect("open v2");
    assert!(o2.diff.upgraded);
    assert!(o2.diff.created.is_empty(), "нечего создавать");
    assert!(o2.diff.deleted.is_empty(), "нечего удалять");
    assert_eq!(o2.diff.kept, vec!["authors"]);
    assert_eq!(o2.db.version(), 2);

    o2.db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

// ---------------- удаление store через план ----------------

#[wasm_bindgen_test]
async fn plan_delete_removes_store() {
    let name = test_db();

    // v1: authors + books.
    let plan_v1 = SchemaPlan::new()
        .add(StoreSpec::with_key("authors", "id"))
        .add(StoreSpec::with_key("books", "isbn"));
    let o1 = idb_schema::open_with_plan(&name, 1, &plan_v1)
        .await
        .expect("v1");
    o1.db.close();

    // v2: только authors — books удаляется планом.
    let plan_v2 = SchemaPlan::new().add(StoreSpec::with_key("authors", "id"));
    let o2 = idb_schema::open_with_plan(&name, 2, &plan_v2)
        .await
        .expect("v2");

    assert_eq!(o2.diff.deleted, vec!["books"]);
    assert_eq!(o2.diff.kept, vec!["authors"]);
    assert_eq!(o2.db.store_names(), vec!["authors"]);

    o2.db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

// ---------------- замок схемы: только в upgrade ----------------

#[wasm_bindgen_test]
async fn create_outside_upgrade_fails() {
    let name = test_db();
    let plan = SchemaPlan::new().add(StoreSpec::with_key("authors", "id"));
    let o = idb_schema::open_with_plan(&name, 1, &plan)
        .await
        .expect("open");

    let spec = StoreSpec::with_key("late_store", "id");
    let result = idb_schema::try_create_on_open(&o.db, &spec);

    assert!(
        result.is_err(),
        "createObjectStore вне upgrade должен упасть"
    );
    let name_str = err_name(&result.unwrap_err());
    assert_eq!(
        name_str, "InvalidStateError",
        "ожидали InvalidStateError, получили {name_str}"
    );

    o.db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn delete_outside_upgrade_fails() {
    let name = test_db();
    let plan = SchemaPlan::new().add(StoreSpec::with_key("authors", "id"));
    let o = idb_schema::open_with_plan(&name, 1, &plan)
        .await
        .expect("open");

    let result = idb_schema::try_delete_on_open(&o.db, "authors");
    assert!(
        result.is_err(),
        "deleteObjectStore вне upgrade должен упасть"
    );
    let name_str = err_name(&result.unwrap_err());
    assert_eq!(name_str, "InvalidStateError");

    o.db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

// ---------------- монотонность версий ----------------

#[wasm_bindgen_test]
async fn version_cannot_go_back() {
    let name = test_db();
    let plan = SchemaPlan::new().add(StoreSpec::with_key("authors", "id"));

    let o = idb_schema::open_with_plan(&name, 2, &plan)
        .await
        .expect("v2");
    o.db.close();

    // Попытка открыть базу в версии ниже существующей.
    let result = idb_schema::open_with_plan(&name, 1, &plan).await;
    assert!(result.is_err(), "понижение версии запрещено");

    let err = result.err().unwrap();
    assert_eq!(err_name(&err), "VersionError");

    idb_factory::delete_by_name(&name).await.expect("cleanup");
}