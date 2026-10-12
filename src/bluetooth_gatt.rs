//! 45. Web Bluetooth: GATT — сервисы, характеристики, нотификации.
//!
//! ## Публичный API
//!
//! - `get_primary_services(gatt) -> Vec<service>` — все сервисы.
//! - `get_primary_service(gatt, uuid)` — конкретный сервис.
//! - `get_characteristics(service) -> Vec<char>` — все характеристики.
//! - `get_characteristic(service, uuid)` — конкретная характеристика.
//! - `read_value(char) -> Vec<u8>` — разовое чтение.
//! - `write_value(char, data)` — запись.
//! - `start_notifications(char)` / `stop_notifications(char)`.
//! - `subscribe_value_changed(char, callback)` — подписка на события.
//! - `parse_heart_rate(data) -> Option<u16>` — разбор HR-измерения.
//! - `format_service_uuid(uuid) -> String`,
//!   `format_characteristic_uuid(uuid) -> String`
//!   — человекочитаемые имена известных UUID.

use js_sys::{Array, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::video_frame::format_str;

// =============================================================================
// Константы — стандартные UUID Bluetooth SIG
// =============================================================================

pub const SERVICE_HEART_RATE: &str = "heart_rate";
pub const SERVICE_BATTERY: &str = "battery_service";
pub const SERVICE_ENVIRONMENTAL: &str = "environmental_sensing";
pub const SERVICE_DEVICE_INFORMATION: &str = "device_information";

pub const CHAR_HEART_RATE_MEASUREMENT: &str = "heart_rate_measurement";
pub const CHAR_BATTERY_LEVEL: &str = "battery_level";
pub const CHAR_MANUFACTURER_NAME: &str = "manufacturer_name_string";
pub const CHAR_MODEL_NUMBER: &str = "model_number_string";

// =============================================================================
// Ошибки
// =============================================================================

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

fn format_gatt_error(op: &str, name: &str, message: &str) -> String {
    match name {
        "NotFoundError" => format_str(format_args!(
            "{op}: сервис или характеристика не найдены"
        )),
        "NetworkError" => format_str(format_args!(
            "{op}: устройство отключено"
        )),
        "InvalidStateError" => format_str(format_args!(
            "{op}: устройство не в состоянии выполнить операцию"
        )),
        "NotSupportedError" => format_str(format_args!(
            "{op}: операция не поддерживается этим устройством"
        )),
        "SecurityError" => format_str(format_args!(
            "{op}: сервис не был указан в optionalServices при requestDevice"
        )),
        _ => format_str(format_args!("{op}: {name} — {message}")),
    }
}

// =============================================================================
// Утилиты вызова
// =============================================================================

/// Вызывает метод объекта, возвращающий `Promise`.
async fn call_method_promise(
    obj: &JsValue,
    method: &str,
    args: &[JsValue],
    op_label: &str,
) -> Result<JsValue, String> {
    let f = Reflect::get(obj, &JsValue::from_str(method))
        .map_err(|_| format_str(format_args!("{op_label}: нет метода {method}")))?;
    let f = f
        .dyn_into::<js_sys::Function>()
        .map_err(|_| format_str(format_args!("{op_label}: {method} — не функция")))?;

    let call_result = match args.len() {
        0 => f.call0(obj),
        1 => f.call1(obj, &args[0]),
        2 => f.call2(obj, &args[0], &args[1]),
        _ => return Err(format_str(format_args!("{op_label}: слишком много аргументов"))),
    };

    let promise = call_result
        .map_err(|e| {
            let (name, message) = read_exception(&e);
            format_gatt_error(op_label, &name, &message)
        })?
        .dyn_into::<Promise>()
        .map_err(|_| format_str(format_args!("{op_label}: результат — не Promise")))?;

    JsFuture::from(promise).await.map_err(|e| {
        let (name, message) = read_exception(&e);
        format_gatt_error(op_label, &name, &message)
    })
}

fn array_to_vec(value: &JsValue) -> Result<Vec<JsValue>, String> {
    // `dyn_into` забирает `self` — клонируем `JsValue` (это дешёвая
    // операция: увеличивает refcount на JS-объект).
    let arr = value
        .clone()
        .dyn_into::<Array>()
        .map_err(|_| String::from("ожидался Array"))?;
    let mut out = Vec::with_capacity(arr.length() as usize);
    for i in 0..arr.length() {
        out.push(arr.get(i));
    }
    return Ok(out);
}

/// `DataView` → `Vec<u8>`.
fn dataview_to_vec(value: &JsValue) -> Result<Vec<u8>, String> {
    let len = Reflect::get(value, &JsValue::from_str("byteLength"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as usize;
    let getter = Reflect::get(value, &JsValue::from_str("getUint8"))
        .map_err(|_| String::from("DataView без getUint8"))?;
    let getter = getter
        .dyn_into::<js_sys::Function>()
        .map_err(|_| String::from("getUint8 — не функция"))?;
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let byte = getter
            .call1(value, &JsValue::from_f64(i as f64))
            .map_err(|e| format_str(format_args!("getUint8: {e:?}")))?
            .as_f64()
            .unwrap_or(0.0) as u8;
        out.push(byte);
    }
    return Ok(out);
}

// =============================================================================
// Сервисы
// =============================================================================

/// `gatt.getPrimaryServices()` → все primary-сервисы.
pub async fn get_primary_services(gatt: &JsValue) -> Result<Vec<JsValue>, String> {
    let result = call_method_promise(gatt, "getPrimaryServices", &[], "getPrimaryServices").await?;
    return array_to_vec(&result);
}

/// `gatt.getPrimaryService(uuid)` → один сервис.
pub async fn get_primary_service(gatt: &JsValue, uuid: &str) -> Result<JsValue, String> {
    let arg = JsValue::from_str(uuid);
    return call_method_promise(
        gatt,
        "getPrimaryService",
        &[arg],
        "getPrimaryService",
    )
    .await;
}

/// UUID сервиса — `BluetoothRemoteGATTService.uuid`.
pub fn read_service_uuid(service: &JsValue) -> String {
    return Reflect::get(service, &JsValue::from_str("uuid"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
}

// =============================================================================
// Характеристики
// =============================================================================

/// `service.getCharacteristics()` → все характеристики.
pub async fn get_characteristics(service: &JsValue) -> Result<Vec<JsValue>, String> {
    let result = call_method_promise(
        service,
        "getCharacteristics",
        &[],
        "getCharacteristics",
    )
    .await?;
    return array_to_vec(&result);
}

/// `service.getCharacteristic(uuid)` → одна характеристика.
pub async fn get_characteristic(service: &JsValue, uuid: &str) -> Result<JsValue, String> {
    let arg = JsValue::from_str(uuid);
    return call_method_promise(
        service,
        "getCharacteristic",
        &[arg],
        "getCharacteristic",
    )
    .await;
}

/// UUID характеристики — `BluetoothRemoteGATTCharacteristic.uuid`.
pub fn read_characteristic_uuid(characteristic: &JsValue) -> String {
    return Reflect::get(characteristic, &JsValue::from_str("uuid"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
}

/// `characteristic.properties` — объект с полями `read`, `write`,
/// `notify`, `indicate`. Возвращает массив строк-флагов.
pub fn read_characteristic_properties(characteristic: &JsValue) -> Vec<&'static str> {
    let props = Reflect::get(characteristic, &JsValue::from_str("properties"))
        .unwrap_or(JsValue::UNDEFINED);
    let mut out = Vec::new();
    for key in ["read", "write", "writeWithoutResponse", "notify", "indicate"] {
        let has = Reflect::get(&props, &JsValue::from_str(key))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if has {
            match key {
                "read" => out.push("read"),
                "write" => out.push("write"),
                "writeWithoutResponse" => out.push("write-no-resp"),
                "notify" => out.push("notify"),
                "indicate" => out.push("indicate"),
                _ => {}
            }
        }
    }
    return out;
}

// =============================================================================
// Read / write
// =============================================================================

/// `characteristic.readValue()` → `Vec<u8>`.
pub async fn read_value(characteristic: &JsValue) -> Result<Vec<u8>, String> {
    let value = call_method_promise(characteristic, "readValue", &[], "readValue").await?;
    return dataview_to_vec(&value);
}

/// `characteristic.writeValue(data)`. `data` копируется в `Uint8Array`.
pub async fn write_value(characteristic: &JsValue, data: &[u8]) -> Result<(), String> {
    let u8 = js_sys::Uint8Array::from(data);
    let u8_js: JsValue = u8.into();
    let _ = call_method_promise(
        characteristic,
        "writeValue",
        &[u8_js],
        "writeValue",
    )
    .await?;
    return Ok(());
}

// =============================================================================
// Notifications
// =============================================================================

/// `characteristic.startNotifications()`. После вызова устройство
/// начнёт слать события `characteristicvaluechanged`.
pub async fn start_notifications(characteristic: &JsValue) -> Result<(), String> {
    let _ = call_method_promise(
        characteristic,
        "startNotifications",
        &[],
        "startNotifications",
    )
    .await?;
    return Ok(());
}

/// `characteristic.stopNotifications()`. Идемпотентно.
pub async fn stop_notifications(characteristic: &JsValue) -> Result<(), String> {
    let _ = call_method_promise(
        characteristic,
        "stopNotifications",
        &[],
        "stopNotifications",
    )
    .await?;
    return Ok(());
}

/// Подписка на `characteristicvaluechanged`. Возвращает handle —
/// держите его живым столько, сколько нужна подписка.
pub struct NotificationHandle {
    closure: wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>,
}

impl NotificationHandle {
    /// Возвращает raw `JsValue`-функцию для `removeEventListener`,
    /// если понадобится отписка.
    pub fn as_function(&self) -> &js_sys::Function {
        return self.closure.as_ref().unchecked_ref();
    }
}

/// `characteristic.addEventListener("characteristicvaluechanged", cb)`.
///
/// Callback получает `Vec<u8>` — уже разобранный `DataView` из
/// `event.target.value`.
pub fn subscribe_value_changed<F>(
    characteristic: &JsValue,
    mut callback: F,
) -> Result<NotificationHandle, String>
where
    F: FnMut(Vec<u8>) + 'static,
{
    let add = Reflect::get(characteristic, &JsValue::from_str("addEventListener"))
        .map_err(|_| String::from("no addEventListener"))?;
    let add = add
        .dyn_into::<js_sys::Function>()
        .map_err(|_| String::from("addEventListener — не функция"))?;

    let closure =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
            let Ok(target) = Reflect::get(&event, &JsValue::from_str("target")) else {
                return;
            };
            let Ok(value) = Reflect::get(&target, &JsValue::from_str("value")) else {
                return;
            };
            if let Ok(bytes) = dataview_to_vec(&value) {
                callback(bytes);
            }
        });

    add.call2(
        characteristic,
        &JsValue::from_str("characteristicvaluechanged"),
        closure.as_ref().unchecked_ref(),
    )
    .map_err(|e| format_str(format_args!("addEventListener: {e:?}")))?;

    return Ok(NotificationHandle { closure });
}

// =============================================================================
// Парсинг специфичных payload-ов
// =============================================================================

/// `Heart Rate Measurement` (0x2A37):
///
/// ```text
/// byte 0: flags
///   bit 0: 0 = uint8 bpm, 1 = uint16 bpm
/// bytes 1..: heart rate value (LE)
/// ```
pub fn parse_heart_rate(data: &[u8]) -> Option<u16> {
    if data.len() < 2 {
        return None;
    }
    let flags = data[0];
    let is_uint16 = (flags & 0x01) != 0;
    if is_uint16 {
        if data.len() < 3 {
            return None;
        }
        return Some(u16::from_le_bytes([data[1], data[2]]));
    }
    return Some(data[1] as u16);
}

/// `Battery Level` (0x2A19): один байт 0–100.
pub fn parse_battery_level(data: &[u8]) -> Option<u8> {
    if data.is_empty() {
        return None;
    }
    return Some(data[0].min(100));
}

// =============================================================================
// Человекочитаемые имена UUID
// =============================================================================

/// Известные UUID сервисов → читаемые имена.
pub fn format_service_uuid(uuid: &str) -> String {
    let lower = uuid.to_lowercase();
    let name = match lower.as_str() {
        "180d" | "0000180d-0000-1000-8000-00805f9b34fb" => "Heart Rate",
        "180f" | "0000180f-0000-1000-8000-00805f9b34fb" => "Battery Service",
        "181a" | "0000181a-0000-1000-8000-00805f9b34fb" => "Environmental Sensing",
        "180a" | "0000180a-0000-1000-8000-00805f9b34fb" => "Device Information",
        "1800" | "00001800-0000-1000-8000-00805f9b34fb" => "Generic Access",
        "1801" | "00001801-0000-1000-8000-00805f9b34fb" => "Generic Attribute",
        _ => return uuid.to_string(),
    };
    return format_str(format_args!("{name} ({lower})"));
}

/// Известные UUID характеристик → читаемые имена.
pub fn format_characteristic_uuid(uuid: &str) -> String {
    let lower = uuid.to_lowercase();
    let name = match lower.as_str() {
        "2a37" | "00002a37-0000-1000-8000-00805f9b34fb" => "Heart Rate Measurement",
        "2a38" | "00002a38-0000-1000-8000-00805f9b34fb" => "Body Sensor Location",
        "2a19" | "00002a19-0000-1000-8000-00805f9b34fb" => "Battery Level",
        "2a29" | "00002a29-0000-1000-8000-00805f9b34fb" => "Manufacturer Name",
        "2a24" | "00002a24-0000-1000-8000-00805f9b34fb" => "Model Number",
        _ => return uuid.to_string(),
    };
    return format_str(format_args!("{name} ({lower})"));
}

// =============================================================================
// Форматирование для UI
// =============================================================================

/// Преобразует массив байт в hex-строку с пробелами.
pub fn hex_bytes(data: &[u8]) -> String {
    if data.is_empty() {
        return String::from("—");
    }
    let mut out = String::with_capacity(data.len() * 3);
    for (i, b) in data.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format_str(format_args!("{b:02X}")));
    }
    return out;
}