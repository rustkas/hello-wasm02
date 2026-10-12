use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use hello_wasm02::file_api::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_format_file_size() {
    assert_eq!(format_file_size(512.0), "512.00 B");
    assert_eq!(format_file_size(2048.0), "2.00 KB");
    assert_eq!(format_file_size(1024.0 * 1024.0), "1.00 MB");
    assert_eq!(format_file_size(1024.0 * 1024.0 * 1024.0), "1.00 GB");
}

#[wasm_bindgen_test]
fn test_file_metadata_from_file() {
    // FilePropertyBag позволяет задать MIME-тип явно —
    // иначе File::type_() вернёт пустую строку.
    let parts = js_sys::Array::of1(&JsValue::from_str("hello"));
    let bag = web_sys::FilePropertyBag::new();
    bag.set_type("text/plain");

    let file = web_sys::File::new_with_str_sequence_and_options(
        &parts,
        "test.txt",
        &bag,
    )
    .expect("File::new_with_str_sequence_and_options");

    let meta = FileMetadata::from_file(&file);
    assert_eq!(meta.name, "test.txt");
    assert_eq!(meta.mime_type, "text/plain");
    assert!(meta.size > 0.0);
    assert!(meta.last_modified > 0.0);
}

#[wasm_bindgen_test]
fn test_is_image_by_mime() {
    let parts = js_sys::Array::of1(&JsValue::from_str("png-bytes"));
    let bag = web_sys::FilePropertyBag::new();
    bag.set_type("image/png");

    let file = web_sys::File::new_with_str_sequence_and_options(
        &parts,
        "pic.png",
        &bag,
    )
    .expect("File::new_with_str_sequence_and_options");

    assert!(is_image(&file));
}