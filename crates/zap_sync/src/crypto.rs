//! AES-256-GCM encryption/decryption for synced secrets.
//!
//! The data-encryption key is derived from the user's sync token with
//! **Argon2id** (a memory-hard password KDF) over a random per-message salt,
//! rather than a bare double-SHA-256. The salt is generated fresh per `encrypt`
//! and stored alongside the ciphertext, so:
//!   - a low-entropy token is no longer cheap to brute-force against the public
//!     gist ciphertext (Argon2id adds a memory/time work factor), and
//!   - identical tokens no longer derive identical keys across messages/gists.
//!
//! Wire format (base64): `salt (16 bytes) || nonce (12 bytes) || ciphertext+tag`.
//!
//! Note: the key is still derived from the sync token, so this does not decouple
//! encryption from gist-access (a full fix would use an independent passphrase);
//! it closes the "not a real KDF / unsalted / brute-forceable" weakness.

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use thiserror::Error;

/// Length of the random per-message KDF salt, in bytes.
const SALT_LEN: usize = 16;
/// Length of the AES-GCM nonce, in bytes.
const NONCE_LEN: usize = 12;

/// Encryption/decryption error
#[derive(Debug, Error)]
pub enum CryptoError {
    /// Encryption failed
    #[error("Encryption failed: {0}")]
    Encrypt(String),
    /// Decryption failed
    #[error("Decryption failed: {0}")]
    Decrypt(String),
}

/// Derives a 32-byte key from the user token and a salt using Argon2id.
fn derive_key(token: &str, salt: &[u8]) -> Result<[u8; 32], argon2::Error> {
    let mut key = [0u8; 32];
    Argon2::default().hash_password_into(token.as_bytes(), salt, &mut key)?;
    Ok(key)
}

/// Encrypts plaintext using AES-256-GCM with an Argon2id-derived key.
///
/// Returns a Base64-encoded `salt || nonce || ciphertext`. The key is derived
/// from the user token and a fresh random salt embedded in the output.
pub fn encrypt(token: &str, plaintext: &str) -> Result<String, CryptoError> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);

    let key = derive_key(token, &salt).map_err(|e| CryptoError::Encrypt(e.to_string()))?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|e| CryptoError::Encrypt(e.to_string()))?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| CryptoError::Encrypt(e.to_string()))?;

    let mut combined = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    combined.extend_from_slice(&salt);
    combined.extend_from_slice(&nonce);
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(&combined))
}

/// Decrypts a Base64-encoded `salt || nonce || ciphertext`.
///
/// Re-derives the key from the user token and the embedded salt (must match the
/// token used for encryption).
pub fn decrypt(token: &str, encoded: &str) -> Result<String, CryptoError> {
    let combined = BASE64
        .decode(encoded)
        .map_err(|e| CryptoError::Decrypt(e.to_string()))?;
    if combined.len() < SALT_LEN + NONCE_LEN {
        return Err(CryptoError::Decrypt("Data too short".to_string()));
    }

    let (salt, rest) = combined.split_at(SALT_LEN);
    let (nonce_bytes, ciphertext) = rest.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = derive_key(token, salt).map_err(|e| CryptoError::Decrypt(e.to_string()))?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|e| CryptoError::Decrypt(e.to_string()))?;
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| CryptoError::Decrypt(e.to_string()))?;
    String::from_utf8(plaintext).map_err(|e| CryptoError::Decrypt(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TOKEN: &str = "test_token_for_crypto";
    /// Fixed salt for the direct `derive_key` unit tests (production uses a
    /// random per-message salt via `encrypt`).
    const TEST_SALT: [u8; SALT_LEN] = [0x42; SALT_LEN];

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let plaintext = "my_secret_password";
        let encrypted = encrypt(TEST_TOKEN, plaintext).unwrap();
        let decrypted = decrypt(TEST_TOKEN, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_same_token_same_result() {
        let encrypted = encrypt(TEST_TOKEN, "secret").unwrap();
        let decrypted = decrypt(TEST_TOKEN, &encrypted).unwrap();
        assert_eq!(decrypted, "secret");
    }

    #[test]
    fn test_empty_string() {
        let encrypted = encrypt(TEST_TOKEN, "").unwrap();
        let decrypted = decrypt(TEST_TOKEN, &encrypted).unwrap();
        assert_eq!(decrypted, "");
    }

    #[test]
    fn test_decrypt_invalid_base64() {
        let result = decrypt(TEST_TOKEN, "!!!not-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_data_too_short() {
        // Shorter than salt + nonce, must be rejected before AES-GCM.
        let short = BASE64.encode([0u8; 8]);
        let result = decrypt(TEST_TOKEN, &short);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_wrong_ciphertext() {
        // salt + nonce present but no valid ciphertext/tag.
        let data = vec![0u8; SALT_LEN + NONCE_LEN + 1];
        let encoded = BASE64.encode(&data);
        let result = decrypt(TEST_TOKEN, &encoded);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_produces_different_ciphertexts() {
        let plaintext = "same_input";
        let e1 = encrypt(TEST_TOKEN, plaintext).unwrap();
        let e2 = encrypt(TEST_TOKEN, plaintext).unwrap();
        // Different random salt + nonce should produce different ciphertexts...
        assert_ne!(e1, e2);
        // ...but both should decrypt correctly.
        assert_eq!(decrypt(TEST_TOKEN, &e1).unwrap(), plaintext);
        assert_eq!(decrypt(TEST_TOKEN, &e2).unwrap(), plaintext);
    }

    #[test]
    fn test_encrypt_unicode() {
        let plaintext = "你好世界🌍";
        let encrypted = encrypt(TEST_TOKEN, plaintext).unwrap();
        let decrypted = decrypt(TEST_TOKEN, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_long_string() {
        let plaintext = "a".repeat(10_000);
        let encrypted = encrypt(TEST_TOKEN, &plaintext).unwrap();
        let decrypted = decrypt(TEST_TOKEN, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_tokens_produce_different_keys() {
        let plaintext = "secret_data";
        let encrypted = encrypt("token_alpha", plaintext).unwrap();
        let result = decrypt("token_beta", &encrypted);
        assert!(result.is_err(), "decrypting with a different token should fail");
    }

    #[test]
    fn test_empty_token_roundtrip() {
        let plaintext = "secret_data";
        let encrypted = encrypt("", plaintext).unwrap();
        let decrypted = decrypt("", &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_derive_key_deterministic() {
        let key1 = derive_key("my_token", &TEST_SALT).unwrap();
        let key2 = derive_key("my_token", &TEST_SALT).unwrap();
        assert_eq!(key1, key2, "the same token + salt should derive the same key");
    }

    #[test]
    fn test_derive_key_different_tokens() {
        let key1 = derive_key("token_a", &TEST_SALT).unwrap();
        let key2 = derive_key("token_b", &TEST_SALT).unwrap();
        assert_ne!(key1, key2, "different tokens should derive different keys");
    }

    #[test]
    fn test_derive_key_different_salts() {
        let key1 = derive_key("my_token", &[0x01; SALT_LEN]).unwrap();
        let key2 = derive_key("my_token", &[0x02; SALT_LEN]).unwrap();
        assert_ne!(key1, key2, "different salts should derive different keys");
    }

    #[test]
    fn test_decrypt_exact_header_size() {
        // Exactly salt + nonce (no ciphertext); AES-GCM decryption must fail.
        let data = vec![0u8; SALT_LEN + NONCE_LEN];
        let encoded = BASE64.encode(&data);
        let result = decrypt(TEST_TOKEN, &encoded);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_tampered_ciphertext() {
        let encrypted = encrypt(TEST_TOKEN, "hello").unwrap();
        let mut combined = BASE64.decode(&encrypted).unwrap();
        // Tamper with a byte in the ciphertext region (after salt + nonce).
        let idx = SALT_LEN + NONCE_LEN;
        combined[idx] ^= 0xFF;
        let tampered = BASE64.encode(&combined);
        let result = decrypt(TEST_TOKEN, &tampered);
        assert!(result.is_err(), "the tampered ciphertext should fail to decrypt");
    }

    #[test]
    fn test_decrypt_tampered_salt() {
        // Flipping a salt byte changes the derived key, so authentication fails.
        let encrypted = encrypt(TEST_TOKEN, "hello").unwrap();
        let mut combined = BASE64.decode(&encrypted).unwrap();
        combined[0] ^= 0xFF;
        let tampered = BASE64.encode(&combined);
        let result = decrypt(TEST_TOKEN, &tampered);
        assert!(result.is_err(), "tampering the salt should fail to decrypt");
    }

    #[test]
    fn test_crypto_error_display_encrypt() {
        let err = CryptoError::Encrypt("something went wrong".to_string());
        assert_eq!(format!("{err}"), "Encryption failed: something went wrong");
    }

    #[test]
    fn test_crypto_error_display_decrypt() {
        let err = CryptoError::Decrypt("bad data".to_string());
        assert_eq!(format!("{err}"), "Decryption failed: bad data");
    }

    #[test]
    fn test_encrypt_with_special_char_token() {
        let token = "tok\0en\nwith\tspecial";
        let plaintext = "secret";
        let encrypted = encrypt(token, plaintext).unwrap();
        let decrypted = decrypt(token, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_whitespace_token() {
        let token = "   ";
        let plaintext = "data";
        let encrypted = encrypt(token, plaintext).unwrap();
        let decrypted = decrypt(token, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_very_long_token() {
        let token = "x".repeat(10_000);
        let plaintext = "short";
        let encrypted = encrypt(&token, plaintext).unwrap();
        let decrypted = decrypt(&token, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
