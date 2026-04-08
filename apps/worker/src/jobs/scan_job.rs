use deadpool_redis::Pool as RedisPool;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use hera_types::ChainId;

/// Represents the queue payload for a scan request so Redis transport stays
/// decoupled from the database control-plane record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ScanJobMessage {
    pub job_id: Uuid,
    pub case_id: Uuid,
    pub chain: ChainId,
    pub priority: u8,
}

/// Pushes a scan job into the pending queue for later worker pickup.
#[allow(dead_code)]
pub async fn enqueue(
    redis: &RedisPool,
    queue_name: &str,
    msg: &ScanJobMessage,
) -> Result<(), anyhow::Error> {
    let payload = serde_json::to_string(msg)?;
    let mut conn = redis.get().await?;
    let _: i64 = redis::cmd("LPUSH")
        .arg(queue_name)
        .arg(payload)
        .query_async(&mut conn)
        .await?;
    Ok(())
}

/// Pulls the next scan job into the processing queue using BRPOPLPUSH. BRPOPLPUSH
/// means if this worker crashes mid-job, the job sits in the processing list and
/// can be recovered. Pure RPOP would silently drop it.
pub async fn dequeue(
    redis: &RedisPool,
    queue_name: &str,
    processing_queue_name: &str,
) -> Result<Option<ScanJobMessage>, anyhow::Error> {
    let mut conn = redis.get().await?;
    let payload: Option<String> = redis::cmd("BRPOPLPUSH")
        .arg(queue_name)
        .arg(processing_queue_name)
        .arg(1)
        .query_async(&mut conn)
        .await?;

    payload
        .map(|payload| serde_json::from_str::<ScanJobMessage>(&payload))
        .transpose()
        .map_err(Into::into)
}
