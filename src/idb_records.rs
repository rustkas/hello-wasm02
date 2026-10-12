//! Операции над `IDBObjectStore` через `js_sys::Reflect`.
//!
//! ПРАВИЛО: `RecordStore` — это binding на store (db + name + mode),
//! а НЕ на транзакцию. Каждая async-операция открывает свою транзакцию
//! и ждёт её `oncomplete`. Так один и тот же `RecordStore` переиспользуется
//! в цикле без `TransactionInactiveError`.
//!
//! ПРАВИЛО: подписываемся на `tx.oncomplete` ДО первого запроса. Иначе
//! событие улетает в предыдущий макротакт, и `await` висит навсегда.
//!
//! ПРАВИЛО: `openCursor(range, direction)` — direction передаём всегда.
//! Без второго аргумента браузер жёстко использует "next".
//!
//! ПРАВИЛО: лимит итерации проверяем внутри `on_success`, до `continue()`.
//! Проверка между yield'ами ненадёжна: за один макротакт курсор
//! продвигается десятки раз.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

// ============================================================
//                       RecordStore
// ============================================================

/// Binding на object store: db + name + mode.
///
/// Не хранит транзакцию: каждая операция (add/put/get/cursor)
/// открывает свою. Это даёт переиспользование handle'а без
/// `TransactionInactiveError`.
pub struct RecordStore {
    db: JsValue,
    name: String,
    mode: String,
}

impl RecordStore {
    /// Принимает `db.raw()` (сам `IDBDatabase`), имя store'а,
    /// строку `"readonly"` или `"readwrite"`.
    pub fn open(db: &JsValue, name: &str, mode: &str) -> Result<Self, JsValue> {
        let mode_s = match mode {
            "readonly" | "readwrite" => mode.to_string(),
            other => {
                return Err(JsValue::from_str(&format!(
                    "open: mode должен быть readonly/readwrite, получено {other:?}"
                )));
            }
        };
        Ok(Self {
            db: db.clone(),
            name: name.to_string(),
            mode: mode_s,
        })
    }

    pub fn name(&self) -> &str { &self.name }
    pub fn mode(&self) -> &str { &self.mode }

    /// Открывает транзакцию и берёт в ней object store.
    /// Возвращает `(objectStore, transaction)`.
    fn open_tx(&self) -> Result<(JsValue, JsValue), JsValue> {
        return open_tx_and_store(&self.db, &self.name, &self.mode);
    }
}

fn open_tx_and_store(
    db: &JsValue,
    name: &str,
    mode: &str,
) -> Result<(JsValue, JsValue), JsValue> {
    let tx_fn: js_sys::Function =
        js_sys::Reflect::get(db, &JsValue::from_str("transaction"))
            .map_err(|e| JsValue::from_str(&format!("Reflect transaction: {e:?}")))?
            .dyn_into()
            .map_err(|_| JsValue::from_str("transaction не функция"))?;
    let names = js_sys::Array::new();
    names.push(&JsValue::from_str(name));
    let tx = tx_fn
        .call2(db, &names, &JsValue::from_str(mode))
        .map_err(|e| JsValue::from_str(&format!("transaction: {e:?}")))?;
    let os_fn: js_sys::Function =
        js_sys::Reflect::get(&tx, &JsValue::from_str("objectStore"))
            .map_err(|e| JsValue::from_str(&format!("Reflect objectStore: {e:?}")))?
            .dyn_into()
            .map_err(|_| JsValue::from_str("objectStore не функция"))?;
    let os = os_fn
        .call1(&tx, &JsValue::from_str(name))
        .map_err(|e| JsValue::from_str(&format!("objectStore: {e:?}")))?;
    Ok((os, tx))
}

// ============================================================
//                       Direction
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Next,
    Prev,
    NextUnique,
    PrevUnique,
}

impl Direction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Direction::Next => "next",
            Direction::Prev => "prev",
            Direction::NextUnique => "nextunique",
            Direction::PrevUnique => "prevunique",
        }
    }
}

// ============================================================
//                       KeyRangeSpec
// ============================================================

#[derive(Clone, Debug)]
pub enum KeyRangeSpec {
    Only(JsValue),
    LowerBound { value: JsValue, open: bool },
    UpperBound { value: JsValue, open: bool },
    Bound { lower: JsValue, upper: JsValue, lower_open: bool, upper_open: bool },
}

impl KeyRangeSpec {
    pub fn to_js(&self) -> Result<JsValue, JsValue> {
        let kr = key_range_ctor()?;
        match self {
            KeyRangeSpec::Only(v) => {
                let f: js_sys::Function = js_sys::Reflect::get(&kr, &"only".into())?
                    .dyn_into()
                    .map_err(|_| JsValue::from_str("only не функция"))?;
                f.call1(&kr, v)
                    .map_err(|e| JsValue::from_str(&format!("IDBKeyRange.only: {e:?}")))
            }
            KeyRangeSpec::LowerBound { value, open } => {
                let f: js_sys::Function = js_sys::Reflect::get(&kr, &"lowerBound".into())?
                    .dyn_into()
                    .map_err(|_| JsValue::from_str("lowerBound не функция"))?;
                f.call2(&kr, value, &JsValue::from_bool(*open))
                    .map_err(|e| JsValue::from_str(&format!("IDBKeyRange.lowerBound: {e:?}")))
            }
            KeyRangeSpec::UpperBound { value, open } => {
                let f: js_sys::Function = js_sys::Reflect::get(&kr, &"upperBound".into())?
                    .dyn_into()
                    .map_err(|_| JsValue::from_str("upperBound не функция"))?;
                f.call2(&kr, value, &JsValue::from_bool(*open))
                    .map_err(|e| JsValue::from_str(&format!("IDBKeyRange.upperBound: {e:?}")))
            }
            KeyRangeSpec::Bound { lower, upper, lower_open, upper_open } => {
                let f: js_sys::Function = js_sys::Reflect::get(&kr, &"bound".into())?
                    .dyn_into()
                    .map_err(|_| JsValue::from_str("bound не функция"))?;
                f.call4(
                    &kr,
                    lower,
                    upper,
                    &JsValue::from_bool(*lower_open),
                    &JsValue::from_bool(*upper_open),
                )
                .map_err(|e| JsValue::from_str(&format!("IDBKeyRange.bound: {e:?}")))
            }
        }
    }
}

// ============================================================
//                       CursorResult
// ============================================================

#[derive(Clone, Debug)]
pub struct CursorResult {
    pub scanned: u32,
    pub keys: Vec<JsValue>,
    pub values: Vec<JsValue>,
    pub elapsed_ms: f64,
    pub truncated: bool,
}

// ============================================================
//                       CRUD
//
// Каждая операция открывает собственную транзакцию и ждёт её
// `oncomplete`. Один `RecordStore` можно переиспользовать в цикле.
// ============================================================

pub async fn add(store: &RecordStore, record: &JsValue) -> Result<JsValue, JsValue> {
    let (os, tx) = store.open_tx()?;
    let complete = tx_complete_promise(&tx);
    let f = method(&os, "add")?;
    let req = f
        .call1(&os, record)
        .map_err(|e| JsValue::from_str(&format!("add: {e:?}")))?;
    let res = await_request(&req).await;
    // Дожидаемся завершения транзакции В ЛЮБОМ СЛУЧАЕ — даже при ошибке
    // request'а (ConstraintError → abort). Иначе соединение остаётся
    // активным и следующий `deleteDatabase` уходит в `onblocked`.
    JsFuture::from(complete).await.ok();
    res
}

pub async fn add_with_key(
    store: &RecordStore,
    record: &JsValue,
    key: &JsValue,
) -> Result<JsValue, JsValue> {
    let (os, tx) = store.open_tx()?;
    let complete = tx_complete_promise(&tx);
    let f = method(&os, "add")?;
    let req = f
        .call2(&os, record, key)
        .map_err(|e| JsValue::from_str(&format!("add: {e:?}")))?;
    let res = await_request(&req).await;
    JsFuture::from(complete).await.ok();
    res
}

pub async fn put(store: &RecordStore, record: &JsValue) -> Result<JsValue, JsValue> {
    let (os, tx) = store.open_tx()?;
    let complete = tx_complete_promise(&tx);
    let f = method(&os, "put")?;
    let req = f
        .call1(&os, record)
        .map_err(|e| JsValue::from_str(&format!("put: {e:?}")))?;
    let res = await_request(&req).await;
    JsFuture::from(complete).await.ok();
    res
}

pub async fn delete(store: &RecordStore, key: &JsValue) -> Result<(), JsValue> {
    let (os, tx) = store.open_tx()?;
    let complete = tx_complete_promise(&tx);
    let f = method(&os, "delete")?;
    let req = f
        .call1(&os, key)
        .map_err(|e| JsValue::from_str(&format!("delete: {e:?}")))?;
    let res = await_request(&req).await;
    JsFuture::from(complete).await.ok();
    res.map(|_| ())
}

pub async fn clear(store: &RecordStore) -> Result<(), JsValue> {
    let (os, tx) = store.open_tx()?;
    let complete = tx_complete_promise(&tx);
    let f = method(&os, "clear")?;
    let req = f
        .call0(&os)
        .map_err(|e| JsValue::from_str(&format!("clear: {e:?}")))?;
    let res = await_request(&req).await;
    JsFuture::from(complete).await.ok();
    res.map(|_| ())
}

pub async fn get(store: &RecordStore, key: &JsValue) -> Result<Option<JsValue>, JsValue> {
    let (os, tx) = store.open_tx()?;
    let complete = tx_complete_promise(&tx);
    let f = method(&os, "get")?;
    let req = f
        .call1(&os, key)
        .map_err(|e| JsValue::from_str(&format!("get: {e:?}")))?;
    let res = await_request(&req).await;
    JsFuture::from(complete).await.ok();
    match res {
        Ok(v) if v.is_null() || v.is_undefined() => Ok(None),
        Ok(v) => Ok(Some(v)),
        Err(e) => Err(e),
    }
}

pub async fn count(store: &RecordStore) -> Result<u32, JsValue> {
    let (os, tx) = store.open_tx()?;
    let complete = tx_complete_promise(&tx);
    let f = method(&os, "count")?;
    let req = f
        .call0(&os)
        .map_err(|e| JsValue::from_str(&format!("count: {e:?}")))?;
    let res = await_request(&req).await;
    JsFuture::from(complete).await.ok();
    res.map(|v| v.as_f64().unwrap_or(0.0) as u32)
}

pub async fn get_all(store: &RecordStore) -> Result<Vec<JsValue>, JsValue> {
    let (os, tx) = store.open_tx()?;
    let complete = tx_complete_promise(&tx);
    let f = method(&os, "getAll")?;
    let req = f
        .call0(&os)
        .map_err(|e| JsValue::from_str(&format!("getAll: {e:?}")))?;
    let res = await_request(&req).await;
    JsFuture::from(complete).await.ok();
    match res {
        Ok(v) => array_to_vec(v),
        Err(e) => Err(e),
    }
}

pub async fn get_all_keys(store: &RecordStore) -> Result<Vec<JsValue>, JsValue> {
    let (os, tx) = store.open_tx()?;
    let complete = tx_complete_promise(&tx);
    let f = method(&os, "getAllKeys")?;
    let req = f
        .call0(&os)
        .map_err(|e| JsValue::from_str(&format!("getAllKeys: {e:?}")))?;
    let res = await_request(&req).await;
    JsFuture::from(complete).await.ok();
    match res {
        Ok(v) => array_to_vec(v),
        Err(e) => Err(e),
    }
}

pub async fn seed_bulk(store: &RecordStore, records: &[JsValue]) -> Result<(), JsValue> {
    let (os, tx) = store.open_tx()?;
    let complete = tx_complete_promise(&tx);
    let put_fn = method(&os, "put")?;
    let mut first_err: Option<JsValue> = None;
    for rec in records {
        if let Err(e) = put_fn.call1(&os, rec) {
            if first_err.is_none() {
                first_err = Some(e);
            }
            break; // дальнейшие put'ы в abort-транзакцию бессмысленны
        }
    }
    JsFuture::from(complete).await.ok();
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

// ============================================================
//                       CURSOR
// ============================================================

pub async fn cursor_visit<F>(
    store: &RecordStore,
    range: Option<KeyRangeSpec>,
    direction: Direction,
    limit: Option<u32>,
    mut on_progress: F,
) -> Result<CursorResult, JsValue>
where
    F: FnMut(u32),
{
    let t0 = now_ms();
    let (os, tx) = store.open_tx()?;

    // Подписываемся на tx.oncomplete ДО первого касания курсора.
    // Между последним `null`-результатом курсора и коммитом транзакции
    // проходит как минимум одна task; если подписаться позже, событие
    // `oncomplete` уже улетит, и await зависнет.
    let tx_complete = tx_complete_promise(&tx);

    struct State {
        keys: Vec<JsValue>,
        values: Vec<JsValue>,
        done: bool,
        error: Option<JsValue>,
    }

    let state = Rc::new(RefCell::new(State {
        keys: Vec::new(),
        values: Vec::new(),
        done: false,
        error: None,
    }));
    let limit_cell: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(limit));
    // Отдельный флаг: остановились ли мы по лимиту (а не по концу).
    let trunc_cell: Rc<Cell<bool>> = Rc::new(Cell::new(false));

    let open_fn = method(&os, "openCursor")?;
    let request = match range {
        Some(spec) => {
            let range_js = spec.to_js()?;
            open_fn
                .call2(&os, &range_js, &JsValue::from_str(direction.as_str()))
                .map_err(|e| JsValue::from_str(&format!("openCursor: {e:?}")))?
        }
        None => {
            // direction передаём всегда — без второго аргумента
            // openCursor() жёстко использует "next".
            open_fn
                .call2(
                    &os,
                    &JsValue::NULL,
                    &JsValue::from_str(direction.as_str()),
                )
                .map_err(|e| JsValue::from_str(&format!("openCursor: {e:?}")))?
        }
    };

    // onsuccess: продвигает курсор, проверяет лимит ДО continue().
    {
        let state_cb = state.clone();
        let limit_cb = limit_cell.clone();
        let trunc_cb = trunc_cell.clone();
        let on_success =
            Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
                if state_cb.borrow().done {
                    return;
                }
                let target = match ev.target() {
                    Some(t) => t,
                    None => return,
                };
                let cursor = js_sys::Reflect::get(&target, &"result".into())
                    .unwrap_or(JsValue::NULL);
                if cursor.is_null() || cursor.is_undefined() {
                    state_cb.borrow_mut().done = true;
                    return;
                }
                let key = js_sys::Reflect::get(&cursor, &"key".into())
                    .unwrap_or(JsValue::NULL);
                let value = js_sys::Reflect::get(&cursor, &"value".into())
                    .unwrap_or(JsValue::NULL);

                let should_stop = {
                    let mut st = state_cb.borrow_mut();
                    st.keys.push(key);
                    st.values.push(value);
                    match limit_cb.get() {
                        Some(lim) => st.keys.len() as u32 >= lim,
                        None => false,
                    }
                };

                if should_stop {
                    state_cb.borrow_mut().done = true;
                    trunc_cb.set(true); // ← отдельный флаг для truncated
                    return; // НЕ вызываем continue() — итерация остановлена
                }

                if let Ok(cont) = js_sys::Reflect::get(&cursor, &"continue".into())
                    .and_then(|f| f.dyn_into::<js_sys::Function>())
                {
                    let _ = cont.call0(&cursor);
                }
            });
        let _ = js_sys::Reflect::set(
            &request,
            &"onsuccess".into(),
            on_success.as_ref().unchecked_ref(),
        );
        on_success.forget();
    }

    // onerror: явное приведение target к JsValue для единого типа match.
    {
        let state_cb = state.clone();
        let on_error =
            Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
                let target: JsValue = match ev.target() {
                    Some(t) => t.into(),
                    None => JsValue::NULL,
                };
                let err = js_sys::Reflect::get(&target, &"error".into())
                    .unwrap_or(JsValue::UNDEFINED);
                state_cb.borrow_mut().error = Some(err);
                state_cb.borrow_mut().done = true;
            });
        let _ = js_sys::Reflect::set(
            &request,
            &"onerror".into(),
            on_error.as_ref().unchecked_ref(),
        );
        on_error.forget();
    }

    // Ждём завершения, изредка отдаём управление UI.
    let mut last_yield = now_ms();
    loop {
        if state.borrow().done {
            break;
        }
        TimeoutFuture::new(0).await;
        let now = now_ms();
        if now - last_yield >= 16.0 {
            on_progress(state.borrow().keys.len() as u32);
            last_yield = now;
        }
    }

    let scanned = state.borrow().keys.len() as u32;
    on_progress(scanned);

    if let Some(err) = state.borrow().error.clone() {
        return Err(err);
    }

    // Транзакция завершается вслед за курсором. Подписка уже сделана.
    JsFuture::from(tx_complete).await.ok();

    let elapsed_ms = now_ms() - t0;
    let st = state.borrow();
    Ok(CursorResult {
        scanned,
        keys: st.keys.clone(),
        values: st.values.clone(),
        elapsed_ms,
        truncated: trunc_cell.get(),
    })
}

// ============================================================
//                       HELPERS
// ============================================================

fn method(obj: &JsValue, name: &str) -> Result<js_sys::Function, JsValue> {
    js_sys::Reflect::get(obj, &JsValue::from_str(name))
        .map_err(|e| JsValue::from_str(&format!("Reflect {name}: {e:?}")))?
        .dyn_into()
        .map_err(|_| JsValue::from_str(&format!("{name} не функция")))
}

fn array_to_vec(v: JsValue) -> Result<Vec<JsValue>, JsValue> {
    let arr: js_sys::Array = v
        .dyn_into()
        .map_err(|_| JsValue::from_str("ожидали Array"))?;
    let mut out = Vec::with_capacity(arr.length() as usize);
    for i in 0..arr.length() {
        out.push(arr.get(i));
    }
    Ok(out)
}

fn key_range_ctor() -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let kr = js_sys::Reflect::get(&global, &JsValue::from_str("IDBKeyRange"))
        .map_err(|e| JsValue::from_str(&format!("Reflect IDBKeyRange: {e:?}")))?;
    if kr.is_undefined() || kr.is_null() {
        return Err(JsValue::from_str("IDBKeyRange недоступен"));
    }
    Ok(kr)
}

async fn await_request(req: &JsValue) -> Result<JsValue, JsValue> {
    let r_ok_set = req.clone();
    let r_ok_in = req.clone();
    let r_err_set = req.clone();
    let r_err_in = req.clone();

    let promise = js_sys::Promise::new(&mut move |resolve, reject| {
        let r = r_ok_in.clone();
        let resolve_c = resolve.clone();
        let on_success = Closure::once_into_js(move |_e: web_sys::Event| {
            let result = js_sys::Reflect::get(&r, &"result".into())
                .unwrap_or(JsValue::UNDEFINED);
            let _ = resolve_c.call1(&JsValue::UNDEFINED, &result);
        });
        let _ = js_sys::Reflect::set(&r_ok_set, &"onsuccess".into(), &on_success);

        let r = r_err_in.clone();
        let reject_c = reject.clone();
        let on_error = Closure::once_into_js(move |_e: web_sys::Event| {
            let err = js_sys::Reflect::get(&r, &"error".into())
                .unwrap_or(JsValue::UNDEFINED);
            let _ = reject_c.call1(&JsValue::UNDEFINED, &err);
        });
        let _ = js_sys::Reflect::set(&r_err_set, &"onerror".into(), &on_error);
    });

    JsFuture::from(promise).await
}

/// Promise, который резолвится, когда транзакция завершена — успешно
/// (`oncomplete`) или через откат (`onabort`).
///
/// Никаких reject'ов: об ошибке конкретного запроса знает сам request.
/// Наша задача — дождаться, чтобы соединение было освобождено и следующий
/// `deleteDatabase` не ушёл в `onblocked`.
fn tx_complete_promise(tx: &JsValue) -> js_sys::Promise {
    let tx_set = tx.clone();
    js_sys::Promise::new(&mut move |resolve, _reject| {
        // Успешный коммит.
        let resolve_complete = resolve.clone();
        let on_complete = Closure::once_into_js(move || {
            let _ = resolve_complete.call0(&JsValue::UNDEFINED);
        });
        let _ = js_sys::Reflect::set(&tx_set, &"oncomplete".into(), &on_complete);

        // Откат — тоже завершение. Резолвим, чтобы await не висел вечно.
        let resolve_abort = resolve.clone();
        let on_abort = Closure::once_into_js(move || {
            let _ = resolve_abort.call0(&JsValue::UNDEFINED);
        });
        let _ = js_sys::Reflect::set(&tx_set, &"onabort".into(), &on_abort);
    })
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}