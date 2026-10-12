//! Интеграционные тесты `src/fsa_dir_tree.rs` через OPFS.
//!
//! OPFS (`navigator.storage.getDirectory()`) не требует user gesture,
//! поэтому тесты выполняются в headless Chrome без пропусков.

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

use hello_wasm02::fsa_dir_tree::*;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

async fn write_file(
    root: &web_sys::FileSystemDirectoryHandle,
    path: &str,
    bytes: &[u8],
) {
    // Родительская цепочка — идемпотентно.
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

// ---------------------------------------------------------------------------
// ensure_path
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_ensure_path_idempotent() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let base = "fsa_dir_tree_ensure";
    opfs::cleanup(&root, base).await;

    // Три раза подряд создаём одну и ту же цепочку.
    for _ in 0..3 {
        let dir = ensure_path(&root, &format!("{base}/a/b/c"))
            .await
            .expect("ensure idempotent");
        assert_eq!(dir.name(), "c");
    }

    // Родители реально существуют.
    let _ = open_dir(&root, &format!("{base}/a")).await.expect("a");
    let _ = open_dir(&root, &format!("{base}/a/b")).await.expect("a/b");
    let _ = open_dir(&root, &format!("{base}/a/b/c")).await.expect("a/b/c");

    opfs::cleanup(&root, base).await;
}

// ---------------------------------------------------------------------------
// open_dir / find_by_path
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_find_by_path_file_and_dir() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let base = "fsa_dir_tree_find";
    opfs::cleanup(&root, base).await;

    write_file(&root, &format!("{base}/x/y.txt"), b"hi").await;

    let f = find_by_path(&root, &format!("{base}/x/y.txt"))
        .await
        .expect("find file");
    assert!(matches!(f, Found::File(_)), "должен быть файл");

    let d = find_by_path(&root, &format!("{base}/x"))
        .await
        .expect("find dir");
    assert!(matches!(d, Found::Dir(_)), "должна быть директория");

    let missing = find_by_path(&root, &format!("{base}/nope.txt")).await;
    assert!(missing.is_err(), "отсутствующий путь — ошибка");

    opfs::cleanup(&root, base).await;
}

// ---------------------------------------------------------------------------
// build_tree
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_build_tree_nested() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let base = "fsa_dir_tree_build";
    opfs::cleanup(&root, base).await;

    write_file(&root, &format!("{base}/readme.md"), b"# hi").await;
    write_file(&root, &format!("{base}/src/main.rs"), b"fn main(){}").await;
    write_file(&root, &format!("{base}/src/lib.rs"), b"pub fn x(){}").await;
    write_file(&root, &format!("{base}/src/util/io.rs"), b"// io").await;

    let dir = open_dir(&root, base).await.expect("open base");
    let tree = build_tree(&dir, &TreeOptions::default(), |_| {}, || false)
        .await
        .expect("build tree");

    // Ожидаем: readme.md + src/ (main.rs, lib.rs, util/io.rs)
    assert_eq!(tree.total_files(), 4, "должно быть 4 файла");
    assert!(tree.total_bytes() > 0.0);

    // Папки идут первыми.
    let first_is_dir = tree.children.first().map(|n| n.is_dir()).unwrap_or(false);
    assert!(first_is_dir, "первым ребёнком должна быть директория");

    // Рендер — не пустой и содержит имена.
    let text = tree.render();
    assert!(text.contains("src/"));
    assert!(text.contains("readme.md"));
    assert!(text.contains("main.rs"));

    opfs::cleanup(&root, base).await;
}

#[wasm_bindgen_test]
async fn test_build_tree_skips_configured_dirs() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let base = "fsa_dir_tree_skip";
    opfs::cleanup(&root, base).await;

    write_file(&root, &format!("{base}/keep.txt"), b"k").await;
    write_file(&root, &format!("{base}/node_modules/big.js"), b"n").await;
    write_file(&root, &format!("{base}/.git/HEAD"), b"g").await;

    let dir = open_dir(&root, base).await.expect("open base");
    let tree = build_tree(&dir, &TreeOptions::default(), |_| {}, || false)
        .await
        .expect("build tree");

    assert_eq!(tree.total_files(), 1, "только keep.txt");
    let text = tree.render();
    assert!(!text.contains("node_modules"));
    assert!(!text.contains(".git"));

    opfs::cleanup(&root, base).await;
}

#[wasm_bindgen_test]
async fn test_build_tree_respects_max_depth() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let base = "fsa_dir_tree_depth";
    opfs::cleanup(&root, base).await;

    write_file(&root, &format!("{base}/a/b/c/d.txt"), b"x").await;

    let dir = open_dir(&root, base).await.expect("open base");
    let opts = TreeOptions {
        max_depth: 1,
        ..Default::default()
    };
    let tree = build_tree(&dir, &opts, |_| {}, || false)
        .await
        .expect("build tree");

    // На глубине 1 видны только «a» (директория) — файл d.txt глубже.
    assert_eq!(tree.total_files(), 0, "файл слишком глубоко");

    opfs::cleanup(&root, base).await;
}

// ---------------------------------------------------------------------------
// path_of (resolve)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_path_of_uses_resolve() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let base = "fsa_dir_tree_pathof";
    opfs::cleanup(&root, base).await;

    write_file(&root, &format!("{base}/p/q.txt"), b"qq").await;
    let file_handle = find_by_path(&root, &format!("{base}/p/q.txt"))
        .await
        .expect("find");
    let fh = file_handle.as_file().expect("это файл").clone();
    let child_js: &wasm_bindgen::JsValue = fh.as_ref();

    let path = path_of(&root, child_js).await.expect("resolve");
    assert_eq!(path.as_deref(), Some(&*format!("{base}/p/q.txt")));

    opfs::cleanup(&root, base).await;
}

// ---------------------------------------------------------------------------
// remove_path
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn test_remove_path_idempotent() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let base = "fsa_dir_tree_remove";
    opfs::cleanup(&root, base).await;

    write_file(&root, &format!("{base}/dir/file.txt"), b"bye").await;

    // Трижды удаляем один и тот же путь — всё Ok.
    for _ in 0..3 {
        remove_path(&root, &format!("{base}/dir"), true)
            .await
            .expect("remove idempotent");
    }

    // Путь «родителя нет» — тоже Ok.
    remove_path(&root, "never/existed/at/all.txt", false)
        .await
        .expect("missing parent → Ok");

    opfs::cleanup(&root, base).await;
}