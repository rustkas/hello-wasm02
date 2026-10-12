use wasm_bindgen_test::*;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen::JsValue;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_async_timeout() {
    // Создаём Promise, который разрешается через setTimeout.
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                &resolve,
                10,
            )
            .unwrap();
    });

    let result = JsFuture::from(promise).await;
    assert!(result.is_ok());
}

#[wasm_bindgen_test]
async fn test_rejected_promise() {
    let promise = js_sys::Promise::reject(&JsValue::from("expected error"));
    let result = JsFuture::from(promise).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().as_string().unwrap(), "expected error");
}