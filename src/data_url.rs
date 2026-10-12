//! Data URL — представление бинарных данных в виде `data:` URL.
//! Модуль автономен: не зависит от других модулей проекта.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, FileReader};

/// Разобранные части Data URL.
#[derive(Debug, Clone, PartialEq)]
pub struct DataUrlParts {
    /// MIME-тип из префикса, например `image/png`.
    /// Для `data:,hello` возвращается `text/plain;charset=US-ASCII`
    /// (по умолчанию согласно спецификации).
    pub mime_type: String,
    /// `true`, если payload закодирован в base64 (`;base64,`).
    pub is_base64: bool,
    /// Полезная нагрузка после запятой — base64-строка
    /// или URL-encoded содержимое.
    pub payload: String,
}

/// Асинхронно читает `Blob` как Data URL.
///
/// Возвращает строку вида `data:image/png;base64,iVBORw0KGgo=`.
/// Для больших файлов длина строки примерно в 1.37 раза больше
/// размера исходных данных.
pub async fn read_as_data_url(blob: &Blob) -> Result<String, JsValue> {
    let reader = FileReader::new()?;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let reader_clone = reader.clone();
        let onload = Closure::once_into_js(move || {
            let _ = resolve.call0(&JsValue::NULL);
        });
        let onerror = Closure::once_into_js(move |err: JsValue| {
            let _ = reject.call1(&JsValue::NULL, &err);
        });
        reader_clone.set_onload(Some(onload.unchecked_ref()));
        reader_clone.set_onerror(Some(onerror.unchecked_ref()));
    });

    reader.read_as_data_url(blob)?;
    JsFuture::from(promise).await?;

    return reader
        .result()?
        .as_string()
        .ok_or_else(|| JsValue::from_str("Data URL не является строкой"));
}

/// Разбирает Data URL на части.
///
/// Формат: `data:[<mediatype>][;base64],<data>`
///
/// * `mediatype` может отсутствовать — тогда возвращается
///   `text/plain;charset=US-ASCII` (значение по умолчанию).
/// * `;base64` может отсутствовать — тогда payload в URL-encoded виде,
///   и `is_base64` будет `false`.
///
/// Возвращает `None`, если строка не начинается с `data:` или
/// не содержит запятой.
pub fn parse_data_url(url: &str) -> Option<DataUrlParts> {
    let after_data = url.strip_prefix("data:")?;
    let comma = after_data.find(',')?;
    let (meta, rest) = after_data.split_at(comma);
    // rest начинается с запятой — пропускаем её.
    let payload = &rest[1..];

    let is_base64 = meta.ends_with(";base64");
    let mime_raw = if is_base64 {
        &meta[..meta.len() - 7] // ";base64" — ровно 7 символов
    } else {
        meta
    };

    let mime_type = if mime_raw.is_empty() {
        "text/plain;charset=US-ASCII".to_string()
    } else {
        mime_raw.to_string()
    };

    return Some(DataUrlParts {
        mime_type,
        is_base64,
        payload: payload.to_string(),
    });
}