//! Функции для работы со Streams API.
//!
//! Модуль библиотечный: содержит всё, что нужно для демо и тестов
//! глав 17.1–17.5. Демо (`app.rs`) использует только своё подмножество,
//! но модуль обслуживает все главы сразу.
//!
//! Разделы:
//! - Источник: `readable_from_strings`, `readable_from_generator`,
//!   `readable_async`, `readable_bytes`.
//! - Приёмник: `writable_with_callbacks`, `writable_into_buffer`.
//! - Чтение: `read_all`, `read_all_strings`.
//! - Утилиты: `stream_is_locked`, `tee_stream`.
//! - Запись: `write_all`.
//! - TransformStream: `transform_with`, `transform_with_flush`,
//!   `transform_with_error`, `line_splitter`, `line_splitter_no_flush`.
//! - Соединение: `call_pipe_through`, `pipe_to`, `process_transform`.
//! - JS → Rust: `js_to_rust_stream`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use futures::channel::mpsc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::fetch::{classify_error, FetchError};

// ============================================================
// Источник данных
// ============================================================

/// Создаёт `ReadableStream` из массива строк.
pub fn readable_from_strings(chunks: Vec<String>) -> Result<web_sys::ReadableStream, JsValue> {
    let queue: Rc<RefCell<VecDeque<String>>> = Rc::new(RefCell::new(chunks.into()));

    let pull_callback = {
        let queue = queue.clone();
        Closure::<dyn FnMut(web_sys::ReadableStreamDefaultController)>::wrap(Box::new(
            move |controller: web_sys::ReadableStreamDefaultController| {
                let mut q = queue.borrow_mut();
                if let Some(chunk) = q.pop_front() {
                    controller
                        .enqueue_with_chunk(&JsValue::from_str(&chunk))
                        .ok();
                } else {
                    controller.close().ok();
                }
            },
        ))
    };

    let source = js_sys::Object::new();
    js_sys::Reflect::set(
        &source,
        &"pull".into(),
        pull_callback.as_ref().unchecked_ref(),
    )
    .ok();
    pull_callback.forget();

    web_sys::ReadableStream::new_with_underlying_source(&source)
}

/// Создаёт **byte stream** (`type: "bytes"`) из массива байтовых чанков.
///
/// Такой поток поддерживает BYOB-чтение (`ReadableStreamBYOBReader`),
/// которое нужно для `wasm-streams::into_async_read()`.
pub fn readable_bytes(chunks: Vec<Vec<u8>>) -> Result<web_sys::ReadableStream, JsValue> {
    let queue: Rc<RefCell<VecDeque<Vec<u8>>>> = Rc::new(RefCell::new(chunks.into()));

    let pull_callback = {
        let queue = queue.clone();
        Closure::<dyn FnMut(JsValue)>::wrap(Box::new(move |controller: JsValue| {
            let mut q = queue.borrow_mut();

            let enqueue = js_sys::Reflect::get(&controller, &"enqueue".into())
                .ok()
                .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
            let close = js_sys::Reflect::get(&controller, &"close".into())
                .ok()
                .and_then(|f| f.dyn_into::<js_sys::Function>().ok());

            if let Some(chunk) = q.pop_front() {
                if let Some(enqueue) = enqueue {
                    let arr = js_sys::Uint8Array::new_with_length(chunk.len() as u32);
                    arr.copy_from(&chunk);
                    enqueue.call1(&controller, &arr).ok();
                }
            } else if let Some(close) = close {
                close.call0(&controller).ok();
            }
        }))
    };

    let source = js_sys::Object::new();
    js_sys::Reflect::set(&source, &"type".into(), &JsValue::from_str("bytes")).ok();
    js_sys::Reflect::set(
        &source,
        &"pull".into(),
        pull_callback.as_ref().unchecked_ref(),
    )
    .ok();
    pull_callback.forget();

    web_sys::ReadableStream::new_with_underlying_source(&source)
}

/// Создаёт `ReadableStream` из Rust-генератора.
pub fn readable_from_generator<F>(next: F) -> Result<web_sys::ReadableStream, JsValue>
where
    F: FnMut() -> Option<String> + 'static,
{
    let next = Rc::new(RefCell::new(next));

    let pull_callback = {
        let next = next.clone();
        Closure::<dyn FnMut(web_sys::ReadableStreamDefaultController)>::wrap(Box::new(
            move |controller: web_sys::ReadableStreamDefaultController| {
                let value = (next.borrow_mut())();
                match value {
                    Some(chunk) => {
                        controller
                            .enqueue_with_chunk(&JsValue::from_str(&chunk))
                            .ok();
                    }
                    None => {
                        controller.close().ok();
                    }
                }
            },
        ))
    };

    let source = js_sys::Object::new();
    js_sys::Reflect::set(
        &source,
        &"pull".into(),
        pull_callback.as_ref().unchecked_ref(),
    )
    .ok();
    pull_callback.forget();

    web_sys::ReadableStream::new_with_underlying_source(&source)
}

/// Создаёт `ReadableStream` с асинхронным `pull`.
pub fn readable_async<F, Fut>(next: F) -> Result<web_sys::ReadableStream, JsValue>
where
    F: FnMut() -> Fut + 'static,
    Fut: std::future::Future<Output = Option<String>> + 'static,
{
    let next = Rc::new(RefCell::new(next));

    let pull_callback =
        Closure::<dyn FnMut(JsValue) -> JsValue>::wrap(Box::new(move |controller: JsValue| {
            let fut = (next.borrow_mut())();
            let controller_for_async = controller.clone();

            let async_block = async move {
                let value = fut.await;

                match value {
                    Some(chunk) => {
                        if let Ok(enqueue) =
                            js_sys::Reflect::get(&controller_for_async, &"enqueue".into())
                                .and_then(|f| f.dyn_into::<js_sys::Function>())
                        {
                            enqueue
                                .call1(&controller_for_async, &JsValue::from_str(&chunk))
                                .ok();
                        }
                    }
                    None => {
                        if let Ok(close) =
                            js_sys::Reflect::get(&controller_for_async, &"close".into())
                                .and_then(|f| f.dyn_into::<js_sys::Function>())
                        {
                            close.call0(&controller_for_async).ok();
                        }
                    }
                }

                Ok(JsValue::UNDEFINED)
            };

            wasm_bindgen_futures::future_to_promise(async_block).into()
        }));

    let source = js_sys::Object::new();
    js_sys::Reflect::set(
        &source,
        &"pull".into(),
        pull_callback.as_ref().unchecked_ref(),
    )
    .ok();
    pull_callback.forget();

    web_sys::ReadableStream::new_with_underlying_source(&source)
}

// ============================================================
// Приёмник данных
// ============================================================

/// Создаёт `WritableStream` с Rust-колбэками.
pub fn writable_with_callbacks<W, C, A>(
    on_write: W,
    on_close: C,
    on_abort: A,
) -> Result<web_sys::WritableStream, JsValue>
where
    W: FnMut(String) + 'static,
    C: FnMut() + 'static,
    A: FnMut(Option<String>) + 'static,
{
    let on_write = Rc::new(RefCell::new(on_write));
    let on_close = Rc::new(RefCell::new(on_close));
    let on_abort = Rc::new(RefCell::new(on_abort));

    let write_callback = {
        let on_write = on_write.clone();
        Closure::<dyn FnMut(JsValue)>::wrap(Box::new(move |chunk: JsValue| {
            let text = chunk.as_string().unwrap_or_default();
            (on_write.borrow_mut())(text);
        }))
    };

    let close_callback = {
        let on_close = on_close.clone();
        Closure::<dyn FnMut()>::wrap(Box::new(move || {
            (on_close.borrow_mut())();
        }))
    };

    let abort_callback = {
        let on_abort = on_abort.clone();
        Closure::<dyn FnMut(JsValue)>::wrap(Box::new(move |reason: JsValue| {
            let reason_str = if reason.is_undefined() || reason.is_null() {
                None
            } else {
                reason.as_string()
            };
            (on_abort.borrow_mut())(reason_str);
        }))
    };

    let sink = js_sys::Object::new();
    js_sys::Reflect::set(
        &sink,
        &"write".into(),
        write_callback.as_ref().unchecked_ref(),
    )
    .ok();
    js_sys::Reflect::set(
        &sink,
        &"close".into(),
        close_callback.as_ref().unchecked_ref(),
    )
    .ok();
    js_sys::Reflect::set(
        &sink,
        &"abort".into(),
        abort_callback.as_ref().unchecked_ref(),
    )
    .ok();

    write_callback.forget();
    close_callback.forget();
    abort_callback.forget();

    web_sys::WritableStream::new_with_underlying_sink(&sink)
}

/// Создаёт `WritableStream`, который складывает чанки в буфер.
pub fn writable_into_buffer() -> Result<(web_sys::WritableStream, Rc<RefCell<Vec<String>>>), JsValue>
{
    let buffer: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let buffer_c = buffer.clone();

    let stream = writable_with_callbacks(
        move |chunk| buffer_c.borrow_mut().push(chunk),
        || {},
        |_| {},
    )?;

    Ok((stream, buffer))
}

// ============================================================
// Чтение потока
// ============================================================

/// Читает поток и возвращает все чанки как `Vec<u8>`.
pub async fn read_all(stream: &web_sys::ReadableStream) -> Result<Vec<u8>, FetchError> {
    let reader: web_sys::ReadableStreamDefaultReader = stream
        .get_reader()
        .dyn_into()
        .map_err(|_| FetchError::Parse("failed to get stream reader".into()))?;

    let mut result = Vec::new();

    loop {
        let promise = reader.read();
        let value = JsFuture::from(promise).await.map_err(classify_error)?;

        let done = js_sys::Reflect::get(&value, &JsValue::from_str("done"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if done {
            break;
        }

        let chunk =
            js_sys::Reflect::get(&value, &JsValue::from_str("value")).map_err(classify_error)?;

        let bytes = if let Ok(arr) = chunk.clone().dyn_into::<js_sys::Uint8Array>() {
            arr.to_vec()
        } else if let Some(s) = chunk.as_string() {
            s.into_bytes()
        } else {
            Vec::new()
        };

        result.extend_from_slice(&bytes);
    }

    reader.release_lock();
    Ok(result)
}

/// Читает поток и возвращает все чанки как `Vec<String>`.
pub async fn read_all_strings(stream: &web_sys::ReadableStream) -> Result<Vec<String>, FetchError> {
    let reader: web_sys::ReadableStreamDefaultReader = stream
        .get_reader()
        .dyn_into()
        .map_err(|_| FetchError::Parse("failed to get stream reader".into()))?;

    let mut result = Vec::new();

    loop {
        let promise = reader.read();
        let value = JsFuture::from(promise).await.map_err(classify_error)?;

        let done = js_sys::Reflect::get(&value, &JsValue::from_str("done"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if done {
            break;
        }

        let chunk =
            js_sys::Reflect::get(&value, &JsValue::from_str("value")).map_err(classify_error)?;

        let text = if let Some(s) = chunk.as_string() {
            s
        } else if let Ok(arr) = chunk.clone().dyn_into::<js_sys::Uint8Array>() {
            String::from_utf8_lossy(&arr.to_vec()).to_string()
        } else {
            String::new()
        };

        result.push(text);
    }

    reader.release_lock();
    Ok(result)
}

// ============================================================
// Утилиты
// ============================================================

/// Возвращает `true`, если у потока есть активный reader.
pub fn stream_is_locked(stream: &web_sys::ReadableStream) -> bool {
    stream.locked()
}

/// Разветвляет поток на два независимых.
pub fn tee_stream(
    stream: &web_sys::ReadableStream,
) -> Result<(web_sys::ReadableStream, web_sys::ReadableStream), FetchError> {
    let fn_val = js_sys::Reflect::get(stream, &JsValue::from_str("tee")).map_err(classify_error)?;

    let func: js_sys::Function = fn_val
        .dyn_into()
        .map_err(|_| FetchError::Parse("tee is not a function".into()))?;

    let result = func.call0(stream).map_err(classify_error)?;

    let array: js_sys::Array = result
        .dyn_into()
        .map_err(|_| FetchError::Parse("tee did not return an Array".into()))?;

    if array.length() != 2 {
        return Err(FetchError::Parse(format!(
            "tee returned array of length {}, expected 2",
            array.length()
        )));
    }

    let first: web_sys::ReadableStream = array
        .get(0)
        .dyn_into()
        .map_err(|_| FetchError::Parse("first branch is not a ReadableStream".into()))?;

    let second: web_sys::ReadableStream = array
        .get(1)
        .dyn_into()
        .map_err(|_| FetchError::Parse("second branch is not a ReadableStream".into()))?;

    Ok((first, second))
}

// ============================================================
// Запись
// ============================================================

/// Записывает все чанки в поток и закрывает его.
pub async fn write_all(
    stream: &web_sys::WritableStream,
    chunks: Vec<String>,
) -> Result<(), FetchError> {
    let writer: web_sys::WritableStreamDefaultWriter =
        stream.get_writer().map_err(classify_error)?;

    for chunk in chunks {
        let promise = writer.write_with_chunk(&JsValue::from_str(&chunk));
        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(classify_error)?;
    }

    let close_promise = writer.close();
    wasm_bindgen_futures::JsFuture::from(close_promise)
        .await
        .map_err(classify_error)?;

    Ok(())
}

// ============================================================
// TransformStream
// ============================================================

/// Создаёт `TransformStream` с простым преобразованием.
pub fn transform_with<F>(transform: F) -> Result<web_sys::TransformStream, JsValue>
where
    F: FnMut(String) -> String + 'static,
{
    let transform = Rc::new(RefCell::new(transform));

    let transform_callback = {
        let transform = transform.clone();
        Closure::<dyn FnMut(JsValue, web_sys::TransformStreamDefaultController)>::wrap(Box::new(
            move |chunk: JsValue, controller: web_sys::TransformStreamDefaultController| {
                let input = chunk.as_string().unwrap_or_default();
                let output = (transform.borrow_mut())(input);
                controller
                    .enqueue_with_chunk(&JsValue::from_str(&output))
                    .ok();
            },
        ))
    };

    let transformer = js_sys::Object::new();
    js_sys::Reflect::set(
        &transformer,
        &"transform".into(),
        transform_callback.as_ref().unchecked_ref(),
    )
    .ok();
    transform_callback.forget();

    web_sys::TransformStream::new_with_transformer(&transformer)
}

/// Создаёт `TransformStream` с `transform` и `flush`.
pub fn transform_with_flush<T, F>(
    transform: T,
    flush: F,
) -> Result<web_sys::TransformStream, JsValue>
where
    T: FnMut(String) -> Option<String> + 'static,
    F: FnMut() -> Option<String> + 'static,
{
    let transform = Rc::new(RefCell::new(transform));
    let flush = Rc::new(RefCell::new(flush));

    let transform_callback = {
        let transform = transform.clone();
        Closure::<dyn FnMut(JsValue, web_sys::TransformStreamDefaultController)>::wrap(Box::new(
            move |chunk: JsValue, controller: web_sys::TransformStreamDefaultController| {
                let input = chunk.as_string().unwrap_or_default();
                if let Some(output) = (transform.borrow_mut())(input) {
                    controller
                        .enqueue_with_chunk(&JsValue::from_str(&output))
                        .ok();
                }
            },
        ))
    };

    let flush_callback = {
        let flush = flush.clone();
        Closure::<dyn FnMut(web_sys::TransformStreamDefaultController)>::wrap(Box::new(
            move |controller: web_sys::TransformStreamDefaultController| {
                if let Some(output) = (flush.borrow_mut())() {
                    controller
                        .enqueue_with_chunk(&JsValue::from_str(&output))
                        .ok();
                }
            },
        ))
    };

    let transformer = js_sys::Object::new();
    js_sys::Reflect::set(
        &transformer,
        &"transform".into(),
        transform_callback.as_ref().unchecked_ref(),
    )
    .ok();
    js_sys::Reflect::set(
        &transformer,
        &"flush".into(),
        flush_callback.as_ref().unchecked_ref(),
    )
    .ok();

    transform_callback.forget();
    flush_callback.forget();

    web_sys::TransformStream::new_with_transformer(&transformer)
}

/// Создаёт `TransformStream`, где `transform` может сообщить об ошибке.
pub fn transform_with_error<F>(transform: F) -> Result<web_sys::TransformStream, JsValue>
where
    F: FnMut(String) -> Result<String, String> + 'static,
{
    let transform = Rc::new(RefCell::new(transform));

    let transform_callback = {
        let transform = transform.clone();
        Closure::<dyn FnMut(JsValue, web_sys::TransformStreamDefaultController)>::wrap(Box::new(
            move |chunk: JsValue, controller: web_sys::TransformStreamDefaultController| {
                let input = chunk.as_string().unwrap_or_default();
                match (transform.borrow_mut())(input) {
                    Ok(output) => {
                        controller
                            .enqueue_with_chunk(&JsValue::from_str(&output))
                            .ok();
                    }
                    Err(reason) => {
                        if let Ok(val) = js_sys::Reflect::get(&controller, &"error".into()) {
                            if let Ok(func) = val.dyn_into::<js_sys::Function>() {
                                func.call1(&controller, &JsValue::from_str(&reason)).ok();
                            }
                        }
                    }
                }
            },
        ))
    };

    let transformer = js_sys::Object::new();
    js_sys::Reflect::set(
        &transformer,
        &"transform".into(),
        transform_callback.as_ref().unchecked_ref(),
    )
    .ok();
    transform_callback.forget();

    web_sys::TransformStream::new_with_transformer(&transformer)
}

// ============================================================
// Line splitter
// ============================================================

/// Возвращает `TransformStream`, который разбивает входной поток
/// на строки по `\n`. Остаток отдаётся в `flush`.
pub fn line_splitter() -> Result<web_sys::TransformStream, JsValue> {
    let buffer = Rc::new(RefCell::new(String::new()));

    let buffer_transform = buffer.clone();
    let buffer_flush = buffer.clone();

    transform_with_flush(
        move |chunk: String| {
            let mut buf = buffer_transform.borrow_mut();
            buf.push_str(&chunk);

            let mut result = String::new();
            while let Some(pos) = buf.find('\n') {
                let line: String = buf.drain(..=pos).collect();
                result.push_str(&line);
            }

            if result.is_empty() {
                None
            } else {
                Some(result)
            }
        },
        move || {
            let mut buf = buffer_flush.borrow_mut();
            if buf.is_empty() {
                None
            } else {
                let remaining = std::mem::take(&mut *buf);
                Some(remaining)
            }
        },
    )
}

/// То же, что `line_splitter`, но **без** `flush`.
pub fn line_splitter_no_flush() -> Result<web_sys::TransformStream, JsValue> {
    let buffer = Rc::new(RefCell::new(String::new()));

    let buffer_transform = buffer.clone();

    transform_with_flush(
        move |chunk: String| {
            let mut buf = buffer_transform.borrow_mut();
            buf.push_str(&chunk);

            let mut result = String::new();
            while let Some(pos) = buf.find('\n') {
                let line: String = buf.drain(..=pos).collect();
                result.push_str(&line);
            }

            if result.is_empty() {
                None
            } else {
                Some(result)
            }
        },
        || None,
    )
}

// ============================================================
// Прогон TransformStream
// ============================================================

/// Прогоняет `input` через `TransformStream` и возвращает выходные чанки.
pub async fn process_transform(
    transform: web_sys::TransformStream,
    input: Vec<String>,
) -> Result<Vec<String>, FetchError> {
    let readable = transform.readable();
    let writable = transform.writable();

    let writer: web_sys::WritableStreamDefaultWriter =
        writable.get_writer().map_err(classify_error)?;

    let write_task = async move {
        for chunk in input {
            let _ = wasm_bindgen_futures::JsFuture::from(
                writer.write_with_chunk(&JsValue::from_str(&chunk)),
            )
            .await;
        }
        let _ = wasm_bindgen_futures::JsFuture::from(writer.close()).await;
    };

    let read_task = async move { read_all_strings(&readable).await };

    let (_, read_result) = futures::future::join(write_task, read_task).await;
    read_result
}

// ============================================================
// pipeThrough и pipeTo
// ============================================================

/// Соединяет `ReadableStream` с `TransformStream` через `pipeThrough`.
pub fn call_pipe_through(
    source: &web_sys::ReadableStream,
    transform: &web_sys::TransformStream,
) -> Result<web_sys::ReadableStream, FetchError> {
    let fn_val =
        js_sys::Reflect::get(source, &JsValue::from_str("pipeThrough")).map_err(classify_error)?;

    let func: js_sys::Function = fn_val
        .dyn_into()
        .map_err(|_| FetchError::Parse("pipeThrough is not a function".into()))?;

    let result = func.call1(source, transform).map_err(classify_error)?;

    result
        .dyn_into()
        .map_err(|_| FetchError::Parse("pipeThrough result is not ReadableStream".into()))
}

/// Соединяет `ReadableStream` с `WritableStream` через `pipeTo`
/// с опциями.
pub async fn pipe_to(
    source: &web_sys::ReadableStream,
    sink: &web_sys::WritableStream,
    prevent_close: bool,
    prevent_abort: bool,
    prevent_cancel: bool,
) -> Result<(), FetchError> {
    let fn_val =
        js_sys::Reflect::get(source, &JsValue::from_str("pipeTo")).map_err(classify_error)?;

    let func: js_sys::Function = fn_val
        .dyn_into()
        .map_err(|_| FetchError::Parse("pipeTo is not a function".into()))?;

    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &"preventClose".into(),
        &JsValue::from_bool(prevent_close),
    )
    .ok();
    js_sys::Reflect::set(
        &options,
        &"preventAbort".into(),
        &JsValue::from_bool(prevent_abort),
    )
    .ok();
    js_sys::Reflect::set(
        &options,
        &"preventCancel".into(),
        &JsValue::from_bool(prevent_cancel),
    )
    .ok();

    let promise = func.call2(source, sink, &options).map_err(classify_error)?;

    let promise: js_sys::Promise = promise
        .dyn_into()
        .map_err(|_| FetchError::Parse("pipeTo did not return a Promise".into()))?;

    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(classify_error)?;

    Ok(())
}

// ============================================================
// JS-поток → Rust Stream через mpsc
// ============================================================

/// Преобразует `web_sys::ReadableStream` в `futures::Stream` через `mpsc`.
pub fn js_to_rust_stream(
    stream: web_sys::ReadableStream,
) -> impl futures::Stream<Item = Result<Vec<u8>, FetchError>> {
    let (tx, rx) = mpsc::unbounded::<Result<Vec<u8>, FetchError>>();

    wasm_bindgen_futures::spawn_local(async move {
        let reader: web_sys::ReadableStreamDefaultReader = match stream.get_reader().dyn_into() {
            Ok(r) => r,
            Err(_) => {
                let _ =
                    tx.unbounded_send(Err(FetchError::Parse("failed to get stream reader".into())));
                return;
            }
        };

        loop {
            let promise = reader.read();
            let value = match JsFuture::from(promise).await {
                Ok(v) => v,
                Err(e) => {
                    let _ = tx.unbounded_send(Err(classify_error(e)));
                    break;
                }
            };

            let done = js_sys::Reflect::get(&value, &JsValue::from_str("done"))
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if done {
                break;
            }

            let chunk = match js_sys::Reflect::get(&value, &JsValue::from_str("value")) {
                Ok(v) => v,
                Err(e) => {
                    let _ = tx.unbounded_send(Err(classify_error(e)));
                    break;
                }
            };

            let bytes = if let Ok(arr) = chunk.clone().dyn_into::<js_sys::Uint8Array>() {
                arr.to_vec()
            } else if let Some(s) = chunk.as_string() {
                s.into_bytes()
            } else {
                Vec::new()
            };

            if tx.unbounded_send(Ok(bytes)).is_err() {
                break;
            }
        }

        reader.release_lock();
    });

    rx
}

// ============================================================
// 17.4: Backpressure и очереди
// ============================================================

/// Создаёт `CountQueuingStrategy` — каждый чанк считается за 1.
///
/// Используем Reflect + JS-конструктор, а не `js_sys::eval` с
/// `dyn_into`: `QueuingStrategy` — это интерфейс, а не класс,
/// у него нет `instanceof`. Приведение делаем через `unchecked_into`.
pub fn count_strategy(high_water_mark: u32) -> Result<web_sys::QueuingStrategy, JsValue> {
    make_queuing_strategy("CountQueuingStrategy", high_water_mark)
}

/// Создаёт `ByteLengthQueuingStrategy` — размер в байтах.
pub fn byte_length_strategy(high_water_mark: u32) -> Result<web_sys::QueuingStrategy, JsValue> {
    make_queuing_strategy("ByteLengthQueuingStrategy", high_water_mark)
}

/// Общая логика: получить JS-конструктор по имени, создать init,
/// вызвать `new Ctor(init)`, привести к `QueuingStrategy`.
fn make_queuing_strategy(
    name: &str,
    high_water_mark: u32,
) -> Result<web_sys::QueuingStrategy, JsValue> {
    let ctor_value = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str(name))?;

    let ctor: js_sys::Function = ctor_value
        .dyn_into()
        .map_err(|_| JsValue::from_str(&format!("{name} is not a constructor")))?;

    let init = js_sys::Object::new();
    js_sys::Reflect::set(
        &init,
        &"highWaterMark".into(),
        &JsValue::from_f64(high_water_mark as f64),
    )?;

    // Вызов конструктора через Reflect::construct.
    // Метод Function::construct появился только в свежих версиях js-sys.
    let instance = js_sys::Reflect::construct(ctor.as_ref(), &js_sys::Array::of1(&init))?;
    Ok(instance.unchecked_into::<web_sys::QueuingStrategy>())
}

/// Создаёт `ReadableStream` из массива строк с заданной стратегией.
pub fn readable_with_strategy(
    chunks: Vec<String>,
    strategy: &web_sys::QueuingStrategy,
) -> Result<web_sys::ReadableStream, JsValue> {
    readable_with_pull_callback(chunks, strategy, |_| {})
}

/// Создаёт `ReadableStream` с колбэком на каждый `pull`.
///
/// `on_pull` получает `desiredSize` перед `enqueue`. Удобно для
/// демонстрации: видно, как очередь меняется по ходу чтения.
pub fn readable_with_pull_callback<F>(
    chunks: Vec<String>,
    strategy: &web_sys::QueuingStrategy,
    on_pull: F,
) -> Result<web_sys::ReadableStream, JsValue>
where
    F: FnMut(Option<f64>) + 'static,
{
    let queue: Rc<RefCell<VecDeque<String>>> = Rc::new(RefCell::new(chunks.into()));
    let on_pull = Rc::new(RefCell::new(on_pull));

    let pull_callback = {
        let queue = queue.clone();
        let on_pull = on_pull.clone();
        Closure::<dyn FnMut(web_sys::ReadableStreamDefaultController)>::wrap(Box::new(
            move |controller: web_sys::ReadableStreamDefaultController| {
                // desired_size() может возвращать Result<Option<f64>, JsValue>
                // в зависимости от версии web-sys. Приводим к Option<f64>.
                let desired: Option<f64> = controller.desired_size();
                (on_pull.borrow_mut())(desired);

                let mut q = queue.borrow_mut();
                if let Some(chunk) = q.pop_front() {
                    controller
                        .enqueue_with_chunk(&JsValue::from_str(&chunk))
                        .ok();
                } else {
                    controller.close().ok();
                }
            },
        ))
    };

    let source = js_sys::Object::new();
    js_sys::Reflect::set(
        &source,
        &"pull".into(),
        pull_callback.as_ref().unchecked_ref(),
    )?;
    pull_callback.forget();

    // Вызываем JS-конструктор ReadableStream через Reflect.
    // Не полагаемся на точное имя метода в web-sys: сигнатура
    // с (source, strategy) есть не во всех версиях.
    let ctor_value = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("ReadableStream"))?;

    let ctor: js_sys::Function = ctor_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("ReadableStream is not a constructor"))?;

    let args = js_sys::Array::new();
    args.push(&source);
    args.push(strategy);

    let stream = js_sys::Reflect::construct(ctor.as_ref(), &args)?;
    Ok(stream.unchecked_into::<web_sys::ReadableStream>())
}

/// Создаёт `WritableStream` в буфер с заданным `highWaterMark`.
pub fn writable_with_strategy(
    high_water_mark: u32,
) -> Result<(web_sys::WritableStream, Rc<RefCell<Vec<String>>>), JsValue> {
    let buffer: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let buffer_c = buffer.clone();

    let write_callback = Closure::<dyn FnMut(JsValue)>::wrap(Box::new(move |chunk: JsValue| {
        let text = chunk.as_string().unwrap_or_default();
        buffer_c.borrow_mut().push(text);
    }));

    let close_callback = Closure::<dyn FnMut()>::wrap(Box::new(|| {}));

    let sink = js_sys::Object::new();
    js_sys::Reflect::set(
        &sink,
        &"write".into(),
        write_callback.as_ref().unchecked_ref(),
    )?;
    js_sys::Reflect::set(
        &sink,
        &"close".into(),
        close_callback.as_ref().unchecked_ref(),
    )?;
    write_callback.forget();
    close_callback.forget();

    let strategy = count_strategy(high_water_mark)?;

    // WritableStream через Reflect — та же причина, что и выше.
    let ctor_value = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("WritableStream"))?;

    let ctor: js_sys::Function = ctor_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("WritableStream is not a constructor"))?;

    let args = js_sys::Array::new();
    args.push(&sink);
    args.push(&strategy);

    let stream_value = js_sys::Reflect::construct(ctor.as_ref(), &args)?;
    let stream: web_sys::WritableStream = stream_value.unchecked_into();

    Ok((stream, buffer))
}

/// Создаёт **byte stream** (`type: "bytes"`) с заданным HWM.
///
/// Byte stream не принимает `size()` в стратегии: размер каждого
/// чанка вычисляется автоматически как `byteLength`. Поэтому
/// мы формируем простую стратегию `{ highWaterMark: N }` внутри.
pub fn readable_bytes_with_strategy(
    chunks: Vec<Vec<u8>>,
    high_water_mark: u32,
) -> Result<web_sys::ReadableStream, JsValue> {
    let queue: Rc<RefCell<VecDeque<Vec<u8>>>> = Rc::new(RefCell::new(chunks.into()));

    let pull_callback = {
        let queue = queue.clone();
        Closure::<dyn FnMut(web_sys::ReadableStreamDefaultController)>::wrap(Box::new(
            move |controller: web_sys::ReadableStreamDefaultController| {
                let mut q = queue.borrow_mut();
                if let Some(chunk) = q.pop_front() {
                    let arr = js_sys::Uint8Array::new_with_length(chunk.len() as u32);
                    arr.copy_from(&chunk);
                    controller.enqueue_with_chunk(&arr).ok();
                } else {
                    controller.close().ok();
                }
            },
        ))
    };

    let source = js_sys::Object::new();
    js_sys::Reflect::set(&source, &"type".into(), &JsValue::from_str("bytes"))?;
    js_sys::Reflect::set(
        &source,
        &"pull".into(),
        pull_callback.as_ref().unchecked_ref(),
    )?;
    pull_callback.forget();

    // Plain strategy: только highWaterMark, без size().
    let strategy = plain_strategy(high_water_mark)?;

    let ctor_value =
        js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("ReadableStream"))?;

    let ctor: js_sys::Function = ctor_value
        .dyn_into()
        .map_err(|_| JsValue::from_str("ReadableStream is not a constructor"))?;

    let args = js_sys::Array::new();
    args.push(&source);
    args.push(&strategy);

    let stream = js_sys::Reflect::construct(ctor.as_ref(), &args)?;
    Ok(stream.unchecked_into::<web_sys::ReadableStream>())
}

/// Создаёт простую стратегию `{ highWaterMark: N }` без `size()`.
///
/// Нужна для byte streams: спецификация Streams запрещает
/// передавать `size()` вместе с `type: "bytes"`. Размер чанка
/// в byte stream вычисляется автоматически как `byteLength`.
pub fn plain_strategy(high_water_mark: u32) -> Result<web_sys::QueuingStrategy, JsValue> {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &"highWaterMark".into(),
        &JsValue::from_f64(high_water_mark as f64),
    )?;
    Ok(obj.unchecked_into::<web_sys::QueuingStrategy>())
}

// ============================================================
// 17.6: Byte streams и BYOB
// ============================================================

/// Создаёт **byte stream** с полной поддержкой BYOB-читателя.
///
/// В `pull`-колбэке проверяет наличие `controller.byobRequest`:
/// - если есть — пишет данные прямо в `byobRequest.view` и вызывает
///   `respond(n)`;
/// - если нет — работает как обычный `enqueue` для default reader.
///
/// При пустой очереди вызывает `controller.close()`. По спецификации
/// этот вызов сам разрешает pending `byobRequest` через `respond(0)` —
/// дополнительно вызывать `respond(0)` не нужно.
pub fn readable_bytes_with_byob_support(
    chunks: Vec<Vec<u8>>,
) -> Result<web_sys::ReadableStream, JsValue> {
    let queue: Rc<RefCell<VecDeque<Vec<u8>>>> = Rc::new(RefCell::new(chunks.into()));

    let pull_callback = {
        let queue = queue.clone();
        Closure::<dyn FnMut(JsValue)>::wrap(Box::new(move |controller: JsValue| {
            let mut q = queue.borrow_mut();

            // Проверяем, ждёт ли BYOB-читатель со своим буфером.
            let byob = js_sys::Reflect::get(&controller, &JsValue::from_str("byobRequest"))
                .ok()
                .filter(|v| !v.is_null() && !v.is_undefined());

            if let Some(req) = byob {
                let view_opt = js_sys::Reflect::get(&req, &JsValue::from_str("view"))
                    .ok()
                    .and_then(|v| v.dyn_into::<js_sys::Uint8Array>().ok());

                if let Some(view) = view_opt {
                    if let Some(chunk) = q.pop_front() {
                        let n = chunk.len().min(view.length() as usize);
                        view.subarray(0, n as u32).copy_from(&chunk[..n]);

                        if let Ok(respond) =
                            js_sys::Reflect::get(&req, &JsValue::from_str("respond"))
                                .and_then(|f| f.dyn_into::<js_sys::Function>())
                        {
                            respond.call1(&req, &JsValue::from_f64(n as f64)).ok();
                        }

                        // Если данных больше, чем поместилось, — вернуть
                        // остаток в очередь. Следующий pull отдаст его.
                        if n < chunk.len() {
                            q.push_front(chunk[n..].to_vec());
                        }
                        return;
                                        } else {
                        // Очередь пуста. Порядок важен:
                        // 1. close() — устанавливает closeRequested = true.
                        //    Сам по себе он не разрешает pending byobRequest.
                        // 2. respond(0) — разрешает pending-буфер читателя
                        //    с {done: true}. Требует closeRequested = true,
                        //    поэтому вызывается после close().
                        if let Ok(close) =
                            js_sys::Reflect::get(&controller, &JsValue::from_str("close"))
                                .and_then(|f| f.dyn_into::<js_sys::Function>())
                        {
                            close.call0(&controller).ok();
                        }

                        if let Ok(respond) =
                            js_sys::Reflect::get(&req, &JsValue::from_str("respond"))
                                .and_then(|f| f.dyn_into::<js_sys::Function>())
                        {
                            respond.call1(&req, &JsValue::from_f64(0.0)).ok();
                        }
                        return;
                    }
                }
            }

            // Default reader: работаем через enqueue.
            if let Some(chunk) = q.pop_front() {
                if let Ok(enqueue) =
                    js_sys::Reflect::get(&controller, &JsValue::from_str("enqueue"))
                        .and_then(|f| f.dyn_into::<js_sys::Function>())
                {
                    let arr = js_sys::Uint8Array::new_with_length(chunk.len() as u32);
                    arr.copy_from(&chunk);
                    enqueue.call1(&controller, &arr).ok();
                }
            } else if let Ok(close) =
                js_sys::Reflect::get(&controller, &JsValue::from_str("close"))
                    .and_then(|f| f.dyn_into::<js_sys::Function>())
            {
                close.call0(&controller).ok();
            }
        }))
    };

    let source = js_sys::Object::new();
    js_sys::Reflect::set(
        &source,
        &JsValue::from_str("type"),
        &JsValue::from_str("bytes"),
    )?;
    js_sys::Reflect::set(
        &source,
        &JsValue::from_str("pull"),
        pull_callback.as_ref().unchecked_ref(),
    )?;
    pull_callback.forget();

    let ctor_val =
        js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("ReadableStream"))?;
    let ctor: js_sys::Function = ctor_val
        .dyn_into()
        .map_err(|_| JsValue::from_str("ReadableStream is not a constructor"))?;

    let stream = js_sys::Reflect::construct(ctor.as_ref(), &js_sys::Array::of1(&source))?;
    Ok(stream.unchecked_into::<web_sys::ReadableStream>())
}

/// Читает byte stream через BYOB-читатель с буфером заданного размера.
///
/// `reader.read(buffer)` возвращает `Uint8Array` с `byteLength`,
/// равным фактически прочитанным байтам. Мы складываем их в `Vec<u8>`.
/// Когда поток закрыт, `read` возвращает `{done: true}` — цикл
/// завершается.
///
/// `buffer_size` задаёт размер буфера на одну операцию чтения. Если
/// чанк в потоке больше, он будет отдан несколькими порциями.
pub async fn read_byob(
    stream: &web_sys::ReadableStream,
    buffer_size: usize,
) -> Result<Vec<u8>, FetchError> {
    // stream.getReader({ mode: "byob" }) — через Reflect, потому что
    // web-sys не предоставляет типизированного метода с опциями.
    let get_reader_val = js_sys::Reflect::get(stream, &JsValue::from_str("getReader"))
        .map_err(classify_error)?;
    let get_reader: js_sys::Function = get_reader_val
        .dyn_into()
        .map_err(|_| FetchError::Parse("getReader is not a function".into()))?;

    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("mode"),
        &JsValue::from_str("byob"),
    )
    .map_err(classify_error)?;

    let reader: JsValue = get_reader
        .call1(stream.as_ref(), &options)
        .map_err(classify_error)?;

    let mut result = Vec::new();

    loop {
        // Свежий буфер на каждый read — после respond(n) переданный
        // буфер может быть отсоединён, повторное использование
        // небезопасно.
        let buffer = js_sys::Uint8Array::new_with_length(buffer_size as u32);

        let read_fn_val =
            js_sys::Reflect::get(&reader, &JsValue::from_str("read")).map_err(classify_error)?;
        let read_fn: js_sys::Function = read_fn_val
            .dyn_into()
            .map_err(|_| FetchError::Parse("read is not a function".into()))?;

        let promise_val = read_fn
            .call1(&reader, buffer.as_ref())
            .map_err(classify_error)?;
        let promise: js_sys::Promise = promise_val
            .dyn_into()
            .map_err(|_| FetchError::Parse("read did not return a Promise".into()))?;

        let result_value = JsFuture::from(promise).await.map_err(classify_error)?;

        let done = js_sys::Reflect::get(&result_value, &JsValue::from_str("done"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if done {
            break;
        }

        let value = js_sys::Reflect::get(&result_value, &JsValue::from_str("value"))
            .map_err(classify_error)?;
        let arr: js_sys::Uint8Array = value
            .dyn_into()
            .map_err(|_| FetchError::Parse("value is not a Uint8Array".into()))?;

        result.extend_from_slice(&arr.to_vec());
    }

    // Освобождаем читателя, чтобы поток можно было переиспользовать.
    if let Ok(release) =
        js_sys::Reflect::get(&reader, &JsValue::from_str("releaseLock"))
            .and_then(|f| f.dyn_into::<js_sys::Function>())
    {
        release.call0(&reader).ok();
    }

    Ok(result)
}

// ============================================================
// 17.7: LineReader и progress-callback
// ============================================================

/// Читает `AsyncRead` и отдаёт строки по одной.
///
/// Внутри — буфер `Vec<u8>`. На каждом `next_line()` ищет `\n`,
/// извлекает строку, декодирует в `String`. Если `\n` нет,
/// читает следующий кусок из reader'а.
///
/// Корректно обрабатывает:
/// - последнюю строку без завершающего `\n`;
/// - Windows-переводы строк (`\r\n`);
/// - ошибки чтения и некорректный UTF-8 (возвращает `FetchError`).
pub struct LineReader<R> {
    reader: R,
    buffer: Vec<u8>,
}

impl<R> LineReader<R>
where
    R: futures::io::AsyncRead + Unpin,
{
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Vec::new(),
        }
    }

    /// Возвращает следующую строку без `\n`.
    ///
    /// `Ok(Some(line))` — строка получена.
    /// `Ok(None)` — поток закрыт, строк больше нет.
    /// `Err(_)` — ошибка чтения или декодирования.
    pub async fn next_line(&mut self) -> Result<Option<String>, FetchError> {
        use futures::io::AsyncReadExt;

        loop {
            // Ищем \n в буфере.
            if let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
                let mut line_bytes: Vec<u8> = self.buffer.drain(..=pos).collect();
                // Убираем завершающий \n.
                line_bytes.pop();
                // Убираем \r перед \n (Windows).
                if line_bytes.last() == Some(&b'\r') {
                    line_bytes.pop();
                }
                let s = String::from_utf8(line_bytes).map_err(|e| {
                    FetchError::Parse(format!("UTF-8 error in line: {e}"))
                })?;
                return Ok(Some(s));
            }

            // \n нет — читаем следующий кусок.
            let mut chunk = [0u8; 4096];
            match self.reader.read(&mut chunk).await {
                Ok(0) => {
                    // EOF.
                    if self.buffer.is_empty() {
                        return Ok(None);
                    }
                    let line_bytes = std::mem::take(&mut self.buffer);
                    let s = String::from_utf8(line_bytes).map_err(|e| {
                        FetchError::Parse(format!("UTF-8 error in last line: {e}"))
                    })?;
                    return Ok(Some(s));
                }
                Ok(n) => {
                    self.buffer.extend_from_slice(&chunk[..n]);
                }
                Err(e) => {
                    return Err(FetchError::Network(format!(
                        "AsyncRead error in LineReader: {e}"
                    )));
                }
            }
        }
    }
}

/// Читает `AsyncRead` целиком в `Vec<u8>`, вызывая `on_chunk`
/// после каждого успешного чтения.
///
/// Колбэк получает накопленное количество байт. Удобно для
/// прогресс-бара: не нужно самому считать.
pub async fn read_all_async_with_progress<R, F>(
    mut reader: R,
    mut on_chunk: F,
) -> Result<Vec<u8>, FetchError>
where
    R: futures::io::AsyncRead + Unpin,
    F: FnMut(usize),
{
    use futures::io::AsyncReadExt;

    let mut result = Vec::new();
    let mut chunk = [0u8; 4096];

    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                result.extend_from_slice(&chunk[..n]);
                on_chunk(result.len());
            }
            Err(e) => {
                return Err(FetchError::Network(format!(
                    "AsyncRead error: {e}"
                )));
            }
        }
    }

    Ok(result)
}