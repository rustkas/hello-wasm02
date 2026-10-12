//! Тесты записи: `src/fsa_api_writer.rs`.
//!
//! Тесты используют **OPFS** (Origin Private File System) через
//! `navigator.storage.getDirectory()`. OPFS не требует user gesture,
//! поэтому работает в headless Chrome.
//!
//! Покрытие:
//! * `write_all_bytes` — round-trip через `get_file` + `read_bytes`.
//! * `write_all_text`  — round-trip через `read_text` (UTF-8, включая кириллицу).
//! * `write_chunked`   — успешный путь с прогрессом до 1.0 на 2 МиБ.
//! * `write_chunked`   — немедленная отмена возвращает `WriteOutcome::Cancelled`.
//!
//! Если OPFS недоступен (старый браузер) — тесты мягко пропускаются.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;
use web_sys::{FileSystemDirectoryHandle, FileSystemFileHandle};

use hello_wasm02::fsa_api_writer::{write_all_bytes, write_all_text, write_chunked, WriteOutcome};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// OPFS helpers — без user gesture.
// ---------------------------------------------------------------------------

/// Возвращает корневой handle OPFS через `navigator.storage.getDirectory()`.
async fn opfs_root() -> Result<FileSystemDirectoryHandle, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let navigator_js = JsValue::from(window.navigator());

    let storage_js = js_sys::Reflect::get(&navigator_js, &JsValue::from_str("storage"))?;
    if storage_js.is_undefined() {
        return Err(JsValue::from_str("navigator.storage недоступен"));
    }
    let get_dir_js = js_sys::Reflect::get(&storage_js, &JsValue::from_str("getDirectory"))?;
    if get_dir_js.is_undefined() {
        return Err(JsValue::from_str("OPFS недоступен"));
    }
    let get_dir: js_sys::Function = get_dir_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("getDirectory не функция"))?;
    let promise_js = get_dir.call0(&storage_js)?;
    let promise: js_sys::Promise = promise_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("getDirectory не Promise"))?;
    let value: JsValue = JsFuture::from(promise).await?;
    value
        .dyn_into::<FileSystemDirectoryHandle>()
        .map_err(|_| JsValue::from_str("не FileSystemDirectoryHandle"))
}

/// Создаёт (или переиспользует) файл в OPFS. Без user gesture.
async fn opfs_file(
    root: &FileSystemDirectoryHandle,
    name: &str,
) -> Result<FileSystemFileHandle, JsValue> {
    hello_wasm02::fsa_api_directory::get_file_handle(root, name, true).await
}

/// Удаляет файл, игнорируя «нет такого файла».
async fn opfs_remove(root: &FileSystemDirectoryHandle, name: &str) {
    let _ = hello_wasm02::fsa_api_directory::remove_entry(root, name, false).await;
}

// ---------------------------------------------------------------------------
// write_all_bytes — успешный путь + round-trip.
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_write_all_bytes_roundtrip() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return, // OPFS недоступен — тест мягко пропускается
    };
    let name = "fsa_api_writer_bytes.bin";
    opfs_remove(&root, name).await;
    let handle = opfs_file(&root, name).await.expect("create file");

    // Все 256 значений байта — ловит ошибки кодировки.
    let data: Vec<u8> = (0..=255).collect();
    let written = write_all_bytes(&handle, &data).await.expect("write");
    assert_eq!(written, name);

    // Читаем обратно через штатные средства и сверяем содержимое.
    let file = hello_wasm02::fsa_api_file::get_file(&handle)
        .await
        .expect("get_file");
    assert_eq!(file.size() as usize, data.len(), "size mismatch");

    let read = hello_wasm02::fsa_api_reader::read_bytes(&file)
        .await
        .expect("read_bytes");
    assert_eq!(read, data, "content mismatch");

    opfs_remove(&root, name).await;
}

// ---------------------------------------------------------------------------
// write_all_text — кириллица + многострочность.
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_write_all_text_roundtrip() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_api_writer_text.txt";
    opfs_remove(&root, name).await;
    let handle = opfs_file(&root, name).await.expect("create file");

    let text = "Hello, FSA writer!\nВторая строка.\n\u{1F600} эмодзи.";
    let written = write_all_text(&handle, text).await.expect("write");
    assert_eq!(written, name);

    let file = hello_wasm02::fsa_api_file::get_file(&handle)
        .await
        .expect("get_file");
    let stored = hello_wasm02::fsa_api_reader::read_text(&file)
        .await
        .expect("read_text");
    assert_eq!(stored, text);

    opfs_remove(&root, name).await;
}

// ---------------------------------------------------------------------------
// write_chunked — успешный путь.
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_write_chunked_progress_reaches_one() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_api_writer_chunked.bin";
    opfs_remove(&root, name).await;
    let handle = opfs_file(&root, name).await.expect("create file");

    // 2 МиБ — гарантированно больше одного чанка
    // (adaptive_chunk_size даёт ≥ 64 КиБ на 200 итераций).
    let data: Vec<u8> = (0..2 * 1024 * 1024).map(|i| (i & 0xFF) as u8).collect();

    let seen: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
    let seen_c = seen.clone();

    let outcome = write_chunked(
        &handle,
        &data,
        move |f| seen_c.borrow_mut().push(f),
        || false,
    )
    .await
    .expect("write_chunked");

    assert!(matches!(outcome, WriteOutcome::Ok));

    // Прогресс монотонный и завершается ровно на 1.0.
    let v = seen.borrow();
    assert!(!v.is_empty(), "progress callback must be called at least once");
    for w in v.windows(2) {
        assert!(w[1] + 1e-9 >= w[0], "progress must be monotonic");
    }
    let last = *v.last().unwrap();
    assert!(
        (last - 1.0).abs() < 1e-6,
        "final progress must be 1.0, got {last}",
    );

    // Файл действительно записан полностью.
    let file = hello_wasm02::fsa_api_file::get_file(&handle)
        .await
        .expect("get_file");
    assert_eq!(file.size() as usize, data.len(), "written size mismatch");

    opfs_remove(&root, name).await;
}

// ---------------------------------------------------------------------------
// write_chunked — отмена.
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_write_chunked_cancel() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_api_writer_cancel.bin";
    opfs_remove(&root, name).await;
    let handle = opfs_file(&root, name).await.expect("create file");

    let data = vec![0xABu8; 512 * 1024];

    // Отмена «с порога»: первая же проверка is_cancelled() в цикле
    // даёт WriteOutcome::Cancelled, ничего не записав.
    let outcome = write_chunked(&handle, &data, |_| {}, || true)
        .await
        .expect("write_chunked");

    assert!(matches!(outcome, WriteOutcome::Cancelled));

    opfs_remove(&root, name).await;
}