use wasm_bindgen_test::*;
use hello_wasm02::validation::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_detect_png() {
    let bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
    assert_eq!(detect_format(&bytes), Format::Png);
}

#[wasm_bindgen_test]
fn test_detect_jpeg() {
    let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
    assert_eq!(detect_format(&bytes), Format::Jpeg);
}

#[wasm_bindgen_test]
fn test_detect_gif() {
    let bytes = *b"GIF89a\x01\x00\x01\x00";
    assert_eq!(detect_format(&bytes), Format::Gif);
}

#[wasm_bindgen_test]
fn test_detect_webp() {
    let bytes = *b"RIFF\x24\x00\x00\x00WEBPVP8 ";
    assert_eq!(detect_format(&bytes), Format::Webp);
}

#[wasm_bindgen_test]
fn test_detect_wav() {
    let bytes = *b"RIFF\x24\x00\x00\x00WAVEfmt ";
    assert_eq!(detect_format(&bytes), Format::Wav);
}

#[wasm_bindgen_test]
fn test_detect_pdf() {
    let bytes = *b"%PDF-1.7\n";
    assert_eq!(detect_format(&bytes), Format::Pdf);
}

#[wasm_bindgen_test]
fn test_detect_zip() {
    let bytes = [0x50, 0x4B, 0x03, 0x04, 0x14, 0x00];
    assert_eq!(detect_format(&bytes), Format::Zip);
}

#[wasm_bindgen_test]
fn test_detect_gzip() {
    let bytes = [0x1F, 0x8B, 0x08, 0x00];
    assert_eq!(detect_format(&bytes), Format::Gzip);
}

#[wasm_bindgen_test]
fn test_detect_wasm() {
    let bytes = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    assert_eq!(detect_format(&bytes), Format::Wasm);
}

#[wasm_bindgen_test]
fn test_detect_mp4() {
    let bytes = [
        0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p',
        b'i', b's', b'o', b'm',
    ];
    assert_eq!(detect_format(&bytes), Format::Mp4);
}

#[wasm_bindgen_test]
fn test_detect_webm() {
    let bytes = [0x1A, 0x45, 0xDF, 0xA3, 0x01, 0x00, 0x00, 0x00];
    assert_eq!(detect_format(&bytes), Format::Webm);
}

#[wasm_bindgen_test]
fn test_detect_text() {
    let bytes = b"Hello, world!\nThis is a test.";
    assert_eq!(detect_format(bytes), Format::Text);
}

#[wasm_bindgen_test]
fn test_detect_unknown_binary() {
    // Случайные байты без сигнатур — не текст (менее 90 % печатаемых).
    let bytes = [0xF7, 0x14, 0xA3, 0x88, 0x00, 0xCC, 0xDD, 0xEE];
    assert_eq!(detect_format(&bytes), Format::Unknown);
}

#[wasm_bindgen_test]
fn test_detect_empty() {
    let bytes: [u8; 0] = [];
    assert_eq!(detect_format(&bytes), Format::Unknown);
}

#[wasm_bindgen_test]
fn test_format_name_and_mime() {
    assert_eq!(Format::Png.name(), "PNG");
    assert_eq!(Format::Png.mime_type(), "image/png");
    assert!(Format::Png.is_image());
    assert!(!Format::Pdf.is_image());
}

#[wasm_bindgen_test]
fn test_check_size_within() {
    let check = check_size(500.0, 1000.0);
    assert!(check.ok);
    assert_eq!(check.actual, 500.0);
    assert_eq!(check.limit, 1000.0);
}

#[wasm_bindgen_test]
fn test_check_size_over() {
    let check = check_size(2000.0, 1000.0);
    assert!(!check.ok);
}

#[wasm_bindgen_test]
fn test_check_size_no_limit() {
    let check = check_size(999_999_999.0, 0.0);
    assert!(check.ok);
    assert_eq!(check.limit, 0.0);
}

#[wasm_bindgen_test]
async fn test_read_header_png() {
    // Файл с PNG-сигнатурой в начале.
    let bytes = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x01, 0x02, 0x03, 0x04,
    ];
    let uint8 = js_sys::Uint8Array::from(&bytes[..]);
    let parts = js_sys::Array::of1(&uint8.buffer());
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
        .expect("Blob::new_with_u8_array_sequence");

    let header = read_header(&blob, 8).await.expect("read_header");
    assert_eq!(header.len(), 8);
    assert_eq!(detect_format(&header), Format::Png);
}

#[wasm_bindgen_test]
async fn test_read_header_truncates() {
    // Если файл короче запрошенного N, вернётся всё содержимое.
    let bytes = vec![0x01, 0x02, 0x03];
    let uint8 = js_sys::Uint8Array::from(&bytes[..]);
    let parts = js_sys::Array::of1(&uint8.buffer());
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
        .expect("Blob::new_with_u8_array_sequence");

    let header = read_header(&blob, 32).await.expect("read_header");
    assert_eq!(header.len(), 3);
    assert_eq!(header, vec![0x01, 0x02, 0x03]);
}

#[wasm_bindgen_test]
async fn test_spoofed_txt_with_png_content() {
    // Файл с именем "fake.txt", но содержимым PNG. Имя не влияет
    // на sniffing — детектируется реальный формат.
    let bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
    let uint8 = js_sys::Uint8Array::from(&bytes[..]);
    let parts = js_sys::Array::of1(&uint8.buffer());
    let file = web_sys::File::new_with_u8_array_sequence(&parts, "fake.txt")
        .expect("File::new_with_u8_array_sequence");

    let header = read_header(&file, 8).await.expect("read_header");
    assert_eq!(detect_format(&header), Format::Png);
    assert_eq!(file.name(), "fake.txt");
}