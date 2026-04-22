use chrono::{DateTime, Utc};
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

/// Groups encrypted viewing-key fields into one persistence payload so callers
/// cannot mix ciphertext components across separate parameters.
#[derive(Debug, Clone)]
pub struct EncryptedViewKeyRecord<'a> {
    pub case_id: Uuid,
    pub chain: ChainId,
    pub key_ref: &'a str,
    pub ciphertext: &'a [u8],
    pub nonce: &'a [u8],
    pub encrypted_data_key: &'a [u8],
    pub birthday_height: Option<u64>,
}

/// Provides a workspace-level summary of imported viewing keys without ever
/// exposing ciphertext or plaintext key material to API consumers.
#[derive(Debug, Clone)]
pub struct WorkspaceViewKeyRecord {
    pub id: Uuid,
    pub case_id: Uuid,
    pub chain: ChainId,
    pub key_ref: String,
    pub birthday_height: Option<u64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
        record: EncryptedViewKeyRecord<'_>,
    ) -> Result<StoredViewingKey, DbError> {
        let EncryptedViewKeyRecord {
            case_id,
            chain,
            key_ref,
            ciphertext,
            nonce,
            encrypted_data_key,
            birthday_height,
        } = record;

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

    /// Lists encrypted viewing keys visible to one workspace so the API can
    /// render key inventory pages without exposing secret material.
    pub async fn list_view_keys_for_workspace(
        &self,
        workspace_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkspaceViewKeyRecord>, DbError> {
        let rows = sqlx::query_as::<_, WorkspaceViewKeyRow>(
            r#"
            SELECT
                view_keys.id,
                view_keys.case_id,
                view_keys.chain,
                view_keys.key_ref,
                view_keys.birthday_height,
                view_keys.created_at,
                view_keys.updated_at
            FROM view_keys
            INNER JOIN cases ON cases.id = view_keys.case_id
            WHERE cases.workspace_id = $1
            ORDER BY view_keys.created_at DESC, view_keys.id DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(workspace_id)
        .bind(
            i64::try_from(limit)
                .map_err(|_| DbError::InvalidData("view key list limit overflow".into()))?,
        )
        .bind(
            i64::try_from(offset)
                .map_err(|_| DbError::InvalidData("view key list offset overflow".into()))?,
        )
        .fetch_all(self.pool)
        .await
        .map_err(DbError::Query)?;

        rows.into_iter()
            .map(WorkspaceViewKeyRow::try_into_workspace_view_key)
            .collect()
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

#[derive(Debug, FromRow)]
struct WorkspaceViewKeyRow {
    id: Uuid,
    case_id: Uuid,
    chain: String,
    key_ref: String,
    birthday_height: Option<i64>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
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

impl WorkspaceViewKeyRow {
    fn try_into_workspace_view_key(self) -> Result<WorkspaceViewKeyRecord, DbError> {
        Ok(WorkspaceViewKeyRecord {
            id: self.id,
            case_id: self.case_id,
            chain: parse_chain(&self.chain)?,
            key_ref: self.key_ref,
            birthday_height: self
                .birthday_height
                .map(|height| {
                    u64::try_from(height)
                        .map_err(|_| DbError::InvalidData("negative birthday height".into()))
                })
                .transpose()?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}
