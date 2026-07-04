//! Sealed-box and key-wrapping primitives.
//!
//! Sealed-box layout:  ephemeral_pub(32) || nonce(12) || chacha20poly1305 ct+tag
//! Wrapped-key layout: nonce(12) || chacha20poly1305 ct+tag

use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce,
};
use sha2::{Digest, Sha256};
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

use crate::error::AppError;

fn sealed_box_key(shared: &[u8], eph_pub: &[u8; 32], recipient_pub: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"wealthagent-sealed-v1");
    h.update(shared);
    h.update(eph_pub);
    h.update(recipient_pub);
    h.finalize().into()
}

/// Encrypt to a user's public key. Needs no secret — this is the ingest path.
pub fn seal(recipient_pub: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
    let eph = EphemeralSecret::random_from_rng(OsRng);
    let eph_pub = PublicKey::from(&eph);
    let shared = eph.diffie_hellman(&PublicKey::from(*recipient_pub));
    let key = sealed_box_key(shared.as_bytes(), eph_pub.as_bytes(), recipient_pub);

    let cipher = ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|_| AppError::InternalServerError("seal: cipher init".into()))?;
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ct = cipher.encrypt(&nonce, plaintext)
        .map_err(|_| AppError::InternalServerError("seal: encrypt".into()))?;

    let mut blob = Vec::with_capacity(32 + 12 + ct.len());
    blob.extend_from_slice(eph_pub.as_bytes());
    blob.extend_from_slice(nonce.as_slice());
    blob.extend_from_slice(&ct);
    Ok(blob)
}

/// Open a sealed blob with the user's private key.
pub fn unseal(secret: &[u8; 32], blob: &[u8]) -> Result<String, AppError> {
    if blob.len() < 32 + 12 {
        return Err(AppError::InternalServerError("unseal: blob too short".into()));
    }
    let eph_pub: [u8; 32] = blob[..32].try_into().unwrap();
    let nonce = Nonce::from_slice(&blob[32..44]);
    let sk = StaticSecret::from(*secret);
    let recipient_pub = PublicKey::from(&sk);
    let shared = sk.diffie_hellman(&PublicKey::from(eph_pub));
    let key = sealed_box_key(shared.as_bytes(), &eph_pub, recipient_pub.as_bytes());

    let cipher = ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|_| AppError::InternalServerError("unseal: cipher init".into()))?;
    let pt = cipher.decrypt(nonce, &blob[44..])
        .map_err(|_| AppError::InternalServerError("unseal: wrong key or corrupt data".into()))?;
    String::from_utf8(pt)
        .map_err(|_| AppError::InternalServerError("unseal: invalid UTF-8".into()))
}

/// Derive the passphrase KEK with Argon2id.
pub fn derive_passphrase_kek(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], AppError> {
    let mut out = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| AppError::InternalServerError(format!("kdf: {e}")))?;
    Ok(out)
}

/// Derive a KEK from a personal API token's raw value. The DB stores only the
/// token's Argon2 hash, so this KEK is reproducible only by whoever holds the token.
pub fn derive_token_kek(raw_token: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"wealthagent-pat-kek-v1");
    h.update(raw_token.as_bytes());
    h.finalize().into()
}

pub fn wrap_secret(kek: &[u8; 32], secret: &[u8; 32]) -> Result<Vec<u8>, AppError> {
    let cipher = ChaCha20Poly1305::new_from_slice(kek)
        .map_err(|_| AppError::InternalServerError("wrap: cipher init".into()))?;
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ct = cipher.encrypt(&nonce, secret.as_slice())
        .map_err(|_| AppError::InternalServerError("wrap: encrypt".into()))?;
    let mut blob = Vec::with_capacity(12 + ct.len());
    blob.extend_from_slice(nonce.as_slice());
    blob.extend_from_slice(&ct);
    Ok(blob)
}

pub fn unwrap_secret(kek: &[u8; 32], blob: &[u8]) -> Result<[u8; 32], AppError> {
    if blob.len() < 12 {
        return Err(AppError::Unauthorized);
    }
    let cipher = ChaCha20Poly1305::new_from_slice(kek)
        .map_err(|_| AppError::InternalServerError("unwrap: cipher init".into()))?;
    let pt = cipher.decrypt(Nonce::from_slice(&blob[..12]), &blob[12..])
        .map_err(|_| AppError::Unauthorized)?; // wrong passphrase/token
    pt.as_slice().try_into()
        .map_err(|_| AppError::InternalServerError("unwrap: bad key length".into()))
}

/// A private key held in memory. Manual Debug so it can never leak into logs
/// via the `#[derive(Debug)]` on AuthUser.
#[derive(Clone)]
pub struct PrivacySecret(pub [u8; 32]);

impl std::fmt::Debug for PrivacySecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrivacySecret(REDACTED)")
    }
}

#[cfg(test)]
pub(crate) fn test_keypair() -> ([u8; 32], [u8; 32]) {
    let sk = StaticSecret::random_from_rng(OsRng);
    let pk = PublicKey::from(&sk);
    (sk.to_bytes(), pk.to_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_unseal_roundtrip() {
        let (sk, pk) = test_keypair();
        let blob = seal(&pk, b"STARBUCKS #1234 AUSTIN TX").unwrap();
        assert_eq!(unseal(&sk, &blob).unwrap(), "STARBUCKS #1234 AUSTIN TX");
    }

    #[test]
    fn unseal_fails_with_wrong_key() {
        let (_, pk) = test_keypair();
        let (other_sk, _) = test_keypair();
        let blob = seal(&pk, b"secret").unwrap();
        assert!(unseal(&other_sk, &blob).is_err());
    }

    #[test]
    fn seal_is_randomized() {
        let (_, pk) = test_keypair();
        assert_ne!(seal(&pk, b"same").unwrap(), seal(&pk, b"same").unwrap());
    }

    #[test]
    fn passphrase_wrap_roundtrip_and_wrong_passphrase() {
        let (sk, _) = test_keypair();
        let salt = [7u8; 16];
        let kek = derive_passphrase_kek("correct horse battery", &salt).unwrap();
        let wrapped = wrap_secret(&kek, &sk).unwrap();
        assert_eq!(unwrap_secret(&kek, &wrapped).unwrap(), sk);

        let bad = derive_passphrase_kek("wrong passphrase", &salt).unwrap();
        assert!(unwrap_secret(&bad, &wrapped).is_err());
    }

    #[test]
    fn token_kek_wrap_roundtrip() {
        let (sk, _) = test_keypair();
        let kek = derive_token_kek("wa_pat_abc123");
        let wrapped = wrap_secret(&kek, &sk).unwrap();
        assert_eq!(unwrap_secret(&kek, &wrapped).unwrap(), sk);
        assert!(unwrap_secret(&derive_token_kek("wa_pat_other"), &wrapped).is_err());
    }
}
