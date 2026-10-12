//! Модели данных WebAuthn-операций.

/// Конфигурация Relying Party (нашего сервиса).
#[derive(Debug, Clone)]
pub struct RpConfig {
    pub rp_id: String,
    pub rp_name: String,
    pub origin: String,
}

impl RpConfig {
    pub fn new(rp_id: &str, rp_name: &str) -> Self {
        let origin = crate::format_str::format_str(
            format_args!("https://{rp_id}"),
        );
        return Self {
            rp_id: rp_id.to_string(),
            rp_name: rp_name.to_string(),
            origin,
        };
    }
}

/// Результат `navigator.credentials.create()`.
#[derive(Debug, Clone)]
pub struct RegistrationResult {
    pub raw_id: Vec<u8>,
    pub client_data_json: Vec<u8>,
    pub attestation_object: Vec<u8>,
    /// `platform` / `cross-platform` / `unknown`.
    pub attachment: String,
}

/// Результат `navigator.credentials.get()`.
#[derive(Debug, Clone)]
pub struct AuthResult {
    pub raw_id: Vec<u8>,
    pub client_data_json: Vec<u8>,
    pub authenticator_data: Vec<u8>,
    pub signature: Vec<u8>,
    pub user_handle: Option<Vec<u8>>,
}