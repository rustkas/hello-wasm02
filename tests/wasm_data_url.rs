use wasm_bindgen_test::*;
use hello_wasm02::base64_utils;
use hello_wasm02::data_url;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_base64_roundtrip_ascii() {
    let original = "Hello, WASM!";
    let encoded = base64_utils::encode(original.as_bytes());
    assert_eq!(encoded, "SGVsbG8sIFdBU00h");

    let decoded = base64_utils::decode(&encoded).expect("decode");
    assert_eq!(decoded, original.as_bytes());
}

#[wasm_bindgen_test]
fn test_base64_roundtrip_binary() {
    let original: Vec<u8> = vec![0x00, 0x01, 0x02, 0x7F, 0x80, 0xFE, 0xFF];
    let encoded = base64_utils::encode(&original);
    let decoded = base64_utils::decode(&encoded).expect("decode");
    assert_eq!(original, decoded);
}

#[wasm_bindgen_test]
fn test_base64_decode_invalid() {
    // Символ '*' не входит в стандартный алфавит base64.
    let result = base64_utils::decode("SGVsbG8*");
    assert!(result.is_err(), "ожидалась ошибка декодирования");
}

#[wasm_bindgen_test]
fn test_base64_matches_browser_btoa() {
    // btoa работает только для ASCII (latin1).
    let s = "Hello, WASM!";
    let rust_encoded = base64_utils::encode(s.as_bytes());

    let btoa = js_sys::Function::new_with_args("s", "return btoa(s);");
    let browser_result = btoa
        .call1(
            &wasm_bindgen::JsValue::NULL,
            &wasm_bindgen::JsValue::from_str(s),
        )
        .expect("btoa")
        .as_string()
        .expect("btoa вернул строку");

    assert_eq!(
        rust_encoded, browser_result,
        "Rust crate `base64` и браузерная btoa должны давать одинаковый результат"
    );
}

#[wasm_bindgen_test]
fn test_parse_data_url_base64() {
    let url = "data:image/png;base64,iVBORw0KGgo=";
    let parts = data_url::parse_data_url(url).expect("parse");
    assert_eq!(parts.mime_type, "image/png");
    assert!(parts.is_base64);
    assert_eq!(parts.payload, "iVBORw0KGgo=");
}

#[wasm_bindgen_test]
fn test_parse_data_url_plain() {
    let url = "data:text/plain,Hello%20World";
    let parts = data_url::parse_data_url(url).expect("parse");
    assert_eq!(parts.mime_type, "text/plain");
    assert!(!parts.is_base64);
    assert_eq!(parts.payload, "Hello%20World");
}

#[wasm_bindgen_test]
fn test_parse_data_url_default_mime() {
    // data:,hello — без MIME; по спецификации text/plain;charset=US-ASCII.
    let url = "data:,hello";
    let parts = data_url::parse_data_url(url).expect("parse");
    assert_eq!(parts.mime_type, "text/plain;charset=US-ASCII");
    assert!(!parts.is_base64);
    assert_eq!(parts.payload, "hello");
}

#[wasm_bindgen_test]
fn test_parse_data_url_invalid() {
    assert!(data_url::parse_data_url("http://example.com").is_none());
    assert!(data_url::parse_data_url("data:image/png;base64").is_none());
}

#[wasm_bindgen_test]
async fn test_read_as_data_url_from_blob() {
    let blob = web_sys::Blob::new_with_str_sequence(
        &js_sys::Array::of1(&"hello".into()),
    )
    .expect("Blob::new_with_str_sequence");

    let url = data_url::read_as_data_url(&blob)
        .await
        .expect("read_as_data_url");
    assert!(url.starts_with("data:"), "Data URL должен начинаться с 'data:'");
    assert!(url.contains(";base64,"), "Data URL должен содержать ';base64,'");

    let parts = data_url::parse_data_url(&url).expect("parse");
    let decoded = base64_utils::decode(&parts.payload).expect("decode");
    assert_eq!(decoded, b"hello");
}