//! XHR-загрузка с прогрессом и отменой.
//!
//! fetch не имеет события `upload.onprogress`. Единственный
//! способ получить реальный прогресс отправки — XMLHttpRequest.
//! XHR также даёт `abort()` для мгновенной отмены.
//!
//! **Устойчивость к гонке.** XHR может завершиться (или быть
//! отменённым) **до** того, как `await_finish` подпишется на
//! события. Используем **subscribe-then-check**: сначала
//! подписываемся на события, затем проверяем `ready_state`.
//! Если запрос уже в терминальном состоянии, обрабатываем
//! вручную через `status` и `response_text`.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, FormData, ProgressEvent, XmlHttpRequest};

/// Начинает XHR-загрузку файла, возвращает сам XHR для отмены.
pub fn start_upload<F>(
    file: &File,
    url: &str,
    mut on_progress: F,
) -> Result<XmlHttpRequest, JsValue>
where
    F: FnMut(f64, f64) + 'static,
{
    let xhr = XmlHttpRequest::new()?;
    xhr.open("POST", url)?;

    let on_progress_cb = Closure::wrap(Box::new(move |e: ProgressEvent| {
        if e.length_computable() {
            on_progress(e.loaded(), e.total());
        } else {
            on_progress(e.loaded(), 0.0);
        }
    }) as Box<dyn FnMut(ProgressEvent)>);
    xhr.upload()?
        .set_onprogress(Some(on_progress_cb.as_ref().unchecked_ref()));
    on_progress_cb.forget();

    let form_data = FormData::new()?;
    form_data.append_with_blob("file", file)?;
    xhr.send_with_opt_form_data(Some(&form_data))?;

    return Ok(xhr);
}

/// Ждёт завершения XHR-запроса, возвращает текст ответа.
///
/// **Subscribe-then-check.** Порядок критичен:
/// 1. Подписываемся на `onload` / `onerror` / `onabort`.
/// 2. Проверяем `ready_state` — если уже терминальное, обрабатываем
///    вручную. Событие после подписки может не выстрелить.
///
/// **Терминальные состояния:**
/// * `DONE` (4) — запрос завершён (успех, ошибка HTTP, ошибка сети);
/// * `UNSENT` (0) — запрос отменён через `abort()` после завершения.
///
/// Если проверять только `DONE`, вариант «abort после успеха»
/// оставит промис без резолва — он зависнет навсегда.
pub async fn await_finish(xhr: &XmlHttpRequest) -> Result<String, JsValue> {
    let xhr_c = xhr.clone();
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve_c = resolve.clone();
        let reject_c = reject.clone();

        // ---- Подписки. ----
        let xhr_for_load = xhr_c.clone();
        let on_load = Closure::once_into_js(move || {
            let text: String = xhr_for_load
                .response_text()
                .ok()
                .flatten()
                .unwrap_or_default();
            let _ = resolve_c.call1(&JsValue::NULL, &JsValue::from_str(&text));
        });

        let reject_c2 = reject_c.clone();
        let on_error = Closure::once_into_js(move || {
            let _ = reject_c2.call1(&JsValue::NULL, &JsValue::from_str("network error"));
        });

        let reject_c3 = reject_c.clone();
        let on_abort = Closure::once_into_js(move || {
            let _ = reject_c3.call1(&JsValue::NULL, &JsValue::from_str("aborted"));
        });

        xhr_c.set_onload(Some(on_load.unchecked_ref()));
        xhr_c.set_onerror(Some(on_error.unchecked_ref()));
        xhr_c.set_onabort(Some(on_abort.unchecked_ref()));

        // ---- Проверка после подписки. ----
        // Терминальные состояния: DONE (завершён) или UNSENT (отменён
        // после завершения). В обоих случаях события не выстрелят.
        let state = xhr_c.ready_state();
        let terminal = state == XmlHttpRequest::DONE
            || state == XmlHttpRequest::UNSENT;
        if terminal {
            let status = xhr_c.status().unwrap_or(0);
            if status == 0 {
                // Abort или сетевая ошибка.
                let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("aborted"));
            } else {
                let text: String = xhr_c
                    .response_text()
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let _ = resolve.call1(&JsValue::NULL, &JsValue::from_str(&text));
            }
        }
    });
    let value = JsFuture::from(promise).await?;
    return value
        .as_string()
        .ok_or_else(|| JsValue::from_str("ответ не строка"));
}

/// Проверяет, была ли ошибка вызвана отменой XHR.
pub fn is_abort_error(e: &JsValue) -> bool {
    if let Some(s) = e.as_string() {
        return s == "aborted";
    }
    return false;
}

/// Удобная обёртка для сценария без отмены.
pub async fn upload_with_progress<F>(
    file: &File,
    url: &str,
    on_progress: F,
) -> Result<String, JsValue>
where
    F: FnMut(f64, f64) + 'static,
{
    let xhr = start_upload(file, url, on_progress)?;
    return await_finish(&xhr).await;
}