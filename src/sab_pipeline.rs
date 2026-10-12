//! Практический проект Главы 29: обработка аудио двумя стратегиями.
//!
//! **Главный урок: «zero-copy» через typed array — иллюзия.**
//!
//!   1. `process_audio_bulk` — принимает `Vec<f32>`, обрабатывает нативно
//!      в Rust, возвращает новый `Vec<f32>`. Две memcpy (JS → WASM и
//!      WASM → JS), но обработка целиком в Rust без пересечений границы.
//!
//!   2. `process_audio_elementwise` — принимает `&Float32Array`, обходит
//!      через `get_index`/`set_index`. Ноль memcpy, но каждый доступ — 
//!      пересечение границы JS↔WASM. Для N сэмплов это 2N границ.
//!
//! Бенчмарк показывает: bulk быстрее elementwise в 3–5 раз.
//!
//! **Настоящий zero-copy** — когда Rust владеет буфером, а JS читает его
//! через `Uint8Array::view()`. Тип `js_sys::Float32Array` — это JS-owned
//! буфер, работа с ним из Rust идёт через JS-вызовы.

use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioEffect {
    Invert,
    Attenuate(f32),
    Offset(f32),
}

/// Чистая логика: применяет эффект к срезу, возвращает новый `Vec<f32>`.
/// Тестируется на host без браузера.
pub fn apply_effect_copy(data: &[f32], effect: AudioEffect) -> Vec<f32> {
    data.iter()
        .map(|&x| match effect {
            AudioEffect::Invert => -x,
            AudioEffect::Attenuate(f) => x * f,
            AudioEffect::Offset(o) => x + o,
        })
        .collect()
}

/// Применяет эффект поэлементно через `Float32Array`.
/// **Каждый `get_index`/`set_index` — JS-вызов.** Медленно для больших буферов.
pub fn apply_effect_elementwise(view: &js_sys::Float32Array, effect: AudioEffect) {
    let len = view.length();
    for i in 0..len {
        let x = view.get_index(i);
        let y = match effect {
            AudioEffect::Invert => -x,
            AudioEffect::Attenuate(f) => x * f,
            AudioEffect::Offset(o) => x + o,
        };
        view.set_index(i, y);
    }
}

/// **Рекомендуемая стратегия.** Bulk через `Vec<f32>`:
/// одна memcpy на входе, нативная обработка, одна memcpy на выходе.
#[wasm_bindgen]
pub fn process_audio_bulk(data: Vec<f32>, effect_code: u32) -> Vec<f32> {
    let effect = match effect_code {
        0 => AudioEffect::Invert,
        1 => AudioEffect::Attenuate(0.5),
        2 => AudioEffect::Offset(0.1),
        _ => AudioEffect::Invert,
    };
    apply_effect_copy(&data, effect)
}

/// **Анти-пример.** Поэлементно через JS typed array:
/// 2N пересечений границы JS↔WASM. Медленнее bulk-варианта.
#[wasm_bindgen]
pub fn process_audio_elementwise(view: &js_sys::Float32Array, effect_code: u32) {
    let effect = match effect_code {
        0 => AudioEffect::Invert,
        1 => AudioEffect::Attenuate(0.5),
        2 => AudioEffect::Offset(0.1),
        _ => AudioEffect::Invert,
    };
    apply_effect_elementwise(view, effect);
}

/// Возвращает `Float32Array` с синусоидой. Rust владеет данными в момент
/// создания; JS получает владение после возврата.
#[wasm_bindgen]
pub fn make_sine_view(samples: usize, frequency: f32) -> js_sys::Float32Array {
    let mut data = Vec::with_capacity(samples);
    for i in 0..samples {
        let t = i as f32 / samples as f32;
        data.push((t * frequency * std::f32::consts::TAU).sin());
    }
    js_sys::Float32Array::from(&data[..])
}

/// Замер: bulk-обработка, N итераций. Возвращает миллисекунды.
#[wasm_bindgen]
pub fn bench_bulk(data: Vec<f32>, iterations: u32) -> f64 {
    let start = js_sys::Date::now();
    for _ in 0..iterations {
        let _ = process_audio_bulk(data.clone(), 0);
    }
    js_sys::Date::now() - start
}

/// Замер: поэлементная обработка через JS, N итераций. Возвращает миллисекунды.
#[wasm_bindgen]
pub fn bench_elementwise(view: &js_sys::Float32Array, iterations: u32) -> f64 {
    let start = js_sys::Date::now();
    for _ in 0..iterations {
        process_audio_elementwise(view, 0);
    }
    js_sys::Date::now() - start
}