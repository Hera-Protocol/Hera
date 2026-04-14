use chrono::{TimeZone, Utc};
use ed25519_dalek::SigningKey;
use hera_db::{
    repos::{cases::CaseRepo, reports::ReportRepo, workspaces::WorkspaceRepo},
    DbPool,
};
use hera_reporter::{build_manifest, build_pdf, build_s3_client, ReportStorage};
use hera_types::{
    Asset, CanonicalEvent, ChainId, Counterparty, CounterpartyVisibility, EventMemo,
    EventProvenance, EventType, Network,
};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hera:devpassword@localhost:5432/hera".to_string())
}

fn timestamp() -> chrono::DateTime<Utc> {
    match Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0) {
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
    .bind(format!("token-{tenant_id}"))
    .execute(pool)
    .await;

    if let Err(err) = result {
        panic!("failed to create tenant fixture: {err}");
    }

    tenant_id
}

#[tokio::test]
#[ignore]
async fn stores_report_artifacts_in_localstack_and_db() {
    let db = match hera_db::connect(&database_url()).await {
        Ok(value) => value,
        Err(err) => panic!("failed to connect integration database: {err}"),
    };
    let tenant_id = create_tenant(&db).await;
    let workspace = match WorkspaceRepo::new(&db)
        .create_workspace(tenant_id, "reporter-workspace")
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("failed to create workspace fixture: {err}"),
    };
    let case = match CaseRepo::new(&db)
        .create_case(workspace.id, ChainId::Namada, Network::Testnet, "created")
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("failed to create case fixture: {err}"),
    };

    let event = CanonicalEvent {
        event_id: Uuid::new_v4(),
        case_id: case.id,
        chain: ChainId::Namada,
        network: Network::Testnet,
        event_type: EventType::Shield,
        txid: "report-tx-1".into(),
        block_height: 55,
        timestamp: timestamp(),
        asset: Asset {
            symbol: "NAM".into(),
            asset_id: "nam".into(),
            decimals: 6,
        },
        amount: "1.500000".into(),
        counterparty: Counterparty {
            visibility: CounterpartyVisibility::Unknown,
            value: None,
        },
        memo: EventMemo {
            present: false,
            hash: None,
        },
        evidence_refs: vec!["indexer:tx/report-tx-1:report-tx-1".into()],
        provenance: EventProvenance {
            source: "namada_masp_indexer".into(),
            pool: Some("masp".into()),
            scan_version: "integration".into(),
        },
        notes: vec!["storage integration".into()],
    };

    let signer = SigningKey::from_bytes(&[7u8; 32]);
    let manifest = match build_manifest(&case, &[event], &signer) {
        Ok(value) => value,
        Err(err) => panic!("failed to build manifest: {err}"),
    };
    let pdf = match build_pdf(&manifest, &case) {
        Ok(value) => value,
        Err(err) => panic!("failed to build pdf: {err}"),
    };

    let bucket = format!("hera-reports-{}", Uuid::new_v4().simple());
    let endpoint =
        std::env::var("AWS_ENDPOINT_URL").unwrap_or_else(|_| "http://localhost:4566".to_string());
    let storage = ReportStorage::new(
        build_s3_client("us-east-1", Some(endpoint.as_str())).await,
        &bucket,
        db.clone(),
        None,
    );
    for attempt in 0..30 {
        match storage.ensure_bucket().await {
            Ok(_) => break,
            Err(err) if err.to_string().contains("dispatch failure") && attempt < 29 => {
                sleep(Duration::from_secs(1)).await;
            }
            Err(err) => panic!("failed to create localstack bucket: {err}"),
        }
    }
    let refs = match storage.store_report(case.id, &manifest, &pdf).await {
        Ok(value) => value,
        Err(err) => panic!("failed to store report artifacts: {err}"),
    };

    let loaded_json = match storage.load_artifact_bytes(&refs.json_s3_key).await {
        Ok(value) => value,
        Err(err) => panic!("failed to read stored json artifact: {err}"),
    };
    let loaded_pdf = match storage.load_artifact_bytes(&refs.pdf_s3_key).await {
        Ok(value) => value,
        Err(err) => panic!("failed to read stored pdf artifact: {err}"),
    };

    assert!(!loaded_json.is_empty());
    assert!(!loaded_pdf.is_empty());

    let persisted = match ReportRepo::new(&db)
        .get_report_artifacts_for_case(case.id)
        .await
    {
        Ok(Some(value)) => value,
        Ok(None) => panic!("expected persisted report refs"),
        Err(err) => panic!("failed to read persisted report refs: {err}"),
    };
    assert_eq!(persisted.json_s3_key, refs.json_s3_key);
    assert_eq!(persisted.pdf_s3_key, refs.pdf_s3_key);
}
