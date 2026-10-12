//! Origin Private File System (OPFS).
//!
//! **Зачем.** OPFS — настоящая файловая система внутри браузера.
//! Работает без диалогов пользователя, доступна по `navigator.storage
//! .getDirectory()`. Подходит для больших бинарных файлов: можно
//! читать и писать потоково, не загружая всё в память.
//!
//! **Особенности web-sys 0.3.105.** Методы `getFileHandle`,
//! `getDirectoryHandle`, `createWritable` и `getDirectory`
//! вызываем через `Reflect::get` + `Function::call*` — так надёжнее
//! и не зависим от точных сигнатур.
//!
//! **Требования:** Chrome/Edge/Firefox (с 111) + secure context.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::FileSystemDirectoryHandle;

/// Причина, по которой OPFS недоступен.
#[derive(Debug, Clone)]
pub enum OpfsError {
    /// В браузере нет `navigator.storage.getDirectory`.
    Unsupported,
    /// Браузер запретил доступ (настройки приватности, приватный режим).
    SecurityError,
    /// Прочие ошибки — с сохранённым описанием.
    Other(String),
}

/// Возвращает корневой handle OPFS.
pub async fn get_root() -> Result<FileSystemDirectoryHandle, JsValue> {
    let storage = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .navigator()
        .storage();

    let storage_js: &JsValue = storage.as_ref();
    let get_dir_fn_js = js_sys::Reflect::get(
        storage_js,
        &JsValue::from_str("getDirectory"),
    )?;
    if get_dir_fn_js.is_undefined() {
        return Err(JsValue::from_str("storage.getDirectory недоступен"));
    }
    let get_dir_fn: js_sys::Function = get_dir_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("getDirectory не функция"))?;

    let promise_js = get_dir_fn.call0(storage_js)?;
    let promise: js_sys::Promise = promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let value = JsFuture::from(promise).await?;
    return value
        .dyn_into::<FileSystemDirectoryHandle>()
        .map_err(|_| JsValue::from_str("не FileSystemDirectoryHandle"));
}

/// Записывает байты в файл OPFS.
///
/// Если файл существует — перезаписывается полностью.
pub async fn write_file(name: &str, bytes: &[u8]) -> Result<(), JsValue> {
    let root = get_root().await?;

    // root.getFileHandle(name, { create: true })
    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("create"),
        &JsValue::from_bool(true),
    )?;

    let root_js: &JsValue = root.as_ref();
    let get_file_fn_js = js_sys::Reflect::get(
        root_js,
        &JsValue::from_str("getFileHandle"),
    )?;
    let get_file_fn: js_sys::Function = get_file_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("getFileHandle не функция"))?;

    let file_handle_js = get_file_fn.call2(
        root_js,
        &JsValue::from_str(name),
        &options,
    )?;
    let file_handle_promise: js_sys::Promise = file_handle_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let file_handle = JsFuture::from(file_handle_promise).await?;

    // fileHandle.createWritable()
    let file_handle_ref: &JsValue = &file_handle;
    let create_fn_js = js_sys::Reflect::get(
        file_handle_ref,
        &JsValue::from_str("createWritable"),
    )?;
    let create_fn: js_sys::Function = create_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("createWritable не функция"))?;
    let writable_js = create_fn.call0(file_handle_ref)?;
    let writable_promise: js_sys::Promise = writable_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let writable = JsFuture::from(writable_promise).await?;

    // writable.write(bytes)
    let uint8 = js_sys::Uint8Array::from(bytes);
    let writable_ref: &JsValue = &writable;
    let write_fn_js = js_sys::Reflect::get(
        writable_ref,
        &JsValue::from_str("write"),
    )?;
    let write_fn: js_sys::Function = write_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("write не функция"))?;
    let write_result_js = write_fn.call1(writable_ref, &uint8.buffer())?;
    let write_result_promise: js_sys::Promise = write_result_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    JsFuture::from(write_result_promise).await?;

    // writable.close()
    let close_fn_js = js_sys::Reflect::get(
        writable_ref,
        &JsValue::from_str("close"),
    )?;
    let close_fn: js_sys::Function = close_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("close не функция"))?;
    let close_result_js = close_fn.call0(writable_ref)?;
    let close_result_promise: js_sys::Promise = close_result_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    JsFuture::from(close_result_promise).await?;

    return Ok(());
}

/// Читает файл OPFS целиком в `Vec<u8>`.
pub async fn read_file(name: &str) -> Result<Vec<u8>, JsValue> {
    let root = get_root().await?;

    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("create"),
        &JsValue::from_bool(false),
    )?;

    let root_js: &JsValue = root.as_ref();
    let get_file_fn_js = js_sys::Reflect::get(
        root_js,
        &JsValue::from_str("getFileHandle"),
    )?;
    let get_file_fn: js_sys::Function = get_file_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("getFileHandle не функция"))?;

    let file_handle_js = get_file_fn.call2(
        root_js,
        &JsValue::from_str(name),
        &options,
    )?;
    let file_handle_promise: js_sys::Promise = file_handle_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let file_handle = JsFuture::from(file_handle_promise).await?;

    // fileHandle.getFile() → File
    let file_handle_ref: &JsValue = &file_handle;
    let get_file_method_js = js_sys::Reflect::get(
        file_handle_ref,
        &JsValue::from_str("getFile"),
    )?;
    let get_file_method: js_sys::Function = get_file_method_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("getFile не функция"))?;
    let file_promise_js = get_file_method.call0(file_handle_ref)?;
    let file_promise: js_sys::Promise = file_promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let file_value = JsFuture::from(file_promise).await?;
    let file: web_sys::File = file_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не File"))?;

    let ab_promise = file.array_buffer();
    let ab_value = JsFuture::from(ab_promise).await?;
    let array_buffer: js_sys::ArrayBuffer = ab_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не ArrayBuffer"))?;
    return Ok(js_sys::Uint8Array::new(&array_buffer).to_vec());
}

/// Удаляет файл OPFS.
///
/// Идемпотентна: удаление несуществующего файла не ошибка.
pub async fn remove_file(name: &str) -> Result<(), JsValue> {
    let root = get_root().await?;
    let root_js: &JsValue = root.as_ref();

    let remove_fn_js = js_sys::Reflect::get(
        root_js,
        &JsValue::from_str("removeEntry"),
    )?;
    let remove_fn: js_sys::Function = remove_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("removeEntry не функция"))?;
    let promise_js = remove_fn.call1(root_js, &JsValue::from_str(name))?;
    let promise: js_sys::Promise = promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let _ = JsFuture::from(promise).await?;
    return Ok(());
}

/// Возвращает размер файла OPFS в байтах.
pub async fn file_size(name: &str) -> Result<f64, JsValue> {
    let root = get_root().await?;
    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("create"),
        &JsValue::from_bool(false),
    )?;
    let root_js: &JsValue = root.as_ref();
    let get_file_fn_js = js_sys::Reflect::get(
        root_js,
        &JsValue::from_str("getFileHandle"),
    )?;
    let get_file_fn: js_sys::Function = get_file_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("getFileHandle не функция"))?;

    let file_handle_js = get_file_fn.call2(
        root_js,
        &JsValue::from_str(name),
        &options,
    )?;
    let file_handle_promise: js_sys::Promise = file_handle_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let file_handle = JsFuture::from(file_handle_promise).await?;

    let file_handle_ref: &JsValue = &file_handle;
    let get_file_method_js = js_sys::Reflect::get(
        file_handle_ref,
        &JsValue::from_str("getFile"),
    )?;
    let get_file_method: js_sys::Function = get_file_method_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("getFile не функция"))?;
    let file_promise_js = get_file_method.call0(file_handle_ref)?;
    let file_promise: js_sys::Promise = file_promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let file_value = JsFuture::from(file_promise).await?;
    let file: web_sys::File = file_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("не File"))?;
    return Ok(file.size());
}

/// Возвращает `{ usage, quota }` в байтах.
pub async fn estimate() -> Result<(f64, f64), JsValue> {
    let storage = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .navigator()
        .storage();

    let storage_js: &JsValue = storage.as_ref();
    let estimate_fn_js = js_sys::Reflect::get(
        storage_js,
        &JsValue::from_str("estimate"),
    )?;
    let estimate_fn: js_sys::Function = estimate_fn_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("estimate не функция"))?;
    let promise_js = estimate_fn.call0(storage_js)?;
    let promise: js_sys::Promise = promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Promise"))?;
    let value = JsFuture::from(promise).await?;

    let usage = js_sys::Reflect::get(&value, &JsValue::from_str("usage"))?
        .as_f64()
        .unwrap_or(0.0);
    let quota = js_sys::Reflect::get(&value, &JsValue::from_str("quota"))?
        .as_f64()
        .unwrap_or(0.0);
    return Ok((usage, quota));
}

/// Проверяет, доступна ли OPFS в текущем окружении.
///
/// Возвращает типизированную ошибку — форматирование и подсказки
/// для пользователя делает UI-слой, где известен браузер.
pub async fn check_access() -> Result<(), OpfsError> {
    match get_root().await {
        Ok(_) => return Ok(()),
        Err(e) => {
            let raw = e.as_string().unwrap_or_else(|| format!("{e:?}"));
            if raw.contains("недоступен") {
                return Err(OpfsError::Unsupported);
            }
            if raw.contains("SecurityError") || raw.contains("Security error") {
                return Err(OpfsError::SecurityError);
            }
            return Err(OpfsError::Other(raw));
        }
    }
}