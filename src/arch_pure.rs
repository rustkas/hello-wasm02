//! 50. Архитектура: чистые вычисления, которые должны жить в Rust.
//!
//! ## Пять задач-представителей
//!
//! - **Алгоритмы** — Fibonacci (рекурсия).
//! - **Данные** — сортировка `u32`.
//! - **Бинарная обработка** — CRC-32.
//! - **Строки** — подсчёт code points UTF-8.
//! - **Протоколы** — парсинг кадров `55 AA len payload CRC16`.
//!
//! Эти задачи объединяет одно: на входе — байты / числа / строки,
//! на выходе — тоже простые данные. Никакого DOM, никаких JsValue.
//! Именно поэтому их стоит держать в Rust — они не пересекают
//! границу на каждый элемент.

// =============================================================================
// 1. Алгоритмы: Fibonacci
// =============================================================================

/// Рекурсивный Fibonacci. Медленный, но показательный.
pub fn fib_recursive(n: u32) -> u64 {
    if n < 2 {
        return n as u64;
    }
    return fib_recursive(n - 1) + fib_recursive(n - 2);
}

/// Итеративный Fibonacci — для проверки равенства.
pub fn fib_iterative(n: u32) -> u64 {
    let mut a: u64 = 0;
    let mut b: u64 = 1;
    for _ in 0..n {
        let next = a + b;
        a = b;
        b = next;
    }
    return a;
}

// =============================================================================
// 2. Данные: сортировка
// =============================================================================

/// In-place сортировка `u32` (pattern-defeating quicksort).
pub fn sort_u32(data: &mut [u32]) {
    data.sort_unstable();
}

// =============================================================================
// 3. Бинарная обработка: CRC-32 (IEEE 802.3, полином 0xEDB88320)
// =============================================================================

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    return !crc;
}

// =============================================================================
// 4. Строки: подсчёт code points
// =============================================================================

/// Число Unicode code points (не байт и не UTF-16 units).
pub fn count_code_points(text: &str) -> usize {
    return text.chars().count();
}

// =============================================================================
// 5. Протоколы: разбор кадра `55 AA len payload CRC16`
// =============================================================================

pub const FRAME_HEADER: [u8; 2] = [0x55, 0xAA];

pub struct Frame {
    pub payload: Vec<u8>,
    pub crc_ok: bool,
}

/// CRC-16/CCITT-FALSE, полином 0x1021, init 0xFFFF.
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    return crc;
}

/// Собирает кадр: `55 AA len payload crc_lo crc_hi`.
pub fn build_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 5);
    frame.push(FRAME_HEADER[0]);
    frame.push(FRAME_HEADER[1]);
    frame.push(payload.len() as u8);
    frame.extend_from_slice(payload);
    let crc = crc16_ccitt(payload);
    frame.push((crc & 0xFF) as u8);
    frame.push((crc >> 8) as u8);
    return frame;
}

/// Разбирает поток кадров. Останавливается на первом невалидном
/// (не хватает байт / не тот заголовок). Совпадает по семантике
/// с JS-версией в `arch_js_bench.rs`.
pub fn parse_frames(data: &[u8]) -> Vec<Frame> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 5 <= data.len() {
        if data[i] != FRAME_HEADER[0] || data[i + 1] != FRAME_HEADER[1] {
            break;
        }
        let len = data[i + 2] as usize;
        if i + 5 + len > data.len() {
            break;
        }
        let payload = data[i + 3..i + 3 + len].to_vec();
        let crc_expected = crc16_ccitt(&payload);
        let crc_actual =
            (data[i + 3 + len] as u16) | ((data[i + 4 + len] as u16) << 8);
        out.push(Frame {
            payload,
            crc_ok: crc_expected == crc_actual,
        });
        i += 5 + len;
    }
    return out;
}

// =============================================================================
// Вспомогательное: детерминированный xorshift для генерации данных
// =============================================================================

pub fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    return x;
}

/// N псевдослучайных u32 (детерминированно, от seed).
pub fn gen_u32_array(n: usize, seed: u32) -> Vec<u32> {
    let mut state = seed;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(xorshift32(&mut state));
    }
    return out;
}

/// N байт (детерминированно, от seed).
pub fn gen_bytes(n: usize, seed: u32) -> Vec<u8> {
    let mut state = seed;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push((xorshift32(&mut state) & 0xFF) as u8);
    }
    return out;
}

/// Строка из `count` code points, каждый — либо ASCII, либо кириллица,
/// либо emoji. Детерминированно.
pub fn gen_text(count: usize, seed: u32) -> String {
    let mut state = seed;
    let mut s = String::new();
    let alphabet: &[char] = &[
        'A', 'b', 'я', 'Ж', '☃', '∑', '中', '🦀',
    ];
    for _ in 0..count {
        let idx = (xorshift32(&mut state) as usize) % alphabet.len();
        s.push(alphabet[idx]);
    }
    return s;
}

/// Поток кадров: `frame_count` кадров, у каждого payload длины `payload_size`.
pub fn simulate_stream(frame_count: usize, payload_size: usize, seed: u32) -> Vec<u8> {
    let mut state = seed;
    let mut out = Vec::with_capacity(frame_count * (payload_size + 5));
    for _ in 0..frame_count {
        let mut payload = Vec::with_capacity(payload_size);
        for _ in 0..payload_size {
            payload.push((xorshift32(&mut state) & 0xFF) as u8);
        }
        out.extend_from_slice(&build_frame(&payload));
    }
    return out;
}

/// Только счётчик валидных кадров — без копирования payload.
/// Семантика совпадает с JS-версией
/// в `arch_js_bench::compile_parse_frames`. Именно это используется
/// в бенчмарке — обе стороны делают одну и ту же работу (считают
/// CRC-16 и число кадров).
pub fn parse_frames_count(data: &[u8]) -> u32 {
    let mut count: u32 = 0;
    let mut i = 0;
    while i + 5 <= data.len() {
        if data[i] != FRAME_HEADER[0] || data[i + 1] != FRAME_HEADER[1] {
            break;
        }
        let len = data[i + 2] as usize;
        if i + 5 + len > data.len() {
            break;
        }
        let crc_expected = crc16_ccitt(&data[i + 3..i + 3 + len]);
        let crc_actual =
            (data[i + 3 + len] as u16) | ((data[i + 4 + len] as u16) << 8);
        if crc_expected == crc_actual {
            count += 1;
        }
        i += 5 + len;
    }
    return count;
}