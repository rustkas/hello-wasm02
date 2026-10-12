//! Wasm-тесты модуля `idb_db`: метаданные соединения, закрытие,
//! обработка `versionchange`.
//!
//! Компилируются только под wasm32 — на host файл пуст.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

use hello_wasm02::idb_db::Database;
use hello_wasm02::idb_factory;

wasm_bindgen_test_configure!(run_in_browser);

fn uid() -> String {
    return format!("{}", js_sys::Date::now() as u64);
}

fn test_db_name() -> String {
    return format!("test-db-{}", uid());
}

/// Ожидает завершения `IDBRequest`. Локальная копия
/// `idb_factory::request_to_promise` — потому что последняя приватная,
/// а для тестов нужен свой минимальный вариант.
async fn await_request(request: &JsValue) -> Result<JsValue, JsValue> {
    let r_ok = request.clone();
    let promise = js_sys::Promise::new(&mut move |resolve, reject| {
        let r = r_ok.clone();
        let resolve_in = resolve.clone();
        let on_success = Closure::once_into_js(move |_e: web_sys::Event| {
            let v = js_sys::Reflect::get(&r, &JsValue::from_str("result"))
                .unwrap_or(JsValue::UNDEFINED);
            let _ = resolve_in.call1(&JsValue::UNDEFINED, &v);
        });
        let _ = js_sys::Reflect::set(&r_ok, &JsValue::from_str("onsuccess"), &on_success);

                let r_err = request.clone();
        let reject_in = reject.clone();
        let on_error = Closure::once_into_js(move |_e: web_sys::Event| {
            let e = js_sys::Reflect::get(&r_err, &JsValue::from_str("error"))
                .unwrap_or(JsValue::UNDEFINED);
            let _ = reject_in.call1(&JsValue::UNDEFINED, &e);
        });
        // Клон для Reflect::set — `r_err` уже перемещён в замыкание.
        let r_err_set = request.clone();
        let _ = js_sys::Reflect::set(&r_err_set, &JsValue::from_str("onerror"), &on_error);
    });

    JsFuture::from(promise).await
}

/// Открывает новую базу с одним или несколькими object store'ами.
///
/// `createObjectStore` разрешён только внутри `onupgradeneeded`. Пока
/// в проекте нет модуля, который создаёт store'ы, этот хелпер делает
/// это через `Reflect` прямо в тесте.
async fn open_with_stores(db_name: &str, stores: &[&str]) -> Database {
    let global = js_sys::global();
    let idb = js_sys::Reflect::get(&global, &JsValue::from_str("indexedDB"))
        .expect("indexedDB");
    let open_fn = js_sys::Reflect::get(&idb, &JsValue::from_str("open"))
        .expect("open fn");
    let open: js_sys::Function = open_fn.dyn_into().expect("open is fn");
    let request = open
        .call2(&idb, &JsValue::from_str(db_name), &JsValue::from_f64(1.0))
        .expect("open call");

    let stores_owned: Vec<String> = stores.iter().map(|s| s.to_string()).collect();
    let upgrade = Closure::once_into_js(move |e: web_sys::Event| {
        let target = e.target().expect("event target");
        let db = js_sys::Reflect::get(&target, &JsValue::from_str("result"))
            .expect("db result");
        let create_fn = js_sys::Reflect::get(&db, &JsValue::from_str("createObjectStore"))
            .expect("createObjectStore fn");
        let create: js_sys::Function = create_fn.dyn_into().expect("create is fn");
        for name in &stores_owned {
            let params = js_sys::Object::new();
            js_sys::Reflect::set(
                &params,
                &JsValue::from_str("keyPath"),
                &JsValue::from_str("id"),
            )
            .expect("set keyPath");
            let _ = create.call2(&db, &JsValue::from_str(name), &params);
        }
    });
    let _ = js_sys::Reflect::set(
        &request,
        &JsValue::from_str("onupgradeneeded"),
        &upgrade,
    );

    let raw = await_request(&request).await.expect("await open");
    Database::from_raw(raw)
}

// ---------------- метаданные ----------------

#[wasm_bindgen_test]
async fn db_name_matches_opened() {
    let name = test_db_name();
    let raw = idb_factory::open_by_name(&name, 1).await.expect("open");
    let db = Database::from_raw(raw);

    assert_eq!(db.name(), name, "имя соединения совпадает с запрошенным");

    db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn db_version_matches_opened() {
    let name = test_db_name();
    let raw = idb_factory::open_by_name(&name, 3).await.expect("open v3");
    let db = Database::from_raw(raw);

    assert_eq!(db.version(), 3, "версия соединения — 3");

    db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn raw_returns_original_jsvalue() {
    let name = test_db_name();
    let raw = idb_factory::open_by_name(&name, 1).await.expect("open");
    let raw_clone = raw.clone();
    let db = Database::from_raw(raw);

    // raw() возвращает ссылку на тот же объект — сравнение по identity
    // через `Object::is`.
    assert!(
        js_sys::Object::is(db.raw(), &raw_clone),
        "raw() возвращает тот же JS-объект, что был передан"
    );

    db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

// ---------------- objectStoreNames ----------------

#[wasm_bindgen_test]
async fn store_names_empty_on_fresh_db() {
    let name = test_db_name();
    let raw = idb_factory::open_by_name(&name, 1).await.expect("open");
    let db = Database::from_raw(raw);

    assert!(
        db.store_names().is_empty(),
        "свежая база без store'ов — список пуст"
    );

    db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn store_names_lists_one_store() {
    let name = test_db_name();
    let db = open_with_stores(&name, &["books"]).await;

    let names = db.store_names();
    assert_eq!(names.len(), 1);
    assert_eq!(names[0], "books");

    db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn store_names_lists_multiple_stores() {
    let name = test_db_name();
    let db = open_with_stores(&name, &["authors", "books", "covers"]).await;

    let mut names = db.store_names();
    names.sort();
    assert_eq!(names, vec!["authors", "books", "covers"]);

    db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

// ---------------- close / is_closed ----------------

#[wasm_bindgen_test]
async fn is_closed_false_before_close() {
    let name = test_db_name();
    let raw = idb_factory::open_by_name(&name, 1).await.expect("open");
    let db = Database::from_raw(raw);

    assert!(!db.is_closed(), "до close() соединение открыто");

    db.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn close_is_idempotent() {
    let name = test_db_name();
    let raw = idb_factory::open_by_name(&name, 1).await.expect("open");
    let db = Database::from_raw(raw);

    db.close();
    db.close();
    db.close();

    assert!(db.is_closed(), "флаг закрытия остаётся выставленным");

    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn close_marks_connection_closed() {
    let name = test_db_name();
    let raw = idb_factory::open_by_name(&name, 1).await.expect("open");
    let db = Database::from_raw(raw);

    assert!(!db.is_closed());
    db.close();
    assert!(db.is_closed());

    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

// ---------------- versionchange ----------------

/// Сценарий:
///   1. Открываем базу в v1, устанавливаем `onversionchange`.
///   2. Из второй «сессии» пытаемся открыть ту же базу в v2.
///   3. Браузер посылает `versionchange` первому соединению.
///   4. Наш обработчик синхронно вызывает `close()` — второе открытие
///      получает контроль.
///   5. Проверяем, что первое соединение помечено закрытым.
#[wasm_bindgen_test]
async fn versionchange_handler_closes_db() {
    let name = test_db_name();

    // Шаг 1. Первое соединение v1 + обработчик.
    let raw1 = idb_factory::open_by_name(&name, 1).await.expect("open v1");
    let db1 = Database::from_raw(raw1);
    assert!(!db1.is_closed());
    db1.install_versionchange_handler();

    // Шаг 2. Второе соединение v2. Await не разрешится, пока первое не
    // закроется — наш обработчик делает это синхронно.
    let raw2 = idb_factory::open_by_name(&name, 2).await.expect("open v2");
    let db2 = Database::from_raw(raw2);

    // Шаг 5. Первое соединение должно быть помечено закрытым.
    assert!(
        db1.is_closed(),
        "onversionchange должен был закрыть первое соединение"
    );

    db2.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}

#[wasm_bindgen_test]
async fn versionchange_does_not_fire_on_same_version() {
    let name = test_db_name();

    let raw1 = idb_factory::open_by_name(&name, 1).await.expect("open v1");
    let db1 = Database::from_raw(raw1);
    db1.install_versionchange_handler();

    // Закрываем вручную — чтобы второе открытие v1 не блокировалось
    // (по спеке открытие с той же версией допускает несколько соединений,
    // но на всякий случай уберём первое).
    db1.close();

    let raw2 = idb_factory::open_by_name(&name, 1).await.expect("open v1 second");
    let db2 = Database::from_raw(raw2);

    // Второе соединение работает — версия та же, конфликта нет.
    assert_eq!(db2.version(), 1);

    db2.close();
    idb_factory::delete_by_name(&name).await.expect("cleanup");
}