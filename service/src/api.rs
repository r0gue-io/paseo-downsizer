//! The axum HTTP+JSON API (SPEC "Service ↔ UI API contract"). CORS is open so
//! the UI origin can read it directly.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};

use crate::model::ControlRequest;
use crate::shared::Shared;

pub fn router(shared: Arc<Shared>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/state", get(get_state))
        .route("/api/plan", get(get_plan))
        .route("/api/health", get(get_health))
        .route("/api/history", get(get_history))
        .route("/api/control", post(post_control))
        .layer(cors)
        .with_state(shared)
}

async fn get_state(State(shared): State<Arc<Shared>>) -> impl IntoResponse {
    let snap = shared.inner.read().snapshot.clone();
    match snap {
        Some(s) => Json(s).into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "state not yet available" })),
        )
            .into_response(),
    }
}

async fn get_plan(State(shared): State<Arc<Shared>>) -> impl IntoResponse {
    Json(shared.plan_view())
}

async fn get_health(State(shared): State<Arc<Shared>>) -> impl IntoResponse {
    Json(shared.health_view())
}

async fn get_history(State(shared): State<Arc<Shared>>) -> impl IntoResponse {
    let history = shared.inner.read().persisted.history.clone();
    Json(history)
}

async fn post_control(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Json(req): Json<ControlRequest>,
) -> impl IntoResponse {
    // Bearer auth if a control token is configured.
    if let Some(expected) = &shared.control_token {
        let ok = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.trim_start_matches("Bearer ").trim() == expected)
            .unwrap_or(false);
        if !ok {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "unauthorized" })),
            )
                .into_response();
        }
    }

    match req.action.as_str() {
        "pause" => {
            shared.set_paused(true);
            tracing::warn!(target: "api", "schedule PAUSED via control endpoint");
            Json(serde_json::json!({ "status": "paused" })).into_response()
        }
        "resume" => {
            shared.set_paused(false);
            tracing::warn!(target: "api", "schedule RESUMED via control endpoint");
            Json(serde_json::json!({ "status": "ok" })).into_response()
        }
        other => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("unknown action: {other}") })),
        )
            .into_response(),
    }
}
