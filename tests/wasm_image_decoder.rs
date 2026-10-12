#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

use hello_wasm02::image_decoder::{
    close_image_decoder, decode_frame, first_track,
    frame_display_height, frame_display_width, frame_close,
    is_image_decoder_supported, is_type_supported,
    make_image_decoder, track_animated, track_frame_count,
    track_repetition_count, track_selected,
    tracks_length, wait_tracks_ready,
};

wasm_bindgen_test_configure!(run_in_browser);

// =============================================================================
// Минимальные PNG-байты (1×1, красный пиксель)
// =============================================================================

const PNG_1X1_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

fn decode_base64(s: &str) -> Vec<u8> {
    // base64-декодер без внешних зависимостей — маленький, но
    // достаточный для теста. 1×1 PNG = 68 байт.
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    engine.decode(s).unwrap_or_default()
}

// =============================================================================
// Feature detection
// =============================================================================

#[wasm_bindgen_test]
fn image_decoder_is_supported() {
    assert!(is_image_decoder_supported(), "ImageDecoder must be available");
}

#[wasm_bindgen_test]
async fn png_is_supported() {
    let ok = is_type_supported("image/png").await.unwrap_or(false);
    assert!(ok, "image/png must be supported");
}

#[wasm_bindgen_test]
async fn gif_is_supported() {
    let ok = is_type_supported("image/gif").await.unwrap_or(false);
    assert!(ok, "image/gif must be supported");
}

#[wasm_bindgen_test]
async fn bogus_mime_not_supported() {
    let ok = is_type_supported("image/not-a-real-format")
        .await
        .unwrap_or(false);
    assert!(!ok);
}

// =============================================================================
// Конструктор и метаданные
// =============================================================================

#[wasm_bindgen_test]
async fn decode_png_frame_zero() {
    let bytes = decode_base64(PNG_1X1_BASE64);
    assert!(!bytes.is_empty(), "PNG decode must succeed");

    let decoder = make_image_decoder(&bytes, "image/png").expect("construct");
    wait_tracks_ready(&decoder).await.expect("tracks ready");

    assert_eq!(tracks_length(&decoder), 1);

    let track = first_track(&decoder).expect("track 0");
    assert!(!track_animated(&track), "PNG не анимирован");
    assert_eq!(track_frame_count(&track), 1);
    assert!(track_selected(&track));

    let result = decode_frame(&decoder, 0, true).await.expect("decode");
    assert!(result.complete);
    assert_eq!(frame_display_width(&result.frame), 1);
    assert_eq!(frame_display_height(&result.frame), 1);

    let _ = frame_close(&result.frame);
    let _ = close_image_decoder(&decoder);
}

#[wasm_bindgen_test]
async fn decode_invalid_bytes_fails() {
    let bogus = vec![0u8; 64]; // не PNG
    let result = make_image_decoder(&bogus, "image/png");
    // Конструктор сам может не бросить — ошибка придёт при tracks.ready.
    if let Ok(decoder) = result {
        let ready = wait_tracks_ready(&decoder).await;
        assert!(ready.is_err(), "невалидный PNG должен упасть на tracks.ready");
        let _ = close_image_decoder(&decoder);
    }
}

// =============================================================================
// Close: после закрытия decode бросает
// =============================================================================

#[wasm_bindgen_test]
async fn decode_after_close_throws() {
    let bytes = decode_base64(PNG_1X1_BASE64);
    let decoder = make_image_decoder(&bytes, "image/png").expect("construct");
    wait_tracks_ready(&decoder).await.expect("tracks ready");

    close_image_decoder(&decoder).expect("close");
    // Повторный decode должен провалиться.
    let result = decode_frame(&decoder, 0, true).await;
    assert!(result.is_err(), "decode после close должен бросить");
}

#[wasm_bindgen_test]
async fn close_is_idempotent() {
    let bytes = decode_base64(PNG_1X1_BASE64);
    let decoder = make_image_decoder(&bytes, "image/png").expect("construct");
    wait_tracks_ready(&decoder).await.expect("tracks ready");
    assert!(close_image_decoder(&decoder).is_ok());
    // Второй close не должен паниковать.
    let _ = close_image_decoder(&decoder);
}

// =============================================================================
// Frame duration для статики
// =============================================================================

#[wasm_bindgen_test]
async fn png_frame_duration_is_zero() {
    let bytes = decode_base64(PNG_1X1_BASE64);
    let decoder = make_image_decoder(&bytes, "image/png").expect("construct");
    wait_tracks_ready(&decoder).await.expect("tracks ready");
    let result = decode_frame(&decoder, 0, true).await.expect("decode");
    let duration = hello_wasm02::image_decoder::frame_duration_us(&result.frame);
    // PNG не имеет длительности кадра.
    assert_eq!(duration, 0.0);
    let _ = frame_close(&result.frame);
    let _ = close_image_decoder(&decoder);
}

// =============================================================================
// repetitionCount для статики
// =============================================================================

#[wasm_bindgen_test]
async fn png_repetition_count_is_zero() {
    let bytes = decode_base64(PNG_1X1_BASE64);
    let decoder = make_image_decoder(&bytes, "image/png").expect("construct");
    wait_tracks_ready(&decoder).await.expect("tracks ready");
    let track = first_track(&decoder).expect("track 0");
    assert_eq!(track_repetition_count(&track), 0.0);
    let _ = close_image_decoder(&decoder);
}

// =============================================================================
// Ошибки на невалидном входе
// =============================================================================

#[wasm_bindgen_test]
async fn helpers_handle_non_decoder_input() {
    let v = JsValue::from_str("not a decoder");
    assert_eq!(tracks_length(&v), 0);
    assert!(first_track(&v).is_err());
}