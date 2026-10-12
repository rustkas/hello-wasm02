//! 23.11/23.12. Muxing: EncodedVideoChunk + Audio passthrough → MP4.
//!
//! ## Двухпроходная сборка moov
//!
//! `stco` содержит offset начала `mdat` body. Но offset зависит от
//! размера `moov`, который сам содержит `stco`. Решение: первый
//! проход строит `moov` с offset = 0, измеряет размер, второй —
//! с реальным offset.
//!
//! ## Duration в mdhd и stts
//!
//! **Критично:** и `mdhd.duration`, и `stts.sample_delta` должны
//! быть в единицах `mdhd.timescale`, не в микросекундах.
//!
//! Для video `timescale = MOVIE_TIMESCALE = 1_000_000`, так что
//! duration_us == duration_ts.
//!
//! Для audio `timescale = 44100/48000`. Без пересчёта Chrome
//! видит `sum(stts) >> mdhd.duration` и отбрасывает audio-track.

use crate::video_frame::format_str;

pub const VIDEO_TRACK_ID: u32 = 1;
pub const AUDIO_TRACK_ID: u32 = 2;
pub const MOVIE_TIMESCALE: u32 = 1_000_000;
pub const FTYP_SIZE: usize = 32;

// =============================================================================
// Входные структуры
// =============================================================================

#[derive(Debug, Clone)]
pub struct VideoChunk {
    pub is_key: bool,
    pub timestamp_us: u64,
    pub duration_us: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct MuxConfig {
    pub width: u32,
    pub height: u32,
    pub avcc: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub timestamp_us: u64,
    pub duration_us: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct AudioTrackInput {
    pub stsd_payload: Vec<u8>,
    pub timescale: u32,
    pub sample_rate: u32,
    pub channels: u16,
    pub chunks: Vec<AudioChunk>,
}

// =============================================================================
// Низкоуровневые хелперы
// =============================================================================

fn u16_be(v: u16) -> [u8; 2] { return v.to_be_bytes(); }
fn u32_be(v: u32) -> [u8; 4] { return v.to_be_bytes(); }

fn build_box(box_type: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let size = (8 + body.len()) as u32;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(&u32_be(size));
    out.extend_from_slice(box_type);
    out.extend_from_slice(body);
    return out;
}

fn build_full_box(box_type: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let vf = ((version as u32) << 24) | (flags & 0xFFFFFF);
    let mut full_body = Vec::with_capacity(4 + body.len());
    full_body.extend_from_slice(&u32_be(vf));
    full_body.extend_from_slice(body);
    return build_box(box_type, &full_body);
}

fn unity_matrix() -> [u8; 36] {
    return [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00,
    ];
}

fn build_ftyp() -> Vec<u8> {
    let mut body = Vec::with_capacity(24);
    body.extend_from_slice(b"isom");
    body.extend_from_slice(&u32_be(0x200));
    body.extend_from_slice(b"isom");
    body.extend_from_slice(b"iso2");
    body.extend_from_slice(b"avc1");
    body.extend_from_slice(b"mp41");
    return build_box(b"ftyp", &body);
}

fn build_mvhd(duration: u32, next_track_id: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(96);
    body.extend_from_slice(&u32_be(0));
    body.extend_from_slice(&u32_be(0));
    body.extend_from_slice(&u32_be(MOVIE_TIMESCALE));
    body.extend_from_slice(&u32_be(duration));
    body.extend_from_slice(&u32_be(0x00010000));
    body.extend_from_slice(&u16_be(0x0100));
    body.extend_from_slice(&u16_be(0));
    body.extend_from_slice(&[0u8; 8]);
    body.extend_from_slice(&unity_matrix());
    body.extend_from_slice(&[0u8; 24]);
    body.extend_from_slice(&u32_be(next_track_id));
    return build_full_box(b"mvhd", 0, 0, &body);
}

fn build_tkhd(track_id: u32, duration: u32, width: u32, height: u32, is_audio: bool) -> Vec<u8> {
    let mut body = Vec::with_capacity(80);
    body.extend_from_slice(&u32_be(0));
    body.extend_from_slice(&u32_be(0));
    body.extend_from_slice(&u32_be(track_id));
    body.extend_from_slice(&u32_be(0));
    body.extend_from_slice(&u32_be(duration));
    body.extend_from_slice(&[0u8; 8]);
    body.extend_from_slice(&u16_be(0));
    body.extend_from_slice(&u16_be(0));
    let volume: u16 = if is_audio { 0x0100 } else { 0 };
    body.extend_from_slice(&u16_be(0));
    body.extend_from_slice(&u16_be(volume));
    body.extend_from_slice(&unity_matrix());
    if is_audio {
        body.extend_from_slice(&u32_be(0));
        body.extend_from_slice(&u32_be(0));
    } else {
        body.extend_from_slice(&u32_be(width << 16));
        body.extend_from_slice(&u32_be(height << 16));
    }
    return build_full_box(b"tkhd", 0, 0x07, &body);
}

fn build_mdhd(timescale: u32, duration_in_ts: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(20);
    body.extend_from_slice(&u32_be(0));
    body.extend_from_slice(&u32_be(0));
    body.extend_from_slice(&u32_be(timescale));
    body.extend_from_slice(&u32_be(duration_in_ts));
    body.extend_from_slice(&u16_be(0x55C4));
    body.extend_from_slice(&u16_be(0));
    return build_full_box(b"mdhd", 0, 0, &body);
}

fn build_hdlr(handler: &[u8; 4], name: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(24 + name.len() + 1);
    body.extend_from_slice(&u32_be(0));
    body.extend_from_slice(handler);
    body.extend_from_slice(&[0u8; 12]);
    body.extend_from_slice(name);
    body.push(0);
    return build_full_box(b"hdlr", 0, 0, &body);
}

fn build_url_() -> Vec<u8> { return build_full_box(b"url ", 0, 0x01, &[]); }

fn build_dref() -> Vec<u8> {
    let url_box = build_url_();
    let mut body = Vec::with_capacity(8 + url_box.len());
    body.extend_from_slice(&u32_be(1));
    body.extend_from_slice(&url_box);
    return build_full_box(b"dref", 0, 0, &body);
}

fn build_dinf() -> Vec<u8> {
    let dref = build_dref();
    return build_box(b"dinf", &dref);
}

fn build_vmhd() -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&u16_be(0));
    body.extend_from_slice(&[0u8; 6]);
    return build_full_box(b"vmhd", 0, 0x01, &body);
}

fn build_avc1(width: u32, height: u32, avcc: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(78 + 8 + avcc.len());
    body.extend_from_slice(&[0u8; 6]);
    body.extend_from_slice(&u16_be(1));
    body.extend_from_slice(&[0u8; 16]);
    body.extend_from_slice(&u16_be(width as u16));
    body.extend_from_slice(&u16_be(height as u16));
    body.extend_from_slice(&u32_be(0x00480000));
    body.extend_from_slice(&u32_be(0x00480000));
    body.extend_from_slice(&u32_be(0));
    body.extend_from_slice(&u16_be(1));
    body.extend_from_slice(&[0u8; 32]);
    body.extend_from_slice(&u16_be(0x0018));
    body.extend_from_slice(&u16_be(0xFFFF));
    body.extend_from_slice(&build_box(b"avcC", avcc));
    return build_box(b"avc1", &body);
}

fn build_video_stsd(width: u32, height: u32, avcc: &[u8]) -> Vec<u8> {
    let avc1 = build_avc1(width, height, avcc);
    let mut body = Vec::with_capacity(4 + avc1.len());
    body.extend_from_slice(&u32_be(1));
    body.extend_from_slice(&avc1);
    return build_full_box(b"stsd", 0, 0, &body);
}

fn build_smhd() -> Vec<u8> {
    let mut body = Vec::with_capacity(4);
    body.extend_from_slice(&u16_be(0));
    body.extend_from_slice(&u16_be(0));
    return build_full_box(b"smhd", 0, 0, &body);
}

fn build_audio_stsd(stsd_payload: &[u8]) -> Vec<u8> {
    return build_box(b"stsd", stsd_payload);
}

fn build_stts(entries: &[(u32, u32)]) -> Vec<u8> {
    let mut body = Vec::with_capacity(8 + entries.len() * 8);
    body.extend_from_slice(&u32_be(entries.len() as u32));
    for &(count, delta) in entries {
        body.extend_from_slice(&u32_be(count));
        body.extend_from_slice(&u32_be(delta));
    }
    return build_full_box(b"stts", 0, 0, &body);
}

fn build_stts_from_durations_ts(durations_ts: &[u32]) -> Vec<u8> {
    let mut entries: Vec<(u32, u32)> = Vec::new();
    for &d in durations_ts {
        if let Some(last) = entries.last_mut() {
            if last.1 == d {
                last.0 += 1;
                continue;
            }
        }
        entries.push((1, d));
    }
    return build_stts(&entries);
}

fn build_stsc(total: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&u32_be(1));
    body.extend_from_slice(&u32_be(1));
    body.extend_from_slice(&u32_be(total));
    body.extend_from_slice(&u32_be(1));
    return build_full_box(b"stsc", 0, 0, &body);
}

fn build_stsz(sizes: &[u32]) -> Vec<u8> {
    let mut body = Vec::with_capacity(8 + 4 * sizes.len());
    body.extend_from_slice(&u32_be(0));
    body.extend_from_slice(&u32_be(sizes.len() as u32));
    for &s in sizes {
        body.extend_from_slice(&u32_be(s));
    }
    return build_full_box(b"stsz", 0, 0, &body);
}

fn build_stco(mdat_body_start: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&u32_be(1));
    body.extend_from_slice(&u32_be(mdat_body_start));
    return build_full_box(b"stco", 0, 0, &body);
}

fn build_stss(key_indices: &[u32]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + 4 * key_indices.len());
    body.extend_from_slice(&u32_be(key_indices.len() as u32));
    for &idx in key_indices {
        body.extend_from_slice(&u32_be(idx));
    }
    return build_full_box(b"stss", 0, 0, &body);
}

/// **FIX**: используем per-chunk durations для stts (а не первый chunk).
/// CFR-видео: все durations равны → 1 entry. VFR-видео: строим по факту.
fn build_video_stbl(chunks: &[VideoChunk], config: &MuxConfig, mdat_body_start: u32) -> Vec<u8> {
    let total = chunks.len() as u32;
    let sizes: Vec<u32> = chunks.iter().map(|c| c.data.len() as u32).collect();
    let key_indices: Vec<u32> = chunks
        .iter().enumerate()
        .filter(|(_, c)| c.is_key)
        .map(|(i, _)| (i + 1) as u32)
        .collect();

    // stts: если все durations равны — единый entry (компактно), иначе per-chunk.
    let stts = if chunks.is_empty() {
        build_stts(&[])
    } else {
        let first_dur = chunks[0].duration_us;
        let all_same = chunks.iter().all(|c| c.duration_us == first_dur);
        if all_same {
            build_stts(&[(total, first_dur)])
        } else {
            let durations: Vec<u32> = chunks.iter().map(|c| c.duration_us).collect();
            build_stts_from_durations_ts(&durations)
        }
    };

    let mut body = Vec::new();
    body.extend_from_slice(&build_video_stsd(config.width, config.height, &config.avcc));
    body.extend_from_slice(&stts);
    body.extend_from_slice(&build_stsc(total));
    body.extend_from_slice(&build_stsz(&sizes));
    body.extend_from_slice(&build_stco(mdat_body_start));
    body.extend_from_slice(&build_stss(&key_indices));
    return build_box(b"stbl", &body);
}

fn build_video_minf(chunks: &[VideoChunk], config: &MuxConfig, mdat_body_start: u32) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&build_vmhd());
    body.extend_from_slice(&build_dinf());
    body.extend_from_slice(&build_video_stbl(chunks, config, mdat_body_start));
    return build_box(b"minf", &body);
}

fn build_video_mdia(
    chunks: &[VideoChunk], config: &MuxConfig,
    mdat_body_start: u32, movie_duration_us: u32,
) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&build_mdhd(MOVIE_TIMESCALE, movie_duration_us));
    body.extend_from_slice(&build_hdlr(b"vide", b"VideoHandler"));
    body.extend_from_slice(&build_video_minf(chunks, config, mdat_body_start));
    return build_box(b"mdia", &body);
}

fn build_video_trak(
    chunks: &[VideoChunk], config: &MuxConfig,
    mdat_body_start: u32, movie_duration_us: u32,
) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&build_tkhd(
        VIDEO_TRACK_ID, movie_duration_us, config.width, config.height, false,
    ));
    body.extend_from_slice(&build_video_mdia(chunks, config, mdat_body_start, movie_duration_us));
    return build_box(b"trak", &body);
}

/// **КРИТИЧНО:** durations в stts должны быть в единицах audio.timescale,
/// а не в микросекундах. Иначе Chrome видит sum(stts) >> mdhd.duration
/// и отбрасывает audio-track как невалидный.
fn build_audio_stbl(audio: &AudioTrackInput, mdat_body_start: u32) -> Vec<u8> {
    let total = audio.chunks.len() as u32;
    let sizes: Vec<u32> = audio.chunks.iter().map(|c| c.data.len() as u32).collect();

    let durations_ts: Vec<u32> = audio.chunks.iter().map(|c| {
        if audio.timescale == 0 {
            0
        } else {
            ((c.duration_us as u64) * (audio.timescale as u64) / 1_000_000) as u32
        }
    }).collect();

    let mut body = Vec::new();
    body.extend_from_slice(&build_audio_stsd(&audio.stsd_payload));
    body.extend_from_slice(&build_stts_from_durations_ts(&durations_ts));
    body.extend_from_slice(&build_stsc(total));
    body.extend_from_slice(&build_stsz(&sizes));
    body.extend_from_slice(&build_stco(mdat_body_start));
    return build_box(b"stbl", &body);
}

fn build_audio_minf(audio: &AudioTrackInput, mdat_body_start: u32) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&build_smhd());
    body.extend_from_slice(&build_dinf());
    body.extend_from_slice(&build_audio_stbl(audio, mdat_body_start));
    return build_box(b"minf", &body);
}

fn build_audio_mdia(
    audio: &AudioTrackInput, mdat_body_start: u32, movie_duration_us: u32,
) -> Vec<u8> {
    // mdhd.duration — в audio.timescale.
    let media_duration_ts = if audio.timescale == 0 {
        0
    } else {
        ((movie_duration_us as u64) * (audio.timescale as u64) / 1_000_000) as u32
    };
    let mut body = Vec::new();
    body.extend_from_slice(&build_mdhd(audio.timescale, media_duration_ts));
    body.extend_from_slice(&build_hdlr(b"soun", b"SoundHandler"));
    body.extend_from_slice(&build_audio_minf(audio, mdat_body_start));
    return build_box(b"mdia", &body);
}

fn build_audio_trak(
    audio: &AudioTrackInput, mdat_body_start: u32, movie_duration_us: u32,
) -> Vec<u8> {
    let mut body = Vec::new();
    // tkhd.duration — в MOVIE_TIMESCALE == µs.
    body.extend_from_slice(&build_tkhd(AUDIO_TRACK_ID, movie_duration_us, 0, 0, true));
    body.extend_from_slice(&build_audio_mdia(audio, mdat_body_start, movie_duration_us));
    return build_box(b"trak", &body);
}

fn total_video_duration_us(chunks: &[VideoChunk]) -> u32 {
    let last_end = chunks.last()
        .map(|c| c.timestamp_us + c.duration_us as u64)
        .unwrap_or(0);
    return last_end.min(u32::MAX as u64) as u32;
}

fn total_audio_duration_us(chunks: &[AudioChunk]) -> u32 {
    let last_end = chunks.last()
        .map(|c| c.timestamp_us + c.duration_us as u64)
        .unwrap_or(0);
    return last_end.min(u32::MAX as u64) as u32;
}

fn build_moov(
    video_chunks: &[VideoChunk], video_config: &MuxConfig,
    audio: Option<&AudioTrackInput>,
    video_mdat_offset: u32, audio_mdat_offset: u32,
) -> Vec<u8> {
    let video_duration = total_video_duration_us(video_chunks);
    let audio_duration = audio.map(|a| total_audio_duration_us(&a.chunks)).unwrap_or(0);
    let duration = video_duration.max(audio_duration);
    let next_track_id = if audio.is_some() { 3 } else { 2 };

    let mut body = Vec::new();
    body.extend_from_slice(&build_mvhd(duration, next_track_id));
    body.extend_from_slice(&build_video_trak(
        video_chunks, video_config, video_mdat_offset, video_duration,
    ));
    if let Some(a) = audio {
        body.extend_from_slice(&build_audio_trak(a, audio_mdat_offset, audio_duration));
    }
    return build_box(b"moov", &body);
}

fn build_mdat(video_chunks: &[VideoChunk], audio: Option<&AudioTrackInput>) -> Vec<u8> {
    let video_total: usize = video_chunks.iter().map(|c| c.data.len()).sum();
    let audio_total: usize = audio
        .map(|a| a.chunks.iter().map(|c| c.data.len()).sum())
        .unwrap_or(0);
    let total = video_total + audio_total;
    let size = (8 + total) as u32;

    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(&u32_be(size));
    out.extend_from_slice(b"mdat");
    for c in video_chunks {
        out.extend_from_slice(&c.data);
    }
    if let Some(a) = audio {
        for c in &a.chunks {
            out.extend_from_slice(&c.data);
        }
    }
    return out;
}

pub fn build_mp4(chunks: &[VideoChunk], config: &MuxConfig) -> Result<Vec<u8>, String> {
    return build_mp4_with_audio(chunks, config, None);
}

pub fn build_mp4_with_audio(
    chunks: &[VideoChunk], config: &MuxConfig, audio: Option<&AudioTrackInput>,
) -> Result<Vec<u8>, String> {
    if chunks.is_empty() {
        return Err(String::from("build_mp4: пустой массив chunks"));
    }
    if config.width == 0 || config.height == 0 {
        return Err(String::from("build_mp4: width/height = 0"));
    }
    if config.avcc.len() < 4 {
        return Err(String::from("build_mp4: avcc слишком короткий"));
    }
    if config.avcc[0] != 1 {
        return Err(format_str(format_args!(
            "build_mp4: avcc.version = {}, ожидалось 1", config.avcc[0],
        )));
    }
    if let Some(a) = audio {
        if a.chunks.is_empty() {
            return Err(String::from("build_mp4: audio.chunks пусто"));
        }
        if a.timescale == 0 {
            return Err(String::from("build_mp4: audio.timescale = 0"));
        }
        if a.stsd_payload.len() < 8 {
            return Err(String::from("build_mp4: audio.stsd слишком короткий"));
        }
    }

    let moov_stub = build_moov(chunks, config, audio, 0, 0);
    let video_mdat_body_start = (FTYP_SIZE + moov_stub.len() + 8) as u32;

    let video_bytes: u64 = chunks.iter().map(|c| c.data.len() as u64).sum();
    let audio_mdat_body_start_u64 = video_mdat_body_start as u64 + video_bytes;
    if audio_mdat_body_start_u64 > u32::MAX as u64 {
        return Err(String::from("build_mp4: mdat больше 4 ГБ"));
    }
    let audio_mdat_body_start = audio_mdat_body_start_u64 as u32;

    let ftyp = build_ftyp();
    let moov = build_moov(
        chunks, config, audio, video_mdat_body_start, audio_mdat_body_start,
    );
    debug_assert_eq!(moov.len(), moov_stub.len());

    let mdat = build_mdat(chunks, audio);

    let total = ftyp.len() + moov.len() + mdat.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&ftyp);
    out.extend_from_slice(&moov);
    out.extend_from_slice(&mdat);
    return Ok(out);
}