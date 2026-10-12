use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use hello_wasm02::fsa_api_handle::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_kind_and_name_on_non_handle() {
    let v = JsValue::from_str("not a handle");
    assert_eq!(kind(&v), "");
    assert_eq!(name(&v), "");
}

#[wasm_bindgen_test]
fn test_as_file_handle_rejects_non_handle() {
    let v = JsValue::from_str("x");
    assert!(as_file_handle(v).is_err());
}

#[wasm_bindgen_test]
fn test_as_directory_handle_rejects_non_handle() {
    let v = JsValue::from_str("x");
    assert!(as_directory_handle(v).is_err());
}