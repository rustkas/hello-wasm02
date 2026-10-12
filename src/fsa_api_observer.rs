//! `FileSystemObserver`: подписка на изменения файлов и папок.
//!
//! **Что это.** Интерфейс, который позволяет получать **события**
//! при изменении файла/папки — вместо того чтобы опрашивать
//! (polling) состояние каждые N миллисекунд.
//!
//! **Чем отличается от polling.**
//! * Observer: zero-cost в покое — браузер сам присылает уведомление,
//!   когда что-то реально изменилось. Нет задержки на интервал опроса.
//! * Polling: постоянная нагрузка, даже когда ничего не происходит.
//!   Задержка между изменением и обнаружением равна интервалу опроса.
//!   Дорого на больших деревьях.
//!
//! **Поддержка.** Chrome 133+ (февраль 2025). Firefox/Safari — пока нет.
//! Поэтому модуль строится по правилу «feature detection»: если
//! `FileSystemObserver` недоступен, `is_supported()` вернёт `false`,
//! а `observe` — `Err` с внятным сообщением.
//!
//! **Почему через Reflect.** В `web-sys` интерфейс `FileSystemObserver`
//! пока не экспортируется. Все методы вызываем через `js_sys::Reflect`.
//! Это общий приём для нестабильных API — он не зависит от версии
//! `web-sys`.
//!
//! **Правило DOM-конструктора.** `FileSystemObserver` вызывается
//! через `new` — в Rust это `js_sys::Reflect::construct`. Обычный
//! `Function::call1` падает с TypeError «Please use the 'new' operator».
//!
//! **Правило проекта.** Feature detection — **первое**, что делает
//! демо. Если API нет, демо не падает, а объясняет пользователю,
//! что нужно обновить браузер.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{FileSystemDirectoryHandle, FileSystemFileHandle};

// ===========================================================================
// Строковые хелперы (правило WASM-инференса)
// ===========================================================================

#[inline]
fn format_str(args: std::fmt::Arguments<'_>) -> String {
    args.to_string()
}

// ===========================================================================
// Модель событий
// ===========================================================================

/// Тип изменения, полученного от observer'а.
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeKind {
    /// Запись появилась.
    Appeared,
    /// Запись исчезла.
    Disappeared,
    /// Содержимое изменилось (размер, mtime).
    Modified,
    /// Запись перемещена (переименована или в другую папку).
    Moved,
    /// Что-то ещё — на случай будущих категорий.
    Unknown,
}

impl ChangeKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "appeared" => ChangeKind::Appeared,
            "disappeared" => ChangeKind::Disappeared,
            "modified" => ChangeKind::Modified,
            "moved" => ChangeKind::Moved,
            _ => ChangeKind::Unknown,
        }
    }

    pub fn label_ru(&self) -> &'static str {
        match self {
            ChangeKind::Appeared => "появился",
            ChangeKind::Disappeared => "удалён",
            ChangeKind::Modified => "изменён",
            ChangeKind::Moved => "перемещён",
            ChangeKind::Unknown => "изменение",
        }
    }
}

/// Одно событие об изменении.
#[derive(Debug, Clone)]
pub struct ChangeRecord {
    pub kind: ChangeKind,
    /// Имя изменённой записи (без пути — API не даёт полный путь).
    pub name: String,
    /// Был ли это файл (иначе директория).
    pub is_file: bool,
}

// ===========================================================================
// Feature detection
// ===========================================================================

/// Проверка: доступен ли `FileSystemObserver` в этом браузере.
///
/// Не паникует, не бросает исключений — возвращает `bool`.
/// Chrome 133+ → `true`. Firefox/Safari/старый Chrome → `false`.
pub fn is_supported() -> bool {
    let global = js_sys::global();
    let ctor = js_sys::Reflect::get(&global, &JsValue::from_str("FileSystemObserver"))
        .ok()
        .unwrap_or(JsValue::UNDEFINED);
    let is_fn: bool = ctor.is_function();
    is_fn
}

// ===========================================================================
// Handle observer'а
// ===========================================================================

/// Handle активной подписки. `disconnect()` идемпотентен.
///
/// Держит `FileSystemObserver` и `Closure` колбэка — оба должны жить
/// столько же, сколько и подписка.
pub struct ObserverHandle {
    observer: JsValue,
    _callback: Closure<dyn FnMut(JsValue, JsValue)>,
}

impl ObserverHandle {
    /// Отписаться от наблюдения. Идемпотентно: повторный вызов — no-op.
    pub fn disconnect(&self) {
        let disconnect_fn = match js_sys::Reflect::get(
            &self.observer,
            &JsValue::from_str("disconnect"),
        ) {
            Ok(f) => f,
            Err(_) => return,
        };
        if !disconnect_fn.is_function() {
            return;
        }
        let func: js_sys::Function = match disconnect_fn.dyn_into() {
            Ok(f) => f,
            Err(_) => return,
        };
        // Игнорируем ошибку — disconnect() не должен падать.
        let _ = func.call0(&self.observer);
    }
}

// ===========================================================================
// Создание observer'а (общий хелпер)
// ===========================================================================

/// Создать `FileSystemObserver` через `new`.
///
/// **Правило DOM-конструктора.** `FileSystemObserver` — DOM-конструктор,
/// вызывать его надо через `Reflect::construct`, а не `Function::call1`.
/// Иначе браузер вернёт `TypeError: Please use the 'new' operator`.
fn construct_observer(
    callback: &Closure<dyn FnMut(JsValue, JsValue)>,
) -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let ctor_js = js_sys::Reflect::get(&global, &JsValue::from_str("FileSystemObserver"))?;
    let ctor: js_sys::Function = ctor_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("FileSystemObserver не функция-конструктор"))?;

    // `Reflect::construct` — аналог `new FileSystemObserver(callback)`.
    let args = js_sys::Array::new();
    args.push(callback.as_ref());
    let observer: JsValue = js_sys::Reflect::construct(&ctor, &args)?;
    Ok(observer)
}

/// Вызвать метод `observer.<name>` и вернуть результат.
///
/// Обёртка для типичного «взять метод по имени и вызвать» через Reflect.
fn call_method(
    observer: &JsValue,
    name: &str,
    args: &js_sys::Array,
) -> Result<JsValue, JsValue> {
    let method_js = js_sys::Reflect::get(observer, &JsValue::from_str(name))?;
    if !method_js.is_function() {
        let msg = format_str(format_args!("observer.{name} не функция"));
        return Err(JsValue::from_str(&msg));
    }
    let method: js_sys::Function = method_js
        .dyn_into()
        .map_err(|_| JsValue::from_str("не Function"))?;
    method.apply(observer, args)
}

// ===========================================================================
// observe
// ===========================================================================

/// Подписаться на изменения директории.
///
/// `on_change` вызывается для каждой пачки изменений — браузер
/// группирует события и присылает массив `FileSystemObserverRecord`.
///
/// Внутри `on_change` разбираем массив в `Vec<ChangeRecord>` и
/// передаём наружу — так вызывающий код видит доменную модель,
/// а не сырые `JsValue`.
pub fn observe_dir(
    dir: &FileSystemDirectoryHandle,
    mut on_change: impl FnMut(Vec<ChangeRecord>) + 'static,
) -> Result<ObserverHandle, JsValue> {
    if !is_supported() {
        return Err(JsValue::from_str(
            "FileSystemObserver не поддерживается браузером (нужен Chrome 133+)",
        ));
    }

    // Колбэк для observer'а: получает (records, observer).
    let callback = Closure::<dyn FnMut(JsValue, JsValue)>::new(
        move |records: JsValue, _observer: JsValue| {
            let parsed = parse_records(&records);
            if !parsed.is_empty() {
                on_change(parsed);
            }
        },
    );

    // 1) Создаём observer через `new`.
    let observer: JsValue = construct_observer(&callback)?;

    // 2) Вызываем `observer.observe(dir, { recursive: true })`.
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &JsValue::from_str("recursive"), &JsValue::TRUE)?;

    let dir_js: &JsValue = dir.as_ref();
    let args = js_sys::Array::new();
    args.push(dir_js);
    args.push(&options);
    // observe() возвращает Promise. Если он отклонён — узнаем об этом
    // асинхронно; синхронные ошибки попадём сюда.
    let _ = call_method(&observer, "observe", &args)?;

    Ok(ObserverHandle {
        observer,
        _callback: callback,
    })
}

/// Подписаться на изменения одного файла.
pub fn observe_file(
    file: &FileSystemFileHandle,
    mut on_change: impl FnMut(Vec<ChangeRecord>) + 'static,
) -> Result<ObserverHandle, JsValue> {
    if !is_supported() {
        return Err(JsValue::from_str(
            "FileSystemObserver не поддерживается браузером (нужен Chrome 133+)",
        ));
    }

    let callback = Closure::<dyn FnMut(JsValue, JsValue)>::new(
        move |records: JsValue, _observer: JsValue| {
            let parsed = parse_records(&records);
            if !parsed.is_empty() {
                on_change(parsed);
            }
        },
    );

    let observer: JsValue = construct_observer(&callback)?;

    // Для файла options не передаём — `observe(file)` достаточно.
    let file_js: &JsValue = file.as_ref();
    let args = js_sys::Array::new();
    args.push(file_js);
    let _ = call_method(&observer, "observe", &args)?;

    Ok(ObserverHandle {
        observer,
        _callback: callback,
    })
}

// ===========================================================================
// Разбор records
// ===========================================================================

/// Разбирает массив `FileSystemObserverRecord` в доменную модель.
///
/// Каждая запись имеет поля:
/// * `type` — `"appeared"` / `"disappeared"` / `"modified"` / `"moved"`;
/// * `changedHandle` — FileSystemFileHandle или DirectoryHandle;
/// * `relativePathComponents` — путь относительно наблюдаемой точки.
fn parse_records(records: &JsValue) -> Vec<ChangeRecord> {
    let mut out: Vec<ChangeRecord> = Vec::new();

    if !js_sys::Array::is_array(records) {
        return out;
    }
    let arr: js_sys::Array = records.clone().into();
    let len: u32 = arr.length();
    for i in 0..len {
        let rec = arr.get(i);
        let kind_str: String = js_sys::Reflect::get(&rec, &JsValue::from_str("type"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let kind = ChangeKind::from_str(&kind_str);

        let handle = js_sys::Reflect::get(&rec, &JsValue::from_str("changedHandle"))
            .ok()
            .unwrap_or(JsValue::UNDEFINED);

        let name: String = js_sys::Reflect::get(&handle, &JsValue::from_str("name"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();

        let kind_field: String = js_sys::Reflect::get(&handle, &JsValue::from_str("kind"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let is_file: bool = kind_field == "file";

        out.push(ChangeRecord {
            kind,
            name,
            is_file,
        });
    }

    out
}

// ===========================================================================
// Публичный API для демо
// ===========================================================================

/// Создать `Rc<RefCell<Vec<String>>>` — накопитель лога.
pub fn make_log_store() -> Rc<RefCell<Vec<String>>> {
    Rc::new(RefCell::new(Vec::new()))
}

/// Утилита для форматирования одного события в строку лога.
pub fn format_record(rec: &ChangeRecord) -> String {
    let kind = rec.kind.label_ru();
    let icon: &str = match rec.kind {
        ChangeKind::Appeared => "+",
        ChangeKind::Disappeared => "−",
        ChangeKind::Modified => "~",
        ChangeKind::Moved => "→",
        ChangeKind::Unknown => "?",
    };
    let what: &str = if rec.is_file { "файл" } else { "папка" };
    let s: String = format_str(format_args!("{icon} {kind} {what}: {}", rec.name));
    s
}