#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::dedicated_worker::{
    create_worker, create_worker_with_options, is_worker_supported, post_command,
    post_echo, post_string, terminate,
};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn worker_is_supported_in_browser() {
    assert!(is_worker_supported(), "Worker should be available");
}

#[wasm_bindgen_test]
fn create_worker_returns_object() {
    let w = create_worker("/nonexistent_worker_xyz.js");
    assert!(w.is_ok(), "Worker object should be constructible");
}

#[wasm_bindgen_test]
fn create_worker_with_module_option() {
    let w = create_worker_with_options(
        "/nonexistent_worker_xyz.js",
        true,
        Some("test-worker"),
    );
    assert!(w.is_ok(), "module Worker should be constructible");
}

#[wasm_bindgen_test]
fn post_string_does_not_panic() {
    let Ok(worker) = create_worker("/nonexistent_worker_xyz.js") else {
        return;
    };
    let _ = post_string(&worker, "hello");
}

#[wasm_bindgen_test]
fn post_echo_does_not_panic() {
    let Ok(worker) = create_worker("/nonexistent_worker_xyz.js") else {
        return;
    };
    let _ = post_echo(&worker, "hello");
}

#[wasm_bindgen_test]
fn post_command_does_not_panic() {
    let Ok(worker) = create_worker("/nonexistent_worker_xyz.js") else {
        return;
    };
    let _ = post_command(&worker, "throw");
}

#[wasm_bindgen_test]
fn terminate_does_not_panic() {
    let Ok(worker) = create_worker("/nonexistent_worker_xyz.js") else {
        return;
    };
    terminate(&worker);
}