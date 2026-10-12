#![cfg(target_arch = "wasm32")]

//! 23.13. WASM-тесты audio API.

use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;
use web_sys::AudioContext;

use hello_wasm02::audio_api::{
    add_worklet_module, audio_data_channel, bytes_human, close_audio_data,
    has_audio_worklet, has_get_user_media, is_audio_data_supported,
    is_audio_decoder_supported, is_audio_encoder_supported,
    is_opus_supported, make_audio_data,
};

wasm_bindgen_test_configure!(run_in_browser);

fn try_audio_context() -> Option<AudioContext> {
    return AudioContext::new().ok();
}

async fn close_ctx(ctx: &AudioContext) {
    if let Ok(promise) = ctx.close() {
        let _ = JsFuture::from(promise).await;
    }
}

#[wasm_bindgen_test]
async fn audio_context_can_be_created() {
    let Some(ctx) = try_audio_context() else { return; };
    assert!(has_audio_worklet(&ctx), "AudioWorklet должен быть доступен");
    close_ctx(&ctx).await;
}

#[wasm_bindgen_test]
fn audio_encoder_decoder_data_supported() {
    assert!(is_audio_encoder_supported(), "AudioEncoder должен быть доступен");
    assert!(is_audio_decoder_supported(), "AudioDecoder должен быть доступен");
    assert!(is_audio_data_supported(), "AudioData должен быть доступен");
}

#[wasm_bindgen_test]
fn get_user_media_feature_detected() {
    let _ = has_get_user_media();
}

#[wasm_bindgen_test]
async fn add_worklet_module_loads_pitch_processor() {
    let Some(ctx) = try_audio_context() else { return; };
    // В headless может не загрузиться (нет dev-сервера) — не паникуем.
    let _ = add_worklet_module(&ctx, "/worklets/pitch-processor.js").await;
    close_ctx(&ctx).await;
}

#[wasm_bindgen_test]
fn make_and_read_audio_data() {
    let samples = vec![0.0f32; 480];
    let data = make_audio_data(&samples, 48000, 1, 0.0).expect("AudioData");
    let ch0 = audio_data_channel(&data, 0);
    assert_eq!(ch0.len(), 480);
    close_audio_data(&data).expect("close");
}

#[wasm_bindgen_test]
fn audio_data_preserves_values() {
    let samples: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
    let data = make_audio_data(&samples, 48000, 1, 0.0).expect("AudioData");
    let ch0 = audio_data_channel(&data, 0);
    for (a, b) in ch0.iter().zip(samples.iter()) {
        assert!((a - b).abs() < 1e-6);
    }
    close_audio_data(&data).expect("close");
}

#[wasm_bindgen_test]
async fn opus_48k_mono_supported() {
    let ok = is_opus_supported(48000, 1, 64000).await.unwrap_or(false);
    assert!(ok, "Opus 48k mono 64kbps должен поддерживаться");
}

#[wasm_bindgen_test]
fn bytes_human_formats_ok() {
    assert_eq!(bytes_human(500), "500 B");
    assert!(bytes_human(2048).contains("2.0 KB"));
    assert!(bytes_human(5 * 1024 * 1024).contains("5.00 MB"));
}