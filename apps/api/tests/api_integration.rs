use std::sync::Arc;

use axum::{
    body::Body,
    http::{header::AUTHORIZATION, Method, Request, StatusCode},
};
use deadpool_redis::{Config as RedisConfig, Runtime};
use hera_api::{router::build_router, state::AppState};
use hera_crypto::{KmsClient, LocalDevKms};
use hera_db::{
    repos::{
        reports::{ReportRepo, StoredArtifactRefs},
        workspaces::WorkspaceRepo,
    },
    DbPool,
};
use hera_reporter::{build_s3_client, ReportStorage};
use hera_worker::jobs::scan_job::dequeue;
use http_body_util::BodyExt as _;
use serde_json::Value;
use tower::util::ServiceExt as _;
use uuid::Uuid;

fn database_url() -> (String, bool) {
    match std::env::var("DATABASE_URL") {
        Ok(value) if !value.trim().is_empty() => (value, true),
        _ => (
            "postgres://hera:devpassword@localhost:5432/hera".to_string(),
            false,
        ),
    }
}

fn redis_url() -> (String, bool) {
    match std::env::var("REDIS_URL") {
        Ok(value) if !value.trim().is_empty() => (value, true),
        _ => ("redis://localhost:6379".to_string(), false),
    }
}

fn localstack_endpoint() -> (String, bool) {
    match std::env::var("AWS_ENDPOINT_URL") {
        Ok(value) if !value.trim().is_empty() => (value, true),
        _ => ("http://localhost:4566".to_string(), false),
    }
}

async fn test_s3_client() -> aws_sdk_s3::Client {
    let (endpoint, _) = localstack_endpoint();
    build_s3_client("us-east-1", Some(endpoint.as_str())).await
}

async fn setup_app(queue_name: &str) -> Option<(axum::Router, DbPool)> {
    let (database_url, explicit_database) = database_url();
    let db = match hera_db::connect(&database_url).await {
        Ok(value) => value,
        Err(err) if !explicit_database => {
            eprintln!("skipping API integration test because Postgres is unavailable: {err}");
            return None;
        }
        Err(err) => panic!("failed to connect integration database: {err}"),
    };
    let (redis_url, explicit_redis) = redis_url();
    let redis = match RedisConfig::from_url(redis_url).create_pool(Some(Runtime::Tokio1)) {
        Ok(value) => value,
        Err(err) => panic!("failed to create redis pool: {err}"),
    };
    let mut redis_conn = match redis.get().await {
        Ok(value) => value,
        Err(err) if !explicit_redis => {
            eprintln!("skipping API integration test because Redis is unavailable: {err}");
            return None;
        }
        Err(err) => panic!("failed to connect to Redis: {err}"),
    };
    if let Err(err) = redis::cmd("PING")
        .query_async::<String>(&mut redis_conn)
        .await
    {
        if !explicit_redis {
            eprintln!("skipping API integration test because Redis did not respond: {err}");
            return None;
        }
        panic!("failed to ping Redis: {err}");
    }
    let crypto: Arc<dyn KmsClient> = match LocalDevKms::new(
        "alias/hera-dev",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    ) {
        Ok(value) => Arc::new(value),
        Err(err) => panic!("failed to construct local dev kms: {err}"),
    };
    let report_storage = Arc::new(ReportStorage::new(
        test_s3_client().await,
        "hera-test-bucket",
        db.clone(),
        None,
    ));
    let (_, explicit_localstack) = localstack_endpoint();
    if let Err(err) = report_storage.ensure_bucket().await {
        if !explicit_localstack {
            eprintln!("skipping API integration test because LocalStack is unavailable: {err}");
            return None;
        }
        panic!("failed to ensure integration test bucket exists: {err}");
    }

    let state = AppState {
        db: db.clone(),
        redis,
        crypto,
        report_storage,
        kms_key_ref: "alias/hera-dev".into(),
        scan_queue_name: queue_name.to_string(),
        namada_chain_id: "namada.5f5de2dd1b88cba30586420".into(),
        attestation_queue_name: "attestation-queue-test".into(),
        demo_mode: false,
    };

    Some((build_router(state), db))
}

async fn create_tenant(pool: &DbPool, api_key: &str) -> Uuid {
    let tenant_id = Uuid::new_v4();
    let result = sqlx::query(
        r#"
        INSERT INTO tenants (id, name, api_key_hash)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(tenant_id)
    .bind(format!("tenant-{tenant_id}"))
    .bind(api_key)
    .execute(pool)
    .await;

    if let Err(err) = result {
        panic!("failed to create tenant fixture: {err}");
    }

    tenant_id
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = match response.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => panic!("failed to read response body: {err}"),
    };

    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) => value,
        Err(err) => panic!("failed to parse response json: {err}"),
    }
}

async fn create_workspace_via_api(app: &axum::Router, token: &str, name: &str) -> Uuid {
    let workspace_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/workspaces")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"name":"{name}"}}"#)))
                .unwrap_or_else(|err| panic!("failed to build workspace request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("workspace request failed unexpectedly: {err}"),
    };
    assert_eq!(workspace_response.status(), StatusCode::CREATED);
    let workspace_json = response_json(workspace_response).await;
    match workspace_json
        .get("id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
    {
        Some(value) => value,
        None => panic!("workspace response did not include a valid id"),
    }
}

async fn create_case_via_api(
    app: &axum::Router,
    token: &str,
    workspace_id: Uuid,
    chain: &str,
    network: &str,
) -> Uuid {
    let case_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/cases")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"workspace_id":"{workspace_id}","chain":"{chain}","network":"{network}"}}"#
                )))
                .unwrap_or_else(|err| panic!("failed to build case request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("case request failed unexpectedly: {err}"),
    };
    assert_eq!(case_response.status(), StatusCode::CREATED);
    let case_json = response_json(case_response).await;
    match case_json
        .get("id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
    {
        Some(value) => value,
        None => panic!("case response did not include a valid id"),
    }
}

#[tokio::test]
async fn api_enforces_auth_tenant_isolation_and_scan_enqueue() {
    let queue_name = format!("hera:test:api:pending:{}", Uuid::new_v4());
    let processing_queue_name = format!("hera:test:api:processing:{}", Uuid::new_v4());
    let Some((app, db)) = setup_app(&queue_name).await else {
        return;
    };
    let tenant_token = format!("token-{}", Uuid::new_v4());
    let other_token = format!("token-{}", Uuid::new_v4());
    let tenant_id = create_tenant(&db, &tenant_token).await;
    let other_tenant_id = create_tenant(&db, &other_token).await;

    let unauthorized = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/workspaces")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"unauthorized"}"#))
                .unwrap_or_else(|err| panic!("failed to build unauthorized request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("unauthorized request failed unexpectedly: {err}"),
    };
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let workspace_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/workspaces")
                .header(AUTHORIZATION, format!("Bearer {tenant_token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"primary-workspace"}"#))
                .unwrap_or_else(|err| panic!("failed to build workspace request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("workspace request failed unexpectedly: {err}"),
    };
    assert_eq!(workspace_response.status(), StatusCode::CREATED);
    let workspace_json = response_json(workspace_response).await;
    let workspace_id = match workspace_json
        .get("id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
    {
        Some(value) => value,
        None => panic!("workspace response did not include a valid id"),
    };

    let forbidden_case_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/cases")
                .header(AUTHORIZATION, format!("Bearer {other_token}"))
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"workspace_id":"{workspace_id}","chain":"ZCASH","network":"TESTNET"}}"#
                )))
                .unwrap_or_else(|err| panic!("failed to build forbidden case request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("forbidden case request failed unexpectedly: {err}"),
    };
    assert_eq!(forbidden_case_response.status(), StatusCode::FORBIDDEN);

    let case_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/cases")
                .header(AUTHORIZATION, format!("Bearer {tenant_token}"))
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"workspace_id":"{workspace_id}","chain":"ZCASH","network":"TESTNET"}}"#
                )))
                .unwrap_or_else(|err| panic!("failed to build case request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("case request failed unexpectedly: {err}"),
    };
    assert_eq!(case_response.status(), StatusCode::CREATED);
    let case_json = response_json(case_response).await;
    let case_id = match case_json
        .get("id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
    {
        Some(value) => value,
        None => panic!("case response did not include a valid id"),
    };

    let scan_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/cases/{case_id}/scan"))
                .header(AUTHORIZATION, format!("Bearer {tenant_token}"))
                .body(Body::empty())
                .unwrap_or_else(|err| panic!("failed to build scan request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("scan request failed unexpectedly: {err}"),
    };
    assert_eq!(scan_response.status(), StatusCode::CREATED);

    let (redis_url, _) = redis_url();
    let redis = match RedisConfig::from_url(redis_url).create_pool(Some(Runtime::Tokio1)) {
        Ok(value) => value,
        Err(err) => panic!("failed to create redis pool for verification: {err}"),
    };
    let queued = match dequeue(&redis, &queue_name, &processing_queue_name).await {
        Ok(Some(value)) => value,
        Ok(None) => panic!("expected an enqueued scan message"),
        Err(err) => panic!("failed to dequeue scan message: {err}"),
    };
    assert_eq!(queued.case_id, case_id);

    let status_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/v1/cases/{case_id}/status"))
                .header(AUTHORIZATION, format!("Bearer {tenant_token}"))
                .body(Body::empty())
                .unwrap_or_else(|err| panic!("failed to build status request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("status request failed unexpectedly: {err}"),
    };
    assert_eq!(status_response.status(), StatusCode::OK);
    let status_json = response_json(status_response).await;
    assert_eq!(
        status_json.get("status").and_then(Value::as_str),
        Some("CREATED")
    );

    let forbidden_status_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/v1/cases/{case_id}/status"))
                .header(AUTHORIZATION, format!("Bearer {other_token}"))
                .body(Body::empty())
                .unwrap_or_else(|err| panic!("failed to build forbidden status request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("forbidden status request failed unexpectedly: {err}"),
    };
    assert_eq!(forbidden_status_response.status(), StatusCode::FORBIDDEN);

    let tenant_workspace = match WorkspaceRepo::new(&db)
        .create_workspace(other_tenant_id, "other-workspace")
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("failed to create other tenant workspace fixture: {err}"),
    };
    assert_ne!(tenant_workspace.tenant_id, tenant_id);
}

#[tokio::test]
async fn api_lists_workspace_resources_and_case_detail() {
    let queue_name = format!("hera:test:api:list:pending:{}", Uuid::new_v4());
    let Some((app, db)) = setup_app(&queue_name).await else {
        return;
    };
    let tenant_token = format!("token-{}", Uuid::new_v4());
    let other_token = format!("token-{}", Uuid::new_v4());
    let tenant_id = create_tenant(&db, &tenant_token).await;
    let _other_tenant_id = create_tenant(&db, &other_token).await;

    let workspace_id = create_workspace_via_api(&app, &tenant_token, "stage-one-workspace").await;
    let case_id = create_case_via_api(&app, &tenant_token, workspace_id, "ZCASH", "TESTNET").await;
    let case_id_string = case_id.to_string();

    let import_key_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/cases/{case_id}/zcash/import-view-key"))
                .header(AUTHORIZATION, format!("Bearer {tenant_token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"raw_key":"zxviews1q0testkey"}"#))
                .unwrap_or_else(|err| panic!("failed to build import key request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("import key request failed unexpectedly: {err}"),
    };
    assert_eq!(import_key_response.status(), StatusCode::CREATED);

    let report_id = match ReportRepo::new(&db)
        .upsert_report_artifacts(
            case_id,
            &StoredArtifactRefs {
                json_s3_key: format!("reports/{case_id}/fixture.json"),
                pdf_s3_key: format!("reports/{case_id}/fixture.pdf"),
                json_sha256: "json-hash".into(),
                pdf_sha256: "pdf-hash".into(),
            },
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("failed to seed report refs: {err}"),
    };
    assert_ne!(report_id, Uuid::nil());

    let workspaces_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/v1/workspaces?limit=10&offset=0")
                .header(AUTHORIZATION, format!("Bearer {tenant_token}"))
                .body(Body::empty())
                .unwrap_or_else(|err| panic!("failed to build workspaces request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("workspaces request failed unexpectedly: {err}"),
    };
    assert_eq!(workspaces_response.status(), StatusCode::OK);
    let workspaces_json = response_json(workspaces_response).await;
    assert_eq!(
        workspaces_json
            .get("items")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );

    let cases_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/v1/workspaces/{workspace_id}/cases?limit=10&offset=0"
                ))
                .header(AUTHORIZATION, format!("Bearer {tenant_token}"))
                .body(Body::empty())
                .unwrap_or_else(|err| panic!("failed to build cases list request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("cases list request failed unexpectedly: {err}"),
    };
    assert_eq!(cases_response.status(), StatusCode::OK);
    let cases_json = response_json(cases_response).await;
    assert_eq!(
        cases_json
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("id"))
            .and_then(Value::as_str),
        Some(case_id_string.as_str())
    );
    assert_eq!(
        cases_json
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("scan_status"))
            .and_then(Value::as_str),
        Some("CREATED")
    );

    let case_detail_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/v1/cases/{case_id}"))
                .header(AUTHORIZATION, format!("Bearer {tenant_token}"))
                .body(Body::empty())
                .unwrap_or_else(|err| panic!("failed to build case detail request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("case detail request failed unexpectedly: {err}"),
    };
    assert_eq!(case_detail_response.status(), StatusCode::OK);
    let case_detail_json = response_json(case_detail_response).await;
    assert_eq!(
        case_detail_json.get("id").and_then(Value::as_str),
        Some(case_id_string.as_str())
    );

    let keys_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/v1/workspaces/{workspace_id}/keys?limit=10&offset=0"
                ))
                .header(AUTHORIZATION, format!("Bearer {tenant_token}"))
                .body(Body::empty())
                .unwrap_or_else(|err| panic!("failed to build keys request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("keys request failed unexpectedly: {err}"),
    };
    assert_eq!(keys_response.status(), StatusCode::OK);
    let keys_json = response_json(keys_response).await;
    assert_eq!(
        keys_json
            .get("items")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );

    let reports_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/v1/workspaces/{workspace_id}/reports?limit=10&offset=0"
                ))
                .header(AUTHORIZATION, format!("Bearer {tenant_token}"))
                .body(Body::empty())
                .unwrap_or_else(|err| panic!("failed to build reports request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("reports request failed unexpectedly: {err}"),
    };
    assert_eq!(reports_response.status(), StatusCode::OK);
    let reports_json = response_json(reports_response).await;
    assert_eq!(
        reports_json
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("case_id"))
            .and_then(Value::as_str),
        Some(case_id_string.as_str())
    );

    let audit_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/v1/workspaces/{workspace_id}/audit-logs?limit=10&offset=0"
                ))
                .header(AUTHORIZATION, format!("Bearer {tenant_token}"))
                .body(Body::empty())
                .unwrap_or_else(|err| panic!("failed to build audit log request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("audit log request failed unexpectedly: {err}"),
    };
    assert_eq!(audit_response.status(), StatusCode::OK);
    let audit_json = response_json(audit_response).await;
    let actions = match audit_json.get("items").and_then(Value::as_array) {
        Some(items) => items
            .iter()
            .filter_map(|item| item.get("action").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        None => panic!("audit response did not include items"),
    };
    assert!(actions.contains(&"CASE_CREATED"));
    assert!(actions.contains(&"KEY_IMPORT"));

    let forbidden_workspace_response = match app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/v1/workspaces/{workspace_id}/cases?limit=10&offset=0"
                ))
                .header(AUTHORIZATION, format!("Bearer {other_token}"))
                .body(Body::empty())
                .unwrap_or_else(|err| panic!("failed to build forbidden workspace request: {err}")),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => panic!("forbidden workspace request failed unexpectedly: {err}"),
    };
    assert_eq!(forbidden_workspace_response.status(), StatusCode::FORBIDDEN);

    assert_ne!(tenant_id, Uuid::nil());
}
