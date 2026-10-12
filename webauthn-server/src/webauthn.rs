//! WebAuthn: encoding, rp_id_hash, парсинг clientDataJSON и authenticatorData.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};

// ============================================================
//                    ENCODING
// ============================================================

/// Кодирует байты в base64url без padding.
pub fn to_base64url(bytes: &[u8]) -> String {
    return URL_SAFE_NO_PAD.encode(bytes);
}

/// Декодирует base64url в байты.
pub fn from_base64url(s: &str) -> Result<Vec<u8>, String> {
    return URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| format!("base64url decode: {e}"));
}

// ============================================================
//                    RP ID HASH
// ============================================================

/// `rpIdHash = SHA-256(rp_id)`. Первые 32 байта authenticatorData.
pub fn rp_id_hash(rp_id: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(rp_id.as_bytes());
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    return arr;
}

// ============================================================
//                    AUTHENTICATOR DATA
// ============================================================

pub struct AuthDataView {
    pub rp_id_hash: [u8; 32],
    pub flags: u8,
    pub sign_count: u32,
}

pub fn parse_auth_data(bytes: &[u8]) -> Result<AuthDataView, String> {
    if bytes.len() < 37 {
        return Err(format!(
            "authenticatorData too short: {} < 37",
            bytes.len(),
        ));
    }
    let mut rp_id_hash = [0u8; 32];
    rp_id_hash.copy_from_slice(&bytes[0..32]);
    let flags = bytes[32];
    let sign_count = u32::from_be_bytes([
        bytes[33], bytes[34], bytes[35], bytes[36],
    ]);
    return Ok(AuthDataView { rp_id_hash, flags, sign_count });
}

// ============================================================
//                    CLIENT DATA JSON
// ============================================================

#[derive(Debug, Deserialize)]
pub struct ClientData {
    #[serde(rename = "type")]
    pub kind: String,
    pub challenge: String,
    pub origin: String,
    #[serde(default, rename = "crossOrigin")]
    pub cross_origin: bool,
}

pub fn parse_client_data(bytes: &[u8]) -> Result<ClientData, String> {
    let s = std::str::from_utf8(bytes)
        .map_err(|e| format!("clientDataJSON utf8: {e}"))?;
    return serde_json::from_str(s)
        .map_err(|e| format!("clientDataJSON json: {e}"));
}