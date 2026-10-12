//! 23.10. Demuxing: MP4 → EncodedVideoChunk.
//!
//! Превращает таблицы `stbl` в плоский список encoded-chunks
//! (samples), готовых для `VideoDecoder.decode(chunk)`.
//!
//! ## Пять таблиц
//!
//! - `stco` — chunk offsets (абсолютные, в файле).
//! - `stsc` — samples per chunk (runs).
//! - `stsz` — размер каждого sample (или общий fixed size).
//! - `stts` — (sample_count, sample_delta) — длительности.
//! - `stss` — 1-based индексы key frames (или пусто = все key).
//!
//! ## Timescale
//!
//! Отдельно передаётся из `mdhd`. Все timestamp-ы выражаются
//! в его единицах и потом конвертируются в микросекунды.
//!
//! ## Публичный API
//!
//! - `parse_sample_table(stbl, timescale) -> SampleTable`
//! - `build_samples(&SampleTable) -> Vec<VideoSample>`
//! - `verify_offsets(&[VideoSample], mdat_start, mdat_end) -> Result<(), String>`
//! - `avc_codec_string(avcc) -> Option<String>`

use wasm_bindgen::JsValue;

use crate::container_parser::BoxInfo;
use crate::video_frame::format_str;

// =============================================================================
// Структуры
// =============================================================================

/// Все таблицы stbl, необходимые для demuxing-а.
#[derive(Debug, Clone)]
pub struct SampleTable {
    /// Timescale из `mdhd` — знаменатель timestamp-ов.
    pub timescale: u32,
    /// Chunk offsets из `stco` (абсолютные).
    pub stco: Vec<u32>,
    /// Rules из `stsc`.
    pub stsc: Vec<StscEntry>,
    /// Размеры сэмплов из `stsz` (если stsz_fixed = None).
    pub stsz: Vec<u32>,
    /// Фиксированный размер сэмплов (если stsz version 0 и sample_size != 0).
    pub stsz_fixed: Option<u32>,
    /// Длительности из `stts`.
    pub stts: Vec<SttsEntry>,
    /// Key frame indices (1-based). Пустой = все ключевые.
    pub stss: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct StscEntry {
    pub first_chunk: u32,
    pub samples_per_chunk: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct SttsEntry {
    pub sample_count: u32,
    pub sample_delta: u32,
}

/// Один encoded chunk — что подаётся в `VideoDecoder.decode`.
#[derive(Debug, Clone)]
pub struct VideoSample {
    pub index: usize,
    pub offset: u64,
    pub size: u32,
    pub timestamp_us: u64,
    pub duration_us: u32,
    pub is_key: bool,
}

impl SampleTable {
    /// Общее число сэмплов.
    pub fn total_samples(&self) -> usize {
        match self.stsz_fixed {
            Some(_) => {
                // sum of samples_per_chunk across all stsc entries.
                let mut total = 0u32;
                let chunks = self.stco.len();
                for k in 1..=chunks {
                    total += samples_in_chunk(&self.stsc, k);
                }
                total as usize
            }
            None => self.stsz.len(),
        }
    }
}

// =============================================================================
// Парсинг отдельных таблиц
// =============================================================================

fn read_u32_be(data: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

/// `stco`: [version+flags: 4][entry_count: 4][offset: 4 × N]
fn parse_stco(p: &[u8]) -> Result<Vec<u32>, String> {
    if p.len() < 8 {
        return Err(String::from("stco too short"));
    }
    let count = read_u32_be(p, 4) as usize;
    if p.len() < 8 + count * 4 {
        return Err(String::from("stco truncated"));
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(read_u32_be(p, 8 + i * 4));
    }
    Ok(out)
}

/// `stsc`: [version+flags: 4][entry_count: 4][(fc,spc,sdi): 12 × N]
fn parse_stsc(p: &[u8]) -> Result<Vec<StscEntry>, String> {
    if p.len() < 8 {
        return Err(String::from("stsc too short"));
    }
    let count = read_u32_be(p, 4) as usize;
    if p.len() < 8 + count * 12 {
        return Err(String::from("stsc truncated"));
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let base = 8 + i * 12;
        out.push(StscEntry {
            first_chunk: read_u32_be(p, base),
            samples_per_chunk: read_u32_be(p, base + 4),
        });
    }
    Ok(out)
}

/// `stsz`: [ver+flags: 4][sample_size: 4][sample_count: 4][sizes × N if sample_size == 0]
fn parse_stsz(p: &[u8]) -> Result<(Option<u32>, Vec<u32>), String> {
    if p.len() < 12 {
        return Err(String::from("stsz too short"));
    }
    let sample_size = read_u32_be(p, 4);
    let count = read_u32_be(p, 8) as usize;
    if sample_size != 0 {
        return Ok((Some(sample_size), Vec::new()));
    }
    if p.len() < 12 + count * 4 {
        return Err(String::from("stsz truncated"));
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(read_u32_be(p, 12 + i * 4));
    }
    Ok((None, out))
}

/// `stts`: [version+flags: 4][entry_count: 4][(count,delta): 8 × N]
fn parse_stts(p: &[u8]) -> Result<Vec<SttsEntry>, String> {
    if p.len() < 8 {
        return Err(String::from("stts too short"));
    }
    let count = read_u32_be(p, 4) as usize;
    if p.len() < 8 + count * 8 {
        return Err(String::from("stts truncated"));
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let base = 8 + i * 8;
        out.push(SttsEntry {
            sample_count: read_u32_be(p, base),
            sample_delta: read_u32_be(p, base + 4),
        });
    }
    Ok(out)
}

/// `stss`: [version+flags: 4][entry_count: 4][sample_number: 4 × N]
fn parse_stss(p: &[u8]) -> Result<Vec<u32>, String> {
    if p.len() < 8 {
        return Err(String::from("stss too short"));
    }
    let count = read_u32_be(p, 4) as usize;
    if p.len() < 8 + count * 4 {
        return Err(String::from("stss truncated"));
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(read_u32_be(p, 8 + i * 4));
    }
    Ok(out)
}

// =============================================================================
// Публичные функции
// =============================================================================

/// Извлекает все таблицы из `stbl`.
///
/// `timescale` берётся из `mdhd` — передаётся снаружи, поскольку
/// `mdhd` лежит в `mdia`, а не внутри `stbl`.
pub fn parse_sample_table(stbl: &BoxInfo, timescale: u32) -> Result<SampleTable, String> {
    let stco = stbl.find("stco")
        .ok_or_else(|| String::from("no stco"))?;
    let stsc = stbl.find("stsc")
        .ok_or_else(|| String::from("no stsc"))?;
    let stsz = stbl.find("stsz")
        .ok_or_else(|| String::from("no stsz"))?;
    let stts = stbl.find("stts")
        .ok_or_else(|| String::from("no stts"))?;
    let stss = stbl.find("stss");

    let (stsz_fixed, stsz_vec) = parse_stsz(&stsz.payload)?;

    Ok(SampleTable {
        timescale,
        stco: parse_stco(&stco.payload)?,
        stsc: parse_stsc(&stsc.payload)?,
        stsz: stsz_vec,
        stsz_fixed,
        stts: parse_stts(&stts.payload)?,
        stss: match stss {
            Some(b) => parse_stss(&b.payload)?,
            None => Vec::new(),
        },
    })
}

/// Сколько сэмплов в chunk-е `chunk_1based`.
fn samples_in_chunk(stsc: &[StscEntry], chunk_1based: usize) -> u32 {
    for i in (0..stsc.len()).rev() {
        if stsc[i].first_chunk as usize <= chunk_1based {
            return stsc[i].samples_per_chunk;
        }
    }
    0
}

/// Строит список `VideoSample` — по одному на encoded chunk.
pub fn build_samples(t: &SampleTable) -> Result<Vec<VideoSample>, String> {
    let num_chunks = t.stco.len();
    let total = t.total_samples();
    let mut samples = Vec::with_capacity(total);

    // Проходим по chunk-ам, внутри — по сэмплам.
    let mut sample_index: usize = 0;
    for chunk_1based in 1..=num_chunks {
        if sample_index >= total {
            break;
        }
        let chunk_offset = t.stco[chunk_1based - 1] as u64;
        let n = samples_in_chunk(&t.stsc, chunk_1based) as usize;
        let mut cur_off = chunk_offset;

        for _ in 0..n {
            if sample_index >= total {
                break;
            }
            let size = match t.stsz_fixed {
                Some(s) => s,
                None => t.stsz[sample_index],
            };
            samples.push(VideoSample {
                index: sample_index,
                offset: cur_off,
                size,
                // timestamp и duration заполним на следующем проходе.
                timestamp_us: 0,
                duration_us: 0,
                is_key: false,
            });
            cur_off += size as u64;
            sample_index += 1;
        }
    }

    if samples.len() != total {
        return Err(format_str(format_args!(
            "построено {} сэмплов, ожидалось {}",
            samples.len(), total,
        )));
    }

    // Timestamp-ы: обходим stts, накапливая media time.
    // timestamp_us = media_time * 1_000_000 / timescale.
    let ts = t.timescale.max(1) as u64;
    let mut cum_time: u64 = 0;
    let mut idx: usize = 0;
    'outer: for entry in &t.stts {
        let delta = entry.sample_delta as u64;
        for _ in 0..entry.sample_count {
            if idx >= samples.len() {
                break 'outer;
            }
            let ts_us = cum_time * 1_000_000 / ts;
            let dur_us = (delta * 1_000_000 / ts) as u32;
            samples[idx].timestamp_us = ts_us;
            samples[idx].duration_us = dur_us;
            cum_time += delta;
            idx += 1;
        }
    }

    // Key frames: если stss пуст — все ключевые.
    if t.stss.is_empty() {
        for s in &mut samples {
            s.is_key = true;
        }
    } else {
        for s in &mut samples {
            // stss содержит 1-based индексы.
            s.is_key = t.stss.binary_search(&(s.index as u32 + 1)).is_ok();
        }
    }

    Ok(samples)
}

/// Проверяет, что все sample offset-ы попадают в диапазон `mdat`.
///
/// Это самопроверка: если demuxer ошибся в арифметике, demo
/// сразу покажет красный флаг.
pub fn verify_offsets(
    samples: &[VideoSample],
    mdat_start: u64,
    mdat_end: u64,
) -> Result<(), String> {
    for s in samples {
        if s.offset < mdat_start || s.offset + s.size as u64 > mdat_end {
            return Err(format_str(format_args!(
                "sample #{}: offset {} size {} выходит за mdat ({}..{})",
                s.index, s.offset, s.size, mdat_start, mdat_end,
            )));
        }
    }
    Ok(())
}

/// Читает `timescale` из payload `mdhd` (version 0).
///
/// Раскладка mdhd version 0:
///   [0..4]   version+flags
///   [4..8]   creation_time
///   [8..12]  modification_time
///   [12..16] timescale
///   [16..20] duration
///
/// Version 1 — timescale на 20..24, но такие файлы крайне редки.
pub fn parse_mdhd_timescale(mdhd: &BoxInfo) -> Option<u32> {
    let p = &mdhd.payload;
    if p.len() < 16 {
        return None;
    }
    let version = p[0];
    let off = if version == 1 { 20 } else { 12 };
    if p.len() < off + 4 {
        return None;
    }
    let ts = read_u32_be(p, off);
    if ts == 0 { None } else { Some(ts) }
}

/// Строит строку codec-а для `VideoDecoder.configure({codec})`.
///
/// Для H.264 формат — `avc1.PPCCLL`, где PP/CC/LL — три байта из `avcC`:
/// `[1]` AVCProfileIndication, `[2]` profile_compatibility, `[3]` level.
pub fn avc_codec_string(avcc: &[u8]) -> Option<String> {
    if avcc.len() < 4 {
        return None;
    }
    Some(format_str(format_args!(
        "avc1.{:02X}{:02X}{:02X}",
        avcc[1], avcc[2], avcc[3],
    )))
}
// =============================================================================
// Audio track (passthrough)
// =============================================================================

/// Audio-сэмпл — те же поля, что и VideoSample, без `is_key`.
#[derive(Debug, Clone)]
pub struct AudioSample {
    pub index: usize,
    pub offset: u64,
    pub size: u32,
    pub timestamp_us: u64,
    pub duration_us: u32,
}

/// Информация об audio-треке для passthrough (без перекодирования).
pub struct AudioTrackInfo {
    /// Полный payload stsd box'а — `[version+flags][entry_count][entry]`.
    /// Копируется в новый MP4 как есть.
    pub stsd_payload: Vec<u8>,
    pub timescale: u32,
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<AudioSample>,
}

/// Извлекает audio-трек из moov. Возвращает None, если audio-трека нет.
pub fn extract_audio_track(moov: &BoxInfo) -> Result<Option<AudioTrackInfo>, String> {
    let mut audio_trak: Option<&BoxInfo> = None;
    for trak in &moov.children {
        if trak.box_type != "trak" {
            continue;
        }
        if let Some(hdlr) = trak.find("hdlr") {
            let p = &hdlr.payload;
            if p.len() >= 12 && &p[8..12] == b"soun" {
                audio_trak = Some(trak);
                break;
            }
        }
    }
    let Some(audio_trak) = audio_trak else {
        return Ok(None);
    };

    let mdhd = audio_trak
        .find("mdhd")
        .ok_or_else(|| String::from("audio: нет mdhd"))?;
    let timescale =
        parse_mdhd_timescale(mdhd).ok_or_else(|| String::from("audio: mdhd.timescale = 0"))?;

    let stbl = audio_trak
        .find("stbl")
        .ok_or_else(|| String::from("audio: нет stbl"))?;
    let table = parse_sample_table(stbl, timescale)?;
    let raw_samples = build_samples(&table)?;

    let samples: Vec<AudioSample> = raw_samples
        .into_iter()
        .map(|s| AudioSample {
            index: s.index,
            offset: s.offset,
            size: s.size,
            timestamp_us: s.timestamp_us,
            duration_us: s.duration_us,
        })
        .collect();

    let stsd = audio_trak
        .find("stsd")
        .ok_or_else(|| String::from("audio: нет stsd"))?;
    let stsd_payload = stsd.payload.clone();

    // Стандартная раскладка AudioSampleEntry внутри stsd payload:
    //   [0..4]   version+flags
    //   [4..8]   entry_count
    //   entry:
    //     [8..12]   size
    //     [12..16]  type
    //     [16..22]  reserved (6)
    //     [22..24]  data_reference_index (2)
    //     [24..32]  reserved (8)
    //     [32..34]  channel_count
    //     [34..36]  sample_size
    //     [36..38]  pre_defined
    //     [38..40]  reserved
    //     [40..44]  sample_rate (16.16, целая часть в старших 16)
    let mut sample_rate: u32 = 0;
    let mut channels: u16 = 0;
    if stsd_payload.len() >= 44 {
        channels = u16::from_be_bytes([stsd_payload[32], stsd_payload[33]]);
        let sr_fixed = u32::from_be_bytes([
            stsd_payload[40],
            stsd_payload[41],
            stsd_payload[42],
            stsd_payload[43],
        ]);
        sample_rate = sr_fixed >> 16;
    }
    if sample_rate == 0 || channels == 0 {
        return Err(String::from("audio: sample_rate или channels = 0"));
    }

    web_sys::console::log_1(&JsValue::from_str(&format!(
        "[demux] audio: {} samples, {} Hz, {} ch, timescale={}",
        samples.len(),
        sample_rate,
        channels,
        timescale,
    )));

    Ok(Some(AudioTrackInfo {
        stsd_payload,
        timescale,
        sample_rate,
        channels,
        samples,
    }))
}