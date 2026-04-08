use hera_db::{repos::checkpoints::CheckpointRepo, DbPool};
use hera_types::ChainId;
use uuid::Uuid;

/// Saves the latest durable scan cursor. Checkpointing lets us resume scans
/// after crashes or restarts. Without this, a 1M block scan that fails at block
/// 999,999 would restart from zero.
pub async fn save_checkpoint(
    db: &DbPool,
    case_id: Uuid,
    chain: ChainId,
    height: u64,
) -> Result<(), hera_db::DbError> {
    CheckpointRepo::new(db)
        .save_checkpoint(case_id, chain, "height", height)
        .await
}

/// Reads the last durable scan cursor for a case and chain so retries can resume
/// from the latest successful checkpoint.
pub async fn get_last_checkpoint(
    db: &DbPool,
    case_id: Uuid,
    chain: ChainId,
) -> Result<Option<u64>, hera_db::DbError> {
    CheckpointRepo::new(db)
        .get_last_checkpoint(case_id, chain, "height")
        .await
}
