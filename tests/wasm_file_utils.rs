use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use hello_wasm02::file_reader;
use hello_wasm02::file_utils::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_vec_blob_arraybuffer_vec_roundtrip() {
    let original: Vec<u8> = vec![0x00, 0x01, 0x02, 0x7F, 0x80, 0xFE, 0xFF];

    let blob = vec_to_blob(&original, "application/octet-stream")
        .expect("vec_to_blob");

    let ab_js = file_reader::read_as_array_buffer(&blob)
        .await
        .expect("read_as_array_buffer");
    let array_buffer: js_sys::ArrayBuffer = ab_js
        .dyn_into()
        .expect("не ArrayBuffer");

    let roundtrip = array_buffer_to_vec(&array_buffer);
    assert_eq!(original, roundtrip);
}

#[wasm_bindgen_test]
fn test_get_extension_simple() {
    assert_eq!(get_extension("image.png"), Some("png"));
    assert_eq!(get_extension("archive.tar.gz"), Some("gz"));
    assert_eq!(get_extension("noext"), None);
}

#[wasm_bindgen_test]
fn test_get_extension_edge_cases() {
    assert_eq!(get_extension("file."), Some(""));
    assert_eq!(get_extension(".gitignore"), Some("gitignore"));
    assert_eq!(get_extension("a.b.c.d"), Some("d"));
}

#[wasm_bindgen_test]
fn test_mime_from_extension_known() {
    assert_eq!(mime_from_extension("png"), "image/png");
    assert_eq!(mime_from_extension("jpg"), "image/jpeg");
    assert_eq!(mime_from_extension("jpeg"), "image/jpeg");
    assert_eq!(mime_from_extension("txt"), "text/plain");
    assert_eq!(mime_from_extension("json"), "application/json");
    assert_eq!(mime_from_extension("pdf"), "application/pdf");
}

#[wasm_bindgen_test]
fn test_mime_from_extension_case_insensitive() {
    assert_eq!(mime_from_extension("PNG"), "image/png");
    assert_eq!(mime_from_extension("JPG"), "image/jpeg");
    assert_eq!(mime_from_extension("TXT"), "text/plain");
}

#[wasm_bindgen_test]
fn test_mime_from_extension_unknown() {
    assert_eq!(mime_from_extension("xyz"), "application/octet-stream");
    assert_eq!(mime_from_extension(""), "application/octet-stream");
}

#[wasm_bindgen_test]
fn test_mime_from_extension_markdown() {
    assert_eq!(mime_from_extension("md"), "text/markdown");
    assert_eq!(mime_from_extension("markdown"), "text/markdown");
    assert_eq!(mime_from_extension("MD"), "text/markdown");
}

#[wasm_bindgen_test]
fn test_mime_from_extension_web_formats() {
    assert_eq!(mime_from_extension("html"), "text/html");
    assert_eq!(mime_from_extension("htm"), "text/html");
    assert_eq!(mime_from_extension("css"), "text/css");
    assert_eq!(mime_from_extension("js"), "text/javascript");
    assert_eq!(mime_from_extension("mjs"), "text/javascript");
    assert_eq!(mime_from_extension("wasm"), "application/wasm");
}

#[wasm_bindgen_test]
fn test_mime_from_extension_media() {
    assert_eq!(mime_from_extension("mp3"), "audio/mpeg");
    assert_eq!(mime_from_extension("mp4"), "video/mp4");
    assert_eq!(mime_from_extension("webm"), "video/webm");
}

#[wasm_bindgen_test]
fn test_effective_mime_type_uses_declared() {
    // Если браузер определил MIME — используем его.
    let parts = js_sys::Array::of1(&"x".into());
    let bag = web_sys::FilePropertyBag::new();
    bag.set_type("image/png");
    let file = web_sys::File::new_with_str_sequence_and_options(
        &parts,
        "pic.png",
        &bag,
    )
    .expect("File::new_with_str_sequence_and_options");

    assert_eq!(effective_mime_type(&file), "image/png");
}

#[wasm_bindgen_test]
fn test_effective_mime_type_falls_back_to_extension() {
    // Если браузер вернул пустой MIME — используем расширение.
    let parts = js_sys::Array::of1(&"# Title".into());
    let bag = web_sys::FilePropertyBag::new();
    bag.set_type(""); // явно пустой
    let file = web_sys::File::new_with_str_sequence_and_options(
        &parts,
        "readme.md",
        &bag,
    )
    .expect("File::new_with_str_sequence_and_options");

    assert_eq!(effective_mime_type(&file), "text/markdown");
}

#[wasm_bindgen_test]
fn test_effective_mime_type_unknown_extension() {
    // Неизвестное расширение → octet-stream.
    let parts = js_sys::Array::of1(&"data".into());
    let bag = web_sys::FilePropertyBag::new();
    bag.set_type("");
    let file = web_sys::File::new_with_str_sequence_and_options(
        &parts,
        "file.xyz",
        &bag,
    )
    .expect("File::new_with_str_sequence_and_options");

    assert_eq!(effective_mime_type(&file), "application/octet-stream");
}