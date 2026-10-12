use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// JSON-ответ десериализуется в serde_json::Value.
#[wasm_bindgen_test]
async fn test_get_json() {
    let url = "https://jsonplaceholder.typicode.com/posts/1";
    let resp = hello_wasm02::fetch::send_request_classified(url)
        .await
        .unwrap();
    let json: serde_json::Value = hello_wasm02::fetch::response_json(&resp)
        .await
        .unwrap();
    assert_eq!(json["id"], 1);
    assert!(json["title"].is_string());
}

/// Текстовый ответ читается как String.
#[wasm_bindgen_test]
async fn test_get_text() {
    let url = "https://jsonplaceholder.typicode.com/posts/1";
    let resp = hello_wasm02::fetch::send_request_classified(url)
        .await
        .unwrap();
    let promise = resp.text().unwrap();
    let value = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .unwrap();
    let text = value.as_string().unwrap();
    assert!(!text.is_empty());
    assert!(text.contains("userId"));
}

/// ArrayBuffer возвращается корректно.
#[wasm_bindgen_test]
async fn test_get_array_buffer() {
    let url = "http://127.0.0.1:3000/api/binary";
    let buffer = hello_wasm02::fetch::get_array_buffer(url).await.unwrap();
    assert!(buffer.byte_length() > 0);
}

/// ArrayBuffer превращается в Vec<u8>.
#[wasm_bindgen_test]
async fn test_get_bytes() {
    let url = "http://127.0.0.1:3000/api/binary";
    let bytes = hello_wasm02::fetch::get_bytes(url).await.unwrap();
    assert_eq!(bytes.len(), 256);
    assert_eq!(bytes[0], 0);
    assert_eq!(bytes[255], 255);
}

/// Blob возвращается корректно.
#[wasm_bindgen_test]
async fn test_get_blob() {
    let url = "http://127.0.0.1:3000/api/image";
    let blob = hello_wasm02::fetch::get_blob(url).await.unwrap();
    assert!(blob.size() > 0.0);
}

/// ArrayBuffer имеет правильную длину.
#[wasm_bindgen_test]
async fn test_array_buffer_length() {
    let url = "http://127.0.0.1:3000/api/binary";
    let buffer = hello_wasm02::fetch::get_array_buffer(url).await.unwrap();
    assert_eq!(buffer.byte_length(), 256);
}

/// Uint8Array создаётся из ArrayBuffer.
#[wasm_bindgen_test]
async fn test_uint8_array_from_buffer() {
    let url = "http://127.0.0.1:3000/api/binary";
    let buffer = hello_wasm02::fetch::get_array_buffer(url).await.unwrap();
    let uint8 = js_sys::Uint8Array::new(&buffer);
    assert_eq!(uint8.length(), 256);
    assert_eq!(uint8.get_index(0), 0);
    assert_eq!(uint8.get_index(255), 255);
}

/// Blob имеет правильный размер.
#[wasm_bindgen_test]
async fn test_blob_size() {
    let url = "http://127.0.0.1:3000/api/image";
    let blob = hello_wasm02::fetch::get_blob(url).await.unwrap();
    // PNG 1x1 прозрачный — примерно 70 байт.
    assert!(blob.size() > 50.0);
    assert!(blob.size() < 200.0);
}

/// Blob имеет правильный MIME-тип.
#[wasm_bindgen_test]
async fn test_blob_type() {
    let url = "http://127.0.0.1:3000/api/image";
    let blob = hello_wasm02::fetch::get_blob(url).await.unwrap();
    assert_eq!(blob.type_(), "image/png");
}

/// JSON с unicode корректно десериализуется.
#[wasm_bindgen_test]
async fn test_json_with_unicode() {
    // JSONPlaceholder не отдаёт unicode, но мы можем проверить
    // десериализацию на собственном сервере.
    let url = "http://127.0.0.1:3000/api/delay/0";
    let resp = hello_wasm02::fetch::send_request_classified(url)
        .await
        .unwrap();
    let json: serde_json::Value = hello_wasm02::fetch::response_json(&resp)
        .await
        .unwrap();
    // Сообщение содержит русский текст.
    let message = json["message"].as_str().unwrap();
    assert!(message.contains("секунд"));
}

/// data: URL с base64 декодируется в байты.
#[wasm_bindgen_test]
async fn test_parse_data_url_base64() {
    let url = "data:video/webm;base64,GkXfo0AgQoaBAUL3gQFC8oEEQvOBCEKCQAR3ZWJtQoeBAkKFgQIYU4BnQI0VSalmQCgq17FAAw9CQE2AQAZ3aGFtbXlXQUAGd2hhbW15RIlACECPQAAAAAAAFlSua0AxrkAu14EBY8WBAZyBACK1nEADdW5khkAFVl9WUDglhohAA1ZQOIOBAeBABrCBCLqBCB9DtnVAIueBAKNAHIEAAIAwAQCdASoIAAgAAUAmJaQAA3AA/vz0AAA=";
    let (mime, bytes) = hello_wasm02::fetch::parse_data_url(url).unwrap();
    assert_eq!(mime, "video/webm");
    assert!(bytes.len() > 100, "должно быть больше 100 байт, получено {}", bytes.len());
    // WebM начинается с сигнатуры EBML: 1A 45 DF A3.
    assert_eq!(bytes[0], 0x1A);
    assert_eq!(bytes[1], 0x45);
    assert_eq!(bytes[2], 0xDF);
    assert_eq!(bytes[3], 0xA3);
}

/// data: URL без base64 (текстовый) декодируется из percent-encoding.
#[wasm_bindgen_test]
async fn test_parse_data_url_plain() {
    let url = "data:text/plain,Hello%2C%20world!";
    let (mime, bytes) = hello_wasm02::fetch::parse_data_url(url).unwrap();
    assert_eq!(mime, "text/plain");
    assert_eq!(String::from_utf8(bytes).unwrap(), "Hello, world!");
}