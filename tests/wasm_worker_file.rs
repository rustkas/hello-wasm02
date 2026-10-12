use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Создаёт Worker из inline-кода через blob URL.
///
/// Позволяет тестировать передачу сообщений и File без загрузки
/// реального `file_worker.js` с сервера.
fn create_inline_worker(code: &str) -> (web_sys::Worker, String) {
    let array = js_sys::Array::of1(&JsValue::from_str(code));
    let blob = web_sys::Blob::new_with_str_sequence(&array)
        .expect("Blob::new_with_str_sequence");
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .expect("create_object_url_with_blob");
    let worker = web_sys::Worker::new(&url).expect("Worker::new");
    return (worker, url);
}

#[wasm_bindgen_test]
async fn test_worker_echo() {
    // Воркер возвращает то, что получил.
    let code = "self.onmessage = (e) => self.postMessage(e.data);";
    let (worker, url) = create_inline_worker(code);

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve_c = resolve.clone();
        let reject_c = reject.clone();
        let on_message = Closure::once_into_js(move |event: web_sys::MessageEvent| {
            let _ = resolve_c.call1(&JsValue::NULL, &event.data());
        });
        let on_error = Closure::once_into_js(move |err: JsValue| {
            let _ = reject_c.call1(&JsValue::NULL, &err);
        });
        worker.set_onmessage(Some(on_message.unchecked_ref()));
        worker.set_onerror(Some(on_error.unchecked_ref()));
    });

    worker
        .post_message(&JsValue::from_str("hello"))
        .expect("post_message");
    let result = JsFuture::from(promise).await.expect("await");

    assert_eq!(result.as_string().as_deref(), Some("hello"));

    worker.terminate();
    web_sys::Url::revoke_object_url(&url).ok();
}

#[wasm_bindgen_test]
async fn test_worker_file_transfer() {
    // Воркер получает File и возвращает его имя и размер.
    // Так мы убеждаемся, что File корректно передаётся через
    // structured clone — содержимое не нужно, важен сам объект.
    let code = r#"
        self.onmessage = (e) => {
            const f = e.data.file;
            self.postMessage({ name: f.name, size: f.size });
        };
    "#;
    let (worker, url) = create_inline_worker(code);

    // Создаём File с известным именем и содержимым.
    let file = web_sys::File::new_with_str_sequence(
        &js_sys::Array::of1(&JsValue::from_str("test content")),
        "sample.txt",
    )
    .expect("File::new_with_str_sequence");

    let msg_obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &msg_obj,
        &JsValue::from_str("file"),
        file.as_ref(),
    )
    .expect("Reflect::set file");
    let msg: JsValue = msg_obj.into();

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve_c = resolve.clone();
        let reject_c = reject.clone();
        let on_message = Closure::once_into_js(move |event: web_sys::MessageEvent| {
            let _ = resolve_c.call1(&JsValue::NULL, &event.data());
        });
        let on_error = Closure::once_into_js(move |err: JsValue| {
            let _ = reject_c.call1(&JsValue::NULL, &err);
        });
        worker.set_onmessage(Some(on_message.unchecked_ref()));
        worker.set_onerror(Some(on_error.unchecked_ref()));
    });

    worker.post_message(&msg).expect("post_message");
    let result = JsFuture::from(promise).await.expect("await");

    let obj: &js_sys::Object = result.dyn_ref().expect("ответ не Object");
    let name = js_sys::Reflect::get(obj, &JsValue::from_str("name"))
        .expect("get name")
        .as_string()
        .expect("name не string");
    let size = js_sys::Reflect::get(obj, &JsValue::from_str("size"))
        .expect("get size")
        .as_f64()
        .expect("size не number");

    assert_eq!(name, "sample.txt");
    assert_eq!(size, 12.0); // длина "test content"

    worker.terminate();
    web_sys::Url::revoke_object_url(&url).ok();
}

#[wasm_bindgen_test]
async fn test_worker_terminate_no_panic() {
    // terminate() не должен паниковать на только что созданном воркере.
    let code = "self.onmessage = () => {};";
    let (worker, url) = create_inline_worker(code);
    worker.terminate();
    web_sys::Url::revoke_object_url(&url).ok();
}