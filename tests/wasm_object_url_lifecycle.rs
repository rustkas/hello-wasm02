#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::object_url_lifecycle::{
    create_object_url, make_payload_blob, read_heap_limit_bytes,
    read_used_heap_bytes, revoke_object_url, try_request_gc, ObjectUrlLab,
};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn blob_creation_has_expected_size_and_type() {
    let blob = make_payload_blob(4).expect("blob");
    assert_eq!(blob.size() as usize, 4 * 1024);
    assert_eq!(blob.type_(), "application/octet-stream");
}

#[wasm_bindgen_test]
fn create_and_revoke_roundtrip() {
    let blob = make_payload_blob(1).expect("blob");
    let url = create_object_url(&blob).expect("url");
    assert!(url.starts_with("blob:"), "unexpected url: {url}");
    revoke_object_url(&url);
    // Повторный revoke не должен паниковать (идемпотентность).
    revoke_object_url(&url);
}

#[wasm_bindgen_test]
fn lab_counts_are_consistent() {
    let mut lab = ObjectUrlLab::new();

    lab.leak(3, 1).expect("leak");
    assert_eq!(lab.live_count(), 3);
    assert_eq!(lab.created_count(), 3);
    assert_eq!(lab.revoked_count(), 0);
    assert_eq!(lab.forgotten_count(), 0);

    assert!(lab.release_one());
    assert_eq!(lab.live_count(), 2);
    assert_eq!(lab.revoked_count(), 1);

    let n = lab.release_all();
    assert_eq!(n, 2);
    assert_eq!(lab.live_count(), 0);
    assert_eq!(lab.revoked_count(), 3);

    // Повторный release_all — ноль.
    assert_eq!(lab.release_all(), 0);
}

#[wasm_bindgen_test]
fn release_one_on_empty_is_false() {
    let mut lab = ObjectUrlLab::new();
    assert!(!lab.release_one());
}

#[wasm_bindgen_test]
fn forget_drops_rust_handles_without_revoking() {
    let mut lab = ObjectUrlLab::new();
    lab.leak(4, 1).expect("leak");

    let n = lab.forget_all();
    assert_eq!(n, 4);
    assert_eq!(lab.live_count(), 0);
    assert_eq!(lab.revoked_count(), 0);
    assert_eq!(lab.forgotten_count(), 4);

    // Дальше нечего освобождать.
    assert_eq!(lab.release_all(), 0);
}

#[wasm_bindgen_test]
fn leak_zero_is_noop() {
    let mut lab = ObjectUrlLab::new();
    lab.leak(0, 1).expect("leak");
    assert_eq!(lab.live_count(), 0);
    assert_eq!(lab.created_count(), 0);
}

#[wasm_bindgen_test]
fn heap_readers_and_gc_are_optional() {
    // В Chromium вернут Some, в Firefox — None. Падать не должны.
    let _ = read_used_heap_bytes();
    let _ = read_heap_limit_bytes();
    // window.gc() доступен только с флагом — вернуть true/false равноправно.
    let _ = try_request_gc();
}