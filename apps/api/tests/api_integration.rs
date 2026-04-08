use std::sync::Arc;

use axum::{
    body::Body,
    http::{header::AUTHORIZATION, Method, Request, StatusCode},
};
use deadpool_redis::{Config as RedisConfig, Runtime};
use hera_api::{router::build_router, state::AppState};
use hera_crypto::{KmsClient, LocalDevKms};
use hera_db::{repos::workspaces::WorkspaceRepo, DbPool};
use hera_reporter::ReportStorage;
use hera_worker::jobs::scan_job::dequeue;
use http_body_util::BodyExt as _;
use serde_json::Value;
use tower::util::ServiceExt as _;
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hera:devpassword@localhost:5432/hera".to_string())
}

fn redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

async fn test_s3_client() -> aws_sdk_s3::Client {
    let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .credentials_provider(aws_sdk_s3::config::Credentials::new(
            "test",
            "test",
            None,
            None,
            "integration-test",
        ))
        .load()
        .await;

    aws_sdk_s3::Client::new(&shared_config)
}

async fn setup_app(queue_name: &str) -> (axum::Router, DbPool) {
    let db = match hera_db::connect(&database_url()).await {
        Ok(value) => value,
        Err(err) => panic!("failed to connect integration database: {err}"),
    };
    let redis = match RedisConfig::from_url(redis_url()).create_pool(Some(Runtime::Tokio1)) {
        Ok(value) => value,
        Err(err) => panic!("failed to create redis pool: {err}"),
    };
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

    let state = AppState {
        db: db.clone(),
        redis,
        crypto,
        report_storage,
        kms_key_ref: "alias/hera-dev".into(),
        scan_queue_name: queue_name.to_string(),
    };

    (build_router(state), db)
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

#[tokio::test]
#[ignore]
async fn api_enforces_auth_tenant_isolation_and_scan_enqueue() {
    let queue_name = format!("hera:test:api:pending:{}", Uuid::new_v4());
    let processing_queue_name = format!("hera:test:api:processing:{}", Uuid::new_v4());
    let (app, db) = setup_app(&queue_name).await;
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

    let redis = match RedisConfig::from_url(redis_url()).create_pool(Some(Runtime::Tokio1)) {
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
