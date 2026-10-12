#![cfg(target_arch = "wasm32")]

//! Тесты 45 — Web Bluetooth.
//!
//! ## Что проверяется без реального Bluetooth-устройства
//!
//! Web Bluetooth требует **user gesture** (клика) и **реального
//! устройства** — оба недоступны в headless-тестах. Поэтому
//! тестируем то, что можно:
//!
//! - Feature detection: `has_web_bluetooth`, `is_secure_context`.
//! - Чистые функции: `parse_heart_rate`, `parse_battery_level`,
//!   `hex_bytes`, `format_service_uuid`, `format_characteristic_uuid`,
//!   `format_bluetooth_error`.
//! - Чтение полей моков: `read_device_info`, `read_service_uuid`,
//!   `read_characteristic_uuid`, `read_characteristic_properties`.
//! - Конструкторы `RequestDeviceOptions` — чистая логика.
//! - Идемпотентность `disconnect` (пустой объект → Ok).
//! - Регистрация callback в `subscribe_disconnect` через мок.
//!
//! ## Что помечено `#[ignore]`
//!
//! `request_device` — требует user gesture. Тест документирует
//! правило: без клика всегда `SecurityError`. Проверяется вручную
//! через демо-пример.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::{Object, Reflect};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

use hello_wasm02::bluetooth_gatt::{
    format_characteristic_uuid, format_service_uuid, hex_bytes,
    parse_battery_level, parse_heart_rate, read_characteristic_properties,
    read_characteristic_uuid, read_service_uuid,
    CHAR_BATTERY_LEVEL, CHAR_HEART_RATE_MEASUREMENT,
    SERVICE_BATTERY, SERVICE_HEART_RATE,
};
use hello_wasm02::bluetooth_utils::{
    disconnect, format_bluetooth_error, get_known_devices,
    has_web_bluetooth, is_secure_context, read_device_info,
    subscribe_disconnect, RequestDeviceOptions,
};

wasm_bindgen_test_configure!(run_in_browser);

// =============================================================================
// Feature detection
// =============================================================================

#[wasm_bindgen_test]
fn feature_detection_does_not_panic() {
    let _ = has_web_bluetooth();
    let _ = is_secure_context();
}

#[wasm_bindgen_test]
fn secure_context_is_true_under_wasm_pack() {
    // wasm-pack test обслуживает на http://127.0.0.1:port/ — это
    // secure context по спецификации (localhost эквивалентен HTTPS).
    assert!(
        is_secure_context(),
        "wasm-pack test должен запускаться в secure context"
    );
}

// =============================================================================
// RequestDeviceOptions — конструкторы
// =============================================================================

#[wasm_bindgen_test]
fn request_options_for_service_produces_filter() {
    let opts = RequestDeviceOptions::for_service("heart_rate");
    assert!(!opts.accept_all, "for_service не должен включать acceptAllDevices");
    assert!(
        opts.optional_services.is_empty(),
        "for_service не заполняет optionalServices"
    );
    assert_eq!(
        opts.filters_services,
        vec!["heart_rate".to_string()],
        "for_service должен положить сервис в filters"
    );
}

#[wasm_bindgen_test]
fn request_options_all_devices_produces_accept_all() {
    let opts = RequestDeviceOptions::all_devices(vec![
        "heart_rate".to_string(),
        "battery_service".to_string(),
    ]);
    assert!(opts.accept_all, "all_devices должен включать acceptAllDevices");
    assert_eq!(
        opts.optional_services,
        vec!["heart_rate".to_string(), "battery_service".to_string()],
    );
    assert!(
        opts.filters_services.is_empty(),
        "all_devices не заполняет filters"
    );
}

#[wasm_bindgen_test]
fn request_options_all_devices_empty_services_is_rejected_at_runtime() {
    // Не проверяем сам вызов requestDevice (нужен user gesture),
    // но документируем: пустой optionalServices при acceptAllDevices
    // даст SecurityError в браузере.
    let opts = RequestDeviceOptions::all_devices(vec![]);
    assert!(opts.accept_all);
    assert!(opts.optional_services.is_empty());
}

// =============================================================================
// Константы
// =============================================================================

#[wasm_bindgen_test]
fn service_and_characteristic_constants_have_expected_values() {
    assert_eq!(SERVICE_HEART_RATE, "heart_rate");
    assert_eq!(SERVICE_BATTERY, "battery_service");
    assert_eq!(CHAR_HEART_RATE_MEASUREMENT, "heart_rate_measurement");
    assert_eq!(CHAR_BATTERY_LEVEL, "battery_level");
}

// =============================================================================
// hex_bytes
// =============================================================================

#[wasm_bindgen_test]
fn hex_bytes_formats_correctly() {
    assert_eq!(hex_bytes(&[]), "—");
    assert_eq!(hex_bytes(&[0x00]), "00");
    assert_eq!(hex_bytes(&[0xDE, 0xAD, 0xBE, 0xEF]), "DE AD BE EF");
    assert_eq!(hex_bytes(&[0x01, 0x02, 0x03]), "01 02 03");
}

// =============================================================================
// parse_heart_rate
// =============================================================================

#[wasm_bindgen_test]
fn parse_heart_rate_uint8_format() {
    assert_eq!(parse_heart_rate(&[0x00, 72]), Some(72));
    assert_eq!(parse_heart_rate(&[0x00, 80, 0xFF, 0xFF]), Some(80));
}

#[wasm_bindgen_test]
fn parse_heart_rate_uint16_format() {
    // flags=0x01 (bit 0 = 1 → uint16), value = 300 LE.
    // 300 = 0x012C → байты [0x2C, 0x01].
    assert_eq!(parse_heart_rate(&[0x01, 0x2C, 0x01]), Some(300));
}

#[wasm_bindgen_test]
fn parse_heart_rate_rejects_short_data() {
    assert_eq!(parse_heart_rate(&[]), None);
    assert_eq!(parse_heart_rate(&[0x00]), None);
    assert_eq!(parse_heart_rate(&[0x01, 0x2C]), None);
}

// =============================================================================
// parse_battery_level
// =============================================================================

#[wasm_bindgen_test]
fn parse_battery_level_basic() {
    assert_eq!(parse_battery_level(&[]), None);
    assert_eq!(parse_battery_level(&[0]), Some(0));
    assert_eq!(parse_battery_level(&[100]), Some(100));
    assert_eq!(parse_battery_level(&[200]), Some(100));
    assert_eq!(parse_battery_level(&[55, 0xFF, 0xFF]), Some(55));
}

// =============================================================================
// format_service_uuid
// =============================================================================

#[wasm_bindgen_test]
fn format_service_uuid_known() {
    let hr_short = format_service_uuid("180d");
    assert!(hr_short.contains("Heart Rate"), "получено: {hr_short}");
    assert!(hr_short.contains("180d"));

    let hr_long = format_service_uuid("0000180D-0000-1000-8000-00805F9B34FB");
    assert!(hr_long.contains("Heart Rate"), "получено: {hr_long}");

    let hr_upper = format_service_uuid("180D");
    assert!(hr_upper.contains("Heart Rate"), "получено: {hr_upper}");

    let battery = format_service_uuid("180f");
    assert!(battery.contains("Battery Service"));
}

#[wasm_bindgen_test]
fn format_service_uuid_unknown_passthrough() {
    let unknown = "deadbeef-dead-beef-dead-beefdeadbeef";
    assert_eq!(format_service_uuid(unknown), unknown);
}

// =============================================================================
// format_characteristic_uuid
// =============================================================================

#[wasm_bindgen_test]
fn format_characteristic_uuid_known() {
    let hr = format_characteristic_uuid("2a37");
    assert!(hr.contains("Heart Rate Measurement"), "получено: {hr}");

    let battery = format_characteristic_uuid("2a19");
    assert!(battery.contains("Battery Level"), "получено: {battery}");

    let manufacturer = format_characteristic_uuid("2a29");
    assert!(manufacturer.contains("Manufacturer Name"));
}

#[wasm_bindgen_test]
fn format_characteristic_uuid_unknown_passthrough() {
    let unknown = "12345678-1234-5678-1234-56789abcdef0";
    assert_eq!(format_characteristic_uuid(unknown), unknown);
}

// =============================================================================
// format_bluetooth_error
// =============================================================================

#[wasm_bindgen_test]
fn format_bluetooth_error_known_codes() {
    let cancelled = format_bluetooth_error("NotFoundError", "");
    assert!(cancelled.contains("отменил"), "получено: {cancelled}");

    let abort = format_bluetooth_error("AbortError", "");
    assert!(abort.contains("отменил"), "получено: {abort}");

    let security = format_bluetooth_error("SecurityError", "");
    assert!(security.contains("secure context"), "получено: {security}");
    assert!(security.contains("user gesture"), "получено: {security}");

    let network = format_bluetooth_error("NetworkError", "");
    assert!(network.contains("Bluetooth-адаптер"), "получено: {network}");

    let unsupported = format_bluetooth_error("NotSupportedError", "");
    assert!(unsupported.contains("не поддерживает"));

    let not_allowed = format_bluetooth_error("NotAllowedError", "");
    assert!(not_allowed.contains("запретил"));
}

#[wasm_bindgen_test]
fn format_bluetooth_error_unknown_code_passthrough() {
    let result = format_bluetooth_error("WeirdError", "some details");
    assert!(result.contains("WeirdError"));
    assert!(result.contains("some details"));
}

// =============================================================================
// read_device_info — мок-объект
// =============================================================================

#[wasm_bindgen_test]
fn read_device_info_from_full_mock() {
    let obj = Object::new();
    Reflect::set(&obj, &JsValue::from_str("id"), &JsValue::from_str("abc-123")).unwrap();
    Reflect::set(&obj, &JsValue::from_str("name"), &JsValue::from_str("My Sensor")).unwrap();

    let gatt = Object::new();
    Reflect::set(&gatt, &JsValue::from_str("connected"), &JsValue::TRUE).unwrap();
    Reflect::set(&obj, &JsValue::from_str("gatt"), &gatt).unwrap();

    let info = read_device_info(obj.as_ref());
    assert_eq!(info.id, "abc-123");
    assert_eq!(info.name, "My Sensor");
    assert!(info.connected);
}

#[wasm_bindgen_test]
fn read_device_info_handles_missing_fields() {
    let obj = Object::new();
    let info = read_device_info(obj.as_ref());
    assert_eq!(info.id, "");
    assert_eq!(info.name, "");
    assert!(!info.connected);
}

#[wasm_bindgen_test]
fn read_device_info_disconnected() {
    let obj = Object::new();
    let gatt = Object::new();
    Reflect::set(&gatt, &JsValue::from_str("connected"), &JsValue::FALSE).unwrap();
    Reflect::set(&obj, &JsValue::from_str("gatt"), &gatt).unwrap();

    let info = read_device_info(obj.as_ref());
    assert!(!info.connected);
}

// =============================================================================
// read_service_uuid / read_characteristic_uuid
// =============================================================================

#[wasm_bindgen_test]
fn read_service_uuid_from_mock() {
    let obj = Object::new();
    Reflect::set(&obj, &JsValue::from_str("uuid"), &JsValue::from_str("180d")).unwrap();
    assert_eq!(read_service_uuid(obj.as_ref()), "180d");
}

#[wasm_bindgen_test]
fn read_service_uuid_missing_returns_empty() {
    let obj = Object::new();
    assert_eq!(read_service_uuid(obj.as_ref()), "");
}

#[wasm_bindgen_test]
fn read_characteristic_uuid_from_mock() {
    let obj = Object::new();
    Reflect::set(&obj, &JsValue::from_str("uuid"), &JsValue::from_str("2a37")).unwrap();
    assert_eq!(read_characteristic_uuid(obj.as_ref()), "2a37");
}

// =============================================================================
// read_characteristic_properties — мок-объект
// =============================================================================

#[wasm_bindgen_test]
fn read_characteristic_properties_all_flags() {
    let obj = Object::new();
    let props = Object::new();
    Reflect::set(&props, &JsValue::from_str("read"), &JsValue::TRUE).unwrap();
    Reflect::set(&props, &JsValue::from_str("write"), &JsValue::TRUE).unwrap();
    Reflect::set(&props, &JsValue::from_str("notify"), &JsValue::TRUE).unwrap();
    Reflect::set(&obj, &JsValue::from_str("properties"), &props).unwrap();

    let result = read_characteristic_properties(obj.as_ref());
    assert!(result.contains(&"read"), "получено: {result:?}");
    assert!(result.contains(&"write"), "получено: {result:?}");
    assert!(result.contains(&"notify"), "получено: {result:?}");
    assert!(!result.contains(&"indicate"), "получено: {result:?}");
}

#[wasm_bindgen_test]
fn read_characteristic_properties_empty_for_missing_properties() {
    let obj = Object::new();
    let result = read_characteristic_properties(obj.as_ref());
    assert!(result.is_empty());
}

#[wasm_bindgen_test]
fn read_characteristic_properties_ignores_false_flags() {
    let obj = Object::new();
    let props = Object::new();
    Reflect::set(&props, &JsValue::from_str("read"), &JsValue::FALSE).unwrap();
    Reflect::set(&props, &JsValue::from_str("notify"), &JsValue::TRUE).unwrap();
    Reflect::set(&obj, &JsValue::from_str("properties"), &props).unwrap();

    let result = read_characteristic_properties(obj.as_ref());
    assert!(!result.contains(&"read"));
    assert!(result.contains(&"notify"));
}

// =============================================================================
// disconnect — идемпотентность
// =============================================================================

#[wasm_bindgen_test]
fn disconnect_is_idempotent_on_empty_object() {
    let obj = Object::new();
    assert!(disconnect(obj.as_ref()).is_ok());
    assert!(disconnect(obj.as_ref()).is_ok());
}

#[wasm_bindgen_test]
fn disconnect_is_ok_when_gatt_has_no_method() {
    let obj = Object::new();
    let gatt = Object::new();
    Reflect::set(&obj, &JsValue::from_str("gatt"), &gatt).unwrap();
    assert!(disconnect(obj.as_ref()).is_ok());
}

#[wasm_bindgen_test]
fn disconnect_calls_gatt_disconnect() {
    let called = Rc::new(Cell::new(false));
    let called_clone = called.clone();

    let obj = Object::new();
    let gatt = Object::new();

    let disconnect_fn =
        Closure::<dyn FnMut()>::new(move || {
            called_clone.set(true);
        });
    Reflect::set(
        &gatt,
        &JsValue::from_str("disconnect"),
        disconnect_fn.as_ref(),
    )
    .unwrap();
    disconnect_fn.forget();

    Reflect::set(&obj, &JsValue::from_str("gatt"), &gatt).unwrap();

    disconnect(obj.as_ref()).unwrap();
    assert!(called.get(), "gatt.disconnect() должен быть вызван");
}

// =============================================================================
// subscribe_disconnect — мок-объект
// =============================================================================

#[wasm_bindgen_test]
fn subscribe_disconnect_registers_correct_event() {
    let captured: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let captured_clone = captured.clone();

    let obj = Object::new();

    let add_fn =
        Closure::<dyn FnMut(JsValue, JsValue)>::new(move |ty: JsValue, _cb: JsValue| {
            if let Some(s) = ty.as_string() {
                captured_clone.borrow_mut().push(s);
            }
        });
    Reflect::set(
        &obj,
        &JsValue::from_str("addEventListener"),
        add_fn.as_ref(),
    )
    .unwrap();
    add_fn.forget();

    let result = subscribe_disconnect(obj.as_ref(), || {});
    assert!(result.is_ok());

    let captured = captured.borrow();
    assert_eq!(captured.len(), 1, "addEventListener вызван один раз");
    assert_eq!(captured[0], "gattserverdisconnected");
}

// =============================================================================
// get_known_devices — не должен падать без API
// =============================================================================

#[wasm_bindgen_test]
async fn get_known_devices_returns_ok_or_clean_error() {
    let result = get_known_devices().await;
    match result {
        Ok(devices) => {
            let _ = devices.len();
        }
        Err(msg) => {
            assert!(
                msg.contains("bluetooth") || msg.contains("navigator"),
                "ошибка должна упоминать bluetooth или navigator: {msg}"
            );
        }
    }
}

// =============================================================================
// request_device — требует user gesture
// =============================================================================

#[wasm_bindgen_test]
#[ignore = "требует user gesture — проверяется вручную через демо-пример"]
async fn request_device_requires_user_gesture() {
    // В headless-тесте вызов упадёт с SecurityError или NotFoundError.
    // Тест помечен `#[ignore]`, но документирует правило:
    // requestDevice работает только из обработчика клика.
    let opts = RequestDeviceOptions::for_service("heart_rate");
    let result = hello_wasm02::bluetooth_utils::request_device(opts).await;
    assert!(result.is_err(), "без user gesture должен быть Err");
}

#[wasm_bindgen_test]
#[ignore = "требует user gesture — проверяется вручную через демо-пример"]
async fn request_device_all_devices_requires_user_gesture() {
    let opts = RequestDeviceOptions::all_devices(vec!["heart_rate".to_string()]);
    let result = hello_wasm02::bluetooth_utils::request_device(opts).await;
    assert!(result.is_err(), "без user gesture должен быть Err");
}