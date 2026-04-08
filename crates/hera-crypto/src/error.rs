use thiserror::Error;

/// Centralizes crypto failures behind sanitized variants because error messages
/// are part of the security boundary and must never leak raw key material.
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("failed to encrypt key material")]
    KeyEncryptionFailed,
    #[error("failed to decrypt key material")]
    KeyDecryptionFailed,
    #[error("failed to sign report artifact")]
    SigningFailed,
    #[error("kms unavailable: {0}")]
    KmsUnavailable(String),
    #[error("invalid key material")]
    InvalidKeyMaterial,
}
