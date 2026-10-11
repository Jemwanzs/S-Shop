//! Encryption at rest for credentials businesses give S'Shop (roadmap 91: M-Pesa consumer secret, passkey, callback
//! token). AES-256-GCM with a random 96-bit nonce per value, stored as `v1:<base64(nonce || ciphertext)>`.
//!
//! The key is `DATA_ENCRYPTION_KEY` (32 bytes as base64 or 64 hex characters). Without it, a key is derived from
//! `JWT_SECRET` (SHA-256 with a fixed context), so setting a dedicated key later needs a re-entry of the credentials;
//! rotating `JWT_SECRET` without a dedicated key does too. Values never leave the server decrypted except to Safaricom.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

/// The data key; never printed (configuration is debug-logged).
#[derive(Clone)]
pub struct DataKey(pub [u8; 32]);
impl std::fmt::Debug for DataKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DataKey(<redacted>)")
    }
}

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// The 32-byte data key from `DATA_ENCRYPTION_KEY`, or derived from the JWT secret.
pub fn key_from_env(dedicated: Option<&str>, jwt_secret: &str) -> anyhow::Result<[u8; 32]> {
    if let Some(k) = dedicated.map(str::trim).filter(|k| !k.is_empty()) {
        let bytes = if k.len() == 64 && k.chars().all(|c| c.is_ascii_hexdigit()) {
            (0..32).map(|i| u8::from_str_radix(&k[i * 2..i * 2 + 2], 16)).collect::<Result<Vec<u8>, _>>()?
        } else {
            B64.decode(k)?
        };
        return bytes.try_into().map_err(|_| anyhow::anyhow!("DATA_ENCRYPTION_KEY must be 32 bytes (base64 or 64 hex characters)"));
    }
    let mut h = Sha256::new();
    h.update(b"sshop/credentials/v1\0");
    h.update(jwt_secret.as_bytes());
    Ok(h.finalize().into())
}

pub fn encrypt(key: &[u8; 32], plain: &str) -> AppResult<String> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = cipher.encrypt(Nonce::from_slice(&nonce), plain.as_bytes()).map_err(|_| AppError::Other(anyhow::anyhow!("encryption failed")))?;
    let mut out = nonce.to_vec();
    out.extend(ct);
    Ok(format!("v1:{}", B64.encode(out)))
}

pub fn decrypt(key: &[u8; 32], stored: &str) -> AppResult<String> {
    let unreadable = || AppError::Rule("Saved credentials can no longer be read — enter them again in Settings → M-Pesa".into());
    let raw = B64.decode(stored.strip_prefix("v1:").ok_or_else(unreadable)?).map_err(|_| unreadable())?;
    if raw.len() < 13 {
        return Err(unreadable());
    }
    let (nonce, ct) = raw.split_at(12);
    let plain = Aes256Gcm::new(key.into()).decrypt(Nonce::from_slice(nonce), ct).map_err(|_| unreadable())?;
    String::from_utf8(plain).map_err(|_| unreadable())
}

/// SHA-256 hex, for looking up a secret without storing it in the clear.
pub fn digest(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 hex of bytes (an uploaded file's fingerprint).
pub fn digest_bytes(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// A random URL-safe token (32 bytes).
pub fn random_token() -> String {
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_tamper_detection() {
        let k = key_from_env(None, "x".repeat(40).as_str()).unwrap();
        let a = encrypt(&k, "sample-credential-value-0123456789abcdef").unwrap();
        let b = encrypt(&k, "sample-credential-value-0123456789abcdef").unwrap();
        assert_ne!(a, b, "a fresh nonce every time");
        assert!(!a.contains("sample-credential"), "never stored in the clear");
        assert_eq!(decrypt(&k, &a).unwrap(), "sample-credential-value-0123456789abcdef");
        let mut bad = a.clone();
        bad.replace_range(10..11, if &a[10..11] == "A" { "B" } else { "A" });
        assert!(decrypt(&k, &bad).is_err(), "tampering detected");
        let other = key_from_env(None, "y".repeat(40).as_str()).unwrap();
        assert!(decrypt(&other, &a).is_err(), "another key cannot read it");
    }

    #[test]
    fn dedicated_key_formats() {
        let hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        assert_eq!(key_from_env(Some(hex), "j").unwrap()[1], 0x11);
        let b64 = B64.encode([7u8; 32]);
        assert_eq!(key_from_env(Some(&b64), "j").unwrap(), [7u8; 32]);
        assert!(key_from_env(Some("short"), "j").is_err());
    }
}
