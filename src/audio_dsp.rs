//! 23.13. Real-time pitch shifting DSP.
//!
//! Простой ресемплер на кольцевом буфере. Обрабатывает один блок
//! (обычно 128 сэмплов) за вызов. Работает и в WASM (внутри
//! AudioWorkletProcessor), и в host (для `cargo test`).
//!
//! ## Алгоритм
//!
//! Запись: input пишется в ring по одному сэмплу, head движется с
//! скоростью 1×. Чтение: read head движется со скоростью `ratio`.
//! Если read догоняет write — сдвигаем read назад на половину ring.
//!
//! ## Артефакты
//!
//! Наивный ресемплинг даёт характерное «chipmunk»-звучание: pitch
//! меняется, но вместе с ним меняется и скорость (то же самое, что
//! воспроизведение магнитофонной плёнки на другой скорости). При
//! скачке read head возникает лёгкий «щелчок» — это часть эффекта.
//!
//! Промышленный voice changer использует WSOLA или фазовый вокодер.
//! Здесь — простая и понятная реализация, чтобы показать принцип.

/// Размер кольцевого буфера. Достаточно для 128-сэмпловых блоков
/// при ratio в диапазоне [0.5, 2.0].
const RING_SIZE: usize = 8192;

/// Минимальная «отставание» read head от write head. Если read
/// подбирается ближе — сдвигаем его назад, чтобы не читать свежие
/// данные (иначе эффект пропадает).
const MIN_LAG: f32 = 128.0;

/// Кольцевой resampler для изменения pitch.
#[derive(Debug)]
pub struct PitchShifter {
    ring: Vec<f32>,
    write: usize,
    read_f: f32,
    ratio: f32,
}

impl PitchShifter {
    /// Создаёт новый shifter с пустым кольцевым буфером.
    pub fn new() -> Self {
        let mut ring = vec![0.0f32; RING_SIZE];
        for s in ring.iter_mut() {
            *s = 0.0;
        }
        Self {
            ring,
            write: 0,
            read_f: (RING_SIZE as f32) * 0.5,
            ratio: 1.0,
        }
    }

    /// Устанавливает коэффициент pitch. 1.0 — без изменений,
    /// 1.5 — на полторы октавы выше, 0.7 — ниже.
    /// Значение клипится в [0.5, 2.0].
    pub fn set_ratio(&mut self, r: f32) {
        self.ratio = r.clamp(0.5, 2.0);
    }

    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    /// Обрабатывает один блок. `input` и `output` — срезы одинаковой
    /// длины (обычно 128). Заполняет output, читая из ring с
    /// коэффициентом `ratio`.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let n = input.len().min(output.len());
        let r = self.ring.len();
        let rf = r as f32;

        // 1. Запись входных сэмплов в ring.
        for i in 0..n {
            self.ring[self.write] = input[i];
            self.write = (self.write + 1) % r;
        }

        // 2. Чтение с переменной скоростью.
        for i in 0..n {
            let read_mod = self.read_f.rem_euclid(rf);
            let i0 = read_mod.floor() as usize % r;
            let i1 = (i0 + 1) % r;
            let frac = read_mod - read_mod.floor();
            output[i] = self.ring[i0] * (1.0 - frac) + self.ring[i1] * frac;
            self.read_f += self.ratio;

            // Защита от «догоняния» write head.
            let write_f = self.write as f32;
            let read_wrapped = self.read_f.rem_euclid(rf);
            let lag = (write_f - read_wrapped).rem_euclid(rf);
            if lag < MIN_LAG {
                self.read_f = write_f - rf * 0.5;
            }
        }
    }

    /// Полный сброс: пустой ring, phase = середина.
    pub fn reset(&mut self) {
        for s in self.ring.iter_mut() {
            *s = 0.0;
        }
        self.write = 0;
        self.read_f = (self.ring.len() as f32) * 0.5;
    }
}

impl Default for PitchShifter {
    fn default() -> Self {
        Self::new()
    }
}

// ----- WASM-обёртки -----

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::PitchShifter;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub struct WasmPitchShifter {
        inner: PitchShifter,
    }

    #[wasm_bindgen]
    impl WasmPitchShifter {
        #[wasm_bindgen(constructor)]
        pub fn new() -> Self {
            Self { inner: PitchShifter::new() }
        }

        pub fn set_ratio(&mut self, r: f32) {
            self.inner.set_ratio(r);
        }

        pub fn ratio(&self) -> f32 {
            self.inner.ratio()
        }

        pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
            self.inner.process(input, output);
        }

        pub fn reset(&mut self) {
            self.inner.reset();
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmPitchShifter;