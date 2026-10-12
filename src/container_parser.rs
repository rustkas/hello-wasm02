//! 23.9. Структура контейнеров: MP4 и WebM.
//!
//! Ручной парсинг двух бинарных форматов без внешних крейтов:
//!
//! - **MP4** — дерево box-ов с 32-битными размерами и FourCC.
//! - **WebM** — EBML-дерево с variable-length integer ID и size.
//!
//! ## Публичный API
//!
//! - `parse_mp4(data) -> Result<Vec<BoxInfo>, String>` — дерево MP4 box-ов.
//! - `parse_webm(data) -> Result<Vec<EbmlElement>, String>` — дерево WebM.
//! - `is_fast_start(boxes) -> bool` — moov перед mdat.
//! - `parse_avcc(avcc) -> Result<(Vec<u8>, Vec<u8>), String>` — SPS/PPS.
//! - `find_avcc_in_stsd(stsd) -> Option<Vec<u8>>` — payload avcC из stsd.
//! - `format_bytes(n) -> String`, `hex_dump(data, limit) -> String`.
//!
//! ## Абсолютные offset-ы
//!
//! Внутренние `parse_mp4_at`/`parse_webm_at` принимают `base_offset` —
//! позицию текущего буфера в файле. Все `BoxInfo::offset` и
//! `EbmlElement::offset` — **абсолютные позиции в исходном файле**.

use crate::video_frame::format_str;

// =============================================================================
// MP4
// =============================================================================

/// Информация об одном MP4 box-е.
#[derive(Debug, Clone)]
pub struct BoxInfo {
    pub box_type: String,
    pub offset: usize,
    pub size: u64,
    pub header_size: u8,
    pub children: Vec<BoxInfo>,
    pub payload: Vec<u8>,
}

impl BoxInfo {
    pub fn find(&self, box_type: &str) -> Option<&BoxInfo> {
        if self.box_type == box_type {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find(box_type) {
                return Some(found);
            }
        }
        None
    }
}

/// Разбирает MP4 из байтов. Offset-ы — абсолютные (от начала файла).
pub fn parse_mp4(data: &[u8]) -> Result<Vec<BoxInfo>, String> {
    parse_mp4_at(data, 0)
}

fn parse_mp4_at(data: &[u8], base_offset: usize) -> Result<Vec<BoxInfo>, String> {
    let mut boxes = Vec::new();
    let mut offset = 0usize;

    while offset + 8 <= data.len() {
        let size_u32 = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as u64;

        let box_type = std::str::from_utf8(&data[offset + 4..offset + 8])
            .map_err(|e| format_str(format_args!("invalid box type: {e}")))?
            .to_string();

        let (header_size, size): (u8, u64) = if size_u32 == 1 {
            if offset + 16 > data.len() {
                return Err(String::from("largesize truncated"));
            }
            let largesize = u64::from_be_bytes([
                data[offset + 8],
                data[offset + 9],
                data[offset + 10],
                data[offset + 11],
                data[offset + 12],
                data[offset + 13],
                data[offset + 14],
                data[offset + 15],
            ]);
            (16u8, largesize)
        } else if size_u32 == 0 {
            (8u8, (data.len() - offset) as u64)
        } else {
            (8u8, size_u32)
        };

        if size < header_size as u64 || offset as u64 + size > data.len() as u64 {
            return Err(format_str(format_args!(
                "invalid box size: type={box_type}, size={size}, offset={offset}"
            )));
        }

        let body_start = offset + header_size as usize;
        let body_end = offset + size as usize;
        let body = &data[body_start..body_end];

        let is_container = matches!(
            box_type.as_str(),
            "moov" | "trak" | "mdia" | "minf" | "stbl" | "edts" | "dinf"
        );

        let (children, payload) = if is_container {
            match parse_mp4_at(body, base_offset + body_start) {
                Ok(ch) => (ch, Vec::new()),
                Err(_) => (Vec::new(), body.to_vec()),
            }
        } else {
            // hdlr нужен для определения типа трека в analyze_mp4.
            let keep_payload = matches!(
    box_type.as_str(),
    "ftyp" | "stsd" | "avcC" | "hdlr" | "mdhd"
    | "stco" | "stsz" | "stss" | "stsc" | "stts"
);
            (Vec::new(), if keep_payload { body.to_vec() } else { Vec::new() })
        };

        boxes.push(BoxInfo {
            box_type,
            offset: base_offset + offset,
            size,
            header_size,
            children,
            payload,
        });

        offset += size as usize;
    }

    Ok(boxes)
}

/// Проверяет fast-start: `moov` перед `mdat`.
pub fn is_fast_start(boxes: &[BoxInfo]) -> bool {
    let mut moov_pos = None;
    let mut mdat_pos = None;
    for (i, b) in boxes.iter().enumerate() {
        if b.box_type == "moov" {
            moov_pos = Some(i);
        }
        if b.box_type == "mdat" && mdat_pos.is_none() {
            mdat_pos = Some(i);
        }
    }
    match (moov_pos, mdat_pos) {
        (Some(m), Some(d)) => m < d,
        _ => false,
    }
}

/// Извлекает SPS/PPS из `avcC`.
pub fn parse_avcc(avcc: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    if avcc.len() < 7 {
        return Err(String::from("avcC too short"));
    }
    let num_sps = (avcc[5] & 0x1F) as usize;
    let mut pos = 6usize;

    if num_sps == 0 {
        return Err(String::from("avcC: no SPS"));
    }
    if pos + 2 > avcc.len() {
        return Err(String::from("avcC: SPS length truncated"));
    }
    let sps_len = u16::from_be_bytes([avcc[pos], avcc[pos + 1]]) as usize;
    pos += 2;
    if pos + sps_len > avcc.len() {
        return Err(String::from("avcC: SPS truncated"));
    }
    let sps = avcc[pos..pos + sps_len].to_vec();
    pos += sps_len;

    if pos >= avcc.len() {
        return Err(String::from("avcC: no PPS count"));
    }
    let num_pps = avcc[pos] as usize;
    pos += 1;
    if num_pps == 0 {
        return Err(String::from("avcC: no PPS"));
    }
    if pos + 2 > avcc.len() {
        return Err(String::from("avcC: PPS length truncated"));
    }
    let pps_len = u16::from_be_bytes([avcc[pos], avcc[pos + 1]]) as usize;
    pos += 2;
    if pos + pps_len > avcc.len() {
        return Err(String::from("avcC: PPS truncated"));
    }
    let pps = avcc[pos..pos + pps_len].to_vec();

    Ok((sps, pps))
}

/// Ищет payload `avcC` внутри `stsd` для video-track.
///
/// `stsd` — не чистый контейнер. Его payload:
///   [version+flags: 4]
///   [entry_count: 4]
///   entry[0]:
///     [size: 4][type: 4]              ← 8 байт
///     VisualSampleEntry: 78 байт
///     child boxes:                     ← здесь лежит avcC
pub fn find_avcc_in_stsd(stsd: &BoxInfo) -> Option<Vec<u8>> {
    let p = &stsd.payload;
    // 8 (stsd header) + 8 (entry header) + 78 (VisualSampleEntry).
    let mut pos = 8 + 8 + 78;
    while pos + 8 <= p.len() {
        let size = u32::from_be_bytes([p[pos], p[pos + 1], p[pos + 2], p[pos + 3]]) as usize;
        if size < 8 || pos + size > p.len() {
            break;
        }
        if &p[pos + 4..pos + 8] == b"avcC" {
            return Some(p[pos + 8..pos + size].to_vec());
        }
        pos += size;
    }
    None
}

// =============================================================================
// WebM (EBML)
// =============================================================================

#[derive(Debug, Clone)]
pub struct EbmlElement {
    pub id: u64,
    pub name: String,
    pub offset: usize,
    pub size: u64,
    pub total_size: u64,
    pub children: Vec<EbmlElement>,
    pub payload: Vec<u8>,
}

impl EbmlElement {
    pub fn find(&self, name: &str) -> Option<&EbmlElement> {
        if self.name == name {
            return Some(self);
        }
        for c in &self.children {
            if let Some(f) = c.find(name) {
                return Some(f);
            }
        }
        None
    }
}

fn read_vint(data: &[u8]) -> Option<(u64, usize)> {
    if data.is_empty() {
        return None;
    }
    let first = data[0];
    let length = first.leading_zeros() as usize + 1;
    if length > 8 || data.len() < length {
        return None;
    }
    let mut value: u64 = 0;
    for &b in &data[..length] {
        value = (value << 8) | b as u64;
    }
    Some((value, length))
}

fn read_id(data: &[u8]) -> Option<(u64, usize)> {
    read_vint(data)
}

fn read_size(data: &[u8]) -> Option<(u64, usize)> {
    let (v, len) = read_vint(data)?;
    if len > 4 {
        return None;
    }
    let marker_bit = 1u64 << (7 * len);
    Some((v & (marker_bit - 1), len))
}

fn ebml_name(id: u64) -> &'static str {
    match id {
        0x1A45DFA3 => "EBML",
        0x18538067 => "Segment",
        0x1654AE6B => "Tracks",
        0xAE => "TrackEntry",
        0x86 => "CodecID",
        0xE0 => "Video",
        0xB0 => "PixelWidth",
        0xBA => "PixelHeight",
        0x1F43B675 => "Cluster",
        0xE7 => "Timestamp",
        0xA3 => "SimpleBlock",
        0xEC => "Void",
        0xBF => "CRC-32",
        0x42F7 => "EBMLVersion",
        0x4286 => "EBMLReadVersion",
        0x42F2 => "EBMLMaxIDLength",
        0x42F3 => "EBMLMaxSizeLength",
        0x4282 => "DocType",
        0x4287 => "DocTypeVersion",
        0x4285 => "DocTypeReadVersion",
        _ => "Unknown",
    }
}

fn is_master(id: u64) -> bool {
    match id {
        0x1A45DFA3 | 0x18538067 | 0x1654AE6B | 0xAE | 0xE0 | 0x1F43B675 => true,
        _ => false,
    }
}

/// Разбирает WebM/EBML из байтов. Offset-ы — абсолютные.
pub fn parse_webm(data: &[u8]) -> Result<Vec<EbmlElement>, String> {
    parse_webm_at(data, 0)
}

fn parse_webm_at(data: &[u8], base_offset: usize) -> Result<Vec<EbmlElement>, String> {
    let mut elements = Vec::new();
    let mut offset = 0usize;

    while offset < data.len() {
        let (id, id_len) = read_id(&data[offset..])
            .ok_or_else(|| format_str(format_args!("invalid EBML id at offset {offset}")))?;

        let (size, size_len) = read_size(&data[offset + id_len..])
            .ok_or_else(|| format_str(format_args!("invalid EBML size at offset {offset}")))?;

        let total_header = id_len + size_len;
        let body_start = offset + total_header;
        let body_end = body_start
            .checked_add(size as usize)
            .ok_or_else(|| String::from("EBML size overflow"))?;

        if body_end > data.len() {
            return Err(format_str(format_args!(
                "EBML element truncated: id=0x{id:X}, size={size}, offset={offset}"
            )));
        }

        let body = &data[body_start..body_end];
        let name = ebml_name(id).to_string();

        let (children, payload) = if is_master(id) {
            match parse_webm_at(body, base_offset + body_start) {
                Ok(ch) => (ch, Vec::new()),
                Err(_) => (Vec::new(), body.to_vec()),
            }
        } else {
            (Vec::new(), body.to_vec())
        };

        elements.push(EbmlElement {
            id,
            name,
            offset: base_offset + offset,
            size,
            total_size: (total_header + size as usize) as u64,
            children,
            payload,
        });

        offset = body_end;
    }

    Ok(elements)
}

// =============================================================================
// Форматирование
// =============================================================================

/// Возвращает hex-строку байтов (до `limit`).
pub fn hex_dump(data: &[u8], limit: usize) -> String {
    let n = data.len().min(limit);
    let mut out = String::with_capacity(n * 3 + 2);
    for (i, b) in data[..n].iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format_str(format_args!("{b:02X}")));
    }
    if data.len() > limit {
        out.push_str(" …");
    }
    out
}

/// Возвращает человекочитаемый размер: 512 B, 3.4 KB, 8.1 MB.
pub fn format_bytes(n: usize) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = n as f64;
    if f < KB {
        format_str(format_args!("{n} B"))
    } else if f < MB {
        format_str(format_args!("{:.1} KB", f / KB))
    } else if f < GB {
        format_str(format_args!("{:.1} MB", f / MB))
    } else {
        format_str(format_args!("{:.2} GB", f / GB))
    }
}
