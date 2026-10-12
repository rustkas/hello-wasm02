//! 45. Web Bluetooth: обнаружение, разрешения, подключение.
//!
//! ## Публичный API
//!
//! - `has_web_bluetooth()` — есть ли `navigator.bluetooth`.
//! - `is_secure_context()` — HTTPS или localhost.
//! - `request_device(services)` — `navigator.bluetooth.requestDevice`
//!   с фильтром по сервисам.
//! - `get_known_devices()` — `navigator.bluetooth.getDevices`.
//! - `read_device_info(device) -> DeviceInfo` — id, name, connected.
//! - `connect(device) -> GATT server`, `disconnect(device)`.
//!
//! ## Почему всё через `Reflect`
//!
//! Web Bluetooth в web-sys 0.3.105 доступен только под
//! `#[cfg(web_sys_unstable_apis)]`. Reflect-путь работает без
//! нестабильных фич и явно проверяет наличие каждого метода.

use js_sys::{Array, Object, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::video_frame::{format_str, set_prop};

// =============================================================================
// Feature detection
// =============================================================================

/// Проверяет наличие `navigator.bluetooth`.
pub fn has_web_bluetooth() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Ok(nav) = Reflect::get(window.as_ref(), &JsValue::from_str("navigator")) else {
        return false;
    };
    let Ok(bt) = Reflect::get(&nav, &JsValue::from_str("bluetooth")) else {
        return false;
    };
    return !bt.is_undefined() && !bt.is_null();
}

/// HTTPS или localhost.
pub fn is_secure_context() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    return Reflect::get(window.as_ref(), &JsValue::from_str("isSecureContext"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
}

/// `navigator.bluetooth.getAvailability()` — deprecated, но всё ещё
/// работает в Chrome. Возвращает `None`, если метод отсутствует.
pub async fn get_availability() -> Option<bool> {
    let window = web_sys::window()?;
    let nav = Reflect::get(window.as_ref(), &JsValue::from_str("navigator")).ok()?;
    let bt = Reflect::get(&nav, &JsValue::from_str("bluetooth")).ok()?;
    let get = Reflect::get(&bt, &JsValue::from_str("getAvailability")).ok()?;
    let get = get.dyn_into::<js_sys::Function>().ok()?;
    let promise = get.call0(&bt).ok()?.dyn_into::<Promise>().ok()?;
    let result = JsFuture::from(promise).await.ok()?;
    return result.as_bool();
}

// =============================================================================
// Ошибки
// =============================================================================

/// Разбирает `DOMException` в пару (name, message).
fn read_exception(e: &JsValue) -> (String, String) {
    let name = Reflect::get(e, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| String::from("Error"));
    let message = Reflect::get(e, &JsValue::from_str("message"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    return (name, message);
}

/// Формирует человекочитаемое сообщение об ошибке `requestDevice`.
pub fn format_bluetooth_error(name: &str, message: &str) -> String {
    match name {
        "NotFoundError" | "AbortError" => {
            "Пользователь отменил выбор устройства.".to_string()
        }
        "SecurityError" => {
            "Web Bluetooth требует secure context (HTTPS или localhost) \
             и user gesture (клик по кнопке)."
                .to_string()
        }
        "NetworkError" => {
            "Bluetooth-адаптер недоступен (выключен или занят другим приложением)."
                .to_string()
        }
        "NotSupportedError" => {
            "Браузер не поддерживает Web Bluetooth.".to_string()
        }
        "InvalidStateError" => {
            "Состояние устройства не позволяет выполнить операцию.".to_string()
        }
        "NotAllowedError" => {
            "Пользователь запретил доступ к Bluetooth.".to_string()
        }
        _ => {
            format_str(format_args!("{name} — {message}"))
        }
    }
}

// =============================================================================
// requestDevice
// =============================================================================

// ... (начало файла без изменений)

// =============================================================================
// requestDevice
// =============================================================================

/// Параметры для `navigator.bluetooth.requestDevice`.
pub struct RequestDeviceOptions {
    /// Если `true`, будут показаны все устройства.
    /// `filters` при этом игнорируется.
    pub accept_all: bool,
    /// UUID сервисов, к которым сайт хочет иметь доступ.
    /// Обязательно для `accept_all = true`, иначе будет `SecurityError`.
    pub optional_services: Vec<String>,
    /// Если `accept_all = false`, это фильтры по сервисам.
    pub filters_services: Vec<String>,
}

impl RequestDeviceOptions {
    /// Поиск конкретного типа устройства по сервису.
    pub fn for_service(service: &str) -> Self {
        Self {
            accept_all: false,
            optional_services: vec![],
            filters_services: vec![service.to_string()],
        }
    }

    /// Показать все устройства, но с доступом к перечисленным сервисам.
    pub fn all_devices(optional_services: Vec<String>) -> Self {
        Self {
            accept_all: true,
            optional_services,
            filters_services: vec![],
        }
    }
}

/// `navigator.bluetooth.requestDevice(options)`.
pub async fn request_device(options: RequestDeviceOptions) -> Result<JsValue, String> {
    if !has_web_bluetooth() {
        return Err(String::from("navigator.bluetooth недоступен"));
    }

    let window = web_sys::window().ok_or_else(|| String::from("no window"))?;
    let nav = Reflect::get(window.as_ref(), &JsValue::from_str("navigator"))
        .map_err(|_| String::from("no navigator"))?;
    let bt = Reflect::get(&nav, &JsValue::from_str("bluetooth"))
        .map_err(|_| String::from("no navigator.bluetooth"))?;
    let request = Reflect::get(&bt, &JsValue::from_str("requestDevice"))
        .map_err(|_| String::from("no requestDevice"))?;
    let request = request
        .dyn_into::<js_sys::Function>()
        .map_err(|_| String::from("requestDevice is not a function"))?;

    // Собираем JS-объект options
    let js_options = Object::new();

    if options.accept_all {
        // Для acceptAllDevices обязателен непустой optionalServices
        if options.optional_services.is_empty() {
            return Err(String::from(
                "optionalServices не может быть пустым при acceptAllDevices = true",
            ));
        }
        set_prop(&js_options, "acceptAllDevices", &JsValue::TRUE)
            .map_err(|e| format_str(format_args!("set acceptAllDevices: {e:?}")))?;

        let opt_services = Array::new();
        for s in &options.optional_services {
            opt_services.push(&JsValue::from_str(s));
        }
        set_prop(&js_options, "optionalServices", &opt_services)
            .map_err(|e| format_str(format_args!("set optionalServices: {e:?}")))?;
    } else {
        // Используем filters
        let services_arr = Array::new();
        for s in &options.filters_services {
            services_arr.push(&JsValue::from_str(s));
        }
        let filter = Object::new();
        set_prop(&filter, "services", &services_arr)
            .map_err(|e| format_str(format_args!("set filter.services: {e:?}")))?;

        let filters_arr = Array::new();
        filters_arr.push(&filter);
        set_prop(&js_options, "filters", &filters_arr)
            .map_err(|e| format_str(format_args!("set filters: {e:?}")))?;
    }

    let promise = request
        .call1(&bt, js_options.as_ref())
        .map_err(|e| format_str(format_args!("call requestDevice: {e:?}")))?
        .dyn_into::<Promise>()
        .map_err(|_| String::from("requestDevice did not return a Promise"))?;

    match JsFuture::from(promise).await {
        Ok(v) => Ok(v),
        Err(e) => {
            let (name, message) = read_exception(&e);
            Err(format_bluetooth_error(&name, &message))
        }
    }
}



/// `navigator.bluetooth.getDevices()` — уже сопряжённые устройства,
/// без нового запроса к пользователю. Доступно в Chrome 108+.
pub async fn get_known_devices() -> Result<Vec<JsValue>, String> {
    if !has_web_bluetooth() {
        return Err(String::from("navigator.bluetooth недоступен"));
    }
    let Some(window) = web_sys::window() else {
        return Err(String::from("no window"));
    };
    let Ok(nav) = Reflect::get(window.as_ref(), &JsValue::from_str("navigator")) else {
        return Err(String::from("no navigator"));
    };
    let Ok(bt) = Reflect::get(&nav, &JsValue::from_str("bluetooth")) else {
        return Err(String::from("no navigator.bluetooth"));
    };
    let get = match Reflect::get(&bt, &JsValue::from_str("getDevices")) {
        Ok(f) => f,
        Err(_) => return Ok(Vec::new()),
    };
    let Ok(get) = get.dyn_into::<js_sys::Function>() else {
        return Ok(Vec::new());
    };
    let Ok(promise) = get.call0(&bt) else {
        return Ok(Vec::new());
    };
    let Ok(promise) = promise.dyn_into::<Promise>() else {
        return Ok(Vec::new());
    };
    let Ok(result) = JsFuture::from(promise).await else {
        return Ok(Vec::new());
    };
    let Ok(arr) = result.clone().dyn_into::<Array>() else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(arr.length() as usize);
    for i in 0..arr.length() {
        out.push(arr.get(i));
    }
    return Ok(out);
}

// =============================================================================
// Device info
// =============================================================================

pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub connected: bool,
}

/// Читает поля `BluetoothDevice`. `name` может быть пустым —
/// некоторые устройства не объявляют его.
pub fn read_device_info(device: &JsValue) -> DeviceInfo {
    let read_str = |key: &str| -> String {
        Reflect::get(device, &JsValue::from_str(key))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default()
    };
    let connected = Reflect::get(device, &JsValue::from_str("gatt"))
        .ok()
        .and_then(|g| {
            Reflect::get(&g, &JsValue::from_str("connected"))
                .ok()
                .and_then(|v| v.as_bool())
        })
        .unwrap_or(false);
    return DeviceInfo {
        id: read_str("id"),
        name: read_str("name"),
        connected,
    };
}

/// Подписка на `gattserverdisconnected`.
///
/// Возвращает `Closure`, который нужно сохранить живым — иначе
/// callback будет собран GC.
pub fn subscribe_disconnect<F>(device: &JsValue, mut callback: F) -> Result<(), String>
where
    F: FnMut() + 'static,
{
    let add = Reflect::get(device, &JsValue::from_str("addEventListener"))
        .map_err(|_| String::from("no addEventListener"))?;
    let add = add
        .dyn_into::<js_sys::Function>()
        .map_err(|_| String::from("addEventListener is not a function"))?;

    let closure = wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(
        move |_event: JsValue| {
            callback();
        },
    );

    add.call2(
        device,
        &JsValue::from_str("gattserverdisconnected"),
        closure.as_ref().unchecked_ref(),
    )
    .map_err(|e| format_str(format_args!("addEventListener: {e:?}")))?;

    // Оставляем closure живым — JS держит ссылку на функцию.
    closure.forget();
    return Ok(());
}

// =============================================================================
// Connect / disconnect
// =============================================================================

/// `device.gatt.connect()` → `BluetoothRemoteGATTServer`.
pub async fn connect(device: &JsValue) -> Result<JsValue, String> {
    let gatt = Reflect::get(device, &JsValue::from_str("gatt"))
        .map_err(|_| String::from("device has no .gatt"))?;
    if gatt.is_undefined() || gatt.is_null() {
        return Err(String::from("device.gatt недоступен"));
    }

    let connect = Reflect::get(&gatt, &JsValue::from_str("connect"))
        .map_err(|_| String::from("no gatt.connect"))?;
    let connect = connect
        .dyn_into::<js_sys::Function>()
        .map_err(|_| String::from("gatt.connect is not a function"))?;

    let promise = connect
        .call0(&gatt)
        .map_err(|e| format_str(format_args!("call connect: {e:?}")))?
        .dyn_into::<Promise>()
        .map_err(|_| String::from("connect did not return a Promise"))?;

    match JsFuture::from(promise).await {
        Ok(v) => Ok(v),
        Err(e) => {
            let (name, message) = read_exception(&e);
            Err(format_str(format_args!("connect: {name} — {message}")))
        }
    }
}

/// `device.gatt.disconnect()`. Идемпотентно.
pub fn disconnect(device: &JsValue) -> Result<(), String> {
    let Ok(gatt) = Reflect::get(device, &JsValue::from_str("gatt")) else {
        return Ok(());
    };
    if gatt.is_undefined() || gatt.is_null() {
        return Ok(());
    }
    let Ok(disconnect) = Reflect::get(&gatt, &JsValue::from_str("disconnect")) else {
        return Ok(());
    };
    let Ok(disconnect) = disconnect.dyn_into::<js_sys::Function>() else {
        return Ok(());
    };
    let _ = disconnect.call0(&gatt);
    return Ok(());
}