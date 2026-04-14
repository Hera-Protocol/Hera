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

#[cfg(test)]
mod tests {
    use deadpool_redis::{Config, Runtime};
    use uuid::Uuid;

    use super::{dequeue, enqueue, ScanJobMessage};
    use hera_types::ChainId;

    fn redis_url() -> (String, bool) {
        match std::env::var("REDIS_URL") {
            Ok(value) if !value.trim().is_empty() => (value, true),
            _ => ("redis://localhost:6379".to_string(), false),
        }
    }

    #[tokio::test]
    async fn enqueue_and_dequeue_round_trip_through_redis() {
        let (redis_url, explicit_redis) = redis_url();
        let pool = match Config::from_url(redis_url).create_pool(Some(Runtime::Tokio1)) {
            Ok(value) => value,
            Err(err) => panic!("failed to create redis pool: {err}"),
        };
        let mut ping_conn = match pool.get().await {
            Ok(value) => value,
            Err(err) if !explicit_redis => {
                eprintln!("skipping Redis integration test because Redis is unavailable: {err}");
                return;
            }
            Err(err) => panic!("failed to connect to Redis: {err}"),
        };
        if let Err(err) = redis::cmd("PING")
            .query_async::<String>(&mut ping_conn)
            .await
        {
            if !explicit_redis {
                eprintln!("skipping Redis integration test because Redis did not respond: {err}");
                return;
            }
            panic!("failed to ping Redis: {err}");
        }
        let queue_name = format!("hera:test:pending:{}", Uuid::new_v4());
        let processing_queue_name = format!("hera:test:processing:{}", Uuid::new_v4());
        let message = ScanJobMessage {
            job_id: Uuid::new_v4(),
            case_id: Uuid::new_v4(),
            chain: ChainId::Namada,
            priority: 9,
        };

        if let Err(err) = enqueue(&pool, &queue_name, &message).await {
            panic!("failed to enqueue scan job: {err}");
        }

        let dequeued = match dequeue(&pool, &queue_name, &processing_queue_name).await {
            Ok(Some(value)) => value,
            Ok(None) => panic!("expected a queued scan job"),
            Err(err) => panic!("failed to dequeue scan job: {err}"),
        };

        assert_eq!(dequeued.job_id, message.job_id);
        assert_eq!(dequeued.case_id, message.case_id);
        assert_eq!(dequeued.chain, message.chain);
        assert_eq!(dequeued.priority, message.priority);

        let mut conn = match pool.get().await {
            Ok(value) => value,
            Err(err) => panic!("failed to get redis connection for cleanup: {err}"),
        };
        let cleanup = redis::cmd("DEL")
            .arg(&queue_name)
            .arg(&processing_queue_name)
            .query_async::<i64>(&mut conn)
            .await;
        if let Err(err) = cleanup {
            panic!("failed to clean up redis test keys: {err}");
        }
    }
}
