//! 35. Wasm-тесты media_devices: feature detection, constraints,
//! парсинг device info, форматирование ошибок.
//!
//! ПРАВИЛО: roundtrip from_str(as_str()) проверяем только для
//! известных вариантов. Catch-all вариант (Unknown) участвует в
//! отдельном тесте — он не хранит исходную строку и не может
//! восстановить её.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

use hello_wasm02::media_devices::{
    self, DeviceInfo, DeviceKind, FacingMode, StreamConstraints, TrackSettings,
};

wasm_bindgen_test_configure!(run_in_browser);

// ---------------- Feature detection ----------------

#[wasm_bindgen_test]
fn wasm_media_devices_supported() {
    assert!(media_devices::has_media_devices());
}

#[wasm_bindgen_test]
fn wasm_get_user_media_supported() {
    assert!(media_devices::has_get_user_media());
}

#[wasm_bindgen_test]
fn wasm_enumerate_devices_supported() {
    assert!(media_devices::has_enumerate_devices());
}

// ---------------- Device kind ----------------

#[wasm_bindgen_test]
fn wasm_device_kind_known_roundtrip() {
    // Roundtrip from_str(as_str()) == x выполняется для всех
    // известных вариантов.
    let cases: [(&str, DeviceKind); 3] = [
        ("videoinput", DeviceKind::VideoInput),
        ("audioinput", DeviceKind::AudioInput),
        ("audiooutput", DeviceKind::AudioOutput),
    ];
    for (s, k) in cases {
        assert_eq!(DeviceKind::from_str(s), k);
        assert_eq!(k.as_str(), s);
    }
}

#[wasm_bindgen_test]
fn wasm_device_kind_unknown_catch_all() {
    // Неизвестная строка → Unknown. Обратного roundtrip нет:
    // Unknown.as_str() возвращает каноничный "unknown", а не исходную
    // строку. Проверяем именно это поведение.
    assert_eq!(DeviceKind::from_str("garbage"), DeviceKind::Unknown);
    assert_eq!(DeviceKind::from_str(""), DeviceKind::Unknown);
    assert_eq!(DeviceKind::from_str("VideoInput"), DeviceKind::Unknown);
    assert_eq!(DeviceKind::Unknown.as_str(), "unknown");
}

// ---------------- Device info ----------------

#[wasm_bindgen_test]
fn wasm_device_display_label_without_permission() {
    // До разрешения label пуст — показываем «Камера N».
    let d = DeviceInfo {
        kind: DeviceKind::VideoInput,
        device_id: String::new(),
        group_id: String::new(),
        label: String::new(),
    };
    assert!(!d.has_label());
    assert_eq!(d.display_label(0), "Камера 1");
    assert_eq!(d.display_label(2), "Камера 3");

    let d = DeviceInfo {
        kind: DeviceKind::AudioInput,
        device_id: String::new(),
        group_id: String::new(),
        label: String::new(),
    };
    assert_eq!(d.display_label(0), "Микрофон 1");
}

#[wasm_bindgen_test]
fn wasm_device_display_label_with_permission() {
    let d = DeviceInfo {
        kind: DeviceKind::VideoInput,
        device_id: "abc123".to_string(),
        group_id: "grp".to_string(),
        label: "FaceTime HD Camera".to_string(),
    };
    assert!(d.has_label());
    assert_eq!(d.display_label(0), "FaceTime HD Camera");
}

// ---------------- Constraints building ----------------

#[wasm_bindgen_test]
fn wasm_constraints_video_only() {
    let c = StreamConstraints {
        video: true,
        audio: false,
        ..Default::default()
    };
    let obj = media_devices::build_constraints(&c).expect("build");
    let video = js_sys::Reflect::get(
        &obj,
        &wasm_bindgen::JsValue::from_str("video"),
    )
    .unwrap();
    assert!(video.is_object());
    let audio = js_sys::Reflect::get(
        &obj,
        &wasm_bindgen::JsValue::from_str("audio"),
    )
    .unwrap();
    assert!(!audio.as_bool().unwrap_or(true));
}

#[wasm_bindgen_test]
fn wasm_constraints_ideal_not_exact() {
    // ideal — не бросает OverconstrainedError при невыполнении.
    let c = StreamConstraints::default();
    let obj = media_devices::build_constraints(&c).expect("build");
    let video = js_sys::Reflect::get(
        &obj,
        &wasm_bindgen::JsValue::from_str("video"),
    )
    .unwrap();
    let width = js_sys::Reflect::get(
        &video,
        &wasm_bindgen::JsValue::from_str("width"),
    )
    .unwrap();
    let ideal = js_sys::Reflect::get(
        &width,
        &wasm_bindgen::JsValue::from_str("ideal"),
    )
    .unwrap();
    assert_eq!(ideal.as_f64(), Some(1280.0));
    let exact = js_sys::Reflect::get(
        &width,
        &wasm_bindgen::JsValue::from_str("exact"),
    )
    .ok();
    assert!(exact.is_none() || exact.unwrap().is_undefined());
}

#[wasm_bindgen_test]
fn wasm_constraints_device_id_exact() {
    let c = StreamConstraints {
        video: true,
        video_device_id: "device-xyz".to_string(),
        ..Default::default()
    };
    let obj = media_devices::build_constraints(&c).expect("build");
    let video = js_sys::Reflect::get(
        &obj,
        &wasm_bindgen::JsValue::from_str("video"),
    )
    .unwrap();
    let dev = js_sys::Reflect::get(
        &video,
        &wasm_bindgen::JsValue::from_str("deviceId"),
    )
    .unwrap();
    let exact = js_sys::Reflect::get(
        &dev,
        &wasm_bindgen::JsValue::from_str("exact"),
    )
    .unwrap();
    assert_eq!(exact.as_string(), Some("device-xyz".to_string()));
}

#[wasm_bindgen_test]
fn wasm_constraints_facing_mode_user() {
    let c = StreamConstraints {
        facing: FacingMode::User,
        ..Default::default()
    };
    let obj = media_devices::build_constraints(&c).expect("build");
    let video = js_sys::Reflect::get(
        &obj,
        &wasm_bindgen::JsValue::from_str("video"),
    )
    .unwrap();
    let fm = js_sys::Reflect::get(
        &video,
        &wasm_bindgen::JsValue::from_str("facingMode"),
    )
    .unwrap();
    let ideal = js_sys::Reflect::get(
        &fm,
        &wasm_bindgen::JsValue::from_str("ideal"),
    )
    .unwrap();
    assert_eq!(ideal.as_string(), Some("user".to_string()));
}

// ---------------- Track settings parsing ----------------

#[wasm_bindgen_test]
fn wasm_track_settings_parses_numeric() {
    let v = js_sys::Object::new();
    js_sys::Reflect::set(
        &v,
        &wasm_bindgen::JsValue::from_str("width"),
        &1280.0.into(),
    )
    .unwrap();
    js_sys::Reflect::set(
        &v,
        &wasm_bindgen::JsValue::from_str("height"),
        &720.0.into(),
    )
    .unwrap();
    js_sys::Reflect::set(
        &v,
        &wasm_bindgen::JsValue::from_str("frameRate"),
        &30.0.into(),
    )
    .unwrap();
    let s = TrackSettings::from_js(&v);
    assert_eq!(s.width, Some(1280));
    assert_eq!(s.height, Some(720));
    assert_eq!(s.frame_rate, Some(30.0));
}

// ---------------- Error formatting ----------------

#[wasm_bindgen_test]
fn wasm_format_error_not_allowed_is_human() {
    let e = js_sys::Error::new("denied");
    js_sys::Reflect::set(
        &e,
        &wasm_bindgen::JsValue::from_str("name"),
        &"NotAllowedError".into(),
    )
    .unwrap();
    let msg = media_devices::format_media_error(&e);
    assert!(msg.contains("отклонён"));
    assert!(msg.contains("настройках"));
}

#[wasm_bindgen_test]
fn wasm_format_error_not_readable_mentions_other_apps() {
    let e = js_sys::Error::new("in use");
    js_sys::Reflect::set(
        &e,
        &wasm_bindgen::JsValue::from_str("name"),
        &"NotReadableError".into(),
    )
    .unwrap();
    let msg = media_devices::format_media_error(&e);
    assert!(msg.contains("занято"));
    assert!(msg.contains("Zoom") || msg.contains("Teams"));
}

#[wasm_bindgen_test]
fn wasm_format_error_overconstrained_mentions_constraint() {
    let e = js_sys::Error::new("too high");
    js_sys::Reflect::set(
        &e,
        &wasm_bindgen::JsValue::from_str("name"),
        &"OverconstrainedError".into(),
    )
    .unwrap();
    js_sys::Reflect::set(
        &e,
        &wasm_bindgen::JsValue::from_str("constraint"),
        &"width".into(),
    )
    .unwrap();
    let msg = media_devices::format_media_error(&e);
    assert!(msg.contains("width"));
}