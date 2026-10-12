use wasm_bindgen_test::*;
use hello_wasm02::upload_progress::*;

wasm_bindgen_test_configure!(run_in_browser);

const UPLOAD_URL: &str = "http://localhost:3000/api/upload";

#[wasm_bindgen_test]
async fn test_upload_with_progress_success() {
    let file = web_sys::File::new_with_str_sequence(
        &js_sys::Array::of1(&"test data".into()),
        "test.txt",
    )
    .expect("File::new_with_str_sequence");

    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    let called_c = called.clone();
    let on_progress = move |_loaded: f64, _total: f64| {
        called_c.set(true);
    };

    let result = upload_with_progress(&file, UPLOAD_URL, on_progress).await;
    assert!(result.is_ok(), "upload failed: {:?}", result.err());
    assert!(called.get(), "прогресс-события не приходили");
}

#[wasm_bindgen_test]
async fn test_upload_abort_immediately_after_send() {
    // Abort до await_finish — проверяет subscribe-then-check:
    // события не выстрелят, но ready_state == DONE и status == 0,
    // поэтому await_finish вернёт Err("aborted") без зависания.
    let file = web_sys::File::new_with_str_sequence(
        &js_sys::Array::of1(&"test data".into()),
        "test.txt",
    )
    .expect("File::new_with_str_sequence");

    let xhr = start_upload(&file, UPLOAD_URL, |_, _| {})
        .expect("start_upload");
    xhr.abort().expect("abort");

    let result = await_finish(&xhr).await;
    assert!(result.is_err(), "ожидалась ошибка отмены");
    let err = result.unwrap_err();
    assert!(is_abort_error(&err), "не abort-ошибка: {err:?}");
}