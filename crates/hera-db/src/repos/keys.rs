use sqlx::FromRow;
use uuid::Uuid;

use hera_types::ChainId;

use crate::{chain_to_db, parse_chain, DbError, DbPool};

/// Holds the encrypted viewing-key record retrieved from Postgres so raw key
/// decryption can happen only inside the crypto boundary.
#[derive(Debug, Clone)]
pub struct StoredViewingKey {
    pub id: Uuid,
    pub case_id: Uuid,
    pub chain: ChainId,
    pub key_ref: String,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub encrypted_data_key: Vec<u8>,
    pub birthday_height: Option<u64>,
}

/// Persists encrypted viewing keys while ensuring database callers only handle
/// ciphertext and metadata, never plaintext key material.
pub struct ViewKeyRepo<'a> {
    pool: &'a DbPool,
}

impl<'a> ViewKeyRepo<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool }
    }

    /// Upserts the encrypted viewing key for a case so later imports replace the
    /// prior ciphertext atomically without ever storing plaintext.
    pub async fn store_encrypted_view_key(
        &self,
        case_id: Uuid,
        chain: ChainId,
        key_ref: &str,
        ciphertext: &[u8],
        nonce: &[u8],
        encrypted_data_key: &[u8],
        birthday_height: Option<u64>,
    ) -> Result<StoredViewingKey, DbError> {
        let row = sqlx::query_as::<_, ViewKeyRow>(
            r#"
            INSERT INTO view_keys (
                case_id, chain, key_ref, ciphertext, nonce, encrypted_data_key, birthday_height
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (case_id)
            DO UPDATE SET
                chain = EXCLUDED.chain,
                key_ref = EXCLUDED.key_ref,
                ciphertext = EXCLUDED.ciphertext,
                nonce = EXCLUDED.nonce,
                encrypted_data_key = EXCLUDED.encrypted_data_key,
                birthday_height = EXCLUDED.birthday_height
            RETURNING id, case_id, chain, key_ref, ciphertext, nonce, encrypted_data_key, birthday_height
            "#,
        )
        .bind(case_id)
        .bind(chain_to_db(&chain))
        .bind(key_ref)
        .bind(ciphertext)
        .bind(nonce)
        .bind(encrypted_data_key)
        .bind(
            birthday_height
                .map(|height| {
                    i64::try_from(height)
                        .map_err(|_| DbError::InvalidData("birthday height overflow".into()))
                })
                .transpose()?,
        )
        .fetch_one(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.try_into_view_key()
    }

    /// Loads the encrypted viewing key for one case so the orchestrator can
    /// decrypt it inside the isolated crypto layer when needed.
    pub async fn get_view_key_for_case(
        &self,
        case_id: Uuid,
    ) -> Result<Option<StoredViewingKey>, DbError> {
        let row = sqlx::query_as::<_, ViewKeyRow>(
            r#"
            SELECT id, case_id, chain, key_ref, ciphertext, nonce, encrypted_data_key, birthday_height
            FROM view_keys
            WHERE case_id = $1
            "#,
        )
        .bind(case_id)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.map(ViewKeyRow::try_into_view_key).transpose()
    }
}

#[derive(Debug, FromRow)]
struct ViewKeyRow {
    id: Uuid,
    case_id: Uuid,
    chain: String,
    key_ref: String,
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
    encrypted_data_key: Vec<u8>,
    birthday_height: Option<i64>,
}

impl ViewKeyRow {
    fn try_into_view_key(self) -> Result<StoredViewingKey, DbError> {
        Ok(StoredViewingKey {
            id: self.id,
            case_id: self.case_id,
            chain: parse_chain(&self.chain)?,
            key_ref: self.key_ref,
            ciphertext: self.ciphertext,
            nonce: self.nonce,
            encrypted_data_key: self.encrypted_data_key,
            birthday_height: self
                .birthday_height
                .map(|height| {
                    u64::try_from(height)
                        .map_err(|_| DbError::InvalidData("negative birthday height".into()))
                })
                .transpose()?,
        })
    }
}
