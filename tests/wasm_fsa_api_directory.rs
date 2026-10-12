//! Интеграционные тесты `src/fsa_api_directory.rs` через OPFS.
//!
//! OPFS (`navigator.storage.getDirectory()`) не требует user gesture,
//! поэтому тесты выполняются в headless Chrome без пропусков.
//!
//! Покрытие:
//! * `get_file_handle` — создание и чтение через `create: true/false`.
//! * `get_directory_handle` — создание подпапки и удаление.
//! * `remove_entry` — удаление файла и директории, повторное удаление.
//! * `resolve_path` — путь относительно корня.
//! * `scan` — прогресс, лимит, немедленная отмена.
//!
//! Если OPFS недоступен (старый браузер) — тесты мягко пропускаются.

mod opfs {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::FileSystemDirectoryHandle;

    /// Возвращает корневой handle OPFS через
    /// `navigator.storage.getDirectory()`.
    pub async fn root() -> Result<FileSystemDirectoryHandle, JsValue> {
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
        let promise: js_sys::Promise = get_dir
            .call0(&storage_js)?
            .dyn_into()
            .map_err(|_| JsValue::from_str("getDirectory не Promise"))?;
        let value: JsValue = JsFuture::from(promise).await?;
        value
            .dyn_into::<FileSystemDirectoryHandle>()
            .map_err(|_| JsValue::from_str("не FileSystemDirectoryHandle"))
    }

    /// Удаляет файл или директорию из корня, игнорируя «нет такого».
    pub async fn cleanup(root: &FileSystemDirectoryHandle, name: &str) {
        // В интеграционном тесте `crate::` указывает на тестовый крейт,
        // а не на библиотеку. Используем полный путь.
        let _ = hello_wasm02::fsa_api_directory::remove_entry(root, name, true).await;
    }
}

use hello_wasm02::fsa_api_directory::*;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// get_file_handle / get_directory_handle / remove_entry
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_file_handle_create_and_read() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_api_dir_file.txt";
    opfs::cleanup(&root, name).await;

    // create: true
    let handle = get_file_handle(&root, name, true).await.expect("create");
    assert_eq!(handle.name(), name);

    // Записываем байты и читаем обратно.
    let bytes = b"hello fsa_api_directory";
    let stream = hello_wasm02::fsa_api_file::create_writable(&handle, false)
        .await
        .expect("create_writable");
    hello_wasm02::fsa_api_writable::write_bytes(&stream, bytes)
        .await
        .expect("write");
    hello_wasm02::fsa_api_writable::close(&stream)
        .await
        .expect("close");

    let file = hello_wasm02::fsa_api_file::get_file(&handle)
        .await
        .expect("get_file");
    assert_eq!(file.size() as usize, bytes.len());

    opfs::cleanup(&root, name).await;
}

#[wasm_bindgen_test]
async fn test_file_handle_missing_without_create() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_api_dir_missing_xyz.txt";
    opfs::cleanup(&root, name).await;

    // Без create — NotFoundError.
    let res = get_file_handle(&root, name, false).await;
    assert!(res.is_err(), "ожидалась ошибка для отсутствующего файла");
}

#[wasm_bindgen_test]
async fn test_directory_handle_create_and_remove() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_api_dir_subdir";
    opfs::cleanup(&root, name).await;

    let subdir = get_directory_handle(&root, name, true)
        .await
        .expect("create dir");
    assert_eq!(subdir.name(), name);

    remove_entry(&root, name, true).await.expect("remove");

    // Правило проекта: повторное удаление — идемпотентно, Ok.
    remove_entry(&root, name, true)
        .await
        .expect("повторное удаление должно быть Ok (идемпотентно)");
}

#[wasm_bindgen_test]
async fn test_remove_entry_file() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_api_dir_rm.txt";
    opfs::cleanup(&root, name).await;
    let _ = get_file_handle(&root, name, true).await.expect("create");

    remove_entry(&root, name, false).await.expect("remove");
    assert!(get_file_handle(&root, name, false).await.is_err());
}

// ---------------------------------------------------------------------------
// resolve_path
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_resolve_path_returns_relative() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_api_dir_resolve.txt";
    opfs::cleanup(&root, name).await;

    let file_handle = get_file_handle(&root, name, true).await.expect("create");
    let child_js: &wasm_bindgen::JsValue = file_handle.as_ref();

    let path = resolve_path(&root, child_js).await.expect("resolve");
    assert_eq!(path.as_deref(), Some(name));

    opfs::cleanup(&root, name).await;
}

// ---------------------------------------------------------------------------
// scan — прогресс, лимит, отмена
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_scan_reports_progress() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let dir_name = "fsa_api_dir_scan";
    opfs::cleanup(&root, dir_name).await;
    let subdir = get_directory_handle(&root, dir_name, true)
        .await
        .expect("mkdir");

    // Три файла.
    for name in ["a.txt", "b.txt", "c.txt"] {
        let _ = get_file_handle(&subdir, name, true).await.expect("create");
    }

    let hits = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let hits_c = hits.clone();
    let on_progress = move |_n: usize| hits_c.set(hits_c.get() + 1);

    let entries = scan(&subdir, 100, on_progress, || false)
        .await
        .expect("scan");

    assert_eq!(entries.len(), 3, "должно быть найдено 3 файла");
    // on_progress вызывается после каждого файла + возможные промежуточные.
    assert!(
        hits.get() >= 3,
        "on_progress должен вызываться не менее 3 раз, получено {}",
        hits.get()
    );

    opfs::cleanup(&root, dir_name).await;
}

#[wasm_bindgen_test]
async fn test_scan_respects_limit() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let dir_name = "fsa_api_dir_limit";
    opfs::cleanup(&root, dir_name).await;
    let subdir = get_directory_handle(&root, dir_name, true)
        .await
        .expect("mkdir");

    for i in 0..5 {
        let _ = get_file_handle(&subdir, &format!("f{i}.txt"), true)
            .await
            .expect("create");
    }

    let entries = scan(&subdir, 2, |_| {}, || false).await.expect("scan");
    assert_eq!(
        entries.len(),
        2,
        "лимит должен остановить обход на 2 файлах"
    );

    opfs::cleanup(&root, dir_name).await;
}

#[wasm_bindgen_test]
async fn test_scan_cancels_immediately() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let entries = scan(&root, 100, |_| {}, || true).await.expect("scan");
    assert!(entries.is_empty(), "отмена с порога даёт пустой результат");
}

// ---------------------------------------------------------------------------
// Идемпотентность удаления (правило проекта)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_remove_entry_idempotent_file() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_api_dir_idem.txt";
    opfs::cleanup(&root, name).await;
    let _ = get_file_handle(&root, name, true).await.expect("create");

    remove_entry(&root, name, false).await.expect("first remove");
    remove_entry(&root, name, false).await.expect("second remove (idempotent)");
    remove_entry(&root, name, false).await.expect("third remove (idempotent)");
}

#[wasm_bindgen_test]
async fn test_remove_entry_idempotent_directory() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_api_dir_idem_dir";
    opfs::cleanup(&root, name).await;
    let _ = get_directory_handle(&root, name, true).await.expect("mkdir");

    remove_entry(&root, name, true).await.expect("first remove");
    remove_entry(&root, name, true).await.expect("second remove");
    remove_entry(&root, name, true).await.expect("third remove");
}

#[wasm_bindgen_test]
async fn test_remove_entry_missing_from_start() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_api_dir_never_existed";
    opfs::cleanup(&root, name).await;

    // Записи изначально нет — удаление всё равно Ok.
    remove_entry(&root, name, true)
        .await
        .expect("удаление отсутствующей записи — Ok (идемпотентно)");
}