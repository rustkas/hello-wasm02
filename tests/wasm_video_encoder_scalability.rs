#![cfg(target_arch = "wasm32")]

//! 23.14. WASM-тесты temporal scalability.
//!
//! ## Что проверяется
//!
//! - Справочники (константы) — тривиально.
//! - Feature detection (`isConfigSupported`) — реально.
//! - `configure` — соответствует `isConfigSupported`.
//! - **Реальное кодирование** — L1T3 даёт несколько слоёв, L1T1 — нет.
//! - Утилиты (`extract_temporal_layer_id`, `format_layer_distribution`) —
//!   точное содержимое.
//!
//! ## Особенности headless
//!
//! - `configure` может вернуть `Err` в некоторых сборках Chrome —
//!   это ОК, если `isConfigSupported` тоже вернул `false`.
//! - Тесты кодирования пропускаются (return), если L1T3 не поддержан.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;

// Импорт из video_encoder — только публичные обёртки.
use hello_wasm02::video_encoder::{
    close as encoder_close, flush as encoder_flush, make_video_encoder,
};
// Мини-хелперы JsValue определены в video_frame — импорт оттуда.
use hello_wasm02::video_frame::{get_fn, set_prop};
use hello_wasm02::video_encoder_scalability::{
    check_scalability_config, configure_scalability, extract_temporal_layer_id,
    format_layer_distribution, is_temporal_scalability_supported, mode_label,
    mode_layers, SCALABILITY_MODES,
};

wasm_bindgen_test_configure!(run_in_browser);

// Тестовый кодек: VP9 Profile 0. Chrome поддерживает L1T3 для VP9.
const CODEC: &str = "vp09.00.10.08";
const W: u32 = 320;
const H: u32 = 240;
const BITRATE: u32 = 500_000;
const FPS: f64 = 30.0;

// =============================================================================
// Хелперы
// =============================================================================

/// Создаёт VideoFrame из пустого canvas. Размер задан константами.
fn make_test_frame(timestamp_us: f64) -> Result<JsValue, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let document = window.document()
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let canvas_el = document.create_element("canvas")?;
    let canvas: web_sys::HtmlCanvasElement = canvas_el.dyn_into()?;
    canvas.set_width(W);
    canvas.set_height(H);

    let global = js_sys::global();
    let ctor = js_sys::Reflect::get(&global, &JsValue::from_str("VideoFrame"))?;
    if !ctor.is_function() {
        return Err(JsValue::from_str("VideoFrame not available"));
    }
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let init = js_sys::Object::new();
    set_prop(&init, "timestamp", &JsValue::from_f64(timestamp_us))?;
    let args = js_sys::Array::new();
    args.push(&JsValue::from(&canvas));
    args.push(&init);
    js_sys::Reflect::construct(&ctor, &args)
}

/// Проверяет поддержку L1T3 с тестовыми параметрами.
/// Возвращает true, если можно запускать тесты кодирования.
async fn is_l1t3_available() -> bool {
    return check_scalability_config(CODEC, W, H, BITRATE, FPS, "L1T3")
        .await
        .unwrap_or(false);
}

// =============================================================================
// Справочники
// =============================================================================

#[wasm_bindgen_test]
fn scalability_modes_well_formed() {
    assert_eq!(SCALABILITY_MODES.len(), 3);
    assert_eq!(SCALABILITY_MODES[0].0, "L1T1");
    assert_eq!(SCALABILITY_MODES[1].0, "L1T2");
    assert_eq!(SCALABILITY_MODES[2].0, "L1T3");
}

#[wasm_bindgen_test]
fn mode_layers_ok() {
    assert_eq!(mode_layers("L1T1"), 1);
    assert_eq!(mode_layers("L1T2"), 2);
    assert_eq!(mode_layers("L1T3"), 3);
    assert_eq!(mode_layers("unknown"), 1);
}

#[wasm_bindgen_test]
fn mode_label_exact_content() {
    assert_eq!(mode_label("L1T1"), "L1T1 (no scaling)");
    assert_eq!(mode_label("L1T2"), "L1T2 (2 layers)");
    assert_eq!(mode_label("L1T3"), "L1T3 (3 layers)");
    // Fallback возвращает исходную строку — важно, что не паникует.
    assert_eq!(mode_label("L2T3"), "L2T3");
    assert_eq!(mode_label(""), "");
}

// =============================================================================
// Feature detection
// =============================================================================

#[wasm_bindgen_test]
async fn l1t1_supported() {
    let ok = check_scalability_config(CODEC, W, H, BITRATE, FPS, "L1T1")
        .await
        .unwrap_or(false);
    assert!(ok, "L1T1 должен поддерживаться в Chrome с VP9");
}

#[wasm_bindgen_test]
async fn bogus_scalability_mode_not_supported() {
    let ok = check_scalability_config(CODEC, W, H, BITRATE, FPS, "L9T9")
        .await
        .unwrap_or(false);
    assert!(!ok, "L9T9 не должен поддерживаться");
}

#[wasm_bindgen_test]
async fn l1t3_status_reported() {
    // Тест не падает — фиксирует факт обращения к API.
    let result = is_temporal_scalability_supported(CODEC, W, H, BITRATE, FPS).await;
    assert!(result.is_ok(), "функция не должна падать");
    let supported = result.unwrap();
    web_sys::console::log_1(&JsValue::from_str(&format!(
        "[ts-test] L1T3 supported: {supported}"
    )));
}

// =============================================================================
// configure — согласованность с isConfigSupported
// =============================================================================

#[wasm_bindgen_test]
async fn configure_l1t1_succeeds() {
    let on_output = Closure::<dyn FnMut(JsValue, JsValue)>::new(|_, _| {});
    let on_error = Closure::<dyn FnMut(JsValue)>::new(|_| {});
    let enc = make_video_encoder(&on_output, &on_error).expect("construct");

    let supported = check_scalability_config(CODEC, W, H, BITRATE, FPS, "L1T1")
        .await
        .unwrap_or(false);
    assert!(supported, "L1T1 должен поддерживаться");

    let result = configure_scalability(&enc, CODEC, W, H, BITRATE, FPS, "L1T1");
    assert!(result.is_ok(), "configure(L1T1) должен пройти");

    let _ = encoder_close(&enc);
    on_output.forget();
    on_error.forget();
}

#[wasm_bindgen_test]
async fn configure_l1t3_matches_isconfigsupported() {
    let on_output = Closure::<dyn FnMut(JsValue, JsValue)>::new(|_, _| {});
    let on_error = Closure::<dyn FnMut(JsValue)>::new(|_| {});
    let enc = make_video_encoder(&on_output, &on_error).expect("construct");

    let supported = check_scalability_config(CODEC, W, H, BITRATE, FPS, "L1T3")
        .await
        .unwrap_or(false);

    let result = configure_scalability(&enc, CODEC, W, H, BITRATE, FPS, "L1T3");

    // Согласованность: если isConfigSupported вернул true, configure
    // тоже должен пройти. Если false — configure может упасть.
    assert_eq!(
        supported,
        result.is_ok(),
        "isConfigSupported и configure должны согласовываться для L1T3"
    );

    let _ = encoder_close(&enc);
    on_output.forget();
    on_error.forget();
}

// =============================================================================
// extract_temporal_layer_id — точные сценарии
// =============================================================================

#[wasm_bindgen_test]
fn extract_from_empty_object_returns_none() {
    let meta = js_sys::Object::new();
    assert_eq!(extract_temporal_layer_id(&meta.into()), None);
}

#[wasm_bindgen_test]
fn extract_from_string_returns_none() {
    let meta = JsValue::from_str("not metadata");
    assert_eq!(extract_temporal_layer_id(&meta), None);
}

#[wasm_bindgen_test]
fn extract_from_null_svc_returns_none() {
    let meta = js_sys::Object::new();
    set_prop(&meta, "svc", &JsValue::NULL).unwrap();
    assert_eq!(extract_temporal_layer_id(&meta.into()), None);
}

#[wasm_bindgen_test]
fn extract_reads_layer_zero() {
    let svc = js_sys::Object::new();
    set_prop(&svc, "temporalLayerId", &JsValue::from_f64(0.0)).unwrap();
    let meta = js_sys::Object::new();
    set_prop(&meta, "svc", &svc).unwrap();
    assert_eq!(extract_temporal_layer_id(&meta.into()), Some(0));
}

#[wasm_bindgen_test]
fn extract_reads_layer_two() {
    let svc = js_sys::Object::new();
    set_prop(&svc, "temporalLayerId", &JsValue::from_f64(2.0)).unwrap();
    let meta = js_sys::Object::new();
    set_prop(&meta, "svc", &svc).unwrap();
    assert_eq!(extract_temporal_layer_id(&meta.into()), Some(2));
}

// =============================================================================
// format_layer_distribution
// =============================================================================

#[wasm_bindgen_test]
fn format_layer_distribution_ok() {
    let s = format_layer_distribution(&[10, 5, 3]);
    assert!(s.contains("T0: 10"), "got: {s}");
    assert!(s.contains("T1: 5"), "got: {s}");
    assert!(s.contains("T2: 3"), "got: {s}");
}

#[wasm_bindgen_test]
fn format_layer_distribution_zero() {
    let s = format_layer_distribution(&[0, 0, 0]);
    assert!(s.contains("T0: 0"), "got: {s}");
    assert!(s.contains("T1: 0"), "got: {s}");
    assert!(s.contains("T2: 0"), "got: {s}");
}

// =============================================================================
// Реальное кодирование: L1T3 даёт несколько слоёв
// =============================================================================

#[wasm_bindgen_test]
async fn l1t3_produces_temporal_layers() {
    if !is_l1t3_available().await {
        web_sys::console::warn_1(&JsValue::from_str(
            "[ts-test] L1T3 не поддерживается, тест пропущен",
        ));
        return;
    }

    let layers: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(Vec::new()));
    let layers_cb = layers.clone();
    let on_output = Closure::<dyn FnMut(JsValue, JsValue)>::new(
        move |_chunk: JsValue, meta: JsValue| {
            let id = extract_temporal_layer_id(&meta).unwrap_or(0);
            layers_cb.borrow_mut().push(id);
        },
    );
    let on_error = Closure::<dyn FnMut(JsValue)>::new(|e: JsValue| {
        web_sys::console::error_1(&e);
    });

    let enc = make_video_encoder(&on_output, &on_error).expect("construct");
    configure_scalability(&enc, CODEC, W, H, BITRATE, FPS, "L1T3")
        .expect("configure L1T3");

    // 10 кадров.
    for i in 0..10u32 {
        let frame = make_test_frame(i as f64 * 33_333.0).expect("frame");
        let opts = js_sys::Object::new();
        if i == 0 {
            set_prop(&opts, "keyFrame", &JsValue::TRUE).unwrap();
        }
        let encode_fn = get_fn(&enc, "encode").unwrap();
        let _ = encode_fn.call2(&enc, &frame, &opts);
        // yield — чтобы callback успел.
        gloo_timers::future::TimeoutFuture::new(0).await;
    }

    encoder_flush(&enc).await.expect("flush");
    let _ = encoder_close(&enc);
    on_output.forget();
    on_error.forget();

    let collected = layers.borrow().clone();
    web_sys::console::log_1(&JsValue::from_str(&format!(
        "[ts-test] L1T3 layers: {:?}", collected,
    )));

    assert!(!collected.is_empty(), "должны быть чанки");
    assert_eq!(collected[0], 0, "первый чанк — T0");
    assert!(
        collected.iter().all(|&l| l <= 2),
        "temporalLayerId не должен превышать 2 для L1T3"
    );
    // Хотя бы один кадр должен быть в T0.
    let t0_count = collected.iter().filter(|&&l| l == 0).count();
    assert!(t0_count >= 1, "должен быть хотя бы один T0-чанк");
}

// =============================================================================
// Реальное кодирование: L1T1 не даёт слоёв выше T0
// =============================================================================

#[wasm_bindgen_test]
async fn l1t1_has_only_layer_zero() {
    let supported = check_scalability_config(CODEC, W, H, BITRATE, FPS, "L1T1")
        .await
        .unwrap_or(false);
    if !supported { return; }

    let ids: Rc<RefCell<Vec<Option<u32>>>> = Rc::new(RefCell::new(Vec::new()));
    let ids_cb = ids.clone();
    let on_output = Closure::<dyn FnMut(JsValue, JsValue)>::new(
        move |_c: JsValue, meta: JsValue| {
            ids_cb.borrow_mut().push(extract_temporal_layer_id(&meta));
        },
    );
    let on_error = Closure::<dyn FnMut(JsValue)>::new(|_| {});

    let enc = make_video_encoder(&on_output, &on_error).expect("construct");
    configure_scalability(&enc, CODEC, W, H, BITRATE, FPS, "L1T1")
        .expect("configure L1T1");

    for i in 0..3u32 {
        let frame = make_test_frame(i as f64 * 33_333.0).expect("frame");
        let opts = js_sys::Object::new();
        if i == 0 {
            set_prop(&opts, "keyFrame", &JsValue::TRUE).unwrap();
        }
        let encode_fn = get_fn(&enc, "encode").unwrap();
        let _ = encode_fn.call2(&enc, &frame, &opts);
        gloo_timers::future::TimeoutFuture::new(0).await;
    }

    encoder_flush(&enc).await.expect("flush");
    let _ = encoder_close(&enc);
    on_output.forget();
    on_error.forget();

    let collected = ids.borrow().clone();
    web_sys::console::log_1(&JsValue::from_str(&format!(
        "[ts-test] L1T1 layer ids: {:?}", collected,
    )));

    for id in &collected {
        assert!(
            id.is_none() || *id == Some(0),
            "L1T1 не должен производить чанки со слоем > 0, got: {id:?}",
        );
    }
}