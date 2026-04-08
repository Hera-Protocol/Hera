use sqlx::FromRow;
use uuid::Uuid;

use hera_types::ChainId;

use crate::{chain_to_db, DbError, DbPool};

/// Stores scan progress markers so workers can resume long-running scans after
/// interruption instead of starting over from zero.
pub struct CheckpointRepo<'a> {
    pool: &'a DbPool,
}

impl<'a> CheckpointRepo<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool }
    }

    /// Upserts one checkpoint per case, chain, and kind so retries always read
    /// the latest durable progress marker.
    pub async fn save_checkpoint(
        &self,
        case_id: Uuid,
        chain: ChainId,
        checkpoint_kind: &str,
        checkpoint_value: u64,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO scan_checkpoints (case_id, chain, checkpoint_kind, checkpoint_value)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (case_id, chain, checkpoint_kind)
            DO UPDATE SET checkpoint_value = EXCLUDED.checkpoint_value
            "#,
        )
        .bind(case_id)
        .bind(chain_to_db(&chain))
        .bind(checkpoint_kind)
        .bind(
            i64::try_from(checkpoint_value)
                .map_err(|_| DbError::InvalidData("checkpoint value overflow".into()))?,
        )
        .execute(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(())
    }

    /// Reads the last checkpoint so orchestration can restart from the latest
    /// persisted cursor for that case and chain.
    pub async fn get_last_checkpoint(
        &self,
        case_id: Uuid,
        chain: ChainId,
        checkpoint_kind: &str,
    ) -> Result<Option<u64>, DbError> {
        let row = sqlx::query_as::<_, CheckpointRow>(
            r#"
            SELECT checkpoint_value
            FROM scan_checkpoints
            WHERE case_id = $1 AND chain = $2 AND checkpoint_kind = $3
            "#,
        )
        .bind(case_id)
        .bind(chain_to_db(&chain))
        .bind(checkpoint_kind)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.map(|row| {
            u64::try_from(row.checkpoint_value)
                .map_err(|_| DbError::InvalidData("negative checkpoint value".into()))
        })
        .transpose()
    }
}

#[derive(Debug, FromRow)]
struct CheckpointRow {
    checkpoint_value: i64,
}
