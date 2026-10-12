//! Опции picker'ов в терминах предметной области.
//!
//! Модуль не вызывает picker'ы (это делает `fsa_choose_exec`), а только
//! **описывает**, что мы хотим выбрать.
//!
//! * `AcceptFilter` — один фильтр: описание + карта MIME → расширения.
//!   Проходит валидацию: MIME содержит `/`, расширения начинаются с `.`.
//! * `FilterPreset` — готовые конфигурации (Images, Text, None).
//! * `build_*_options` — собирают JS-объекты с проверкой фильтров.

use wasm_bindgen::prelude::*;

/// Обёртка над `format!`, фиксирующая тип для rust-analyzer.
/// Хвостовое `format!` в `if/else`/`match` RA выводит как `()`.
#[inline]
pub fn format_str(args: std::fmt::Arguments<'_>) -> String {
    std::fmt::format(args)
}

// ---------------------------------------------------------------------------
// AcceptFilter
// ---------------------------------------------------------------------------

/// Один фильтр: описание + карта MIME → расширения.
#[derive(Debug, Clone, PartialEq)]
pub struct AcceptFilter {
    pub description: String,
    pub mime_to_extensions: Vec<(String, Vec<String>)>,
}

impl AcceptFilter {
    pub fn new(description: &str) -> Self {
        AcceptFilter {
            description: description.to_string(),
            mime_to_extensions: Vec::new(),
        }
    }

    pub fn add(mut self, mime: &str, extensions: &[&str]) -> Self {
        let exts: Vec<String> = extensions.iter().map(|s| (*s).to_string()).collect();
        self.mime_to_extensions.push((mime.to_string(), exts));
        self
    }

    /// Валидация: MIME содержит `/`, расширения начинаются с `.`,
    /// у фильтра есть хотя бы одно расширение.
    pub fn validate(&self) -> Result<(), String> {
        if self.description.trim().is_empty() {
            return Err("описание фильтра пусто".to_string());
        }
        if self.mime_to_extensions.is_empty() {
            return Err(format_str(format_args!(
                "фильтр '{}' не содержит расширений",
                self.description,
            )));
        }
        for (mime, exts) in &self.mime_to_extensions {
            if !mime.contains('/') {
                return Err(format_str(format_args!(
                    "MIME '{mime}' должен содержать '/'",
                )));
            }
            for e in exts {
                if !e.starts_with('.') {
                    return Err(format_str(format_args!(
                        "расширение '{e}' должно начинаться с точки",
                    )));
                }
            }
        }
        Ok(())
    }

    fn build_accept_object(&self) -> Result<JsValue, JsValue> {
        let accept = js_sys::Object::new();
        for (mime, exts) in &self.mime_to_extensions {
            let arr = js_sys::Array::new();
            for e in exts {
                arr.push(&JsValue::from_str(e));
            }
            js_sys::Reflect::set(&accept, &JsValue::from_str(mime), &arr)?;
        }
        Ok(accept.into())
    }
}

// ---------------------------------------------------------------------------
// FilterPreset — готовые конфигурации
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterPreset {
    Images,
    Text,
    None,
}

impl FilterPreset {
    /// Рекомендуемое имя файла для сохранения, согласованное с фильтром.
    pub fn default_filename(self) -> &'static str {
        match self {
            FilterPreset::Images => return "result.png",
            FilterPreset::Text => return "result.txt",
            FilterPreset::None => return "result.bin",
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            FilterPreset::Images => "Изображения (PNG/JPEG)",
            FilterPreset::Text => "Текст (.txt)",
            FilterPreset::None => "Без фильтра",
        }
    }

    pub fn build(self) -> Vec<AcceptFilter> {
        match self {
            FilterPreset::Images => vec![AcceptFilter::images()],
            FilterPreset::Text => vec![AcceptFilter::plain_text()],
            FilterPreset::None => Vec::new(),
        }
    }

    /// Все пресеты в фиксированном порядке — для построения `<select>`.
    pub fn all() -> [FilterPreset; 3] {
        [FilterPreset::Images, FilterPreset::Text, FilterPreset::None]
    }
}

impl AcceptFilter {
    /// Типовой фильтр «изображения»: PNG + JPEG.
    pub fn images() -> Self {
        AcceptFilter::new("Images")
            .add("image/png", &[".png"])
            .add("image/jpeg", &[".jpg", ".jpeg"])
    }

    /// Типовой фильтр «текст»: .txt.
    pub fn plain_text() -> Self {
        AcceptFilter::new("Text").add("text/plain", &[".txt"])
    }
}

// ---------------------------------------------------------------------------
// Сборка JS-объектов
// ---------------------------------------------------------------------------

fn validate_filters(filters: &[AcceptFilter]) -> Result<(), String> {
    for f in filters {
        f.validate()?;
    }
    Ok(())
}

/// Построить `types` — массив `[{description, accept}, ...]`.
pub fn build_accept_array(filters: &[AcceptFilter]) -> Result<JsValue, JsValue> {
    validate_filters(filters).map_err(|e| JsValue::from_str(&e))?;

    let types = js_sys::Array::new();
    for f in filters {
        let entry = js_sys::Object::new();
        js_sys::Reflect::set(
            &entry,
            &JsValue::from_str("description"),
            &JsValue::from_str(&f.description),
        )?;
        js_sys::Reflect::set(
            &entry,
            &JsValue::from_str("accept"),
            &f.build_accept_object()?,
        )?;
        types.push(&entry);
    }
    Ok(types.into())
}

/// Опции для `showOpenFilePicker`.
pub fn build_open_options(
    multiple: bool,
    exclude_accept_all: bool,
    filters: &[AcceptFilter],
) -> Result<JsValue, JsValue> {
    validate_filters(filters).map_err(|e| JsValue::from_str(&e))?;

    let opts = js_sys::Object::new();
    if !filters.is_empty() {
        js_sys::Reflect::set(
            &opts,
            &JsValue::from_str("types"),
            &build_accept_array(filters)?,
        )?;
        js_sys::Reflect::set(
            &opts,
            &JsValue::from_str("excludeAcceptAllOption"),
            &JsValue::from_bool(exclude_accept_all),
        )?;
    }
    js_sys::Reflect::set(
        &opts,
        &JsValue::from_str("multiple"),
        &JsValue::from_bool(multiple),
    )?;
    Ok(opts.into())
}

/// Опции для `showSaveFilePicker`.
pub fn build_save_options(
    suggested_name: &str,
    filters: &[AcceptFilter],
) -> Result<JsValue, JsValue> {
    validate_filters(filters).map_err(|e| JsValue::from_str(&e))?;

    let opts = js_sys::Object::new();
    js_sys::Reflect::set(
        &opts,
        &JsValue::from_str("suggestedName"),
        &JsValue::from_str(suggested_name),
    )?;
    if !filters.is_empty() {
        js_sys::Reflect::set(
            &opts,
            &JsValue::from_str("types"),
            &build_accept_array(filters)?,
        )?;
    }
    Ok(opts.into())
}

/// Режим директории: только чтение или чтение-запись.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryMode {
    Read,
    ReadWrite,
}

impl DirectoryMode {
    pub fn as_str(self) -> &'static str {
        match self {
            DirectoryMode::Read => "read",
            DirectoryMode::ReadWrite => "readwrite",
        }
    }
}

/// Опции для `showDirectoryPicker`.
///
/// `id` — идентификатор, по которому браузер запоминает последнюю
/// выбранную папку для этого приложения. `start_in` — начальная
/// подсказка (`"documents"`, `"downloads"`, `"desktop"`, ...).
pub fn build_directory_options(
    mode: DirectoryMode,
    id: Option<&str>,
    start_in: Option<&str>,
) -> Result<JsValue, JsValue> {
    let opts = js_sys::Object::new();
    js_sys::Reflect::set(
        &opts,
        &JsValue::from_str("mode"),
        &JsValue::from_str(mode.as_str()),
    )?;
    if let Some(v) = id {
        js_sys::Reflect::set(&opts, &JsValue::from_str("id"), &JsValue::from_str(v))?;
    }
    if let Some(v) = start_in {
        js_sys::Reflect::set(&opts, &JsValue::from_str("startIn"), &JsValue::from_str(v))?;
    }
    Ok(opts.into())
}

// ---------------------------------------------------------------------------
// Описания для UI
// ---------------------------------------------------------------------------

/// Строка-описание фильтров: показывает расширения и подсвечивает
/// проблемные.
pub fn describe_filters(filters: &[AcceptFilter]) -> String {
    if filters.is_empty() {
        return "фильтры не заданы (можно выбрать любой файл)".to_string();
    }
    let mut parts: Vec<String> = Vec::with_capacity(filters.len());
    for f in filters {
        let exts: Vec<String> = f
            .mime_to_extensions
            .iter()
            .flat_map(|(_, e)| e.iter().cloned())
            .collect();
        let warning: String = match f.validate() {
            Ok(()) => String::new(),
            Err(msg) => format_str(format_args!(" ⚠ {msg}")),
        };
        parts.push(format_str(format_args!(
            "{}: {} [{}]{}",
            f.description,
            f.mime_to_extensions
                .iter()
                .map(|(m, _)| m.clone())
                .collect::<Vec<_>>()
                .join(", "),
            exts.join(" "),
            warning,
        )));
    }
    parts.join("; ")
}

/// Строка «id директории» для UI.
pub fn describe_directory_id(id: &str) -> String {
    format_str(format_args!(
        "id = \"{id}\" (браузер запоминает последнюю папку по этому ключу)",
    ))
}
