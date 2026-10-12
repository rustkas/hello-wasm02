//! Семантика вставки: `add` vs `put`, `ConstraintError`, диагностика upsert.
//!
//! `IDBObjectStore` даёт два метода записи одной записи с разным контрактом:
//!
//!   • `add(value)` — insert. Если ключ занят — синхронно идёт `error`-событие
//!     с `ConstraintError`, транзакция уходит в abort.
//!   • `put(value)` — upsert. Тот же ключ молча перезаписывается.
//!
//! Разница не косметическая: она определяет, что видит пользователь —
//! «такой id уже занят» или «сохранено». И определяет поведение пачки:
//! один конфликтный `add` откатывает **всю** транзакцию, включая уже
//! успешные записи. `put` в этом смысле идемпотентен.
//!
//! Модуль даёт три уровня API:
//!
//!   1. `add_one` / `put_one` — одна операция, своя транзакция.
//!   2. `save_with_diagnosis` — «умное сохранение»: сначала `add`,
//!      при `ConstraintError` читаем предыдущее значение и делаем `put`.
//!      Возвращает `SaveReport { outcome, previous }` — UI показывает
//!      «создано» или «обновлено (было → стало)».
//!   3. `batch_add_atomic` / `batch_put_idempotent` — пачки в одной
//!      транзакции. Наглядно показывают разницу семантик.
//!
//! ПРАВИЛО: ошибки пробрасываем как `JsValue` без обёртки в строку.
//! `classify_write_error` извлекает `name` из `DOMException`.
//!
//! ПРАВИЛО: после любого запроса ждём `tx.oncomplete`/`tx.onabort`, чтобы
//! соединение освободилось до следующего вызова. Иначе следующий `open`
//! на тот же store даст `TransactionInactiveError`.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

// ============================================================
//                    ПУБЛИЧНЫЕ ТИПЫ
// ============================================================

/// Исход «умного сохранения».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Created,
    Updated,
}

/// Отчёт о сохранении с диагностикой.
#[derive(Debug)]
pub struct SaveReport {
    pub outcome: SaveOutcome,
    /// Предыдущее значение — `Some` только при `SaveOutcome::Updated`.
    pub previous: Option<JsValue>,
}

/// Классифицированный `DOMException` из операции записи.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteError {
    Constraint,
    Data,
    Quota,
    ReadOnly,
    InvalidState,
    TransactionInactive,
    Abort,
    Other(String),
}

impl WriteError {
    /// Короткая интерпретация для UI.
    pub fn short(&self) -> &'static str {
        match self {
            WriteError::Constraint => "ключ уже занят",
            WriteError::Data => "ключ не соответствует keyPath",
            WriteError::Quota => "квота исчерпана",
            WriteError::ReadOnly => "запись в readonly-транзакции",
            WriteError::InvalidState => "некорректное состояние",
            WriteError::TransactionInactive => "транзакция неактивна",
            WriteError::Abort => "транзакция отменена",
            WriteError::Other(_) => "ошибка записи",
        }
    }
    /// Строковое имя, как в `DOMException.name`.
    pub fn name(&self) -> &str {
        match self {
            WriteError::Constraint => "ConstraintError",
            WriteError::Data => "DataError",
            WriteError::Quota => "QuotaExceededError",
            WriteError::ReadOnly => "ReadOnlyError",
            WriteError::InvalidState => "InvalidStateError",
            WriteError::TransactionInactive => "TransactionInactiveError",
            WriteError::Abort => "AbortError",
            WriteError::Other(s) => s.as_str(),
        }
    }
}

/// Результат пачечной операции.
#[derive(Debug)]
pub struct BatchOutcome {
    pub attempted: u32,
    /// Сколько запросов успели завершиться до отката. Если транзакция
    /// прошла целиком — `written == attempted`.
    pub written: u32,
    /// Индекс первой упавшей записи, `None` — пачка прошла целиком.
    pub aborted_at: Option<u32>,
    /// Причина отката. `None` — пачка прошла целиком.
    pub abort_reason: Option<WriteError>,
}

// ============================================================
//                    КЛАССИФИКАЦИЯ
// ============================================================

pub fn classify_write_error(e: &JsValue) -> WriteError {
    let name = error_name(e);
    match name.as_str() {
        "ConstraintError" => WriteError::Constraint,
        "DataError" => WriteError::Data,
        "QuotaExceededError" => WriteError::Quota,
        "ReadOnlyError" => WriteError::ReadOnly,
        "InvalidStateError" => WriteError::InvalidState,
        "TransactionInactiveError" => WriteError::TransactionInactive,
        "AbortError" => WriteError::Abort,
        other => WriteError::Other(other.to_string()),
    }
}

pub fn is_constraint_error(e: &JsValue) -> bool {
   return matches!(classify_write_error(e), WriteError::Constraint)
}

/// Достаёт `name` из `JsValue`. Порядок: `DomException.name` → строка →
/// `Reflect::get("name")`. Никогда не паникует.
fn error_name(e: &JsValue) -> String {
    if let Some(dx) = e.dyn_ref::<web_sys::DomException>() {
        return dx.name();
    }
    if let Some(s) = e.as_string() {
        return s;
    }
    js_sys::Reflect::get(e, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| "unknown".to_string())
}

// ============================================================
//                    ОДНА ОПЕРАЦИЯ
// ============================================================

/// `put` одной записи. Идемпотентен: тот же ключ перезаписывается.
pub async fn put_one(db: &JsValue, store_name: &str, value: &JsValue) -> Result<(), JsValue> {
    single_write(db, store_name, value, "put").await.map(|_| ())
}

/// `add` одной записи. `ConstraintError`, если ключ занят.
pub async fn add_one(db: &JsValue, store_name: &str, value: &JsValue) -> Result<(), JsValue> {
    single_write(db, store_name, value, "add").await.map(|_| ())
}

async fn single_write(
    db: &JsValue,
    store_name: &str,
    value: &JsValue,
    method_name: &str,
) -> Result<JsValue, JsValue> {
    let (os, tx) = open_tx(db, store_name, "readwrite")?;
    let tx_done = tx_done_promise(&tx);
    let f = method(&os, method_name)?;
    let req = f
        .call1(&os, value)
        .map_err(|e| JsValue::from_str(&format!("{method_name}: {e:?}")))?;
    let res = await_request(&req).await;
    // Ждём завершения транзакции в любом случае — иначе следующий вызов
    // получит TransactionInactiveError, а deleteDatabase — onblocked.
    let _ = JsFuture::from(tx_done).await;
    res
}

/// `get` одной записи по ключу. `None` — ключа нет.
pub async fn get_one(
    db: &JsValue,
    store_name: &str,
    key: &JsValue,
) -> Result<Option<JsValue>, JsValue> {
    let (os, tx) = open_tx(db, store_name, "readonly")?;
    let tx_done = tx_done_promise(&tx);
    let f = method(&os, "get")?;
    let req = f
        .call1(&os, key)
        .map_err(|e| JsValue::from_str(&format!("get: {e:?}")))?;
    let res = await_request(&req).await;
    let _ = JsFuture::from(tx_done).await;
    match res? {
        v if v.is_null() || v.is_undefined() => Ok(None),
        v => Ok(Some(v)),
    }
}

// ============================================================
//                    УМНОЕ СОХРАНЕНИЕ
// ============================================================

/// «Создать или обновить» с диагностикой.
///
/// Пробуем `add` в свежей транзакции. Создалось — `Created`. Упало с
/// `ConstraintError` — читаем предыдущее значение (`previous`) и делаем
/// `put` в новой транзакции — `Updated`. Любая другая ошибка — наверх.
///
/// Диагностика race-free: между попыткой `add` и выводом «запись
/// существовала» не остаётся окна, потому что `ConstraintError` —
/// это вердикт самого движка.
pub async fn save_with_diagnosis(
    db: &JsValue,
    store_name: &str,
    value: &JsValue,
    key: &JsValue,
) -> Result<SaveReport, JsValue> {
    match add_one(db, store_name, value).await {
        Ok(()) => Ok(SaveReport {
            outcome: SaveOutcome::Created,
            previous: None,
        }),
        Err(e) => {
            if !is_constraint_error(&e) {
                return Err(e);
            }
            let previous = get_one(db, store_name, key).await?;
            put_one(db, store_name, value).await?;
            Ok(SaveReport {
                outcome: SaveOutcome::Updated,
                previous,
            })
        }
    }
}

// ============================================================
//                    ПАЧКИ
// ============================================================

/// Все `add` в одной транзакции. Первый конфликт откатывает всю пачку.
///
/// `written` показывает, сколько запросов успели получить `onsuccess`
/// до отката. Но в store после отката они не останутся — транзакция
/// атомарна. Это и есть ключевое свойство IndexedDB.
pub async fn batch_add_atomic(
    db: &JsValue,
    store_name: &str,
    values: &[JsValue],
) -> Result<BatchOutcome, JsValue> {
    batch_write(db, store_name, values, "add").await
}

/// Все `put` в одной транзакции. Идемпотентно: пачка всегда проходит
/// целиком, независимо от того, какие ключи уже есть в store.
pub async fn batch_put_idempotent(
    db: &JsValue,
    store_name: &str,
    values: &[JsValue],
) -> Result<BatchOutcome, JsValue> {
    batch_write(db, store_name, values, "put").await
}

struct BatchState {
    written: u32,
    aborted_at: Option<u32>,
    abort_reason: Option<WriteError>,
}

async fn batch_write(
    db: &JsValue,
    store_name: &str,
    values: &[JsValue],
    method_name: &str,
) -> Result<BatchOutcome, JsValue> {
    let (os, tx) = open_tx(db, store_name, "readwrite")?;
    let tx_done = tx_done_promise(&tx);

    let state = Rc::new(RefCell::new(BatchState {
        written: 0,
        aborted_at: None,
        abort_reason: None,
    }));

    let f = method(&os, method_name)?;
    let attempted = values.len() as u32;

    for (i, v) in values.iter().enumerate() {
        let req = match f.call1(&os, v) {
            Ok(r) => r,
            Err(e) => {
                // Синхронная ошибка — транзакция уже неактивна.
                let mut s = state.borrow_mut();
                if s.aborted_at.is_none() {
                    s.aborted_at = Some(i as u32);
                    s.abort_reason = Some(classify_write_error(&e));
                }
                break;
            }
        };

        let state_ok = state.clone();
        let on_ok = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            state_ok.borrow_mut().written += 1;
        });
        let _ = js_sys::Reflect::set(
            &req,
            &JsValue::from_str("onsuccess"),
            on_ok.as_ref().unchecked_ref(),
        );
        on_ok.forget();

        let state_err = state.clone();
        let idx = i as u32;
        let on_err = Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
            let mut s = state_err.borrow_mut();
            if s.aborted_at.is_some() {
                return; // первая причина уже записана
            }
            s.aborted_at = Some(idx);
            let err = ev
                .target()
                .and_then(|t| js_sys::Reflect::get(&t, &JsValue::from_str("error")).ok())
                .unwrap_or(JsValue::UNDEFINED);
            s.abort_reason = Some(classify_write_error(&err));
        });
        let _ = js_sys::Reflect::set(
            &req,
            &JsValue::from_str("onerror"),
            on_err.as_ref().unchecked_ref(),
        );
        on_err.forget();
    }

    // Ждём финала транзакции — complete или abort, неважно.
    let _ = JsFuture::from(tx_done).await;

    let s = state.borrow();
    Ok(BatchOutcome {
        attempted,
        written: s.written,
        aborted_at: s.aborted_at,
        abort_reason: s.abort_reason.clone(),
    })
}

/// `count()` — сколько записей в store.
pub async fn count_all(db: &JsValue, store_name: &str) -> Result<u32, JsValue> {
    let (os, tx) = open_tx(db, store_name, "readonly")?;
    let tx_done = tx_done_promise(&tx);
    let f = method(&os, "count")?;
    let req = f
        .call0(&os)
        .map_err(|e| JsValue::from_str(&format!("count: {e:?}")))?;
    let res = await_request(&req).await;
    let _ = JsFuture::from(tx_done).await;
    res.map(|v| v.as_f64().unwrap_or(0.0) as u32)
}

/// `getAll()` — все записи.
pub async fn list_all(db: &JsValue, store_name: &str) -> Result<Vec<JsValue>, JsValue> {
    let (os, tx) = open_tx(db, store_name, "readonly")?;
    let tx_done = tx_done_promise(&tx);
    let f = method(&os, "getAll")?;
    let req = f
        .call0(&os)
        .map_err(|e| JsValue::from_str(&format!("getAll: {e:?}")))?;
    let res = await_request(&req).await;
    let _ = JsFuture::from(tx_done).await;
    let arr: js_sys::Array = res?
        .dyn_into()
        .map_err(|_| JsValue::from_str("getAll вернул не Array"))?;
    let mut out = Vec::with_capacity(arr.length() as usize);
    for i in 0..arr.length() {
        out.push(arr.get(i));
    }
    Ok(out)
}

// ============================================================
//                    ХЕЛПЕРЫ ТРАНЗАКЦИЙ
// ============================================================

fn open_tx(
    db: &JsValue,
    store_name: &str,
    mode: &str,
) -> Result<(JsValue, JsValue), JsValue> {
    let tx_fn: js_sys::Function = js_sys::Reflect::get(db, &JsValue::from_str("transaction"))
        .map_err(|e| JsValue::from_str(&format!("Reflect transaction: {e:?}")))?
        .dyn_into()
        .map_err(|_| JsValue::from_str("transaction не функция"))?;
    let names = js_sys::Array::new();
    names.push(&JsValue::from_str(store_name));
    let tx = tx_fn
        .call2(db, &names, &JsValue::from_str(mode))
        .map_err(|e| JsValue::from_str(&format!("transaction: {e:?}")))?;
    let os_fn: js_sys::Function = js_sys::Reflect::get(&tx, &JsValue::from_str("objectStore"))
        .map_err(|e| JsValue::from_str(&format!("Reflect objectStore: {e:?}")))?
        .dyn_into()
        .map_err(|_| JsValue::from_str("objectStore не функция"))?;
    let os = os_fn
        .call1(&tx, &JsValue::from_str(store_name))
        .map_err(|e| JsValue::from_str(&format!("objectStore: {e:?}")))?;
    Ok((os, tx))
}

fn method(obj: &JsValue, name: &str) -> Result<js_sys::Function, JsValue> {
    js_sys::Reflect::get(obj, &JsValue::from_str(name))
        .map_err(|e| JsValue::from_str(&format!("Reflect {name}: {e:?}")))?
        .dyn_into()
        .map_err(|_| JsValue::from_str(&format!("{name} не функция")))
}

async fn await_request(req: &JsValue) -> Result<JsValue, JsValue> {
    let r_ok = req.clone();
    let r_err = req.clone();
    let promise = js_sys::Promise::new(&mut move |resolve, reject| {
        let r = r_ok.clone();
        let resolve_c = resolve.clone();
        let on_ok = Closure::once_into_js(move |_e: web_sys::Event| {
            let v = js_sys::Reflect::get(&r, &JsValue::from_str("result"))
                .unwrap_or(JsValue::UNDEFINED);
            let _ = resolve_c.call1(&JsValue::UNDEFINED, &v);
        });
        let _ = js_sys::Reflect::set(&r_ok, &JsValue::from_str("onsuccess"), &on_ok);

        let r = r_err.clone();
        let reject_c = reject.clone();
        let on_err = Closure::once_into_js(move |_e: web_sys::Event| {
            let err = js_sys::Reflect::get(&r, &JsValue::from_str("error"))
                .unwrap_or(JsValue::UNDEFINED);
            let _ = reject_c.call1(&JsValue::UNDEFINED, &err);
        });
        let _ = js_sys::Reflect::set(&r_err, &JsValue::from_str("onerror"), &on_err);
    });
    JsFuture::from(promise).await
}

/// Резолвится, когда транзакция завершилась — успешно или через abort.
/// Не отклоняется: об ошибке конкретного запроса знает сам запрос.
fn tx_done_promise(tx: &JsValue) -> js_sys::Promise {
    let tx_c = tx.clone();
    js_sys::Promise::new(&mut move |resolve, _reject| {
        let r1 = resolve.clone();
        let on_complete = Closure::once_into_js(move || {
            let _ = r1.call0(&JsValue::UNDEFINED);
        });
        let _ = js_sys::Reflect::set(&tx_c, &JsValue::from_str("oncomplete"), &on_complete);

        let r2 = resolve.clone();
        let on_abort = Closure::once_into_js(move || {
            let _ = r2.call0(&JsValue::UNDEFINED);
        });
        let _ = js_sys::Reflect::set(&tx_c, &JsValue::from_str("onabort"), &on_abort);
    })
}

// ============================================================
//                    DEMO-БАЗА
// ============================================================

/// Открывает demo-базу со store `contacts` (`keyPath = "id"`).
/// Создаёт store при первом открытии. Возвращает `IDBDatabase`.
pub async fn open_demo_db(name: &str) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let idb = js_sys::Reflect::get(&global, &JsValue::from_str("indexedDB"))
        .map_err(|e| JsValue::from_str(&format!("Reflect indexedDB: {e:?}")))?;
    if !idb.is_object() {
        return Err(JsValue::from_str("indexedDB недоступен"));
    }
    let open_fn: js_sys::Function = js_sys::Reflect::get(&idb, &JsValue::from_str("open"))
        .map_err(|e| JsValue::from_str(&format!("Reflect open: {e:?}")))?
        .dyn_into()
        .map_err(|_| JsValue::from_str("open не функция"))?;

    let request = open_fn
        .call2(&idb, &JsValue::from_str(name), &JsValue::from_f64(1.0))
        .map_err(|e| JsValue::from_str(&format!("open: {e:?}")))?;

    let on_upgrade = Closure::once_into_js(move |e: web_sys::Event| {
        let Some(target) = e.target() else { return };
        let Ok(db) = js_sys::Reflect::get(&target, &JsValue::from_str("result")) else {
            return;
        };
        let Ok(create_fn_js) =
            js_sys::Reflect::get(&db, &JsValue::from_str("createObjectStore"))
        else {
            return;
        };
        let Ok(create_fn) = create_fn_js.dyn_into::<js_sys::Function>() else {
            return;
        };
        let opts = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &opts,
            &JsValue::from_str("keyPath"),
            &JsValue::from_str("id"),
        );
        let _ = create_fn.call2(&db, &JsValue::from_str("contacts"), &opts);
    });
    let _ = js_sys::Reflect::set(
        &request,
        &JsValue::from_str("onupgradeneeded"),
        &on_upgrade,
    );

    await_request(&request).await
}

/// Удаляет базу целиком. Идемпотентно.
pub async fn delete_demo_db(name: &str) -> Result<(), JsValue> {
    let global = js_sys::global();
    let idb = js_sys::Reflect::get(&global, &JsValue::from_str("indexedDB"))
        .map_err(|e| JsValue::from_str(&format!("Reflect indexedDB: {e:?}")))?;
    let del_fn: js_sys::Function = js_sys::Reflect::get(&idb, &JsValue::from_str("deleteDatabase"))
        .map_err(|e| JsValue::from_str(&format!("Reflect deleteDatabase: {e:?}")))?
        .dyn_into()
        .map_err(|_| JsValue::from_str("deleteDatabase не функция"))?;
    let request = del_fn
        .call1(&idb, &JsValue::from_str(name))
        .map_err(|e| JsValue::from_str(&format!("deleteDatabase: {e:?}")))?;

    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let rc = resolve.clone();
        let on_ok = Closure::once_into_js(move || {
            let _ = rc.call0(&JsValue::UNDEFINED);
        });
        let _ = js_sys::Reflect::set(&request, &JsValue::from_str("onsuccess"), &on_ok);
    });
    JsFuture::from(promise).await?;
    Ok(())
}