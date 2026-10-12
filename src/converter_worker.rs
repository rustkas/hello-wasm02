use futures::future::join_all;
use js_sys::{Array, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, MessageEvent, Url, Worker};

const WORKER_JS: &str = r#"
    self.onmessage = function(e) {
        const d = e.data;
        const rgba = new Uint8ClampedArray(d.buffer);
        const out = new Uint8ClampedArray(rgba.length);
        for (let i = 0; i < rgba.length; i += 4) {
            const y = (0.299 * rgba[i] + 0.587 * rgba[i+1] + 0.114 * rgba[i+2]) | 0;
            out[i] = y; out[i+1] = y; out[i+2] = y; out[i+3] = rgba[i+3];
        }
        self.postMessage({ id: d.id, buffer: out.buffer }, [out.buffer]);
    };
"#;

fn spawn_worker() -> Result<Worker, JsValue> {
    let parts = Array::of1(&JsValue::from_str(WORKER_JS));
    let blob = Blob::new_with_str_sequence(&parts)?;
    let url = Url::create_object_url_with_blob(&blob)?;
    let w = Worker::new(&url)?;
    Url::revoke_object_url(&url)?;
    Ok(w)
}

/// Прогнать один буфер через один worker. Трансфер через ArrayBuffer
/// (второй аргумент postMessage) — чтобы не копировать лишний раз.
async fn process_in_worker(worker: Worker, id: u32, rgba: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    let u8arr = Uint8Array::from(rgba.as_slice());
    let buf = u8arr.buffer();

    // Формируем сообщение { id, buffer } и передаём buffer как transferable.
    let msg = Object::new();
    Reflect::set(&msg, &"id".into(), &JsValue::from_f64(id as f64))?;
    Reflect::set(&msg, &"buffer".into(), &buf)?;

    // Worker::post_message не поддерживает transfer — вызываем через Reflect.
    let post = Reflect::get(&worker, &"postMessage".into())?
        .dyn_into::<js_sys::Function>()?;
    let transfer = Array::of1(&buf);
    post.call2(&worker, &msg.into(), &transfer.into())?;

    // Ждём ответа через Promise, повешенный на onmessage/onerror.
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let on_msg = Closure::once(move |e: MessageEvent| {
            let data = e.data();
            let buf = Reflect::get(&data, &"buffer".into()).unwrap_or(JsValue::UNDEFINED);
            let _ = resolve.call1(&JsValue::NULL, &buf);
        });
        worker.set_onmessage(Some(on_msg.as_ref().unchecked_ref()));
        on_msg.forget();

        let on_err = Closure::once(move |e: JsValue| {
            let _ = reject.call1(&JsValue::NULL, &e);
        });
        worker.set_onerror(Some(on_err.as_ref().unchecked_ref()));
        on_err.forget();
    });

    let result = JsFuture::from(promise).await?;
    let arr = Uint8Array::new(&result);
    // 5.1: воркер не должен висеть после ответа.
    worker.terminate();
    Ok(arr.to_vec())
}

/// 5.1: grayscale в одном worker.
pub async fn grayscale_in_worker(rgba: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    let worker = spawn_worker()?;
    process_in_worker(worker, 0, rgba).await
}

/// 5.2: параллельная обработка N stripes в N worker'ах.
/// N адаптивно подбирается под ядра браузера (правило проекта: параллелизм — параметр).
pub async fn grayscale_parallel(rgba: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    let n = worker_count().max(1);
    let total = rgba.len();
    // 4 байта на пиксель — режем по границе пикселя.
    let stripe = (total / n + 3) & !3;
    if stripe == 0 || total == 0 {
        return grayscale_in_worker(rgba).await;
    }

    let mut futures = Vec::with_capacity(n);
    for i in 0..n {
        let start = i * stripe;
        if start >= total {
            break;
        }
        let end = ((i + 1) * stripe).min(total);
        let slice = rgba[start..end].to_vec();
        let w = spawn_worker()?;
        futures.push(async move {
            let out = process_in_worker(w, i as u32, slice).await?;
            Ok::<(usize, Vec<u8>), JsValue>((start, out))
        });
    }

    let parts = join_all(futures).await;
    // Склеиваем в исходном порядке.
    let mut result = vec![0u8; total];
    for p in parts {
        let (offset, bytes) = p?;
        let len = bytes.len();
        result[offset..offset + len].copy_from_slice(&bytes);
    }
    Ok(result)
}

fn worker_count() -> usize {
    let mut n = 2usize;
    if let Some(w) = web_sys::window() {
        if let Ok(nav) = Reflect::get(&w, &"navigator".into()) {
            if let Ok(hc) = Reflect::get(&nav, &"hardwareConcurrency".into()) {
                if let Some(v) = hc.as_f64() {
                    n = (v as usize).min(4).max(1);
                }
            }
        }
    }
    n
}