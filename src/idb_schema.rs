//! Управление схемой IndexedDB: stores, индексы, версии.
//!
//! ПРАВИЛО: `createObjectStore`, `deleteObjectStore`, `createIndex`,
//! `deleteIndex` работают только внутри versionchange-транзакции
//! (`onupgradeneeded`). Вне её — синхронно `InvalidStateError`.
//!
//! ПРАВИЛО: `keyPath` может быть составным — массив полей. Парсим
//! строку «author_id,year» в массив `["author_id", "year"]`.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::idb_db::Database;

/// Описание индекса на object store.
#[derive(Clone, Debug, PartialEq)]
pub struct IndexSpec {
    pub name: String,
    /// Имя поля или список через запятую для составного индекса.
    pub key_path: String,
    pub unique: bool,
    pub multi_entry: bool,
}

impl IndexSpec {
    pub fn new(name: &str, key_path: &str) -> Self {
        Self {
            name: name.to_string(),
            key_path: key_path.to_string(),
            unique: false,
            multi_entry: false,
        }
    }
    pub fn unique(mut self, v: bool) -> Self { self.unique = v; self }
    pub fn multi(mut self, v: bool) -> Self { self.multi_entry = v; self }
}

/// Описание object store.
#[derive(Clone, Debug, PartialEq)]
pub struct StoreSpec {
    pub name: String,
    /// `None` — autoIncrement без явного keyPath.
    pub key_path: Option<String>,
    pub auto_increment: bool,
    pub indexes: Vec<IndexSpec>,
}

impl StoreSpec {
    pub fn with_key(name: &str, key_path: &str) -> Self {
        Self {
            name: name.to_string(),
            key_path: Some(key_path.to_string()),
            auto_increment: false,
            indexes: vec![],
        }
    }
    pub fn with_auto_key(name: &str) -> Self {
        Self {
            name: name.to_string(),
            key_path: None,
            auto_increment: true,
            indexes: vec![],
        }
    }
    pub fn indexes(mut self, idx: Vec<IndexSpec>) -> Self {
        self.indexes = idx;
        self
    }
}

/// Декларативное описание схемы. План — источник правды.
#[derive(Clone, Debug, Default)]
pub struct SchemaPlan {
    pub stores: Vec<StoreSpec>,
}

impl SchemaPlan {
    pub fn new() -> Self { Self { stores: Vec::new() } }
    pub fn add(mut self, spec: StoreSpec) -> Self {
        self.stores.push(spec);
        self
    }
    pub fn names(&self) -> Vec<String> {
        self.stores.iter().map(|s| s.name.clone()).collect()
    }
}

/// Что изменилось при применении плана.
#[derive(Clone, Debug, Default)]
pub struct Diff {
    pub upgraded: bool,
    pub old_version: u32,
    pub new_version: u32,
    pub created: Vec<String>,
    pub deleted: Vec<String>,
    pub kept: Vec<String>,
    pub indexes_created: Vec<String>,
    pub indexes_deleted: Vec<String>,
    pub errors: Vec<String>,
}

pub struct OpenOutcome {
    pub db: Database,
    pub diff: Diff,
}

pub async fn open_with_plan(
    name: &str,
    version: u32,
    plan: &SchemaPlan,
) -> Result<OpenOutcome, JsValue> {
    let diff = Rc::new(RefCell::new(Diff::default()));
    let diff_c = diff.clone();
    let plan_c = plan.clone();

    let raw_db = open_with_upgrade(
        name, version,
        move |db, tx, old_v, new_v| {
            let mut d = diff_c.borrow_mut();
            d.upgraded = true;
            d.old_version = old_v;
            d.new_version = new_v;
            apply_plan(db, tx, &plan_c, &mut d);
        },
    ).await?;

    let final_diff = diff.borrow().clone();
    Ok(OpenOutcome {
        db: Database::from_raw(raw_db),
        diff: final_diff,
    })
}

pub fn try_create_on_open(db: &Database, spec: &StoreSpec) -> Result<(), JsValue> {
    create_store_raw(db.raw(), spec)
}

pub fn try_delete_on_open(db: &Database, store_name: &str) -> Result<(), JsValue> {
    delete_store_raw(db.raw(), store_name)
}

// ============================================================
//                     ВНУТРЕННЕЕ
// ============================================================

async fn open_with_upgrade<F>(
    name: &str,
    version: u32,
    on_upgrade: F,
) -> Result<JsValue, JsValue>
where
    F: FnOnce(&JsValue, &JsValue, u32, u32) + 'static,
{
    let global = js_sys::global();
    let idb = js_sys::Reflect::get(&global, &JsValue::from_str("indexedDB"))
        .map_err(|e| JsValue::from_str(&format!("Reflect indexedDB: {e:?}")))?;
    if !idb.is_object() {
        return Err(JsValue::from_str("indexedDB недоступен"));
    }
    let open_fn: js_sys::Function = js_sys::Reflect::get(&idb, &JsValue::from_str("open"))
        .map_err(|e| JsValue::from_str(&format!("Reflect open: {e:?}")))?
        .dyn_into().map_err(|_| JsValue::from_str("open не функция"))?;

    let request = open_fn
        .call2(&idb, &JsValue::from_str(name), &JsValue::from_f64(version as f64))
        .map_err(|e| JsValue::from_str(&format!("open: {e:?}")))?;

    // onupgradeneeded — единственное окно, где разрешены мутации схемы.
    let on_upgrade_js = Closure::once_into_js(move |e: web_sys::Event| {
        let target = match e.target() { Some(t) => t, None => return };
        let db = match js_sys::Reflect::get(&target, &JsValue::from_str("result")) {
            Ok(v) => v, Err(_) => return,
        };
        // request.transaction — versionchange-транзакция, активна внутри
        // этого события. Через неё берём store-объекты для sync индекс.
        let tx = match js_sys::Reflect::get(&target, &JsValue::from_str("transaction")) {
            Ok(v) => v, Err(_) => return,
        };
        let old_v = js_sys::Reflect::get(e.as_ref(), &JsValue::from_str("oldVersion"))
            .ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
        let new_v = js_sys::Reflect::get(e.as_ref(), &JsValue::from_str("newVersion"))
            .ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
        on_upgrade(&db, &tx, old_v, new_v);
    });
    js_sys::Reflect::set(&request, &JsValue::from_str("onupgradeneeded"), &on_upgrade_js)?;

    let r_ok = request.clone();
    let r_err = request.clone();
    let r_block = request.clone();
    let promise = js_sys::Promise::new(&mut move |resolve, reject| {
        let r = r_ok.clone();
        let resolve_in = resolve.clone();
        let on_success = Closure::once_into_js(move |_e: web_sys::Event| {
            let result = js_sys::Reflect::get(&r, &JsValue::from_str("result"))
                .unwrap_or(JsValue::UNDEFINED);
            let _ = resolve_in.call1(&JsValue::UNDEFINED, &result);
        });
        let _ = js_sys::Reflect::set(&r_ok, &JsValue::from_str("onsuccess"), &on_success);

        let r = r_err.clone();
        let reject_in = reject.clone();
        let on_error = Closure::once_into_js(move |_e: web_sys::Event| {
            let err = js_sys::Reflect::get(&r, &JsValue::from_str("error"))
                .unwrap_or(JsValue::UNDEFINED);
            let _ = reject_in.call1(&JsValue::UNDEFINED, &err);
        });
        let _ = js_sys::Reflect::set(&r_err, &JsValue::from_str("onerror"), &on_error);

        let reject_b = reject.clone();
        let on_blocked = Closure::once_into_js(move |_e: web_sys::Event| {
            let _ = reject_b.call1(&JsValue::UNDEFINED,
                &JsValue::from_str("blocked: соединение держится другим табом"));
        });
        let _ = js_sys::Reflect::set(&r_block, &JsValue::from_str("onblocked"), &on_blocked);
    });

    JsFuture::from(promise).await
}

fn apply_plan(db: &JsValue, tx: &JsValue, plan: &SchemaPlan, diff: &mut Diff) {
    let existing = list_store_names(db);
    let plan_names: Vec<&str> = plan.stores.iter().map(|s| s.name.as_str()).collect();

    // 1. Создать недостающие store'ы + синхронизировать индексы.
    for spec in &plan.stores {
        let is_new = !existing.iter().any(|n| n == &spec.name);
        if is_new {
            match create_store_raw(db, spec) {
                Ok(()) => diff.created.push(spec.name.clone()),
                Err(e) => {
                    diff.errors.push(format!("create {}: {e:?}", spec.name));
                    continue;
                }
            }
        } else {
            diff.kept.push(spec.name.clone());
        }

        // Синхронизация индексов на этом store.
        if let Ok(store) = get_store_from_tx(tx, &spec.name) {
            sync_indexes(&store, spec, diff);
        }
    }

    // 2. Удалить store'ы, которых нет в плане.
    for name in existing {
        if !plan_names.contains(&name.as_str()) {
            match delete_store_raw(db, &name) {
                Ok(()) => diff.deleted.push(name),
                Err(e) => diff.errors.push(format!("delete {name}: {e:?}")),
            }
        }
    }
}

fn sync_indexes(store: &JsValue, spec: &StoreSpec, diff: &mut Diff) {
    let existing = list_index_names(store);
    let planned: Vec<&str> = spec.indexes.iter().map(|i| i.name.as_str()).collect();

    // Создать недостающие.
    for idx in &spec.indexes {
        if existing.iter().any(|n| n == &idx.name) { continue; }
        match create_index_raw(store, idx) {
            Ok(()) => diff.indexes_created.push(format!("{}.{}", spec.name, idx.name)),
            Err(e) => diff.errors.push(format!("index {}.{}: {e:?}", spec.name, idx.name)),
        }
    }

    // Удалить лишние.
    for name in existing {
        if !planned.contains(&name.as_str()) {
            match delete_index_raw(store, &name) {
                Ok(()) => diff.indexes_deleted.push(format!("{}.{}", spec.name, name)),
                Err(e) => diff.errors.push(format!("delIndex {}.{}: {e:?}", spec.name, name)),
            }
        }
    }
}

fn get_store_from_tx(tx: &JsValue, name: &str) -> Result<JsValue, JsValue> {
    let f: js_sys::Function = js_sys::Reflect::get(tx, &JsValue::from_str("objectStore"))
        .map_err(|e| JsValue::from_str(&format!("Reflect objectStore: {e:?}")))?
        .dyn_into().map_err(|_| JsValue::from_str("objectStore не функция"))?;
    f.call1(tx, &JsValue::from_str(name))
        .map_err(|e| JsValue::from_str(&format!("objectStore: {e:?}")))
}

fn list_store_names(db: &JsValue) -> Vec<String> {
    let list = match js_sys::Reflect::get(db, &JsValue::from_str("objectStoreNames")) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return Vec::new(),
    };
    return list_domstringlist(&list);
}

fn list_index_names(store: &JsValue) -> Vec<String> {
    let list = match js_sys::Reflect::get(store, &JsValue::from_str("indexNames")) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return Vec::new(),
    };
    return list_domstringlist(&list);
}

fn list_domstringlist(list: &JsValue) -> Vec<String> {
    let length = js_sys::Reflect::get(list, &JsValue::from_str("length"))
        .ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
    let item_fn = match js_sys::Reflect::get(list, &JsValue::from_str("item")) {
        Ok(v) => v, Err(_) => return Vec::new(),
    };
    let item: js_sys::Function = match item_fn.dyn_into() { Ok(f) => f, Err(_) => return Vec::new() };
    let mut out = Vec::with_capacity(length as usize);
    for i in 0..length {
        if let Ok(v) = item.call1(list, &JsValue::from_f64(i as f64)) {
            if let Some(s) = v.as_string() { out.push(s); }
        }
    }
    out
}

/// Разбирает `keyPath`: «author_id,year» → `["author_id", "year"]`.
fn parse_key_path(kp: &str) -> JsValue {
    let t = kp.trim();
    if t.contains(',') {
        let arr = js_sys::Array::new();
        for p in t.split(',') {
            arr.push(&JsValue::from_str(p.trim()));
        }
        arr.into()
    } else {
        JsValue::from_str(t)
    }
}

fn create_store_raw(db: &JsValue, spec: &StoreSpec) -> Result<(), JsValue> {
    let create_fn: js_sys::Function =
        js_sys::Reflect::get(db, &JsValue::from_str("createObjectStore"))
            .map_err(|e| JsValue::from_str(&format!("Reflect createObjectStore: {e:?}")))?
            .dyn_into().map_err(|_| JsValue::from_str("createObjectStore не функция"))?;

    let options = js_sys::Object::new();
    if let Some(kp) = &spec.key_path {
        js_sys::Reflect::set(&options, &JsValue::from_str("keyPath"), &parse_key_path(kp))?;
    }
    if spec.auto_increment {
        js_sys::Reflect::set(&options, &JsValue::from_str("autoIncrement"), &JsValue::TRUE)?;
    }
    create_fn.call2(db, &JsValue::from_str(&spec.name), &options)
        .map_err(|e| e)?;
    Ok(())
}

fn create_index_raw(store: &JsValue, idx: &IndexSpec) -> Result<(), JsValue> {
    let create_fn: js_sys::Function =
        js_sys::Reflect::get(store, &JsValue::from_str("createIndex"))
            .map_err(|e| JsValue::from_str(&format!("Reflect createIndex: {e:?}")))?
            .dyn_into().map_err(|_| JsValue::from_str("createIndex не функция"))?;

    let kp = parse_key_path(&idx.key_path);
    let opts = js_sys::Object::new();
    if idx.unique {
        js_sys::Reflect::set(&opts, &JsValue::from_str("unique"), &JsValue::TRUE)?;
    }
    if idx.multi_entry {
        js_sys::Reflect::set(&opts, &JsValue::from_str("multiEntry"), &JsValue::TRUE)?;
    }
    create_fn.call3(store, &JsValue::from_str(&idx.name), &kp, &opts)
        .map_err(|e| e)?;
    Ok(())
}

fn delete_index_raw(store: &JsValue, name: &str) -> Result<(), JsValue> {
    let del_fn: js_sys::Function =
        js_sys::Reflect::get(store, &JsValue::from_str("deleteIndex"))
            .map_err(|e| JsValue::from_str(&format!("Reflect deleteIndex: {e:?}")))?
            .dyn_into().map_err(|_| JsValue::from_str("deleteIndex не функция"))?;
    del_fn.call1(store, &JsValue::from_str(name)).map_err(|e| e)?;
    Ok(())
}

fn delete_store_raw(db: &JsValue, name: &str) -> Result<(), JsValue> {
    let del_fn: js_sys::Function =
        js_sys::Reflect::get(db, &JsValue::from_str("deleteObjectStore"))
            .map_err(|e| JsValue::from_str(&format!("Reflect deleteObjectStore: {e:?}")))?
            .dyn_into().map_err(|_| JsValue::from_str("deleteObjectStore не функция"))?;
    del_fn.call1(db, &JsValue::from_str(name)).map_err(|e| e)?;
    Ok(())
}