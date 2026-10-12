//! Обёртка над `IDBFactory` — точкой входа в IndexedDB.
//!
//! Работаем с `window.indexedDB` напрямую через `js_sys::Reflect`, не
//! используя обёртку `idb` crate: не все методы `IDBFactory` обёрнуты
//! в `idb 0.6.5`, и единый стиль через Reflect даёт простой код,
//! устойчивый к версии `web-sys`.
//!
//! Четыре инструмента фабрики:
//!   • `open_demo()` — открыть demo-базу (создаёт при первом открытии);
//!   • `delete_by_name()` — удалить базу целиком. Идемпотентно;
//!   • `list_databases()` — перечислить все базы origin;
//!   • `compare_keys()` — сравнить два ключа по правилам IndexedDB.
//!
//! ПРАВИЛО: `IDBRequest` — не `Promise`. У него события `success`/`error`.
//! Оборачиваем в `Promise` вручную через `request_to_promise`.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// Имя demo-базы этой подглавы.
pub const DEMO_DB: &str = "hello-wasm-idb-factory-demo";
/// Версия схемы demo-базы.
pub const DEMO_VERSION: u32 = 1;

/// Одна запись в списке баз: имя и текущая версия.
#[derive(Debug, Clone, PartialEq)]
pub struct DbInfo {
    pub name: String,
    pub version: u32,
}

/// Возвращает `window.indexedDB`.
fn indexed_db() -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let idb = js_sys::Reflect::get(&global, &JsValue::from_str("indexedDB"))
        .map_err(|e| JsValue::from_str(&format!("Reflect indexedDB: {e:?}")))?;
    if idb.is_undefined() || idb.is_null() {
        return Err(JsValue::from_str("indexedDB недоступен (нужен secure context)"));
    }
    Ok(idb)
}

/// Оборачивает `IDBRequest` в `Promise`.
///
/// `IDBRequest` не является `Promise`: у него события `success`/`error`.
/// Слушаем и `blocked`: он срабатывает, если другой таб держит соединение
/// и мешает удалить/обновить базу. Без него промис висит навсегда —
/// на success/error в этом случае никто не сработает.
fn request_to_promise(request: &JsValue) -> Result<js_sys::Promise, JsValue> {
    let req_ok = request.clone();
    let req_err = request.clone();
    let req_blocked = request.clone();

    let promise = js_sys::Promise::new(&mut move |resolve, reject| {
        // onsuccess: resolve(request.result)
        let r = req_ok.clone();
        let resolve_in = resolve.clone();
        let on_success = Closure::once_into_js(move |_e: web_sys::Event| {
            let result = js_sys::Reflect::get(&r, &JsValue::from_str("result"))
                .unwrap_or(JsValue::UNDEFINED);
            let _ = resolve_in.call1(&JsValue::UNDEFINED, &result);
        });
        let _ = js_sys::Reflect::set(
            &req_ok,
            &JsValue::from_str("onsuccess"),
            &on_success,
        );

        // onerror: reject(request.error)
        let r = req_err.clone();
        let reject_in = reject.clone();
        let on_error = Closure::once_into_js(move |_e: web_sys::Event| {
            let error = js_sys::Reflect::get(&r, &JsValue::from_str("error"))
                .unwrap_or(JsValue::UNDEFINED);
            let _ = reject_in.call1(&JsValue::UNDEFINED, &error);
        });
        let _ = js_sys::Reflect::set(
            &req_err,
            &JsValue::from_str("onerror"),
            &on_error,
        );

        // onblocked: reject сразу. Иначе — бесконечное ожидание, если
        // другой таб держит открытое соединение с этой базой.
        let reject_blocked = reject.clone();
        let on_blocked = Closure::once_into_js(move |_e: web_sys::Event| {
            let msg = JsValue::from_str("blocked: соединение держится другим табом");
            let _ = reject_blocked.call1(&JsValue::UNDEFINED, &msg);
        });
        let _ = js_sys::Reflect::set(
            &req_blocked,
            &JsValue::from_str("onblocked"),
            &on_blocked,
        );
    });

    Ok(promise)
}

/// Закрывает `IDBDatabase`. Без этого `deleteDatabase` заблокируется.
///
/// `IDBDatabase.close()` — единственный способ освободить соединение.
/// `web-sys` не имеет типизированной обёртки для `IDBDatabase`, поэтому
/// вызываем через Reflect.
pub fn close_db(db: &JsValue) {
    if let Ok(close_fn) = js_sys::Reflect::get(db, &JsValue::from_str("close")) {
        if let Ok(f) = close_fn.dyn_into::<js_sys::Function>() {
            let _ = f.call0(db);
        }
    }
}

/// Открывает demo-базу. При первом открытии — создаёт пустую.
/// Возвращает JS-объект `IDBDatabase`.
pub async fn open_demo() -> Result<JsValue, JsValue> {
    open_by_name(DEMO_DB, DEMO_VERSION).await
}

/// Открывает базу с указанным именем и версией.
///
/// `onupgradeneeded` не устанавливаем — база создаётся пустой, без
/// object stores. Схему (stores, indexes) добавляем в следующих подглавах.
pub async fn open_by_name(name: &str, version: u32) -> Result<JsValue, JsValue> {
    let factory = indexed_db()?;
    let open_fn = js_sys::Reflect::get(&factory, &JsValue::from_str("open"))
        .map_err(|e| JsValue::from_str(&format!("Reflect open: {e:?}")))?;
    let open: js_sys::Function = open_fn
        .dyn_into()
        .map_err(|_| JsValue::from_str("open не функция"))?;

    let request = open
        .call2(
            &factory,
            &JsValue::from_str(name),
            &JsValue::from_f64(version as f64),
        )
        .map_err(|e| JsValue::from_str(&format!("open: {e:?}")))?;

    let promise = request_to_promise(&request)?;
    JsFuture::from(promise).await
}

/// Удаляет demo-базу. Идемпотентно.
pub async fn delete_demo() -> Result<(), JsValue> {
    delete_by_name(DEMO_DB).await
}

/// Удаляет базу с указанным именем.
///
/// Идемпотентно: повторный вызов — `Ok`. Если базы нет, `deleteDatabase`
/// резолвится с `success`, `request.error` не выставляется.
pub async fn delete_by_name(name: &str) -> Result<(), JsValue> {
    let factory = indexed_db()?;
    let del_fn = js_sys::Reflect::get(&factory, &JsValue::from_str("deleteDatabase"))
        .map_err(|e| JsValue::from_str(&format!("Reflect deleteDatabase: {e:?}")))?;
    let del: js_sys::Function = del_fn
        .dyn_into()
        .map_err(|_| JsValue::from_str("deleteDatabase не функция"))?;

    let request = del
        .call1(&factory, &JsValue::from_str(name))
        .map_err(|e| JsValue::from_str(&format!("deleteDatabase: {e:?}")))?;

    let promise = request_to_promise(&request)?;
    JsFuture::from(promise).await?;
    Ok(())
}

/// Возвращает список баз в текущем origin.
///
/// `databases()` возвращает **настоящий** `Promise` (не `IDBRequest`).
/// Каждый элемент — `{ name: string, version: number }`.
pub async fn list_databases() -> Result<Vec<DbInfo>, JsValue> {
    let factory = indexed_db()?;
    let fn_val = js_sys::Reflect::get(&factory, &JsValue::from_str("databases"))
        .map_err(|e| JsValue::from_str(&format!("Reflect databases: {e:?}")))?;
    if fn_val.is_undefined() {
        return Err(JsValue::from_str(
            "IDBFactory.databases() не поддерживается этим браузером",
        ));
    }
    let func: js_sys::Function = fn_val
        .dyn_into()
        .map_err(|_| JsValue::from_str("databases не функция"))?;

    let promise_val = func
        .call0(&factory)
        .map_err(|e| JsValue::from_str(&format!("databases: {e:?}")))?;
    let promise: js_sys::Promise = promise_val
        .dyn_into()
        .map_err(|_| JsValue::from_str("databases не вернул Promise"))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| JsValue::from_str(&format!("await databases: {e:?}")))?;
    let array: js_sys::Array = result
        .dyn_into()
        .map_err(|_| JsValue::from_str("databases вернул не Array"))?;

    let mut out = Vec::with_capacity(array.length() as usize);
    for i in 0..array.length() {
        let info = array.get(i);
        let name = js_sys::Reflect::get(&info, &JsValue::from_str("name"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let version = js_sys::Reflect::get(&info, &JsValue::from_str("version"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u32;
        out.push(DbInfo { name, version });
    }
    Ok(out)
}

/// Сравнивает два ключа по правилам IndexedDB.
///
/// Возвращает -1, 0 или 1. Правило сортировки:
///   number < date < string < binary < array.
/// Внутри одного типа — по естественному порядку.
pub fn compare_keys(a: &JsValue, b: &JsValue) -> Result<i32, JsValue> {
    let factory = indexed_db()?;
    let cmp_fn = js_sys::Reflect::get(&factory, &JsValue::from_str("cmp"))
        .map_err(|e| JsValue::from_str(&format!("Reflect cmp: {e:?}")))?;
    let cmp: js_sys::Function = cmp_fn
        .dyn_into()
        .map_err(|_| JsValue::from_str("cmp не функция"))?;

    let result = cmp
        .call2(&factory, a, b)
        .map_err(|e| JsValue::from_str(&format!("cmp: {e:?}")))?;
    Ok(result.as_f64().unwrap_or(0.0) as i32)
}