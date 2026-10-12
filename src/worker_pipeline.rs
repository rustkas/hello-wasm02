//! Практический проект Главы 28: pipeline обработки изображений.
//!
//! Фильтры выполняются в Worker'е. Главный поток только отправляет
//! данные и рендерит результат.
//!
//! `grayscale` и `box_blur` экспортированы в JS через `#[wasm_bindgen]`,
//! чтобы worker.js мог их вызвать как `wasm_bindgen.grayscale(bytes)`.

use wasm_bindgen::prelude::*;

/// Фильтр обработки изображения.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Filter {
    Grayscale,
    BoxBlur,
}

/// Применяет grayscale к RGBA-пикселям.
#[wasm_bindgen]
pub fn grayscale(pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len());
    let mut i = 0;
    while i + 3 < pixels.len() {
        let r = pixels[i] as u32;
        let g = pixels[i + 1] as u32;
        let b = pixels[i + 2] as u32;
        let a = pixels[i + 3];
        let gray = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
        out.push(gray);
        out.push(gray);
        out.push(gray);
        out.push(a);
        i += 4;
    }
    out
}

/// Применяет box blur радиуса `radius` к RGBA-пикселям.
#[wasm_bindgen]
pub fn box_blur(pixels: &[u8], width: usize, height: usize, radius: usize) -> Vec<u8> {
    let mut out = pixels.to_vec();
    if radius == 0 || width == 0 || height == 0 {
        return out;
    }
    let r = radius as isize;
    for y in 0..height {
        for x in 0..width {
            let mut sum_r = 0u32;
            let mut sum_g = 0u32;
            let mut sum_b = 0u32;
            let mut sum_a = 0u32;
            let mut count = 0u32;
            for dy in -r..=r {
                for dx in -r..=r {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                        continue;
                    }
                    let idx = (ny as usize * width + nx as usize) * 4;
                    if idx + 3 >= pixels.len() {
                        continue;
                    }
                    sum_r += pixels[idx] as u32;
                    sum_g += pixels[idx + 1] as u32;
                    sum_b += pixels[idx + 2] as u32;
                    sum_a += pixels[idx + 3] as u32;
                    count += 1;
                }
            }
            if count == 0 {
                continue;
            }
            let idx = (y * width + x) * 4;
            out[idx] = (sum_r / count) as u8;
            out[idx + 1] = (sum_g / count) as u8;
            out[idx + 2] = (sum_b / count) as u8;
            out[idx + 3] = (sum_a / count) as u8;
        }
    }
    out
}

/// Описание задачи для Worker'а.
#[derive(Debug, Clone)]
pub struct PipelineTask {
    pub filter: Filter,
    pub width: u32,
    pub height: u32,
}

/// Создаёт JS-объект задачи для Worker'а.
pub fn task_to_js(task: &PipelineTask, pixels: &[u8]) -> Result<JsValue, JsValue> {
    let obj = js_sys::Object::new();
    let filter_str = match task.filter {
        Filter::Grayscale => "grayscale",
        Filter::BoxBlur => "box_blur",
    };
    js_sys::Reflect::set(&obj, &JsValue::from_str("filter"), &JsValue::from_str(filter_str))
        .map_err(|e| JsValue::from_str(&format!("set filter: {e:?}")))?;
    js_sys::Reflect::set(&obj, &JsValue::from_str("width"), &JsValue::from_f64(task.width as f64))
        .map_err(|e| JsValue::from_str(&format!("set width: {e:?}")))?;
    js_sys::Reflect::set(&obj, &JsValue::from_str("height"), &JsValue::from_f64(task.height as f64))
        .map_err(|e| JsValue::from_str(&format!("set height: {e:?}")))?;
    let uint8 = js_sys::Uint8Array::from(pixels);
    js_sys::Reflect::set(&obj, &JsValue::from_str("pixels"), &uint8)
        .map_err(|e| JsValue::from_str(&format!("set pixels: {e:?}")))?;
    Ok(obj.into())
}