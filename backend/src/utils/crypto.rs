//! Authenticated encryption for stored connection credentials.
//!
//! Format of an encrypted blob:
//!
//! ```text
//! [version: u8 = 1][key id: u8][nonce: 12 bytes][ciphertext || 16-byte GCM tag]
//! ```
//!
//! Callers pass associated data (AAD) that binds a ciphertext to the row it
//! belongs to (owner + connection id), so a blob copied to another row fails
//! authentication instead of decrypting.

use std::{collections::HashMap, fmt};

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use thiserror::Error;
use zeroize::Zeroizing;

const FORMAT_VERSION: u8 = 1;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const HEADER_LEN: usize = 2 + NONCE_LEN;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("invalid encryption key: {0}")]
    InvalidKey(String),
    #[error("ciphertext is malformed")]
    Malformed,
    #[error("ciphertext uses unsupported format version {0}")]
    UnsupportedVersion(u8),
    #[error("no key available for key id {0}")]
    UnknownKey(u8),
    #[error("ciphertext failed authentication")]
    Authentication,
    #[error("encryption failed")]
    Encryption,
}

/// A 256-bit key tagged with an id, so keys can be rotated without
/// re-encrypting every row at once.
#[derive(Clone)]
pub struct EncryptionKey {
    pub id: u8,
    bytes: Zeroizing<[u8; 32]>,
}

impl EncryptionKey {
    pub fn from_base64(id: u8, encoded: &str) -> Result<Self, CryptoError> {
        let decoded = Zeroizing::new(
            BASE64
                .decode(encoded.trim())
                .map_err(|_| CryptoError::InvalidKey("not valid base64".into()))?,
        );
        if decoded.len() != 32 {
            return Err(CryptoError::InvalidKey(format!(
                "expected 32 bytes, got {}",
                decoded.len()
            )));
        }
        let mut bytes = Zeroizing::new([0u8; 32]);
        bytes.copy_from_slice(&decoded);
        Ok(Self { id, bytes })
    }
}

impl fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncryptionKey")
            .field("id", &self.id)
            .field("bytes", &"[redacted]")
            .finish()
    }
}

/// Encrypts with the primary key; decrypts with any configured key.
pub struct CredentialCipher {
    primary_id: u8,
    ciphers: HashMap<u8, Aes256Gcm>,
}

impl fmt::Debug for CredentialCipher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CredentialCipher")
            .field("primary_id", &self.primary_id)
            .field("key_ids", &self.ciphers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl CredentialCipher {
    pub fn new(primary: &EncryptionKey, previous: &[EncryptionKey]) -> Self {
        let mut ciphers = HashMap::new();
        for key in previous.iter().chain(std::iter::once(primary)) {
            let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.bytes.as_slice()));
            ciphers.insert(key.id, cipher);
        }
        Self {
            primary_id: primary.id,
            ciphers,
        }
    }

    pub fn encrypt(&self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let cipher = self
            .ciphers
            .get(&self.primary_id)
            .ok_or(CryptoError::UnknownKey(self.primary_id))?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, Payload { msg: plaintext, aad })
            .map_err(|_| CryptoError::Encryption)?;

        let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
        out.push(FORMAT_VERSION);
        out.push(self.primary_id);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    pub fn decrypt(&self, blob: &[u8], aad: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        if blob.len() < HEADER_LEN + TAG_LEN {
            return Err(CryptoError::Malformed);
        }
        if blob[0] != FORMAT_VERSION {
            return Err(CryptoError::UnsupportedVersion(blob[0]));
        }
        let key_id = blob[1];
        let cipher = self
            .ciphers
            .get(&key_id)
            .ok_or(CryptoError::UnknownKey(key_id))?;
        let nonce = Nonce::from_slice(&blob[2..HEADER_LEN]);
        cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &blob[HEADER_LEN..],
                    aad,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| CryptoError::Authentication)
    }

    /// Whether a blob was produced with a non-primary key and should be
    /// re-encrypted.
    pub fn needs_rotation(&self, blob: &[u8]) -> bool {
        blob.get(1).is_some_and(|id| *id != self.primary_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: u8, byte: u8) -> EncryptionKey {
        EncryptionKey::from_base64(id, &BASE64.encode([byte; 32])).unwrap()
    }

    #[test]
    fn round_trip() {
        let cipher = CredentialCipher::new(&key(1, 7), &[]);
        let blob = cipher.encrypt(b"s3cret", b"row-1").unwrap();
        assert_eq!(blob[0], FORMAT_VERSION);
        assert_eq!(blob[1], 1);
        assert_eq!(&*cipher.decrypt(&blob, b"row-1").unwrap(), b"s3cret");
    }

    #[test]
    fn ciphertext_is_not_plaintext_and_nonces_differ() {
        let cipher = CredentialCipher::new(&key(1, 7), &[]);
        let a = cipher.encrypt(b"password-password", b"").unwrap();
        let b = cipher.encrypt(b"password-password", b"").unwrap();
        assert_ne!(a, b, "random nonce must make ciphertexts differ");
        assert!(!a.windows(8).any(|w| w == b"password"));
    }

    #[test]
    fn wrong_aad_fails_authentication() {
        let cipher = CredentialCipher::new(&key(1, 7), &[]);
        let blob = cipher.encrypt(b"s3cret", b"row-1").unwrap();
        assert_eq!(
            cipher.decrypt(&blob, b"row-2").unwrap_err(),
            CryptoError::Authentication
        );
    }

    #[test]
    fn tampering_is_detected() {
        let cipher = CredentialCipher::new(&key(1, 7), &[]);
        let mut blob = cipher.encrypt(b"s3cret", b"aad").unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0x01;
        assert_eq!(
            cipher.decrypt(&blob, b"aad").unwrap_err(),
            CryptoError::Authentication
        );
    }

    #[test]
    fn wrong_key_fails() {
        let a = CredentialCipher::new(&key(1, 7), &[]);
        let b = CredentialCipher::new(&key(1, 8), &[]);
        let blob = a.encrypt(b"s3cret", b"aad").unwrap();
        assert_eq!(
            b.decrypt(&blob, b"aad").unwrap_err(),
            CryptoError::Authentication
        );
    }

    #[test]
    fn rotation_decrypts_with_previous_key() {
        let old = CredentialCipher::new(&key(1, 7), &[]);
        let blob = old.encrypt(b"s3cret", b"aad").unwrap();
        let rotated = CredentialCipher::new(&key(2, 9), &[key(1, 7)]);
        assert!(rotated.needs_rotation(&blob));
        assert_eq!(&*rotated.decrypt(&blob, b"aad").unwrap(), b"s3cret");
        let fresh = rotated.encrypt(b"s3cret", b"aad").unwrap();
        assert_eq!(fresh[1], 2);
        assert!(!rotated.needs_rotation(&fresh));
    }

    #[test]
    fn malformed_inputs_are_rejected() {
        let cipher = CredentialCipher::new(&key(1, 7), &[]);
        assert_eq!(cipher.decrypt(&[1, 1, 0], b"").unwrap_err(), CryptoError::Malformed);
        let mut blob = cipher.encrypt(b"x", b"").unwrap();
        blob[0] = 9;
        assert_eq!(
            cipher.decrypt(&blob, b"").unwrap_err(),
            CryptoError::UnsupportedVersion(9)
        );
        blob[0] = 1;
        blob[1] = 42;
        assert_eq!(cipher.decrypt(&blob, b"").unwrap_err(), CryptoError::UnknownKey(42));
    }

    #[test]
    fn key_validation_and_debug_redaction() {
        assert!(EncryptionKey::from_base64(1, "not base64!").is_err());
        assert!(EncryptionKey::from_base64(1, &BASE64.encode([0u8; 16])).is_err());
        let k = key(3, 1);
        assert!(!format!("{k:?}").contains("1, 1"));
        assert!(format!("{k:?}").contains("redacted"));
    }
}
