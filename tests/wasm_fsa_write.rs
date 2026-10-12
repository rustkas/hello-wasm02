//! Тесты `fsa_write_*` — транзакция, seek, truncate, keepExistingData,
//! машина состояний, контраст «naive vs seek».
//!
//! Работаем с OPFS: не требует user gesture, доступен в headless Chrome.

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

use hello_wasm02::fsa_write_ops::{
    describe_content, format_bytes, read_bytes_now, read_size_now, read_text_now,
    write_naive_rewrite, WriteSession,
};
use hello_wasm02::fsa_write_session::{SessionState, WriteError};
use wasm_bindgen_test::*;
use web_sys::FileSystemDirectoryHandle;

wasm_bindgen_test_configure!(run_in_browser);

async fn prepare_file(
    root: &FileSystemDirectoryHandle,
    name: &str,
    initial: &[u8],
) -> web_sys::FileSystemFileHandle {
    let _ = hello_wasm02::fsa_api_directory::remove_entry(root, name, true).await;
    let handle = hello_wasm02::fsa_api_directory::get_file_handle(root, name, true)
        .await
        .expect("create");
    let mut s = WriteSession::open(&handle, false).await.expect("open");
    if !initial.is_empty() {
        s.write(initial).await.expect("write");
    }
    s.commit().await.expect("commit");
    return handle;
}

// ---------------------------------------------------------------------------
// Базовое: write → commit → read
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn write_then_commit_persists() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_a.txt", b"").await;

    let mut s = WriteSession::open(&handle, false).await.expect("open");
    assert_eq!(s.state(), SessionState::Open);
    s.write(b"Hello, transaction!").await.expect("write");
    let stats = s.commit().await.expect("commit");
    assert_eq!(stats.writes, 1);
    assert_eq!(stats.bytes_written, 19);
    assert_eq!(s.state(), SessionState::Committed);

    let text = read_text_now(&handle).await.expect("read");
    assert_eq!(text, "Hello, transaction!");
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_a.txt", true).await;
}

// ---------------------------------------------------------------------------
// Без commit файл не меняется
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn write_without_commit_does_not_persist() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_b.txt", b"original").await;

    {
        let mut s = WriteSession::open(&handle, false).await.expect("open");
        s.write(b"new stuff").await.expect("write");
        // НЕ коммитим — выходим из scope, Drop вызовет abort.
    }

    let text = read_text_now(&handle).await.expect("read");
    assert_eq!(text, "original", "без commit файл не должен меняться");
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_b.txt", true).await;
}

// ---------------------------------------------------------------------------
// abort
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn abort_discards_changes() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_c.txt", b"old").await;

    let mut s = WriteSession::open(&handle, false).await.expect("open");
    s.write(b"new").await.expect("write");
    s.abort().await.expect("abort");
    assert_eq!(s.state(), SessionState::Aborted);

    let text = read_text_now(&handle).await.expect("read");
    assert_eq!(text, "old", "abort откатывает изменения");
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_c.txt", true).await;
}

// ---------------------------------------------------------------------------
// seek
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn seek_patches_at_offset() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_d.txt", b"ABCDEFGHIJ").await;

    let mut s = WriteSession::open(&handle, true).await.expect("open");
    s.seek(5).await.expect("seek");
    s.write(b"XXX").await.expect("write");
    s.commit().await.expect("commit");

        let text = read_text_now(&handle).await.expect("read");
    // Исходно: A B C D E F G H I J (10 байт).
    // seek(5) + write("XXX") заменяет байты 5,6,7: F,G,H → X,X,X.
    // Байты 8 (I) и 9 (J) не трогаются.
    assert_eq!(text, "ABCDEXXXIJ");
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_d.txt", true).await;
}

// ---------------------------------------------------------------------------
// truncate
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn truncate_shrinks_file() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_e.txt", b"ABCDEFGHIJ").await;

    let mut s = WriteSession::open(&handle, true).await.expect("open");
    s.truncate(4).await.expect("truncate");
    s.commit().await.expect("commit");

    let bytes = read_bytes_now(&handle).await.expect("read");
    assert_eq!(bytes.len(), 4);
    assert_eq!(&bytes[..], b"ABCD");
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_e.txt", true).await;
}

// ---------------------------------------------------------------------------
// keepExistingData
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn keep_existing_preserves_tail() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_f.txt", b"ABCDEFGHIJ").await;

    let mut s = WriteSession::open(&handle, true).await.expect("open");
    s.write(b"XY").await.expect("write");
    s.commit().await.expect("commit");

    let text = read_text_now(&handle).await.expect("read");
    assert_eq!(text, "XYCDEFGHIJ");
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_f.txt", true).await;
}

#[wasm_bindgen_test]
async fn without_keep_existing_truncates() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_g.txt", b"ABCDEFGHIJ").await;

    let mut s = WriteSession::open(&handle, false).await.expect("open");
    s.write(b"XY").await.expect("write");
    s.commit().await.expect("commit");

    let text = read_text_now(&handle).await.expect("read");
    assert_eq!(text, "XY", "без keep файл усечён при открытии");
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_g.txt", true).await;
}

// ---------------------------------------------------------------------------
// Машина состояний
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn commit_is_idempotent() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_h.txt", b"").await;

    let mut s = WriteSession::open(&handle, false).await.expect("open");
    s.write(b"hi").await.expect("write");
    let a = s.commit().await.expect("commit 1");
    let b = s.commit().await.expect("commit 2");
    assert_eq!(a, b, "повторный commit возвращает те же stats");
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_h.txt", true).await;
}

#[wasm_bindgen_test]
async fn write_after_commit_errors() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_i.txt", b"").await;

    let mut s = WriteSession::open(&handle, false).await.expect("open");
    s.write(b"x").await.expect("write");
    s.commit().await.expect("commit");
    let res = s.write(b"y").await;
    assert!(matches!(res, Err(WriteError::SessionClosed)));
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_i.txt", true).await;
}

#[wasm_bindgen_test]
async fn write_after_abort_errors() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_j.txt", b"").await;

    let mut s = WriteSession::open(&handle, false).await.expect("open");
    s.write(b"x").await.expect("write");
    s.abort().await.expect("abort");
    let res = s.write(b"y").await;
    assert!(matches!(res, Err(WriteError::SessionClosed)));
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_j.txt", true).await;
}

#[wasm_bindgen_test]
async fn abort_is_idempotent() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_k.txt", b"").await;

    let mut s = WriteSession::open(&handle, false).await.expect("open");
    s.write(b"x").await.expect("write");
    s.abort().await.expect("abort 1");
    s.abort().await.expect("abort 2");
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_k.txt", true).await;
}

#[wasm_bindgen_test]
async fn commit_after_abort_errors() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_l.txt", b"").await;

    let mut s = WriteSession::open(&handle, false).await.expect("open");
    s.write(b"x").await.expect("write");
    s.abort().await.expect("abort");
    let res = s.commit().await;
    assert!(matches!(res, Err(WriteError::SessionClosed)));
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_l.txt", true).await;
}

// ---------------------------------------------------------------------------
// UTF-8 с эмодзи
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn utf8_emoji_roundtrip() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_m.txt", b"").await;

    let text = "Привет, 🚀 world!";
    let mut s = WriteSession::open(&handle, false).await.expect("open");
    s.write(text.as_bytes()).await.expect("write");
    s.commit().await.expect("commit");

    let back = read_text_now(&handle).await.expect("read");
    assert_eq!(back, text);
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_m.txt", true).await;
}

// ---------------------------------------------------------------------------
// Несколько write до commit
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn multiple_writes_before_commit() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_n.txt", b"").await;

    let mut s = WriteSession::open(&handle, false).await.expect("open");
    s.write(b"AAA").await.expect("w1");
    s.write(b"BBB").await.expect("w2");
    s.write(b"CCC").await.expect("w3");
    let stats = s.commit().await.expect("commit");
    assert_eq!(stats.writes, 3);
    assert_eq!(stats.bytes_written, 9);

    let text = read_text_now(&handle).await.expect("read");
    assert_eq!(text, "AAABBBCCC");
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_n.txt", true).await;
}

// ---------------------------------------------------------------------------
// Контраст: naive vs seek
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn naive_rewrite_reads_and_writes_all() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let data = vec![b'A'; 4096];
    let handle = prepare_file(&root, "w_o.bin", &data).await;

    let stats = write_naive_rewrite(&handle, 1000, b"XYZ").await.expect("naive");
    assert_eq!(stats.bytes_read, 4096, "naive читает весь файл");
    assert_eq!(stats.bytes_written, 4096, "naive пишет весь файл");

    let back = read_bytes_now(&handle).await.expect("read");
    assert_eq!(&back[1000..1003], b"XYZ");
    assert_eq!(back.len(), 4096);
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_o.bin", true).await;
}

#[wasm_bindgen_test]
async fn seek_patch_writes_only_3_bytes() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let data = vec![b'A'; 4096];
    let handle = prepare_file(&root, "w_p.bin", &data).await;

    let mut s = WriteSession::open(&handle, true).await.expect("open");
    s.seek(1000).await.expect("seek");
    s.write(b"XYZ").await.expect("write");
    let stats = s.commit().await.expect("commit");
    assert_eq!(stats.bytes_written, 3, "с seek пишем только 3 байта");

    let back = read_bytes_now(&handle).await.expect("read");
    assert_eq!(&back[1000..1003], b"XYZ");
    assert_eq!(back.len(), 4096);
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_p.bin", true).await;
}

// ---------------------------------------------------------------------------
// read_size_now
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn read_size_matches_content() {
    let root = match opfs::root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle = prepare_file(&root, "w_q.txt", b"12345").await;
    let size = read_size_now(&handle).await.expect("size");
    assert_eq!(size, 5);
    let _ = hello_wasm02::fsa_api_directory::remove_entry(&root, "w_q.txt", true).await;
}

// ---------------------------------------------------------------------------
// describe_content / format_bytes
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn describe_content_empty() {
    assert_eq!(describe_content(""), "(файл пуст)");
}

#[wasm_bindgen_test]
fn describe_content_short() {
    let s = describe_content("hello");
    assert!(s.contains("5 симв."));
    assert!(s.contains("hello"));
}

#[wasm_bindgen_test]
fn describe_content_truncates_long() {
    let text = "a".repeat(100);
    let s = describe_content(&text);
    assert!(s.contains("100 симв."));
    assert!(s.contains("…"));
}

#[wasm_bindgen_test]
fn format_bytes_units() {
    assert_eq!(format_bytes(0), "0 Б");
    assert_eq!(format_bytes(512), "512 Б");
    assert_eq!(format_bytes(2048), "2.0 КБ");
}