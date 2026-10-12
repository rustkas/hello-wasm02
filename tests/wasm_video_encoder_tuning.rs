#![cfg(target_arch = "wasm32")]

use js_sys::Object;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::*;

use hello_wasm02::video_encoder::{
    chunk_byte_length, chunk_type, close as encoder_close,
    configure as base_configure, is_video_encoder_supported,
    make_video_encoder,
};
use hello_wasm02::video_encoder_tuning::{
    check_codecs, check_one_codec, configure_tuned, TuningConfig,
    ALPHA_MODES, BITRATE_MODES, CONTENT_HINTS, CODECS,
    HW_ACCELS, LATENCY_MODES, SCALABILITY_MODES,
};
use hello_wasm02::video_frame::{close as frame_close, num, set_prop};

wasm_bindgen_test_configure!(run_in_browser);

// =============================================================================
// Справочники допустимых значений
// =============================================================================

#[wasm_bindgen_test]
fn codec_list_is_well_formed() {
    assert!(CODECS.len() >= 4);
    assert_eq!(CODECS[0].0, "avc1.42001E");
    for (codec, _) in CODECS {
        assert!(!codec.is_empty());
    }
}

#[wasm_bindgen_test]
fn latency_modes_ok() {
    assert!(LATENCY_MODES.contains(&"quality"));
    assert!(LATENCY_MODES.contains(&"realtime"));
}

#[wasm_bindgen_test]
fn bitrate_modes_ok() {
    assert!(BITRATE_MODES.contains(&"variable"));
    assert!(BITRATE_MODES.contains(&"constant"));
}

#[wasm_bindgen_test]
fn content_hints_ok() {
    assert!(CONTENT_HINTS.contains(&"motion"));
    assert!(CONTENT_HINTS.contains(&"detail"));
    assert!(CONTENT_HINTS.contains(&"text"));
}

#[wasm_bindgen_test]
fn hw_accels_ok() {
    assert!(HW_ACCELS.contains(&"no-preference"));
    assert!(HW_ACCELS.contains(&"prefer-hardware"));
    assert!(HW_ACCELS.contains(&"prefer-software"));
}

#[wasm_bindgen_test]
fn scalability_modes_ok() {
    assert_eq!(SCALABILITY_MODES[0], "(off)");
    assert!(SCALABILITY_MODES.contains(&"L1T1"));
    assert!(SCALABILITY_MODES.contains(&"L1T3"));
    assert!(SCALABILITY_MODES.contains(&"L2T1"));
}

#[wasm_bindgen_test]
fn alpha_modes_ok() {
    assert_eq!(ALPHA_MODES[0], "(off)");
    assert!(ALPHA_MODES.contains(&"discard"));
    assert!(ALPHA_MODES.contains(&"keep"));
}

// =============================================================================
// Feature detection
// =============================================================================

#[wasm_bindgen_test]
fn video_encoder_is_supported_in_browser() {
    assert!(is_video_encoder_supported());
}

// =============================================================================
// Фабрика конфигураций
// =============================================================================

fn default_cfg<'a>(codec: &'a str) -> TuningConfig<'a> {
    TuningConfig {
        codec,
        width: 320,
        height: 240,
        bitrate: 1_000_000,
        framerate: 30.0,
        latency_mode: Some("quality"),
        bitrate_mode: Some("variable"),
        content_hint: Some("motion"),
        hardware_acceleration: Some("no-preference"),
        scalability_mode: None,
        alpha: None,
    }
}

// =============================================================================
// isConfigSupported через TuningConfig
// =============================================================================

#[wasm_bindgen_test]
async fn check_one_codec_h264_baseline() {
    let cfg = default_cfg("avc1.42001E");
    let ok = check_one_codec("avc1.42001E", &cfg).await.unwrap_or(false);
    assert!(ok, "avc1.42001E 320x240 1Mbps must be supported");
}

#[wasm_bindgen_test]
async fn check_codecs_returns_all_four() {
    let cfg = default_cfg("avc1.42001E");
    let results = check_codecs(&cfg).await;
    assert_eq!(results.len(), CODECS.len());
    assert!(
        results.iter().any(|r| r.supported),
        "At least one codec must be supported",
    );
}

#[wasm_bindgen_test]
async fn check_codecs_with_realtime_latency() {
    let mut cfg = default_cfg("avc1.42001E");
    cfg.latency_mode = Some("realtime");
    let results = check_codecs(&cfg).await;
    assert!(results.iter().any(|r| r.supported));
}

#[wasm_bindgen_test]
async fn check_codecs_with_content_hint_text() {
    let mut cfg = default_cfg("avc1.42001E");
    cfg.content_hint = Some("text");
    let results = check_codecs(&cfg).await;
    assert_eq!(results.len(), CODECS.len());
}

#[wasm_bindgen_test]
async fn check_codecs_with_constant_bitrate() {
    let mut cfg = default_cfg("avc1.42001E");
    cfg.bitrate_mode = Some("constant");
    let results = check_codecs(&cfg).await;
    assert!(results.iter().any(|r| r.supported));
}

#[wasm_bindgen_test]
async fn check_codecs_with_prefer_software() {
    let mut cfg = default_cfg("avc1.42001E");
    cfg.hardware_acceleration = Some("prefer-software");
    let results = check_codecs(&cfg).await;
    assert!(results.iter().any(|r| r.supported));
}

#[wasm_bindgen_test]
async fn check_codecs_with_scalability_mode() {
    let mut cfg = default_cfg("vp09.00.10.08");
    cfg.scalability_mode = Some("L1T2");
    let results = check_codecs(&cfg).await;
    assert_eq!(results.len(), CODECS.len());
}

#[wasm_bindgen_test]
async fn check_codecs_with_alpha_keep() {
    let mut cfg = default_cfg("vp8");
    cfg.alpha = Some("keep");
    let results = check_codecs(&cfg).await;
    assert_eq!(results.len(), CODECS.len());
}

#[wasm_bindgen_test]
async fn check_codecs_with_unsupported_combo_does_not_panic() {
    let mut cfg = default_cfg("av01.0.04M.08");
    cfg.hardware_acceleration = Some("prefer-hardware");
    cfg.scalability_mode = Some("L3T1");
    cfg.alpha = Some("keep");
    let results = check_codecs(&cfg).await;
    assert_eq!(results.len(), CODECS.len());
}

#[wasm_bindgen_test]
async fn invalid_codec_string_is_not_supported() {
    let cfg = default_cfg("not-a-real-codec");
    let ok = check_one_codec("not-a-real-codec", &cfg)
        .await
        .unwrap_or(false);
    assert!(!ok);
}

#[wasm_bindgen_test]
async fn codec_support_has_codec_and_label() {
    let cfg = default_cfg("avc1.42001E");
    let results = check_codecs(&cfg).await;
    for r in &results {
        assert!(!r.codec.is_empty());
        assert!(!r.label.is_empty());
    }
}

// =============================================================================
// configure_tuned: смоук-тесты
// =============================================================================

#[wasm_bindgen_test]
fn configure_tuned_does_not_throw_on_avc1() {
    let on_output =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue, JsValue)>::new(
            |_c: JsValue, _m: JsValue| {},
        );
    let on_error =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(
            |_e: JsValue| {},
        );
    let enc = make_video_encoder(&on_output, &on_error).expect("construct");

    let cfg = default_cfg("avc1.42001E");
    assert!(configure_tuned(&enc, &cfg).is_ok());

    let _ = encoder_close(&enc);
    on_output.forget();
    on_error.forget();
}

#[wasm_bindgen_test]
fn configure_tuned_with_scalability_and_alpha() {
    let on_output =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue, JsValue)>::new(
            |_c: JsValue, _m: JsValue| {},
        );
    let on_error =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(
            |_e: JsValue| {},
        );
    let enc = make_video_encoder(&on_output, &on_error).expect("construct");

    let mut cfg = default_cfg("vp09.00.10.08");
    cfg.scalability_mode = Some("L1T2");
    cfg.alpha = Some("discard");

    let _ = configure_tuned(&enc, &cfg);
    let _ = encoder_close(&enc);
    on_output.forget();
    on_error.forget();
}

#[wasm_bindgen_test]
fn tuning_config_omits_none_fields() {
    let on_output =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue, JsValue)>::new(
            |_c: JsValue, _m: JsValue| {},
        );
    let on_error =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(
            |_e: JsValue| {},
        );
    let enc = make_video_encoder(&on_output, &on_error).expect("construct");

    let cfg = TuningConfig {
        codec: "avc1.42001E",
        width: 320,
        height: 240,
        bitrate: 1_000_000,
        framerate: 30.0,
        latency_mode: None,
        bitrate_mode: None,
        content_hint: None,
        hardware_acceleration: None,
        scalability_mode: None,
        alpha: None,
    };
    assert!(configure_tuned(&enc, &cfg).is_ok());
    let _ = encoder_close(&enc);
    on_output.forget();
    on_error.forget();
}

// =============================================================================
// Обратная совместимость с 23.3
// =============================================================================

#[wasm_bindgen_test]
fn base_configure_still_works() {
    let on_output =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue, JsValue)>::new(
            |_c: JsValue, _m: JsValue| {},
        );
    let on_error =
        wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(
            |_e: JsValue| {},
        );
    let enc = make_video_encoder(&on_output, &on_error).expect("construct");
    let result = base_configure(&enc, "avc1.42001E", 320, 240, 1_000_000, 30.0);
    assert!(result.is_ok());
    let _ = encoder_close(&enc);
    on_output.forget();
    on_error.forget();
}

#[wasm_bindgen_test]
fn chunk_helpers_are_resilient_to_non_chunks() {
    let v = JsValue::from_str("not a chunk");
    assert!(chunk_type(&v).is_empty());
    assert_eq!(chunk_byte_length(&v), 0);
}

// =============================================================================
// VideoFrame.clone() — критично для side-by-side
// =============================================================================

#[wasm_bindgen_test]
fn video_frame_clone_produces_independent_instance() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document.create_element("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().unwrap();
    canvas.set_width(16);
    canvas.set_height(16);

    let global = js_sys::global();
    let ctor = js_sys::Reflect::get(&global, &JsValue::from_str("VideoFrame"))
        .unwrap();
    let ctor: js_sys::Function = ctor.dyn_into().unwrap();

    let init = Object::new();
    set_prop(&init, "timestamp", &num(0.0)).unwrap();
    let args = js_sys::Array::new();
    args.push(&JsValue::from(&canvas));
    args.push(&init);
    let frame = js_sys::Reflect::construct(&ctor, &args).unwrap();

    let clone_fn = js_sys::Reflect::get(&frame, &JsValue::from_str("clone"))
        .unwrap()
        .dyn_into::<js_sys::Function>()
        .unwrap();
    let clone = clone_fn.call0(&frame).unwrap();

    assert!(!clone.is_null() && !clone.is_undefined());

    let _ = frame_close(&frame);
    let _ = frame_close(&clone);
}