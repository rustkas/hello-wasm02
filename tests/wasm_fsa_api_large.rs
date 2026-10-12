use wasm_bindgen_test::*;
use hello_wasm02::fsa_api_large::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_adaptive_chunk_bounds() {
    assert_eq!(adaptive_chunk_size(0), 64 * 1024);
    assert_eq!(adaptive_chunk_size(1), 64 * 1024);
    let huge: usize = (u32::MAX - 1) as usize;
    assert_eq!(adaptive_chunk_size(huge), 4 * 1024 * 1024);
}

#[wasm_bindgen_test]
fn test_adaptive_chunk_mid() {
    let mid = adaptive_chunk_size(200 * 1024 * 1024);
    assert!(mid >= 64 * 1024 && mid <= 4 * 1024 * 1024);
}