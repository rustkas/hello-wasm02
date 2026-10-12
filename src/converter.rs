use gloo_timers::future::TimeoutFuture;
use js_sys::{Array, Uint8Array};
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, File, ReadableStreamDefaultReader};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

pub const MAGIC_PNG: [u8; 4] = [0x89, 0x50, 0x4E, 0x47];
pub const MAGIC_JPEG: [u8; 3] = [0xFF, 0xD8, 0xFF];
pub const MAGIC_MIN: usize = 4;

/// Валидация magic-байтов с человекочитаемой ошибкой.
pub fn validate_magic_bytes(data: &[u8]) -> Result<ImageFormat, String> {
    if data.len() < MAGIC_MIN {
        return Err(format!(
            "Файл слишком мал: {} байт, минимум {}",
            data.len(),
            MAGIC_MIN
        ));
    }
    if data.starts_with(&MAGIC_PNG) {
        Ok(ImageFormat::Png)
    } else if data.starts_with(&MAGIC_JPEG) {
        Ok(ImageFormat::Jpeg)
    } else {
        Err(format!(
            "Неподдерживаемый формат (magic: {:02X} {:02X} {:02X} {:02X})",
            data[0], data[1], data[2], data[3]
        ))
    }
}

/// Результат потокового чтения.
/// Early-exit: как только накопили ≥4 байта, проверяем magic — не тратим
/// трафик на 200 МБ, если это `.txt`.
pub enum ReadOutcome {
    Ok(Vec<u8>),
    Cancelled,
    InvalidFormat(String),
}

pub async fn read_blob_stream_validated(
    file: &File,
    mut on_progress: impl FnMut(f64),
    is_cancelled: impl Fn() -> bool,
) -> Result<ReadOutcome, JsValue> {
    let stream = file.stream();
    let reader = ReadableStreamDefaultReader::new(&stream)?;

    // Не резервируем по file.size(): файл может быть 8 ГБ, а RAM — нет.
    let cap = (file.size() as usize).min(16 * 1024 * 1024);
    let mut chunks: Vec<u8> = Vec::with_capacity(cap);

    let mut received: u64 = 0;
    let total = file.size() as u64;
    let mut last_yield = 0.0_f64;
    let mut validated = false;

    loop {
        if is_cancelled() {
            return Ok(ReadOutcome::Cancelled);
        }
        let result = JsFuture::from(reader.read()).await?;
        let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))?
            .as_bool()
            .unwrap_or(true);
        if done {
            break;
        }
        let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))?;
        let chunk = Uint8Array::new(&value);
        let len = chunk.length() as u64;
        chunks.extend_from_slice(&chunk.to_vec());
        received += len;

        // Early-exit magic-байтов на первом же чанке, где набралось ≥4 байта.
        if !validated && chunks.len() >= MAGIC_MIN {
            validated = true;
            if let Err(msg) = validate_magic_bytes(&chunks) {
                return Ok(ReadOutcome::InvalidFormat(msg));
            }
        }

        let now = js_sys::Date::now();
        if now - last_yield >= 16.0 {
            on_progress(received as f64 / total as f64);
            TimeoutFuture::new(0).await;
            last_yield = now;
        }
    }
    on_progress(1.0);
    Ok(ReadOutcome::Ok(chunks))
}

/// Адаптивный размер чанка: подбираем так, чтобы получить ~`target_iters`
/// итераций. Правило проекта: «параметр, а не стратегия».
pub fn adaptive_chunk(total: usize, target_iters: usize) -> usize {
    let iters = target_iters.max(1);
    (total / iters).clamp(64 * 1024, 4 * 1024 * 1024)
}

pub struct HashStats {
    pub hex: String,
    pub yields: u32,
    pub chunks: u32,
    pub chunk_size: usize,
}

pub async fn sha256_hex_with_progress(
    data: &[u8],
    mut on_progress: impl FnMut(f64),
) -> HashStats {
    let chunk_size = adaptive_chunk(data.len(), 200);
    let mut hasher = Sha256::new();
    let total = data.len().max(1);
    let mut last_yield = 0.0_f64;
    let mut yields = 0u32;
    let mut chunks = 0u32;

    for (i, chunk) in data.chunks(chunk_size).enumerate() {
        hasher.update(chunk);
        chunks += 1;
        let processed = ((i + 1) * chunk_size).min(data.len());
        let now = js_sys::Date::now();
        if now - last_yield >= 16.0 {
            on_progress(processed as f64 / total as f64);
            TimeoutFuture::new(0).await;
            last_yield = now;
            yields += 1;
        }
    }
    on_progress(1.0);
    HashStats {
        hex: hex::encode(hasher.finalize()),
        yields,
        chunks,
        chunk_size,
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

pub fn to_grayscale_png(data: &[u8], format: ImageFormat) -> Result<Vec<u8>, String> {
    use image::{GenericImageView, ImageFormat as ImgFormat};

    let img = image::load_from_memory_with_format(
        data,
        match format {
            ImageFormat::Png => ImgFormat::Png,
            ImageFormat::Jpeg => ImgFormat::Jpeg,
        },
    )
    .map_err(|e| e.to_string())?;

    let gray = img.grayscale();
    let (w, h) = gray.dimensions();
    if w != img.width() || h != img.height() {
        return Err("Размеры после конвертации изменились".into());
    }
    let mut out = Vec::new();
    gray.write_to(&mut std::io::Cursor::new(&mut out), ImgFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn bytes_to_blob(data: &[u8]) -> Result<Blob, JsValue> {
    let arr = Array::new();
    arr.push(&Uint8Array::from(data));
    Blob::new_with_u8_array_sequence(&arr)
}

/// 5.1–5.3: декодирование PNG/JPEG в сырой RGBA + размеры.
pub fn decode_to_rgba(data: &[u8], format: ImageFormat) -> Result<(Vec<u8>, u32, u32), String> {
    use image::ImageFormat as ImgFormat;
    let img = image::load_from_memory_with_format(
        data,
        match format {
            ImageFormat::Png => ImgFormat::Png,
            ImageFormat::Jpeg => ImgFormat::Jpeg,
        },
    )
    .map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    Ok((img.to_rgba8().into_raw(), w, h))
}

/// 5.1–5.3: собрать PNG из RGBA.
pub fn encode_rgba_png(rgba: &[u8], w: u32, h: u32) -> Result<Vec<u8>, String> {
    let img: image::RgbaImage = image::ImageBuffer::from_raw(w, h, rgba.to_vec())
        .ok_or_else(|| "Неверный размер RGBA-буфера".to_string())?;
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// 2.3: клиентский идентификатор для контракта uploads_root()/{upload_id}/{filename}.
/// Используем crypto.randomUUID(), если есть, иначе fallback.
pub fn generate_upload_id() -> String {
    if let Some(w) = web_sys::window() {
        if let Ok(crypto) = js_sys::Reflect::get(&w, &JsValue::from_str("crypto")) {
            if let Ok(uuid_fn) =
                js_sys::Reflect::get(&crypto, &JsValue::from_str("randomUUID"))
            {
                if !uuid_fn.is_undefined() {
                    if let Ok(f) = uuid_fn.dyn_into::<js_sys::Function>() {
                        if let Ok(v) = f.call0(&crypto) {
                            if let Some(s) = v.as_string() {
                                return s;
                            }
                        }
                    }
                }
            }
        }
    }
    let ts = js_sys::Date::now() as u64;
    return format!("upload-{:x}", ts)
}