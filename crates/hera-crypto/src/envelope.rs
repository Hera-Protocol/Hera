use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng},
    Aes256Gcm,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{error::CryptoError, kms::KmsClient};

const AES_GCM_NONCE_BYTES: usize = 12;
const AES_GCM_KEY_BYTES: usize = 32;

/// Stores the encrypted viewing key and the material required to unwrap it later
/// without ever persisting the plaintext itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct EncryptedViewingKey {
    /// Holds the AES-GCM ciphertext for the viewing key bytes.
    pub ciphertext: Vec<u8>,
    /// Stores the random nonce used for AES-GCM so the ciphertext can be
    /// decrypted deterministically later.
    pub nonce: Vec<u8>,
    /// Records which KMS master key protected this payload so key rotation and
    /// re-wrap workflows can target the correct source material later.
    pub key_ref: String,
    /// Stores the wrapped data-encryption key because reversible envelope
    /// encryption requires both the ciphertext and the encrypted data key.
    pub encrypted_data_key: Vec<u8>,
}

/// Encrypts a viewing key with a one-time data key, then asks KMS to wrap that
/// data key. zeroize ensures the key doesn't linger in memory after use.
pub async fn encrypt_viewing_key(
    kms: &dyn KmsClient,
    key_ref: impl Into<String>,
    plaintext_viewing_key: Vec<u8>,
) -> Result<EncryptedViewingKey, CryptoError> {
    let plaintext_viewing_key = Zeroizing::new(plaintext_viewing_key);

    // Envelope encryption limits the blast radius of any one wrapped secret by
    // using a fresh symmetric key per viewing-key payload.
    let mut data_key_bytes = [0u8; AES_GCM_KEY_BYTES];
    OsRng.fill_bytes(&mut data_key_bytes);
    let data_key = Zeroizing::new(data_key_bytes);

    let encrypted_data_key = kms.encrypt_data_key(data_key.as_ref()).await?;
    let cipher = Aes256Gcm::new_from_slice(data_key.as_ref())
        .map_err(|_| CryptoError::InvalidKeyMaterial)?;

    // AES-GCM uses a random 96-bit nonce here so every viewing-key encryption is
    // unique even when the wrapped plaintext repeats.
    let mut nonce_bytes = [0u8; AES_GCM_NONCE_BYTES];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext_viewing_key.as_slice())
        .map_err(|_| CryptoError::KeyEncryptionFailed)?;

    Ok(EncryptedViewingKey {
        ciphertext,
        nonce: nonce_bytes.to_vec(),
        key_ref: key_ref.into(),
        encrypted_data_key,
    })
}

/// Decrypts a stored viewing key by first unwrapping its data key through KMS
/// and then applying AES-256-GCM with the recorded nonce.
pub async fn decrypt_viewing_key(
    kms: &dyn KmsClient,
    encrypted_viewing_key: &EncryptedViewingKey,
) -> Result<Vec<u8>, CryptoError> {
    if encrypted_viewing_key.nonce.len() != AES_GCM_NONCE_BYTES {
        return Err(CryptoError::InvalidKeyMaterial);
    }

    let unwrapped_data_key = kms
        .decrypt_data_key(&encrypted_viewing_key.encrypted_data_key)
        .await?;
    let data_key = Zeroizing::new(unwrapped_data_key);
    let cipher = Aes256Gcm::new_from_slice(data_key.as_ref())
        .map_err(|_| CryptoError::InvalidKeyMaterial)?;
    let nonce = aes_gcm::Nonce::from_slice(&encrypted_viewing_key.nonce);

    let plaintext = cipher
        .decrypt(nonce, encrypted_viewing_key.ciphertext.as_slice())
        .map_err(|_| CryptoError::KeyDecryptionFailed)?;
    Ok(plaintext)
}
