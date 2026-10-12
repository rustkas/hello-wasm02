use std::cell::RefCell;

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, File, FileSystemFileHandle, HtmlAnchorElement, HtmlInputElement, ProgressEvent, Url,
    XmlHttpRequest, XmlHttpRequestResponseType,
};

use crate::converter::adaptive_chunk;

thread_local! {
    static LAST_SAVE_HANDLE: RefCell<Option<FileSystemFileHandle>> = const { RefCell::new(None) };
}

/// Сбросить кэш сохранения (используется кнопкой «Сбросить»).
pub fn forget_save_handle() {
    LAST_SAVE_HANDLE.with(|h| *h.borrow_mut() = None);
}

pub async fn pick_image_file() -> Result<File, JsValue> {
    let window = web_sys::window().unwrap();

    if let Ok(picker_fn) = js_sys::Reflect::get(&window, &JsValue::from_str("showOpenFilePicker")) {
        if !picker_fn.is_undefined() {
            let opts = js_sys::Object::new();
            let types = js_sys::Array::new();
            let accept = js_sys::Object::new();
            js_sys::Reflect::set(&accept, &"description".into(), &"Images".into())?;

            let mime = js_sys::Object::new();
            js_sys::Reflect::set(
                &mime,
                &"image/png".into(),
                &js_sys::Array::of1(&".png".into()),
            )?;
            js_sys::Reflect::set(
                &mime,
                &"image/jpeg".into(),
                &js_sys::Array::of2(&".jpg".into(), &".jpeg".into()),
            )?;
            js_sys::Reflect::set(&accept, &"accept".into(), &mime)?;
            types.push(&accept);
            js_sys::Reflect::set(&opts, &"types".into(), &types)?;

            let func = picker_fn.dyn_into::<js_sys::Function>()?;
            let promise = func.call1(&window, &opts)?;
            let result = JsFuture::from(promise.dyn_into::<js_sys::Promise>()?).await?;
            let arr = js_sys::Array::from(&result);
            let handle = arr.get(0);
            let file_handle = web_sys::FileSystemFileHandle::from(handle);
            let file = JsFuture::from(file_handle.get_file()).await?;
            return Ok(File::from(file));
        }
    }

    // Fallback: <input type=file>
    let document = window.document().unwrap();
    let input = document
        .create_element("input")?
        .dyn_into::<HtmlInputElement>()?;
    input.set_type("file");
    input.set_accept("image/png,image/jpeg,.png,.jpg,.jpeg");
    input.click();

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let input_clone = input.clone();
        let on_change = Closure::once(move || {
            match input_clone.files().and_then(|f| f.get(0)) {
                Some(file) => {
                    let _ = resolve.call1(&JsValue::NULL, &file);
                }
                None => {
                    let err = js_sys::Error::new("no file selected");
                    let _ = js_sys::Reflect::set(
                        &err,
                        &JsValue::from_str("name"),
                        &JsValue::from_str("AbortError"),
                    );
                    let err_js: JsValue = err.into();
                    let _ = reject.call1(&JsValue::NULL, &err_js);
                }
            }
        });
        input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
        on_change.forget();
    });
    let file = JsFuture::from(promise).await?;
    Ok(File::from(file))
}

pub enum SaveOutcome {
    Fsa,
    FallbackDownload,
    Cancelled,
}

pub async fn save_file_chunked(
    data: &[u8],
    suggested_name: &str,
    mut on_progress: impl FnMut(f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<SaveOutcome, JsValue> {
    let window = web_sys::window().unwrap();

    if let Ok(saver_fn) = js_sys::Reflect::get(&window, &JsValue::from_str("showSaveFilePicker")) {
        if !saver_fn.is_undefined() {
            let cached = LAST_SAVE_HANDLE.with(|h| h.borrow().clone());
            let file_handle = if let Some(h) = cached {
                h
            } else {
                let opts = js_sys::Object::new();
                js_sys::Reflect::set(&opts, &"suggestedName".into(), &suggested_name.into())?;
                let func = saver_fn.dyn_into::<js_sys::Function>()?;
                let promise = func.call1(&window, &opts)?;
                let handle = JsFuture::from(promise.dyn_into::<js_sys::Promise>()?).await?;
                let fh = FileSystemFileHandle::from(handle);
                LAST_SAVE_HANDLE.with(|h| *h.borrow_mut() = Some(fh.clone()));
                fh
            };

            let writable = JsFuture::from(file_handle.create_writable()).await?;
            let writable = web_sys::FileSystemWritableFileStream::from(writable);

            let chunk = adaptive_chunk(data.len(), 200);
            let total = data.len().max(1);
            let mut last_yield = 0.0_f64;

            for (i, c) in data.chunks(chunk).enumerate() {
                if is_cancelled() {
                    let _ = writable.close();
                    return Ok(SaveOutcome::Cancelled);
                }
                JsFuture::from(writable.write_with_u8_array(c)?).await?;
                let processed = ((i + 1) * chunk).min(data.len());
                let now = js_sys::Date::now();
                if now - last_yield >= 16.0 {
                    on_progress(processed as f64 / total as f64);
                    TimeoutFuture::new(0).await;
                    last_yield = now;
                }
            }
            JsFuture::from(writable.close()).await?;
            on_progress(1.0);
            return Ok(SaveOutcome::Fsa);
        }
    }

    // Fallback: <a download>
    on_progress(0.0);
    let blob = Blob::new_with_u8_array_sequence(&js_sys::Array::of1(
        &js_sys::Uint8Array::from(data),
    ))?;
    let url = Url::create_object_url_with_blob(&blob)?;
    let a = window
        .document()
        .unwrap()
        .create_element("a")?
        .dyn_into::<HtmlAnchorElement>()?;
    a.set_href(&url);
    a.set_download(suggested_name);
    a.click();
    Url::revoke_object_url(&url)?;
    on_progress(1.0);
    Ok(SaveOutcome::FallbackDownload)
}

// ---------------------------------------------------------------------------
// 2.3 — загрузка по контракту uploads_root()/{upload_id}/{filename}
// ---------------------------------------------------------------------------
pub async fn upload_to_server(
    data: &[u8],
    upload_id: &str,
    filename: &str,
    mime: &str,
    base_url: &str,
    on_progress: impl FnMut(f64) + 'static,
) -> Result<u16, JsValue> {
    let url = format!(
        "{}/files/{}/{}",
        base_url.trim_end_matches('/'),
        urlencode(upload_id),
        urlencode(filename)
    );
    upload_http(data, mime, &url, on_progress).await
}

async fn upload_http(
    data: &[u8],
    mime: &str,
    url: &str,
    mut on_progress: impl FnMut(f64) + 'static,
) -> Result<u16, JsValue> {
    let xhr = XmlHttpRequest::new()?;
    xhr.open_with_async("POST", url, true)?;
    xhr.set_request_header("Content-Type", mime)?;

    let upload = xhr.upload()?;
    let cb = Closure::<dyn FnMut(ProgressEvent)>::new(move |e: ProgressEvent| {
        if e.length_computable() {
            on_progress(e.loaded() / e.total().max(1.0));
        }
    });
    upload.set_onprogress(Some(cb.as_ref().unchecked_ref()));
    cb.forget();

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let xhr_load = xhr.clone();
        let resolve_cb = Closure::once(move || {
            let status = xhr_load.status().unwrap_or(0);
            let _ = resolve.call1(&JsValue::NULL, &JsValue::from_f64(status as f64));
        });
        xhr.set_onload(Some(resolve_cb.as_ref().unchecked_ref()));
        resolve_cb.forget();

        let reject_cb = Closure::once(move || {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("network error"));
        });
        xhr.set_onerror(Some(reject_cb.as_ref().unchecked_ref()));
        reject_cb.forget();
    });

    xhr.send_with_opt_u8_array(Some(data))?;
    let status = JsFuture::from(promise).await?;
    Ok(status.as_f64().unwrap_or(0.0) as u16)
}

// ---------------------------------------------------------------------------
// 2.4 — идемпотентный DELETE
// ---------------------------------------------------------------------------
pub async fn delete_upload(base_url: &str, upload_id: &str) -> Result<bool, JsValue> {
    let url = format!(
        "{}/files/{}",
        base_url.trim_end_matches('/'),
        urlencode(upload_id)
    );
    let xhr = XmlHttpRequest::new()?;
    xhr.open_with_async("DELETE", &url, true)?;
    xhr.set_response_type(XmlHttpRequestResponseType::Json);

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let xhr_load = xhr.clone();
        let resolve_cb = Closure::once(move || {
            let status = xhr_load.status().unwrap_or(0);
            let resp = xhr_load.response().unwrap_or(JsValue::UNDEFINED);
            let removed = js_sys::Reflect::get(&resp, &"removed".into())
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let payload = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &payload,
                &"status".into(),
                &JsValue::from_f64(status as f64),
            );
            let _ = js_sys::Reflect::set(
                &payload,
                &"removed".into(),
                &JsValue::from_bool(removed),
            );
            let _ = resolve.call1(&JsValue::NULL, &payload.into());
        });
        xhr.set_onload(Some(resolve_cb.as_ref().unchecked_ref()));
        resolve_cb.forget();

        let reject_cb = Closure::once(move || {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("Сеть недоступна"));
        });
        xhr.set_onerror(Some(reject_cb.as_ref().unchecked_ref()));
        reject_cb.forget();
    });

    xhr.send()?;
    let resp = JsFuture::from(promise).await?;
    let status = js_sys::Reflect::get(&resp, &"status".into())
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u16;
    let removed = js_sys::Reflect::get(&resp, &"removed".into())
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if (200..300).contains(&status) {
        Ok(removed)
    } else {
        Err(JsValue::from_str(&format!("HTTP {}", status)))
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}