//! Реализация записи: открыть сессию, писать, фиксировать, откатывать.
//!
//! Все вызовы через `Reflect::get` + `Function::call*` — сигнатуры методов
//! writable-потока нестабильны между версиями web-sys.
//!
//! `impl Drop` откатывает незакоммиченную сессию через `abort()`.

use js_sys::{Function, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::FileSystemFileHandle;

use crate::fsa_choose_options::format_str;
use crate::fsa_write_session::{CommitStats, NaiveStats, SessionState, WriteError};

/// Активная сессия записи.
pub struct WriteSession {
    stream: JsValue,
    state: SessionState,
    writes: u32,
    bytes_written: u64,
    t0: f64,
    keep_existing: bool,
}

impl WriteSession {
    pub async fn open(
        handle: &FileSystemFileHandle,
        keep_existing: bool,
    ) -> Result<Self, WriteError> {
        let handle_js: &JsValue = handle.as_ref();

        let create_fn_js = Reflect::get(handle_js, &"createWritable".into())
            .map_err(|e| WriteError::Io(format_str(format_args!("createWritable: {e:?}"))))?;
        if create_fn_js.is_undefined() {
            return Err(WriteError::Io("createWritable недоступен".to_string()));
        }
        let create_fn: Function = create_fn_js
            .dyn_into()
            .map_err(|_| WriteError::Io("createWritable не функция".to_string()))?;

        let promise_js = if keep_existing {
            let opts = Object::new();
            Reflect::set(&opts, &"keepExistingData".into(), &JsValue::TRUE)
                .map_err(|e| WriteError::Io(format_str(format_args!("opts: {e:?}"))))?;
            create_fn
                .call1(handle_js, &opts.into())
                .map_err(|e| WriteError::Io(format_str(format_args!("call: {e:?}"))))?
        } else {
            create_fn
                .call0(handle_js)
                .map_err(|e| WriteError::Io(format_str(format_args!("call: {e:?}"))))?
        };
        let promise: js_sys::Promise = promise_js
            .dyn_into()
            .map_err(|_| WriteError::Io("не Promise".to_string()))?;
        let stream = JsFuture::from(promise)
            .await
            .map_err(|e| WriteError::Io(format_str(format_args!("await: {e:?}"))))?;

        let t0: f64 = js_sys::Date::now();
        return Ok(WriteSession {
            stream,
            state: SessionState::Open,
            writes: 0,
            bytes_written: 0,
            t0,
            keep_existing,
        });
    }

    pub fn state(&self) -> SessionState { return self.state; }
    pub fn keep_existing(&self) -> bool { return self.keep_existing; }
    pub fn writes(&self) -> u32 { return self.writes; }
    pub fn bytes_written(&self) -> u64 { return self.bytes_written; }

    fn require_open(&self) -> Result<(), WriteError> {
        if self.state == SessionState::Open {
            return Ok(());
        }
        return Err(WriteError::SessionClosed);
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<(), WriteError> {
        self.require_open()?;
        let u8arr = Uint8Array::from(data);
        let fn_js = Reflect::get(&self.stream, &"write".into())
            .map_err(|e| WriteError::Io(format_str(format_args!("write: {e:?}"))))?;
        let f: Function = fn_js
            .dyn_into()
            .map_err(|_| WriteError::Io("write не функция".to_string()))?;
        let promise_js = f
            .call1(&self.stream, &u8arr)
            .map_err(|e| WriteError::Io(format_str(format_args!("write: {e:?}"))))?;
        let promise: js_sys::Promise = promise_js
            .dyn_into()
            .map_err(|_| WriteError::Io("write: не Promise".to_string()))?;
        JsFuture::from(promise)
            .await
            .map_err(|e| WriteError::Io(format_str(format_args!("write await: {e:?}"))))?;
        self.writes += 1;
        self.bytes_written += data.len() as u64;
        return Ok(());
    }

    pub async fn seek(&mut self, offset: u64) -> Result<(), WriteError> {
        self.require_open()?;
        let fn_js = Reflect::get(&self.stream, &"seek".into())
            .map_err(|e| WriteError::Io(format_str(format_args!("seek: {e:?}"))))?;
        let f: Function = fn_js
            .dyn_into()
            .map_err(|_| WriteError::Io("seek не функция".to_string()))?;
        let promise_js = f
            .call1(&self.stream, &JsValue::from_f64(offset as f64))
            .map_err(|e| WriteError::Io(format_str(format_args!("seek: {e:?}"))))?;
        let promise: js_sys::Promise = promise_js
            .dyn_into()
            .map_err(|_| WriteError::Io("seek: не Promise".to_string()))?;
        JsFuture::from(promise)
            .await
            .map_err(|e| WriteError::Io(format_str(format_args!("seek await: {e:?}"))))?;
        return Ok(());
    }

    pub async fn truncate(&mut self, size: u64) -> Result<(), WriteError> {
        self.require_open()?;
        let fn_js = Reflect::get(&self.stream, &"truncate".into())
            .map_err(|e| WriteError::Io(format_str(format_args!("truncate: {e:?}"))))?;
        let f: Function = fn_js
            .dyn_into()
            .map_err(|_| WriteError::Io("truncate не функция".to_string()))?;
        let promise_js = f
            .call1(&self.stream, &JsValue::from_f64(size as f64))
            .map_err(|e| WriteError::Io(format_str(format_args!("truncate: {e:?}"))))?;
        let promise: js_sys::Promise = promise_js
            .dyn_into()
            .map_err(|_| WriteError::Io("truncate: не Promise".to_string()))?;
        JsFuture::from(promise)
            .await
            .map_err(|e| WriteError::Io(format_str(format_args!("truncate await: {e:?}"))))?;
        return Ok(());
    }

    pub async fn commit(&mut self) -> Result<CommitStats, WriteError> {
        if self.state == SessionState::Committed {
            return Ok(self.stats_snapshot());
        }
        if self.state != SessionState::Open {
            return Err(WriteError::SessionClosed);
        }
        let fn_js = Reflect::get(&self.stream, &"close".into())
            .map_err(|e| WriteError::Io(format_str(format_args!("close: {e:?}"))))?;
        let f: Function = fn_js
            .dyn_into()
            .map_err(|_| WriteError::Io("close не функция".to_string()))?;
        let promise_js = f
            .call0(&self.stream)
            .map_err(|e| WriteError::Io(format_str(format_args!("close: {e:?}"))))?;
        let promise: js_sys::Promise = promise_js
            .dyn_into()
            .map_err(|_| WriteError::Io("close: не Promise".to_string()))?;
        JsFuture::from(promise)
            .await
            .map_err(|e| WriteError::Io(format_str(format_args!("close await: {e:?}"))))?;
        self.state = SessionState::Committed;
        return Ok(self.stats_snapshot());
    }

    pub async fn abort(&mut self) -> Result<(), WriteError> {
        if self.state == SessionState::Aborted {
            return Ok(());
        }
        if self.state != SessionState::Open {
            return Err(WriteError::SessionClosed);
        }
        let fn_js = Reflect::get(&self.stream, &"abort".into())
            .map_err(|e| WriteError::Io(format_str(format_args!("abort: {e:?}"))))?;
        let f: Function = fn_js
            .dyn_into()
            .map_err(|_| WriteError::Io("abort не функция".to_string()))?;
        let promise_js = f
            .call0(&self.stream)
            .map_err(|e| WriteError::Io(format_str(format_args!("abort: {e:?}"))))?;
        let promise: js_sys::Promise = promise_js
            .dyn_into()
            .map_err(|_| WriteError::Io("abort: не Promise".to_string()))?;
        // abort может кинуть rejected promise, если stream в ошибке.
        let _ = JsFuture::from(promise).await;
        self.state = SessionState::Aborted;
        return Ok(());
    }

    fn stats_snapshot(&self) -> CommitStats {
        let elapsed = js_sys::Date::now() - self.t0;
        return CommitStats {
            writes: self.writes,
            bytes_written: self.bytes_written,
            elapsed_ms: elapsed,
            keep_existing: self.keep_existing,
        };
    }
}

impl Drop for WriteSession {
    fn drop(&mut self) {
        if self.state != SessionState::Open {
            return;
        }
        if let Ok(fn_js) = Reflect::get(&self.stream, &"abort".into()) {
            if let Ok(f) = fn_js.dyn_into::<Function>() {
                let _ = f.call0(&self.stream);
            }
        }
        self.state = SessionState::Aborted;
    }
}

// ---------------------------------------------------------------------------
// Чтение после записи
// ---------------------------------------------------------------------------

pub async fn read_text_now(handle: &FileSystemFileHandle) -> Result<String, WriteError> {
    let file = crate::fsa_api_file::get_file(handle)
        .await
        .map_err(|e| WriteError::Io(format_str(format_args!("get_file: {e:?}"))))?;
    let promise: js_sys::Promise = file.text();
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| WriteError::Io(format_str(format_args!("text: {e:?}"))))?;
    return value
        .as_string()
        .ok_or_else(|| WriteError::Io("не строка".to_string()));
}

pub async fn read_bytes_now(handle: &FileSystemFileHandle) -> Result<Vec<u8>, WriteError> {
    let file = crate::fsa_api_file::get_file(handle)
        .await
        .map_err(|e| WriteError::Io(format_str(format_args!("get_file: {e:?}"))))?;
    let promise: js_sys::Promise = file.array_buffer();
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| WriteError::Io(format_str(format_args!("array_buffer: {e:?}"))))?;
    let buffer: js_sys::ArrayBuffer = value
        .dyn_into()
        .map_err(|_| WriteError::Io("не ArrayBuffer".to_string()))?;
    return Ok(Uint8Array::new(&buffer).to_vec());
}

pub async fn read_size_now(handle: &FileSystemFileHandle) -> Result<u64, WriteError> {
    let file = crate::fsa_api_file::get_file(handle)
        .await
        .map_err(|e| WriteError::Io(format_str(format_args!("get_file: {e:?}"))))?;
    return Ok(file.size() as u64);
}

// ---------------------------------------------------------------------------
// Наивная перезапись — для контраста с seek
// ---------------------------------------------------------------------------

/// «Наивная» перезапись: прочитать весь файл, изменить, записать весь.
///
/// Используется в демо для контраста с `seek + write`: показывает,
/// сколько байт читается и пишется зря, если не использовать позиционирование.
pub async fn write_naive_rewrite(
    handle: &FileSystemFileHandle,
    offset: u64,
    new_bytes: &[u8],
) -> Result<NaiveStats, WriteError> {
    let t0 = js_sys::Date::now();

    // 1. Прочитать весь файл.
    let mut data = read_bytes_now(handle).await?;
    let bytes_read = data.len() as u64;

    // 2. Изменить в памяти.
    let start = offset as usize;
    if start + new_bytes.len() > data.len() {
        return Err(WriteError::Io(format_str(format_args!(
            "offset {} + {} байт выходит за пределы файла ({} байт)",
            offset, new_bytes.len(), data.len(),
        ))));
    }
    data[start..start + new_bytes.len()].copy_from_slice(new_bytes);

    // 3. Записать целиком.
    let mut session = WriteSession::open(handle, false).await?;
    session.write(&data).await?;
    session.commit().await?;

    let elapsed = js_sys::Date::now() - t0;
    return Ok(NaiveStats {
        bytes_read,
        bytes_written: data.len() as u64,
        elapsed_ms: elapsed,
    });
}

/// Строка «что сейчас в файле»: длина + первые 60 символов.
pub fn describe_content(text: &str) -> String {
    let chars: usize = text.chars().count();
    if chars == 0 {
        return "(файл пуст)".to_string();
    }
    let preview: String = if chars > 60 {
        let head: String = text.chars().take(60).collect();
        format_str(format_args!("{head}…"))
    } else {
        text.to_string()
    };
    let s: String = format_str(format_args!("{chars} симв.: {preview}"));
    return s;
}

/// Строка «N байт».
pub fn format_bytes(n: u64) -> String {
    if n < 1024 {
        return format_str(format_args!("{n} Б"));
    }
    if n < 1024 * 1024 {
        let k: f64 = n as f64 / 1024.0;
        return format_str(format_args!("{k:.1} КБ"));
    }
    let m: f64 = n as f64 / (1024.0 * 1024.0);
    return format_str(format_args!("{m:.2} МБ"));
}