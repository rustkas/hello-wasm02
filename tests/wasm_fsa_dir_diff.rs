//! WASM-тесты `snapshot` и end-to-end цикла diff.
//!
//! OPFS не требует user gesture — работает в headless Chrome.

mod opfs {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::FileSystemDirectoryHandle;

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

    pub async fn cleanup(root: &FileSystemDirectoryHandle, name: &str) {
        let _ = hello_wasm02::fsa_dir_tree::remove_path(root, name, true).await;
    }
}

use hello_wasm02::fsa_dir_diff::{diff, snapshot};
use hello_wasm02::fsa_dir_tree::{ensure_path, remove_path};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

async fn write_file(
    root: &web_sys::FileSystemDirectoryHandle,
    path: &str,
    bytes: &[u8],
) {
    let (parents, name) = match path.rfind('/') {
        Some(i) => (&path[..i], &path[i + 1..]),
        None => ("", path),
    };
    let parent = if parents.is_empty() {
        root.clone()
    } else {
        ensure_path(root, parents).await.expect("ensure parents")
    };
    let fh = hello_wasm02::fsa_api_directory::get_file_handle(&parent, name, true)
        .await
        .expect("create file");
    let stream = hello_wasm02::fsa_api_file::create_writable(&fh, false)
        .await
        .expect("writable");
    hello_wasm02::fsa_api_writable::write_bytes(&stream, bytes)
        .await
        .expect("write");
    hello_wasm02::fsa_api_writable::close(&stream)
        .await
        .expect("close");
}

#[wasm_bindgen_test]
async fn test_snapshot_basic() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let base = "fsa_dir_diff_snap";
    opfs::cleanup(&root, base).await;

    write_file(&root, &format!("{base}/one.txt"), b"1").await;
    write_file(&root, &format!("{base}/two.txt"), b"22").await;

    let dir = hello_wasm02::fsa_dir_tree::open_dir(&root, base)
        .await
        .expect("open");
    let s = snapshot(&dir, 100, |_| {}, || false).await.expect("snapshot");

    assert_eq!(s.len(), 2);
    assert!(s.entries.contains_key("one.txt"));
    assert_eq!(s.entries["two.txt"].size, 2.0);

    opfs::cleanup(&root, base).await;
}

#[wasm_bindgen_test]
async fn test_snapshot_respects_cancel() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let s = snapshot(&root, 100, |_| {}, || true).await.expect("snap");
    assert!(s.is_empty(), "отмена с порога даёт пустой снимок");
}

#[wasm_bindgen_test]
async fn test_end_to_end_diff_cycle() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let base = "fsa_dir_diff_e2e";
    opfs::cleanup(&root, base).await;

    // Стартовое состояние: два файла.
    write_file(&root, &format!("{base}/keep.txt"), b"same").await;
    write_file(&root, &format!("{base}/gone.txt"), b"bye").await;

    let dir = hello_wasm02::fsa_dir_tree::open_dir(&root, base)
        .await
        .expect("open");

    let before = snapshot(&dir, 100, |_| {}, || false).await.expect("before");

    // Меняем состояние:
    // * keep.txt — тот же размер;
    // * gone.txt — удаляем;
    // * fresh.txt — добавляем;
    // * grown.txt — добавляем и сразу дописываем больше байт.
    remove_path(&root, &format!("{base}/gone.txt"), false)
        .await
        .expect("remove gone");
    write_file(&root, &format!("{base}/fresh.txt"), b"new").await;
    write_file(&root, &format!("{base}/grown.txt"), b"12345").await;

    let after = snapshot(&dir, 100, |_| {}, || false).await.expect("after");

    let d = diff(&before, &after);

    assert_eq!(d.removed, vec!["gone.txt".to_string()]);
    assert_eq!(
        d.added,
        vec!["fresh.txt".to_string(), "grown.txt".to_string()]
    );
    assert!(d.size_changed.is_empty(), "размеры не менялись у общих");
    assert_eq!(d.unchanged_count, 1, "keep.txt не изменился");

    opfs::cleanup(&root, base).await;
}