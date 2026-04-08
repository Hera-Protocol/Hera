use chrono::{TimeZone, Utc};
use hera_db::{
    repos::{
        cases::CaseRepo,
        checkpoints::CheckpointRepo,
        events::EventRepo,
        jobs::ScanJobRepo,
        keys::{EncryptedViewKeyRecord, ViewKeyRepo},
        workspaces::WorkspaceRepo,
    },
    DbPool,
};
use hera_types::{
    Asset, CanonicalEvent, ChainId, Counterparty, CounterpartyVisibility, EventMemo,
    EventProvenance, EventType, Network, ScanJobStatus,
};
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hera:devpassword@localhost:5432/hera".to_string())
}

fn test_timestamp() -> chrono::DateTime<Utc> {
    match Utc.with_ymd_and_hms(2025, 5, 1, 12, 0, 0) {
        chrono::LocalResult::Single(value) => value,
        other => panic!("unexpected timestamp result: {other:?}"),
    }
}

async fn create_tenant(pool: &DbPool) -> Uuid {
    let tenant_id = Uuid::new_v4();
    let result = sqlx::query(
        r#"
        INSERT INTO tenants (id, name, api_key_hash)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(tenant_id)
    .bind(format!("tenant-{tenant_id}"))
    .bind(format!("api-key-{tenant_id}"))
    .execute(pool)
    .await;

    if let Err(err) = result {
        panic!("failed to create tenant fixture: {err}");
    }

    tenant_id
}

#[tokio::test]
#[ignore]
async fn repo_round_trip_persists_cases_events_jobs_and_keys() {
    let pool = match hera_db::connect(&database_url()).await {
        Ok(value) => value,
        Err(err) => panic!("failed to connect test database: {err}"),
    };
    let tenant_id = create_tenant(&pool).await;

    let workspace = match WorkspaceRepo::new(&pool)
        .create_workspace(tenant_id, "integration-workspace")
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("failed to create workspace fixture: {err}"),
    };

    let case = match CaseRepo::new(&pool)
        .create_case(
            workspace.id,
            ChainId::Zcash,
            Network::Testnet,
            "created",
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("failed to create case fixture: {err}"),
    };

    let event = CanonicalEvent {
        event_id: Uuid::new_v4(),
        case_id: case.id,
        chain: ChainId::Zcash,
        network: Network::Testnet,
        event_type: EventType::Receive,
        txid: "tx-integration-1".into(),
        block_height: 42,
        timestamp: test_timestamp(),
        asset: Asset {
            symbol: "ZEC".into(),
            asset_id: "zec".into(),
            decimals: 8,
        },
        amount: "1.00000000".into(),
        counterparty: Counterparty {
            visibility: CounterpartyVisibility::Unknown,
            value: None,
        },
        memo: EventMemo {
            present: false,
            hash: None,
        },
        evidence_refs: vec!["compactblock:42:hash-1".into()],
        provenance: EventProvenance {
            source: "lightwalletd".into(),
            pool: Some("sapling".into()),
            scan_version: "integration".into(),
        },
        notes: vec!["integration event".into()],
    };

    if let Err(err) = EventRepo::new(&pool).insert_canonical_event(&event).await {
        panic!("failed to insert canonical event: {err}");
    }
    if let Err(err) = EventRepo::new(&pool).insert_canonical_event(&event).await {
        panic!("failed to reinsert canonical event idempotently: {err}");
    }

    let events = match EventRepo::new(&pool).get_events_for_case(case.id).await {
        Ok(value) => value,
        Err(err) => panic!("failed to read canonical events: {err}"),
    };
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].amount, "1.00000000");

    let stored_key = match ViewKeyRepo::new(&pool)
        .store_encrypted_view_key(EncryptedViewKeyRecord {
            case_id: case.id,
            chain: ChainId::Zcash,
            key_ref: "kms:test-key",
            ciphertext: &[1, 2, 3],
            nonce: &[4; 12],
            encrypted_data_key: &[5, 6, 7],
            birthday_height: Some(100),
        })
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("failed to store encrypted view key: {err}"),
    };
    assert_eq!(stored_key.birthday_height, Some(100));

    let loaded_key = match ViewKeyRepo::new(&pool).get_view_key_for_case(case.id).await {
        Ok(Some(value)) => value,
        Ok(None) => panic!("view key fixture was not found"),
        Err(err) => panic!("failed to load encrypted view key: {err}"),
    };
    assert_eq!(loaded_key.ciphertext, vec![1, 2, 3]);

    let job = match ScanJobRepo::new(&pool)
        .create_scan_job(
            case.id,
            ChainId::Zcash,
            Network::Testnet,
            ScanJobStatus::Created,
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("failed to create scan job: {err}"),
    };

    let updated_job = match ScanJobRepo::new(&pool)
        .update_scan_job_status(job.id, ScanJobStatus::Signed)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => panic!("scan job fixture was not found"),
        Err(err) => panic!("failed to update scan job status: {err}"),
    };
    assert_eq!(updated_job.status, ScanJobStatus::Signed);

    if let Err(err) = CheckpointRepo::new(&pool)
        .save_checkpoint(case.id, ChainId::Zcash, "height", 123)
        .await
    {
        panic!("failed to save checkpoint: {err}");
    }
    let checkpoint = match CheckpointRepo::new(&pool)
        .get_last_checkpoint(case.id, ChainId::Zcash, "height")
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("failed to load checkpoint: {err}"),
    };
    assert_eq!(checkpoint, Some(123));
}
