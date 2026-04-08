use std::env;

use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng},
    Aes256Gcm,
};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use zeroize::Zeroizing;

use crate::error::CryptoError;

const AES_GCM_NONCE_BYTES: usize = 12;
const AES_GCM_KEY_BYTES: usize = 32;

/// Abstracts the backing key-management system so the rest of Hera can depend
/// on key wrapping semantics without depending on any one cloud vendor.
#[async_trait]
pub trait KmsClient: Send + Sync {
    /// Wraps a freshly generated data key before it is stored alongside an
    /// encrypted viewing key.
    async fn encrypt_data_key(&self, plaintext_key: &[u8]) -> Result<Vec<u8>, CryptoError>;

    /// Unwraps a stored data key so the caller can decrypt the associated
    /// viewing key inside the isolated crypto boundary.
    async fn decrypt_data_key(&self, ciphertext_key: &[u8]) -> Result<Vec<u8>, CryptoError>;
}

/// Implements a fixed local-development KMS using AES-256-GCM and an env-sourced
/// master key. This implementation is for local development ONLY. Never ship to prod.
pub struct LocalDevKms {
    key_id: String,
    master_key: Zeroizing<[u8; AES_GCM_KEY_BYTES]>,
}

impl LocalDevKms {
    /// Builds the local-development KMS from explicit inputs so callers can wire
    /// configuration deliberately instead of relying on ambient globals.
    pub fn new(key_id: impl Into<String>, base64_key: &str) -> Result<Self, CryptoError> {
        let decoded = STANDARD
            .decode(base64_key)
            .map_err(|_| CryptoError::InvalidKeyMaterial)?;
        let key_bytes: [u8; AES_GCM_KEY_BYTES] = decoded
            .try_into()
            .map_err(|_| CryptoError::InvalidKeyMaterial)?;

        Ok(Self {
            key_id: key_id.into(),
            master_key: Zeroizing::new(key_bytes),
        })
    }

    /// Reads the local-development KMS settings from the environment explicitly
    /// at startup so secret loading is centralized and easy to audit.
    pub fn from_env() -> Result<Self, CryptoError> {
        let key_id = env::var("KMS_KEY_ID")
            .map_err(|_| CryptoError::KmsUnavailable("missing KMS_KEY_ID".into()))?;
        let base64_key = env::var("DEV_KMS_KEY_BASE64")
            .map_err(|_| CryptoError::KmsUnavailable("missing DEV_KMS_KEY_BASE64".into()))?;

        Self::new(key_id, &base64_key)
    }

    /// Exposes the configured key identifier so encrypted payloads can retain
    /// stable provenance for future key rotation workflows.
    pub fn key_id(&self) -> &str {
        &self.key_id
    }
}

#[async_trait]
impl KmsClient for LocalDevKms {
    async fn encrypt_data_key(&self, plaintext_key: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let cipher = Aes256Gcm::new_from_slice(self.master_key.as_ref())
            .map_err(|_| CryptoError::InvalidKeyMaterial)?;

        // AES-GCM requires a unique nonce per encryption under the same key, so
        // we generate a fresh random 96-bit nonce and prefix it for later unwrap.
        let mut nonce_bytes = [0u8; AES_GCM_NONCE_BYTES];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext_key)
            .map_err(|_| CryptoError::KeyEncryptionFailed)?;

        let mut wrapped = Vec::with_capacity(AES_GCM_NONCE_BYTES + ciphertext.len());
        wrapped.extend_from_slice(&nonce_bytes);
        wrapped.extend_from_slice(&ciphertext);
        Ok(wrapped)
    }

    async fn decrypt_data_key(&self, ciphertext_key: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if ciphertext_key.len() <= AES_GCM_NONCE_BYTES {
            return Err(CryptoError::InvalidKeyMaterial);
        }

        let cipher = Aes256Gcm::new_from_slice(self.master_key.as_ref())
            .map_err(|_| CryptoError::InvalidKeyMaterial)?;
        let (nonce_bytes, ciphertext) = ciphertext_key.split_at(AES_GCM_NONCE_BYTES);
        let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CryptoError::KeyDecryptionFailed)?;
        Ok(plaintext)
    }
}

/// Placeholder for the real AWS KMS implementation. TODO: wire this to the AWS
/// SDK once the cloud-backed key management path is introduced.
pub struct AwsKms;

#[async_trait]
impl KmsClient for AwsKms {
    async fn encrypt_data_key(&self, _plaintext_key: &[u8]) -> Result<Vec<u8>, CryptoError> {
        Err(CryptoError::KmsUnavailable(
            "aws kms client is not implemented yet".into(),
        ))
    }

    async fn decrypt_data_key(&self, _ciphertext_key: &[u8]) -> Result<Vec<u8>, CryptoError> {
        Err(CryptoError::KmsUnavailable(
            "aws kms client is not implemented yet".into(),
        ))
    }
}
