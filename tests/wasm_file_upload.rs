use wasm_bindgen_test::*;
use hello_wasm02::file_upload::*;

wasm_bindgen_test_configure!(run_in_browser);

/// URL эндпоинта загрузки. Совпадает с `app.rs`.
/// Сервер запускается через `npm run server` перед тестами.
const UPLOAD_URL: &str = "http://localhost:3000/api/upload";

#[wasm_bindgen_test]
async fn test_upload_file_success() {
    let file = web_sys::File::new_with_str_sequence(
        &js_sys::Array::of1(&"test data".into()),
        "test.txt",
    )
    .expect("File::new_with_str_sequence");

    let result = upload_file(&file, UPLOAD_URL).await;
    assert!(result.is_ok(), "upload_file failed: {:?}", result.err());

    let body = result.unwrap();
    assert!(body.contains("\"status\":\"ok\""), "unexpected body: {body}");
    assert!(
        body.contains("\"filename\":\"test.txt\""),
        "unexpected body: {body}"
    );
    assert!(body.contains("\"size\":9"), "unexpected body: {body}");
}

#[wasm_bindgen_test]
async fn test_upload_blob_with_filename() {
    let blob = web_sys::Blob::new_with_str_sequence(
        &js_sys::Array::of1(&"some blob content".into()),
    )
    .expect("Blob::new_with_str_sequence");

    let result = upload_blob(&blob, "report.txt", UPLOAD_URL).await;
    assert!(result.is_ok(), "upload_blob failed: {:?}", result.err());

    let body = result.unwrap();
    assert!(
        body.contains("\"filename\":\"report.txt\""),
        "unexpected body: {body}"
    );
}

#[wasm_bindgen_test]
async fn test_upload_empty_file() {
    let file = web_sys::File::new_with_str_sequence(
        &js_sys::Array::of1(&"".into()),
        "empty.txt",
    )
    .expect("File::new_with_str_sequence");

    let result = upload_file(&file, UPLOAD_URL).await;
    assert!(result.is_ok(), "upload failed: {:?}", result.err());

    let body = result.unwrap();
    assert!(body.contains("\"size\":0"), "unexpected body: {body}");
}

#[wasm_bindgen_test]
async fn test_upload_binary_file() {
    // Бинарные данные (не UTF-8) — проверяем, что multipart
    // не портит байты.
    let bytes: Vec<u8> = (0..=255).collect();
    let uint8 = js_sys::Uint8Array::from(&bytes[..]);
    let parts = js_sys::Array::of1(&uint8.buffer());
    let file = web_sys::File::new_with_u8_array_sequence(&parts, "binary.bin")
        .expect("File::new_with_u8_array_sequence");

    let result = upload_file(&file, UPLOAD_URL).await;
    assert!(result.is_ok(), "upload failed: {:?}", result.err());

    let body = result.unwrap();
    assert!(body.contains("\"size\":256"), "unexpected body: {body}");
}

#[wasm_bindgen_test]
async fn test_upload_cancelled() {
    // Предварительно отменённый сигнал — запрос должен
    // завершиться с AbortError.
    let controller = web_sys::AbortController::new()
        .expect("AbortController::new");
    let signal = controller.signal();
    controller.abort();

    let file = web_sys::File::new_with_str_sequence(
        &js_sys::Array::of1(&"test data".into()),
        "test.txt",
    )
    .expect("File::new_with_str_sequence");

    let result = upload_file_cancellable(&file, UPLOAD_URL, &signal).await;
    assert!(result.is_err(), "ожидалась ошибка отмены");

    let err = result.unwrap_err();
    assert!(is_abort_error(&err), "ожидался AbortError, получено: {err:?}");
}

#[wasm_bindgen_test]
fn test_is_abort_error_non_abort() {
    let e = wasm_bindgen::JsValue::from_str("something else");
    assert!(!is_abort_error(&e));
}