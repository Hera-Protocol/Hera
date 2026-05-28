use axum::{
    http::{header, Method},
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::{
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

use crate::{
    handlers::{attestations, audit, cases, demo, keys, reports, workspaces},
    middleware::{auth, tenant_isolation},
    state::AppState,
};

/// Builds the HTTP router with tenant-aware middleware and the Stage 1 case APIs.
pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::ORIGIN,
            header::HeaderName::from_static("ngrok-skip-browser-warning"),
        ])
        .expose_headers([
            header::HeaderName::from_static("x-artifact-sha256"),
            header::HeaderName::from_static("x-request-id"),
        ])
        .max_age(std::time::Duration::from_secs(60 * 60));

    Router::new()
        .route(
            "/v1/workspaces",
            get(workspaces::list_workspaces).post(workspaces::create_workspace),
        )
        .route("/v1/cases", post(cases::create_case))
        .route(
            "/v1/workspaces/:workspace_id/cases",
            get(cases::list_cases_for_workspace),
        )
        .route(
            "/v1/workspaces/:workspace_id/reports",
            get(reports::list_workspace_reports),
        )
        .route(
            "/v1/workspaces/:workspace_id/keys",
            get(keys::list_workspace_view_keys),
        )
        .route(
            "/v1/workspaces/:workspace_id/audit-logs",
            get(audit::list_workspace_audit_logs),
        )
        .route("/v1/cases/:id", get(cases::get_case))
        .route(
            "/v1/cases/:id/zcash/import-view-key",
            post(keys::import_zcash_view_key),
        )
        .route(
            "/v1/cases/:id/namada/import-view-key",
            post(keys::import_namada_view_key),
        )
        .route("/v1/cases/:id/scan", post(cases::scan_case))
        .route("/v1/cases/:id/status", get(cases::get_case_status))
        .route("/v1/cases/:id/events", get(cases::get_case_events))
        .route("/v1/cases/:id/report.json", get(reports::report_json))
        .route("/v1/cases/:id/report.pdf", get(reports::report_pdf))
        .route(
            "/v1/cases/:id/attest",
            post(attestations::create_attestation),
        )
        .route(
            "/v1/cases/:id/attestation.json",
            get(attestations::get_attestation),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            tenant_isolation::tenant_isolation,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        // Demo sandbox routes — no auth required when DEMO_MODE=true.
        .route(
            "/v1/demo/cases/:id/report",
            get(demo::demo_report),
        )
        .route(
            "/v1/demo/attestation-types",
            get(demo::demo_attestation_types),
        )
        // Stage 1 frontend runs on a separate origin during local/ngrok demos.
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}
