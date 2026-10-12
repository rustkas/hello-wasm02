use wasm_bindgen_test::*;
use hello_wasm02::fsa_api_pickers::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_is_supported_returns_bool() {
    let s = is_supported();
    web_sys::console::log_1(&format!("FSA supported: {s}").into());
}

#[wasm_bindgen_test]
async fn test_open_files_without_gesture_is_safe() {
    let result = open_files(None).await;
    match result {
        Ok(_) => {}
        Err(e) => {
            let s = e.as_string().unwrap_or_default();
            assert!(
                s.is_empty()
                    || s.contains("SecurityError")
                    || s.contains("gesture")
                    || s.contains("User activation")
            );
        }
    }
}

#[wasm_bindgen_test]
async fn test_pick_directory_without_gesture_is_safe() {
    let result = pick_directory("read").await;
    match result {
        Ok(_) => {}
        Err(e) => {
            let s = e.as_string().unwrap_or_default();
            web_sys::console::log_1(&format!("pick_directory err: {s}").into());
        }
    }
}

#[wasm_bindgen_test]
fn test_build_types_constructs_array() {
    let types = build_types(&[("Text", &[".txt", ".md"])]).unwrap();
    let arr = js_sys::Array::from(&types);
    assert_eq!(arr.length(), 1);
}