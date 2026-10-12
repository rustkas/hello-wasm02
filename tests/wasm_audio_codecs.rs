#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

use hello_wasm02::audio_codecs::{
    audio_data_close, audio_data_copy_channel, audio_data_format,
    audio_data_number_of_channels, audio_data_number_of_frames,
    audio_data_sample_rate, audio_data_timestamp,
    bytes_human, chunk_byte_length, chunk_timestamp, chunk_type,
    is_audio_config_supported, is_audio_data_supported,
    is_audio_decoder_supported, is_audio_encoder_supported,
    make_audio_data, make_encoded_audio_chunk,
};

wasm_bindgen_test_configure!(run_in_browser);

// =============================================================================
// Feature detection
// =============================================================================

#[wasm_bindgen_test]
fn audio_encoder_is_supported() {
    assert!(is_audio_encoder_supported(), "AudioEncoder must be available");
}

#[wasm_bindgen_test]
fn audio_decoder_is_supported() {
    assert!(is_audio_decoder_supported(), "AudioDecoder must be available");
}

#[wasm_bindgen_test]
fn audio_data_is_supported() {
    assert!(is_audio_data_supported(), "AudioData must be available");
}

// =============================================================================
// AudioData: конструктор и метаданные
// =============================================================================

#[wasm_bindgen_test]
fn audio_data_creation_from_float32_array() {
    // 1 канал, 100 фреймов — моно-сигнал нулей.
    let samples = vec![0.0f32; 100];
    let data = make_audio_data(&samples, 48000, 1, 0.0)
        .expect("AudioData creation");
    assert_eq!(audio_data_number_of_frames(&data), 100);
    assert_eq!(audio_data_number_of_channels(&data), 1);
    assert_eq!(audio_data_sample_rate(&data), 48000);
    assert_eq!(audio_data_format(&data), "f32-planar");
    assert_eq!(audio_data_timestamp(&data), 0.0);
    let _ = audio_data_close(&data);
}

#[wasm_bindgen_test]
fn audio_data_stereo_planar_layout() {
    // 2 канала × 50 фреймов = 100 сэмплов.
    // Канал 0 — нули, канал 1 — единицы. Planar: сначала канал 0, потом канал 1.
    let mut samples = vec![0.0f32; 50];
    samples.extend(vec![1.0f32; 50]);
    let data = make_audio_data(&samples, 48000, 2, 0.0)
        .expect("stereo AudioData");
    assert_eq!(audio_data_number_of_frames(&data), 50);
    assert_eq!(audio_data_number_of_channels(&data), 2);
    let _ = audio_data_close(&data);
}

// =============================================================================
// copyTo: извлечение каналов
// =============================================================================

#[wasm_bindgen_test]
fn copy_to_returns_correct_length() {
    let samples = vec![0.5f32; 480];
    let data = make_audio_data(&samples, 48000, 1, 0.0)
        .expect("AudioData");
    let channel = audio_data_copy_channel(&data, 0);
    assert_eq!(channel.len(), 480);
    let _ = audio_data_close(&data);
}

#[wasm_bindgen_test]
fn copy_to_preserves_values() {
    let samples: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
    let data = make_audio_data(&samples, 48000, 1, 0.0)
        .expect("AudioData");
    let channel = audio_data_copy_channel(&data, 0);
    assert_eq!(channel.len(), samples.len());
    for (a, b) in channel.iter().zip(samples.iter()) {
        assert!((a - b).abs() < 1e-6, "значения должны совпадать");
    }
    let _ = audio_data_close(&data);
}

#[wasm_bindgen_test]
fn copy_to_stereo_plane_1() {
    let mut samples = vec![0.0f32; 30];
    samples.extend(vec![0.7f32; 30]);
    let data = make_audio_data(&samples, 48000, 2, 0.0)
        .expect("stereo AudioData");
    let ch0 = audio_data_copy_channel(&data, 0);
    let ch1 = audio_data_copy_channel(&data, 1);
    assert_eq!(ch0.len(), 30);
    assert_eq!(ch1.len(), 30);
    assert!(ch0.iter().all(|&v| v.abs() < 1e-6));
    assert!(ch1.iter().all(|&v| (v - 0.7).abs() < 1e-6));
    let _ = audio_data_close(&data);
}

#[wasm_bindgen_test]
fn close_is_idempotent() {
    let samples = vec![0.0f32; 10];
    let data = make_audio_data(&samples, 48000, 1, 0.0).expect("AudioData");
    assert!(audio_data_close(&data).is_ok());
    // Повторный close может дать InvalidStateError — но не должен паниковать.
    let _ = audio_data_close(&data);
}

// =============================================================================
// isConfigSupported для Opus
// =============================================================================

#[wasm_bindgen_test]
async fn opus_48khz_mono_supported() {
    let ok = is_audio_config_supported("opus", 48000, 1, 64000)
        .await
        .unwrap_or(false);
    assert!(ok, "opus 48k mono 64kbps must be supported");
}

#[wasm_bindgen_test]
async fn opus_48khz_stereo_supported() {
    let ok = is_audio_config_supported("opus", 48000, 2, 128000)
        .await
        .unwrap_or(false);
    assert!(ok, "opus 48k stereo 128kbps must be supported");
}

#[wasm_bindgen_test]
async fn bogus_codec_not_supported() {
    let ok = is_audio_config_supported("not-a-real-codec", 48000, 1, 64000)
        .await
        .unwrap_or(false);
    assert!(!ok);
}

// =============================================================================
// EncodedAudioChunk: конструктор и чтение
// =============================================================================

#[wasm_bindgen_test]
fn make_and_read_encoded_chunk() {
    let data = vec![0u8; 128];
    let chunk = make_encoded_audio_chunk("key", 0.0, 20000.0, &data)
        .expect("EncodedAudioChunk");
    assert_eq!(chunk_type(&chunk), "key");
    assert_eq!(chunk_byte_length(&chunk), 128);
    assert_eq!(chunk_timestamp(&chunk), 0.0);
}

// =============================================================================
// bytes_human
// =============================================================================

#[wasm_bindgen_test]
fn bytes_human_small() {
    assert_eq!(bytes_human(500), "500");
}

#[wasm_bindgen_test]
fn bytes_human_kb() {
    let s = bytes_human(2048);
    assert!(s.contains("2048"));
    assert!(s.contains("2.0 KB"));
}

// =============================================================================
// Smoke: вспомогательные функции не падают на невалидном входе
// =============================================================================

#[wasm_bindgen_test]
fn helpers_handle_non_data_input() {
    let v = JsValue::from_str("not audio data");
    assert_eq!(audio_data_number_of_frames(&v), 0);
    assert_eq!(audio_data_number_of_channels(&v), 0);
    assert_eq!(audio_data_sample_rate(&v), 0);
    assert!(audio_data_format(&v).is_empty());
    assert_eq!(audio_data_timestamp(&v), 0.0);
    assert!(audio_data_copy_channel(&v, 0).is_empty());
}