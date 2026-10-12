//! Тесты `fsa_grant_*` — модель состояния, preflight, IDB-раундтрип,
//! non-handle приёмники (Reflect не должен падать).

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;
use web_sys::FileSystemDirectoryHandle;

use hello_wasm02::fsa_grant_flow::{self, GrantOutcome};
use hello_wasm02::fsa_grant_state::{
    explain, from_js_error, legend, GrantMode, GrantState,
};
use hello_wasm02::fsa_grant_store;

wasm_bindgen_test_configure!(run_in_browser);

/// Получить OPFS-корень — у него разрешения выданы без gesture.
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
    let promise: js_sys::Promise = get_dir
        .call0(&storage_js)?
        .dyn_into()
        .map_err(|_| JsValue::from_str("getDirectory не Promise"))?;
    let value: JsValue = JsFuture::from(promise).await?;
    value
        .dyn_into::<FileSystemDirectoryHandle>()
        .map_err(|_| JsValue::from_str("не FileSystemDirectoryHandle"))
}

// ---------------------------------------------------------------------------
// GrantState — модель
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn state_from_str_known() {
    assert_eq!(GrantState::from_str("granted"), GrantState::Granted);
    assert_eq!(GrantState::from_str("prompt"), GrantState::Prompt);
    assert_eq!(GrantState::from_str("denied"), GrantState::Denied);
}

#[wasm_bindgen_test]
fn state_from_str_unknown() {
    assert_eq!(GrantState::from_str(""), GrantState::Unknown);
    assert_eq!(GrantState::from_str("garbage"), GrantState::Unknown);
}

#[wasm_bindgen_test]
fn state_roundtrip() {
    for s in [
        GrantState::Granted,
        GrantState::Prompt,
        GrantState::Denied,
        GrantState::Unknown,
    ] {
        assert_eq!(GrantState::from_str(s.as_str()), s);
    }
}

#[wasm_bindgen_test]
fn state_predicates() {
    assert!(GrantState::Granted.is_ready());
    assert!(GrantState::Prompt.needs_user_action());
    assert!(GrantState::Denied.is_blocked());
    assert!(!GrantState::Unknown.is_blocked());
}

#[wasm_bindgen_test]
fn mode_as_str() {
    assert_eq!(GrantMode::Read.as_str(), "read");
    assert_eq!(GrantMode::ReadWrite.as_str(), "readwrite");
}

#[wasm_bindgen_test]
fn explain_nonempty() {
    for s in [
        GrantState::Granted,
        GrantState::Prompt,
        GrantState::Denied,
        GrantState::Unknown,
    ] {
        let text = explain(s, GrantMode::Read);
        assert!(!text.is_empty());
    }
}

#[wasm_bindgen_test]
fn legend_has_four_lines() {
    let text = legend();
    assert!(text.contains("granted"));
    assert!(text.contains("prompt"));
    assert!(text.contains("denied"));
    assert!(text.contains("unknown"));
}

// ---------------------------------------------------------------------------
// from_js_error
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn from_js_error_security_is_prompt() {
    let e = js_sys::Error::new("no gesture");
    let _ = js_sys::Reflect::set(
        &e,
        &JsValue::from_str("name"),
        &JsValue::from_str("SecurityError"),
    );
    let js: JsValue = e.into();
    assert_eq!(from_js_error(&js), GrantState::Prompt);
}

#[wasm_bindgen_test]
fn from_js_error_notfound_is_unknown() {
    let e = js_sys::Error::new("gone");
    let _ = js_sys::Reflect::set(
        &e,
        &JsValue::from_str("name"),
        &JsValue::from_str("NotFoundError"),
    );
    let js: JsValue = e.into();
    assert_eq!(from_js_error(&js), GrantState::Unknown);
}

#[wasm_bindgen_test]
fn from_js_error_other_is_unknown() {
    let e = js_sys::Error::new("whatever");
    let js: JsValue = e.into();
    assert_eq!(from_js_error(&js), GrantState::Unknown);
}

// ---------------------------------------------------------------------------
// query/request — OPFS granted без gesture
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn query_opfs_read_granted() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle_js: &JsValue = root.as_ref();
    let state = fsa_grant_flow::query(handle_js, GrantMode::Read)
        .await
        .expect("query");
    assert_eq!(state, GrantState::Granted);
}

#[wasm_bindgen_test]
async fn request_opfs_readwrite_granted() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle_js: &JsValue = root.as_ref();
    let state = fsa_grant_flow::request(handle_js, GrantMode::ReadWrite)
        .await
        .expect("request");
    assert_eq!(state, GrantState::Granted);
}

// ---------------------------------------------------------------------------
// ensure — preflight
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn ensure_opfs_read_ready() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle_js: &JsValue = root.as_ref();
    let outcome = fsa_grant_flow::ensure(handle_js, GrantMode::Read)
        .await
        .expect("ensure");
    assert!(outcome.is_ready());
    assert_eq!(outcome.state(), GrantState::Granted);
}

#[wasm_bindgen_test]
async fn ensure_opfs_readwrite_ready() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle_js: &JsValue = root.as_ref();
    let outcome = fsa_grant_flow::ensure(handle_js, GrantMode::ReadWrite)
        .await
        .expect("ensure");
    assert!(outcome.is_ready());
}

#[wasm_bindgen_test]
async fn ensure_on_non_handle_blocked() {
    let fake = JsValue::from_str("not a handle");
    let outcome = fsa_grant_flow::ensure(&fake, GrantMode::Read)
        .await
        .expect("ensure");
    assert!(!outcome.is_ready());
    assert_eq!(outcome.state(), GrantState::Unknown);
}

#[wasm_bindgen_test]
fn outcome_message_nonempty() {
    let outcome = GrantOutcome::Ready(GrantState::Granted);
    let m = outcome.message();
    assert!(!m.is_empty());
    assert!(m.contains("granted"));
}

// ---------------------------------------------------------------------------
// Non-handle приёмники — Reflect не должен падать
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn query_on_string_returns_unknown() {
    let fake = JsValue::from_str("not a handle");
    let state = fsa_grant_flow::query(&fake, GrantMode::Read)
        .await
        .expect("query");
    assert_eq!(state, GrantState::Unknown);
}

#[wasm_bindgen_test]
async fn request_on_string_returns_unknown() {
    let fake = JsValue::from_str("not a handle");
    let state = fsa_grant_flow::request(&fake, GrantMode::ReadWrite)
        .await
        .expect("request");
    assert_eq!(state, GrantState::Unknown);
}

#[wasm_bindgen_test]
async fn query_on_null_returns_unknown() {
    let fake = JsValue::NULL;
    let state = fsa_grant_flow::query(&fake, GrantMode::Read)
        .await
        .expect("query");
    assert_eq!(state, GrantState::Unknown);
}

#[wasm_bindgen_test]
async fn query_on_number_returns_unknown() {
    let fake = JsValue::from_f64(42.0);
    let state = fsa_grant_flow::query(&fake, GrantMode::Read)
        .await
        .expect("query");
    assert_eq!(state, GrantState::Unknown);
}

// ---------------------------------------------------------------------------
// IDB — handle сохраняется и восстанавливается
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn idb_roundtrip_saves_and_loads() {
    let root = match opfs_root().await {
        Ok(r) => r,
        Err(_) => return,
    };
    let handle_js: &JsValue = root.as_ref();
    let key = "test_roundtrip";

    let _ = fsa_grant_store::clear_handle(key).await;
    let save_res = fsa_grant_store::save_handle(key, handle_js).await;
    if save_res.is_err() {
        // IDB с FileSystemDirectoryHandle может быть недоступен в headless — skip.
        return;
    }

    let loaded = fsa_grant_store::load_handle(key).await.expect("load");
    assert!(loaded.is_some(), "сохранённый handle должен вернуться");

    let _ = fsa_grant_store::clear_handle(key).await;
}

#[wasm_bindgen_test]
async fn idb_load_missing_returns_none() {
    let key = "test_missing_key";
    let _ = fsa_grant_store::clear_handle(key).await;
    let loaded = fsa_grant_store::load_handle(key).await.expect("load");
    assert!(loaded.is_none(), "отсутствующий ключ → None");
}