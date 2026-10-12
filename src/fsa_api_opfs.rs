//! OPFS: приватная файловая система браузера.
//!
//! Отличие от user-visible FSA (`showSaveFilePicker`):
//! * OPFS доступен **без user gesture** — сразу после
//!   `navigator.storage.getDirectory()`.
//! * Файлы не видны пользователю в проводнике ОС.
//! * Идеален для кэшей, черновиков, рабочих данных.
//!
//! **Sync access handle — только в Worker.** Метод
//! `FileSystemFileHandle::createSyncAccessHandle()` определён
//! только на прототипе внутри Dedicated Worker. В main thread
//! его нет — так устроена спецификация, чтобы синхронные
//! `read`/`write` не блокировали UI.
//!
//! Поэтому проверять доступность через
//! `typeof FileSystemFileHandle.prototype.createSyncAccessHandle`
//! **в main thread бесполезно** — всегда `false`. Единственный
//! надёжный способ — пробный Worker, который делает ту же
//! проверку у себя и отвечает булевым сообщением. Реализовано
//! в `sync_access_available_in_worker()`.
//!
//! **Правило:** при вызове методов через Reflect сначала проверяем,
//! что функция определена (`!func_js.is_undefined()`), иначе
//! получим невнятный `TypeError: x is not a function`.

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{FileSystemDirectoryHandle, FileSystemFileHandle, MessageEvent, Worker};

use crate::fsa_api_directory::get_file_handle;

// ===========================================================================
// OPFS root
// ===========================================================================

/// Получить корневой handle OPFS.
///
/// Работает без user gesture: `navigator.storage.getDirectory()`
/// не требует клика. Если OPFS недоступен — `Err` с внятным
/// сообщением.
pub async fn opfs_root() -> Result<FileSystemDirectoryHandle, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let navigator_js = JsValue::from(window.navigator());

    let storage_js = js_sys::Reflect::get(&navigator_js, &JsValue::from_str("storage"))?;
    if storage_js.is_undefined() {
        return Err(JsValue::from_str("navigator.storage недоступен"));
    }

    let get_dir_js = js_sys::Reflect::get(&storage_js, &JsValue::from_str("getDirectory"))?;
    if get_dir_js.is_undefined() {
        return Err(JsValue::from_str("OPFS: getDirectory недоступен"));
    }

    let get_dir: js_sys::Function = get_dir_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("getDirectory не функция"))?;

    let promise: js_sys::Promise = get_dir
        .call0(&storage_js)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("getDirectory не Promise"))?;

    let value: JsValue = JsFuture::from(promise).await?;
    value
        .dyn_into::<FileSystemDirectoryHandle>()
        .map_err(|_| JsValue::from_str("не FileSystemDirectoryHandle"))
}

// ===========================================================================
// Async-запись/чтение (main thread)
// ===========================================================================

/// Записать текст в файл OPFS через `FileSystemWritableFileStream`.
///
/// `create: true` — идемпотентно: если файл есть, вернётся handle
/// на существующий, содержимое перезапишется.
pub async fn write_text(
    root: &FileSystemDirectoryHandle,
    name: &str,
    text: &str,
) -> Result<FileSystemFileHandle, JsValue> {
    let handle = get_file_handle(root, name, true).await?;
    let stream = crate::fsa_api_file::create_writable(&handle, false).await?;
    crate::fsa_api_writable::write_text(&stream, text).await?;
    crate::fsa_api_writable::close(&stream).await?;
    Ok(handle)
}

/// Записать байты в файл OPFS через `FileSystemWritableFileStream`.
///
/// `create: true` — идемпотентно. Используется в бенчмарке
/// async vs sync I/O.
pub async fn write_bytes(
    root: &FileSystemDirectoryHandle,
    name: &str,
    data: &[u8],
) -> Result<FileSystemFileHandle, JsValue> {
    let handle = get_file_handle(root, name, true).await?;
    let stream = crate::fsa_api_file::create_writable(&handle, false).await?;
    crate::fsa_api_writable::write_bytes(&stream, data).await?;
    crate::fsa_api_writable::close(&stream).await?;
    Ok(handle)
}

/// Прочитать текст из файла OPFS.
pub async fn read_text(
    root: &FileSystemDirectoryHandle,
    name: &str,
) -> Result<String, JsValue> {
    let handle = get_file_handle(root, name, false).await?;
    let file = crate::fsa_api_file::get_file(&handle).await?;
    crate::fsa_api_reader::read_text(&file).await
}

/// Удалить файл из OPFS. Идемпотентно.
pub async fn remove_file(
    root: &FileSystemDirectoryHandle,
    name: &str,
) -> Result<(), JsValue> {
    crate::fsa_api_directory::remove_entry(root, name, false).await
}

// ===========================================================================
// Sync access handle (только Worker)
// ===========================================================================

/// JS-код inline-Worker'а для записи через sync access handle.
/// Живёт строкой, чтобы не тащить отдельный файл и не настраивать
/// сборку.
///
/// Внутри — `createSyncAccessHandle()`, синхронный `write`,
/// `flush`, `close`. Замер времени через `performance.now()`.
const WRITE_WORKER_JS: &str = r#"
    self.onmessage = async (e) => {
        const { name, data } = e.data;
        try {
            const root = await navigator.storage.getDirectory();
            const fileHandle = await root.getFileHandle(name, { create: true });
            const accessHandle = await fileHandle.createSyncAccessHandle();

            const bytes = new Uint8Array(data);
            const t0 = performance.now();
            const written = accessHandle.write(bytes, { at: 0 });
            accessHandle.flush();
            accessHandle.close();
            const t1 = performance.now();

            self.postMessage({ ok: true, written, ms: t1 - t0 });
        } catch (err) {
            self.postMessage({ ok: false, error: String(err) });
        }
    };
"#;

/// Запустить Worker, который через `createSyncAccessHandle` запишет
/// `data` в файл `name` и вернёт время записи в миллисекундах.
///
/// Возвращает `(bytes_written, elapsed_ms)`.
///
/// **Как ждём ответ.** Оборачиваем `onmessage` в `js_sys::Promise`:
/// она резолвится первым сообщением от Worker'а. `JsFuture::from`
/// превращает её в awaitable. Никаких каналов, никаких зависимостей.
pub async fn write_sync_in_worker(
    name: &str,
    data: &[u8],
) -> Result<(usize, f64), JsValue> {
    // 1) Blob из JS-кода → object URL → Worker.
    let blob_parts = js_sys::Array::new();
    blob_parts.push(&JsValue::from_str(WRITE_WORKER_JS));
    let blob = web_sys::Blob::new_with_str_sequence(&blob_parts)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let worker: Worker = Worker::new(&url)?;
    web_sys::Url::revoke_object_url(&url)?;

    // 2) Отправляем имя файла и байты.
    let msg = js_sys::Object::new();
    js_sys::Reflect::set(&msg, &JsValue::from_str("name"), &JsValue::from_str(name))?;
    let u8arr = js_sys::Uint8Array::from(data);
    js_sys::Reflect::set(&msg, &JsValue::from_str("data"), &u8arr.buffer())?;
    worker.post_message(&msg)?;

    // 3) Promise, которая резолвится первым сообщением от Worker'а.
    let worker_ref: &Worker = &worker;
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let onmessage = Closure::once(move |e: MessageEvent| {
            let _ = resolve.call1(&JsValue::UNDEFINED, &e.data());
        });
        worker_ref.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();
    });

    // 4) Ждём сообщение.
    let data_js: JsValue = JsFuture::from(promise).await?;

    // 5) Разбираем ответ.
    let ok: bool = js_sys::Reflect::get(&data_js, &JsValue::from_str("ok"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let result: Result<(usize, f64), JsValue> = if ok {
        let written: usize = js_sys::Reflect::get(&data_js, &JsValue::from_str("written"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as usize;
        let ms: f64 = js_sys::Reflect::get(&data_js, &JsValue::from_str("ms"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        Ok((written, ms))
    } else {
        let err: String = js_sys::Reflect::get(&data_js, &JsValue::from_str("error"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| "unknown worker error".to_string());
        Err(JsValue::from_str(&err))
    };

    worker.terminate();
    result
}

// ===========================================================================
// Проверки доступности
// ===========================================================================

/// Проверка: доступен ли `createSyncAccessHandle` **в Worker**.
///
/// **Почему именно пробный Worker.** Метод `createSyncAccessHandle`
/// определён только на прототипе `FileSystemFileHandle` внутри
/// Dedicated Worker — так устроена спецификация, чтобы синхронные
/// `read`/`write` не блокировали main thread. В main thread его
/// нет, поэтому обычная синхронная проверка всегда вернёт `false`.
///
/// Создаём пустой Worker, он проверяет у себя наличие метода
/// и отвечает булевым сообщением. Это единственный надёжный
/// способ узнать, поддерживается ли sync access handle движком.
pub async fn sync_access_available_in_worker() -> bool {
    let probe_js: &str = r#"
        const ok = typeof FileSystemFileHandle !== 'undefined'
            && FileSystemFileHandle.prototype
            && typeof FileSystemFileHandle.prototype.createSyncAccessHandle === 'function';
        self.postMessage(ok);
    "#;

    let blob_parts = js_sys::Array::new();
    blob_parts.push(&JsValue::from_str(probe_js));
    let blob = match web_sys::Blob::new_with_str_sequence(&blob_parts) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let url = match web_sys::Url::create_object_url_with_blob(&blob) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let worker: Worker = match Worker::new(&url) {
        Ok(w) => w,
        Err(_) => {
            web_sys::Url::revoke_object_url(&url).ok();
            return false;
        }
    };
    web_sys::Url::revoke_object_url(&url).ok();

    // Promise, которая резолвится первым сообщением от Worker'а.
    let worker_ref: &Worker = &worker;
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let onmessage = Closure::once(move |e: MessageEvent| {
            let _ = resolve.call1(&JsValue::UNDEFINED, &e.data());
        });
        worker_ref.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();
    });

    let result = JsFuture::from(promise).await;
    worker.terminate();

    match result {
        Ok(v) => v.as_bool().unwrap_or(false),
        Err(_) => false,
    }
}

/// Проверка: доступен ли `navigator.storage.getDirectory`.
pub fn opfs_available() -> bool {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };
    let navigator_js = JsValue::from(window.navigator());
    let storage_js = js_sys::Reflect::get(&navigator_js, &JsValue::from_str("storage")).ok();
    match storage_js {
        Some(s) if !s.is_undefined() => {
            js_sys::Reflect::has(&s, &JsValue::from_str("getDirectory")).unwrap_or(false)
        }
        _ => false,
    }
}

// ===========================================================================
// Утилиты чтения метаданных
// ===========================================================================

/// Размер файла в байтах. `NotFoundError` → `Ok(0.0)`.
pub async fn file_size(
    root: &FileSystemDirectoryHandle,
    name: &str,
) -> Result<f64, JsValue> {
    match get_file_handle(root, name, false).await {
        Ok(handle) => {
            let file = crate::fsa_api_file::get_file(&handle).await?;
            Ok(file.size())
        }
        Err(_) => Ok(0.0),
    }
}

/// Прочитать **хвост** файла — не больше `max_bytes` с конца.
/// Полезно для live-превью растущих логов, чтобы не тянуть весь файл.
pub async fn read_tail(
    root: &FileSystemDirectoryHandle,
    name: &str,
    max_bytes: usize,
) -> Result<String, JsValue> {
    let handle = match get_file_handle(root, name, false).await {
        Ok(h) => h,
        Err(_) => return Ok(String::new()),
    };
    let file = crate::fsa_api_file::get_file(&handle).await?;
    let size: f64 = file.size();
    let start: f64 = if size > max_bytes as f64 {
        size - max_bytes as f64
    } else {
        0.0
    };
    let blob: web_sys::Blob = file.slice_with_f64(start)?;

    // `Blob::text()` — не во всех версиях web-sys. Идём через Reflect.
    let blob_js: &JsValue = blob.as_ref();
    let text_fn_js = js_sys::Reflect::get(blob_js, &JsValue::from_str("text"))?;
    if text_fn_js.is_undefined() {
        return Err(JsValue::from_str("Blob.text недоступен"));
    }
    let text_fn: js_sys::Function = text_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("text не функция"))?;
    let promise: js_sys::Promise = text_fn
        .call0(blob_js)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("text не Promise"))?;
    let value: JsValue = JsFuture::from(promise).await?;
    value
        .as_string()
        .ok_or_else(|| JsValue::from_str("Blob.text вернул не строку"))
}

/// Список файлов в корне OPFS с размерами.
pub async fn list_files(
    root: &FileSystemDirectoryHandle,
) -> Result<Vec<(String, f64)>, JsValue> {
    let dir_js: &JsValue = root.as_ref();
    let values_fn_js = js_sys::Reflect::get(dir_js, &JsValue::from_str("values"))?;
    if values_fn_js.is_undefined() {
        return Ok(Vec::new());
    }
    let values_fn: js_sys::Function = values_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("values не функция"))?;
    let iter: JsValue = values_fn.call0(dir_js)?;

    let mut out: Vec<(String, f64)> = Vec::new();
    loop {
        let entry = match crate::fsa_api_directory::iter_next(&iter).await? {
            Some(v) => v,
            None => break,
        };
        let kind: String = js_sys::Reflect::get(&entry, &JsValue::from_str("kind"))?
            .as_string()
            .unwrap_or_default();
        let name: String = js_sys::Reflect::get(&entry, &JsValue::from_str("name"))?
            .as_string()
            .unwrap_or_default();
        if kind == "file" {
            let fh: FileSystemFileHandle = match entry.dyn_into() {
                Ok(h) => h,
                Err(_) => continue,
            };
            let file = crate::fsa_api_file::get_file(&fh).await?;
            out.push((name, file.size()));
        }
    }
    Ok(out)
}

// ===========================================================================
// Живой логгер в Worker'е
// ===========================================================================

/// JS-код Worker'а: открывает `createSyncAccessHandle`, ищет конец
/// файла и бесконечно дописывает туда по строке раз в `intervalMs`.
///
/// `flush()` после каждой строки — данные гарантированно попадают
/// на диск. Даже если main thread вызовет `worker.terminate()`,
/// всё записанное уже сохранено.
const LOGGER_WORKER_JS: &str = r#"
    self.onmessage = async (e) => {
        const { name, intervalMs } = e.data;
        try {
            const root = await navigator.storage.getDirectory();
            const fileHandle = await root.getFileHandle(name, { create: true });
            const access = await fileHandle.createSyncAccessHandle();

            let offset = access.getSize();   // append, не перезапись
            let counter = 0;
            const enc = new TextEncoder();

            const tick = () => {
                const now = new Date();
                const hh = String(now.getHours()).padStart(2, "0");
                const mm = String(now.getMinutes()).padStart(2, "0");
                const ss = String(now.getSeconds()).padStart(2, "0");
                const ms = String(now.getMilliseconds()).padStart(3, "0");
                const line = `[${hh}:${mm}:${ss}.${ms}] line ${counter}\n`;
                const bytes = enc.encode(line);
                access.write(bytes, { at: offset });
                access.flush();
                offset += bytes.length;
                counter++;
            };

            tick();
            setInterval(tick, intervalMs);
        } catch (err) {
            // Молча выходим — main thread увидит, что размер не растёт.
        }
    };
"#;

/// Handle живого логгера. `stop()` завершает Worker; всё, что успело
/// записаться, уже на диске благодаря `flush()` в каждой итерации.
pub struct LoggerHandle {
    worker: Worker,
}

impl LoggerHandle {
    /// Остановить Worker. Идемпотентно: повторный `terminate()` — no-op.
    pub fn stop(&self) {
        self.worker.terminate();
    }
}

/// Запустить Worker-логгер. Возвращается **сразу** — Worker живёт
/// в фоне, main thread продолжает работать. Прогресс читаем через
/// `file_size` / `read_tail`.
pub fn start_logger_worker(
    name: &str,
    interval_ms: u32,
) -> Result<LoggerHandle, JsValue> {
    let blob_parts = js_sys::Array::new();
    blob_parts.push(&JsValue::from_str(LOGGER_WORKER_JS));
    let blob = web_sys::Blob::new_with_str_sequence(&blob_parts)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let worker = Worker::new(&url)?;
    web_sys::Url::revoke_object_url(&url)?;

    let msg = js_sys::Object::new();
    js_sys::Reflect::set(&msg, &JsValue::from_str("name"), &JsValue::from_str(name))?;
    js_sys::Reflect::set(
        &msg,
        &JsValue::from_str("intervalMs"),
        &JsValue::from_f64(interval_ms as f64),
    )?;
    worker.post_message(&msg)?;

    Ok(LoggerHandle { worker })
}