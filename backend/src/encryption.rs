use base64::Engine as _;
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce,
};
use crate::error::AppError;

pub fn encrypt_token(key: &[u8; 32], plaintext: &str) -> Result<Vec<u8>, AppError> {
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| AppError::InternalServerError("Encryption init failed".to_string()))?;
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| AppError::InternalServerError("Encryption failed".to_string()))?;

    // Layout: nonce(12 bytes) || ciphertext+tag
    let mut blob = Vec::with_capacity(12 + ciphertext.len());
    blob.extend_from_slice(nonce.as_slice());
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

pub fn decrypt_token(key: &[u8; 32], blob: &[u8]) -> Result<String, AppError> {
    if blob.len() < 12 {
        return Err(AppError::InternalServerError("Encrypted token blob too short".to_string()));
    }
    let (nonce_bytes, ciphertext) = blob.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| AppError::InternalServerError("Decryption init failed".to_string()))?;
    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|_| AppError::InternalServerError("Decryption failed — wrong key or corrupted data".to_string()))?;
    String::from_utf8(plaintext)
        .map_err(|_| AppError::InternalServerError("Decrypted token is not valid UTF-8".to_string()))
}

pub fn load_encryption_key() -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let path = std::env::var("APP_ENCRYPTION_KEY_PATH")
        .unwrap_or_else(|_| "/etc/wealthagent/keyfile".to_string());
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read key from {}: {}", path, e))?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(raw.trim())
        .map_err(|_| "Key file must contain a base64-encoded value")?;
    bytes.try_into().map_err(|_| "Key must be exactly 32 bytes (256-bit)".into())
}
