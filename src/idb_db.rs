//! Обёртка над `IDBDatabase` — соединение с базой IndexedDB.
//!
//! Открытая база (`IDBDatabase`) — активное соединение. Пока оно открыто,
//! браузер держит файл базы; другой запрос `open` с большей версией или
//! `deleteDatabase` ждёт, пока вы закроетесь. В 24.2 мы видели обратную
//! сторону — `onblocked` у второго запроса. Здесь — как отвечать на это
//! ожидание со стороны первого.
//!
//! Инструменты:
//!   • метаданные соединения — `name`, `version`, `store_names`;
//!   • `close()` — освободить соединение (идемпотентно);
//!   • `onversionchange` — событие «кто-то хочет обновить базу — закройтесь»;
//!   • отслеживание состояния — `is_closed`.
//!
//! ПРАВИЛО: `close()` возвращает `()` — не `Result`. Повторный вызов
//! допустим и не даёт ошибок. Использовать закрытое соединение —
//! `InvalidStateError` при первой транзакции.
//!
//! ПРАВИЛО: `objectStoreNames` — `DOMStringList`, а не `Array`. Итерируем
//! через `Reflect` + `Function::call`, а не через `js_sys::Array::from`.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

/// Обёртка над живым соединением `IDBDatabase`.
///
/// Хранит `JsValue` исходного объекта, чтобы не зависеть от типизированной
/// обёртки `web-sys` — её сигнатуры в разных версиях различаются.
pub struct Database {
    raw: JsValue,
    /// Флаг «мы сами вызывали close()». Отдельный от `raw`, потому что
    /// после закрытия объект остаётся живым, но нерабочим.
    closed: Rc<RefCell<bool>>,
}

impl Database {
    /// Оборачивает `JsValue`, полученный от `Factory::open`.
    ///
    /// Проверку, что это действительно `IDBDatabase`, не делает —
    /// ответственность вызывающего (обычно `idb_factory::open_by_name`).
    pub fn from_raw(raw: JsValue) -> Self {
        Self {
            raw,
            closed: Rc::new(RefCell::new(false)),
        }
    }

    /// Сырое `JsValue` — если нужен метод, которого нет в обёртке.
    pub fn raw(&self) -> &JsValue {
        &self.raw
    }

    /// Имя базы.
    pub fn name(&self) -> String {
        js_sys::Reflect::get(&self.raw, &JsValue::from_str("name"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default()
    }

    /// Текущая версия схемы.
    pub fn version(&self) -> u32 {
        js_sys::Reflect::get(&self.raw, &JsValue::from_str("version"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u32
    }

    /// Имена всех object store'ов в базе.
    ///
    /// `objectStoreNames` — `DOMStringList` (не массив, не `Array`).
    /// Итерируем через `.item(i)`, пока `i < .length`.
    pub fn store_names(&self) -> Vec<String> {
        let list = match js_sys::Reflect::get(&self.raw, &JsValue::from_str("objectStoreNames")) {
            Ok(v) if !v.is_undefined() && !v.is_null() => v,
            _ => return Vec::new(),
        };
        let length = js_sys::Reflect::get(&list, &JsValue::from_str("length"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u32;
        let item_fn = match js_sys::Reflect::get(&list, &JsValue::from_str("item")) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        let item: js_sys::Function = match item_fn.dyn_into() {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::with_capacity(length as usize);
        for i in 0..length {
            if let Ok(v) = item.call1(&list, &JsValue::from_f64(i as f64)) {
                if let Some(s) = v.as_string() {
                    out.push(s);
                }
            }
        }
        out
    }

    /// Закрывает соединение. Идемпотентно.
    ///
    /// После закрытия использовать соединение нельзя: первая транзакция
    /// даст `InvalidStateError`. Проверки на это в самой обёртке нет —
    /// транзакции появятся в следующих подглавах.
    pub fn close(&self) {
        // Идемпотентность: если уже закрыто — выходим.
        if *self.closed.borrow() {
            return;
        }
        if let Ok(close_fn) = js_sys::Reflect::get(&self.raw, &JsValue::from_str("close")) {
            if let Ok(f) = close_fn.dyn_into::<js_sys::Function>() {
                let _ = f.call0(&self.raw);
            }
        }
        *self.closed.borrow_mut() = true;
    }

    /// true, если этот объект уже вызывал `close()`.
    ///
    /// Не то же самое, что «браузер освободил ресурсы». Если соединение
    /// было закрыто через `onversionchange` извне, флаг остаётся `false`,
    /// но фактически база закрыта.
    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    /// Устанавливает обработчик `onversionchange`.
    ///
    /// Событие приходит, когда **другое** соединение пытается открыть базу
    /// с большей версией или удалить её. Обработчик должен закрыть текущее
    /// соединение — иначе тот запрос зависнет на `onblocked`.
    ///
    /// `Closure::once_into_js` — событие `versionchange` приходит один раз
    /// за жизненный цикл соединения. Колбэк — `FnOnce`, повторный вызов
    /// даст `unreachable`.
       pub fn install_versionchange_handler(&self) {
        let raw = self.raw.clone();
        let closed = self.closed.clone();

        // onversionchange может сработать несколько раз за жизненный цикл
        // соединения (каждое внешнее обновление/удаление). FnOnce-замыкание
        // (`once_into_js`) при повторном вызове паникует в Rust — поэтому
        // FnMut через `Closure::wrap` + `forget`: функция живёт, пока живёт
        // соединение.
        let cb = Closure::wrap(Box::new(move || {
            if let Ok(close_fn) = js_sys::Reflect::get(&raw, &JsValue::from_str("close")) {
                if let Ok(f) = close_fn.dyn_into::<js_sys::Function>() {
                    let _ = f.call0(&raw);
                }
            }
            *closed.borrow_mut() = true;
        }) as Box<dyn FnMut()>);

        let _ = js_sys::Reflect::set(
            &self.raw,
            &JsValue::from_str("onversionchange"),
            cb.as_ref(),
        );
        cb.forget();
    }
}