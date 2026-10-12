//! Fallback для браузеров без FSA (Firefox, Safari).
//!
//! * открытие — `<input type="file">`;
//! * сохранение — `<a download>` + Object URL.
//!
//! Для директории fallback'а нет: сообщаем об этом явно.
//!
//! Правило проекта: любой макрос или выражение, возвращающее значение,
//! в хвостовой позиции функции/`if`/`match`-arm оборачивается в явный
//! `return`. rust-analyzer не выводит тип макроса в tail-position.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, File, HtmlAnchorElement, HtmlInputElement, Url};

use crate::fsa_choose_exec::PickOutcome;
use crate::fsa_choose_options::format_str;

/// Информация о скачанном файле — что реально ушло в загрузки.
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadInfo {
    pub suggested_name: String,
    pub blob_size: f64,
}

/// Выбор файла через `<input type="file">`.
pub async fn pick_via_input(accept: &str) -> PickOutcome<File> {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return PickOutcome::Failed(format_str(format_args!("no window"))),
    };
    let doc = match window.document() {
        Some(d) => d,
        None => return PickOutcome::Failed(format_str(format_args!("no document"))),
    };

    // create_element возвращает Result<Element, JsValue>,
    // dyn_into — Result<HtmlInputElement, Element>. Разные типы ошибок,
    // поэтому два отдельных шага, а не and_then.
    let input_el = match doc.create_element("input") {
        Ok(e) => e,
        Err(_) => {
            return PickOutcome::Failed(format_str(format_args!(
                "не удалось создать <input>",
            )));
        }
    };
    let input: HtmlInputElement = match input_el.dyn_into() {
        Ok(i) => i,
        Err(_) => {
            return PickOutcome::Failed(format_str(format_args!(
                "не HtmlInputElement",
            )));
        }
    };
    input.set_type("file");
    if !accept.is_empty() {
        input.set_accept(accept);
    }
    if let Some(body) = doc.body() {
        let _ = body.append_child(&input);
    }
    input.click();

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let input_clone = input.clone();
        let on_change = Closure::once(move || {
            let files = input_clone.files();
            match files.and_then(|f| f.get(0)) {
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

    let result = JsFuture::from(promise).await;
    if let Some(body) = doc.body() {
        let _ = body.remove_child(&input);
    }

    return match result {
        Ok(v) => match v.dyn_into::<File>() {
            Ok(f) => PickOutcome::Picked(f),
            Err(_) => PickOutcome::Failed(format_str(format_args!("не File"))),
        },
        Err(_) => PickOutcome::Cancelled,
    };
}

/// Скачать Blob через `<a download>` + Object URL.
///
/// Возвращает полную информацию: имя и размер.
pub fn download_blob(
    blob: &Blob,
    suggested_name: &str,
) -> Result<DownloadInfo, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let doc = window
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;

    let url = Url::create_object_url_with_blob(blob)?;
    let a: HtmlAnchorElement = doc
        .create_element("a")?
        .dyn_into()
        .map_err(|_| JsValue::from_str("не HtmlAnchorElement"))?;
    a.set_href(&url);
    a.set_download(suggested_name);
    a.click();
    Url::revoke_object_url(&url)?;

    let info = DownloadInfo {
        suggested_name: suggested_name.to_string(),
        blob_size: blob.size(),
    };
    return Ok(info);
}

/// Упаковать текст в Blob `text/plain;charset=utf-8`.
pub fn make_text_blob(text: &str) -> Result<Blob, JsValue> {
    let arr = js_sys::Array::of1(&JsValue::from_str(text));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("text/plain;charset=utf-8");
    let blob = Blob::new_with_str_sequence_and_options(&arr, &opts)?;
    return Ok(blob);
}

/// Строка описания скачанного файла для UI.
pub fn describe_download(info: &DownloadInfo) -> String {
    return format_str(format_args!(
        "fallback · скачано как {} ({} байт)\nПроверьте папку «Загрузки».",
        info.suggested_name, info.blob_size,
    ));
}

/// Сообщение о том, что fallback для директорий не существует.
pub fn directory_unsupported_message() -> String {
    return format_str(format_args!(
        "Выбор папки в этом браузере недоступен.\n\
         File System Access API требует Chrome или Edge.\n\
         Открыть отдельный файл — можно, через <input type=file>.",
    ));
}