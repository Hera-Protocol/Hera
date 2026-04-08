use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

use crate::{
    handlers::{cases, keys, reports, workspaces},
    middleware::{auth, tenant_isolation},
    state::AppState,
};

/// Builds the HTTP router with tenant-aware middleware and the Stage 1 case APIs.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/workspaces", post(workspaces::create_workspace))
        .route("/v1/cases", post(cases::create_case))
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
        .layer(middleware::from_fn_with_state(
            state.clone(),
            tenant_isolation::tenant_isolation,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}
