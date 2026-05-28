use deadpool_redis::Pool as RedisPool;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AttestationJobMessage {
    pub job_id: Uuid,
    pub case_id: Uuid,
    pub proof_type: String,
}

#[allow(dead_code)]
pub async fn enqueue(
    redis: &RedisPool,
    queue_name: &str,
    msg: &AttestationJobMessage,
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

pub async fn dequeue(
    redis: &RedisPool,
    queue_name: &str,
    processing_queue_name: &str,
) -> Result<Option<AttestationJobMessage>, anyhow::Error> {
    let mut conn = redis.get().await?;
    let payload: Option<String> = redis::cmd("BRPOPLPUSH")
        .arg(queue_name)
        .arg(processing_queue_name)
        .arg(1)
        .query_async(&mut conn)
        .await?;

    payload
        .map(|payload| serde_json::from_str::<AttestationJobMessage>(&payload))
        .transpose()
        .map_err(Into::into)
}
