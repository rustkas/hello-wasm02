//! Тесты `src/fsa_lifecycle.rs` через OPFS.

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
}

use hello_wasm02::fsa_lifecycle::*;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// describe
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn describe_non_handle_returns_unknown() {
    let v = wasm_bindgen::JsValue::from_str("not a handle");
    let info = describe(&v);
    assert_eq!(info.kind, HandleKind::Unknown);
    assert_eq!(info.name, "");
}

#[wasm_bindgen_test]
async fn describe_opfs_file_handle() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_lifecycle_describe.txt";
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, name, true).await;
    let handle = hello_wasm02::fsa_api_directory::get_file_handle(&root, name, true)
        .await
        .expect("create");

    let handle_js: &wasm_bindgen::JsValue = handle.as_ref();
    let info = describe(handle_js);
    assert_eq!(info.kind, HandleKind::File);
    assert_eq!(info.name, name);

    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, name, true).await;
}

// ---------------------------------------------------------------------------
// take_snapshot
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn take_snapshot_returns_metadata() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_lifecycle_snapshot.bin";
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, name, true).await;
    let handle = hello_wasm02::fsa_api_directory::get_file_handle(&root, name, true)
        .await
        .expect("create");

    let bytes = b"hello lifecycle";
    let stream = hello_wasm02::fsa_api_file::create_writable(&handle, false)
        .await
        .expect("create_writable");
    hello_wasm02::fsa_api_writable::write_bytes(&stream, bytes)
        .await
        .expect("write");
    hello_wasm02::fsa_api_writable::close(&stream)
        .await
        .expect("close");

    let snap = take_snapshot(&handle).await.expect("snapshot");
    assert_eq!(snap.name, name);
    assert_eq!(snap.size as usize, bytes.len());
    assert!(snap.last_modified_ms > 0.0);

    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, name, true).await;
}

// ---------------------------------------------------------------------------
// same_target
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn same_target_two_handles_on_one_file() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let name = "fsa_lifecycle_identity.txt";
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, name, true).await;
    let h1 = hello_wasm02::fsa_api_directory::get_file_handle(&root, name, true)
        .await
        .expect("create h1");
    let h2 = hello_wasm02::fsa_api_directory::get_file_handle(&root, name, false)
        .await
        .expect("open h2");

    let h1_js: &wasm_bindgen::JsValue = h1.as_ref();
    let h2_js: &wasm_bindgen::JsValue = h2.as_ref();
    assert!(same_target(h1_js, h2_js).await.expect("same_target"));

    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, name, true).await;
}

#[wasm_bindgen_test]
async fn same_target_two_different_files() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let a = "fsa_lifecycle_a.txt";
    let b = "fsa_lifecycle_b.txt";
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, a, true).await;
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, b, true).await;
    let ha = hello_wasm02::fsa_api_directory::get_file_handle(&root, a, true)
        .await
        .expect("create a");
    let hb = hello_wasm02::fsa_api_directory::get_file_handle(&root, b, true)
        .await
        .expect("create b");

    let ha_js: &wasm_bindgen::JsValue = ha.as_ref();
    let hb_js: &wasm_bindgen::JsValue = hb.as_ref();
    assert!(!same_target(ha_js, hb_js).await.expect("same_target"));

    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, a, true).await;
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, b, true).await;
}

// ---------------------------------------------------------------------------
// is_snapshot_stale
// ---------------------------------------------------------------------------

fn snap(
    name: &str,
    size: f64,
    mime: &str,
    last_modified_ms: f64,
) -> SnapshotInfo {
    SnapshotInfo {
        name: name.to_string(),
        size,
        mime: mime.to_string(),
        last_modified_ms,
    }
}

#[wasm_bindgen_test]
fn is_snapshot_stale_same_data_not_stale() {
    let a = snap("a", 10.0, "text/plain", 1000.0);
    let b = snap("a", 10.0, "text/plain", 1000.0);
    assert!(!is_snapshot_stale(&a, &b));
}

#[wasm_bindgen_test]
fn is_snapshot_stale_size_changed() {
    let a = snap("a", 10.0, "text/plain", 1000.0);
    let b = snap("a", 20.0, "text/plain", 1000.0);
    assert!(is_snapshot_stale(&a, &b));
}

#[wasm_bindgen_test]
fn is_snapshot_stale_modified_changed() {
    let a = snap("a", 10.0, "text/plain", 1000.0);
    let b = snap("a", 10.0, "text/plain", 2000.0);
    assert!(is_snapshot_stale(&a, &b));
}

// ---------------------------------------------------------------------------
// format_size
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn format_size_units() {
    assert_eq!(format_size(0.0), "0 Б");
    assert_eq!(format_size(512.0), "512 Б");
    assert_eq!(format_size(2048.0), "2.0 КБ");
    assert_eq!(format_size(5.0 * 1024.0 * 1024.0), "5.00 МБ");
    assert_eq!(format_size(3.0 * 1024.0 * 1024.0 * 1024.0), "3.00 ГБ");
}

// ---------------------------------------------------------------------------
// format_identity_pair
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn format_identity_pair_same() {
    let s = format_identity_pair("a.txt", "a.txt", true);
    assert!(s.contains("Первый: a.txt"));
    assert!(s.contains("Второй: a.txt"));
    assert!(s.contains("✅"));
}

#[wasm_bindgen_test]
fn format_identity_pair_different() {
    let s = format_identity_pair("a.txt", "b.txt", false);
    assert!(s.contains("❌"));
}

// ---------------------------------------------------------------------------
// format_snapshot
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn format_snapshot_with_counter_shows_number() {
    let s = snap("a.bin", 1024.0, "application/octet-stream", 12345.0);
    let text = format_snapshot(&s, None, 3);
    assert!(text.contains("# снимок №3"));
    assert!(text.contains("name:         a.bin"));
    assert!(text.contains("size:         1.0 КБ"));
    assert!(text.contains("mime:         application/octet-stream"));
}

#[wasm_bindgen_test]
fn format_snapshot_without_counter_uses_auto_label() {
    let s = snap("a.bin", 100.0, "text/plain", 1.0);
    let text = format_snapshot(&s, None, 0);
    assert!(text.contains("# автообновление"));
}

#[wasm_bindgen_test]
fn format_snapshot_empty_mime_uses_dash() {
    let s = snap("a.bin", 100.0, "", 1.0);
    let text = format_snapshot(&s, None, 1);
    assert!(text.contains("mime:         —"));
}

#[wasm_bindgen_test]
fn format_snapshot_stale_warning() {
    let s = snap("a.bin", 100.0, "text/plain", 1.0);
    let text = format_snapshot(&s, Some(true), 1);
    assert!(text.contains("⚠️"));
}

#[wasm_bindgen_test]
fn format_snapshot_fresh_note() {
    let s = snap("a.bin", 100.0, "text/plain", 1.0);
    let text = format_snapshot(&s, Some(false), 1);
    assert!(text.contains("✅ Совпадает"));
}

// ---------------------------------------------------------------------------
// format_modification_payload
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn format_modification_payload_nonempty() {
    let p = format_modification_payload(1234.5);
    assert!(p.starts_with("modified at "));
    assert!(p.ends_with("\n"));
    assert!(p.contains("1234.5"));
}

// ---------------------------------------------------------------------------
// explain_error
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn explain_error_security() {
    let e = wasm_bindgen::JsValue::from_str("SecurityError: Must be handling a user gesture");
    let s = explain_error(&e);
    assert!(s.contains("жест пользователя"));
}

#[wasm_bindgen_test]
fn explain_error_abort() {
    let e = wasm_bindgen::JsValue::from_str("AbortError: user cancelled");
    let s = explain_error(&e);
    assert_eq!(s, "Диалог закрыт.");
}

#[wasm_bindgen_test]
fn explain_error_other() {
    let e = wasm_bindgen::JsValue::from_str("Some random failure");
    let s = explain_error(&e);
    assert!(s.starts_with("Ошибка FSA:"));
}