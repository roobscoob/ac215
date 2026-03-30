mod credentials;
mod outputs;
pub mod response;
mod simulate;
mod status;
pub mod webhooks;

use std::net::SocketAddr;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::Router;
use log::info;

use crate::AppState;

/// Middleware that rejects requests with 503 unless the proxy is in `running` state.
async fn require_running(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: Next,
) -> axum::response::Response {
    let snapshot = state.status.snapshot();
    let is_running = snapshot
        .get("proxy")
        .is_some_and(|entry| entry.state == "running");

    if is_running {
        next.run(request).await
    } else {
        let proxy_state = snapshot
            .get("proxy")
            .map(|e| e.state.as_str())
            .unwrap_or("unknown");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("proxy is not running (state: {proxy_state})"),
                "health": snapshot,
            })),
        )
            .into_response()
    }
}

pub async fn serve(addr: SocketAddr, state: AppState) {
    // Gated routes — require proxy to be in `running` state.
    let gated = Router::new()
        .route(
            "/v1/panel/outputs",
            get(outputs::get_outputs).post(outputs::post_outputs),
        )
        .route("/v1/webhooks", get(webhooks::list_webhooks).post(webhooks::create_webhook))
        .route("/v1/webhooks/{id}", delete(webhooks::delete_webhook))
        .route("/v1/users", get(credentials::list_users).post(credentials::create_user))
        .route(
            "/v1/users/{id}",
            get(credentials::get_user)
                .patch(credentials::update_user)
                .delete(credentials::delete_user),
        )
        .route("/v1/cards", get(credentials::list_cards).post(credentials::create_card))
        .route(
            "/v1/cards/lookup/{site_code}/{card_code}",
            get(credentials::lookup_card),
        )
        .route(
            "/v1/cards/{id}",
            get(credentials::get_card).delete(credentials::delete_card),
        )
        .route("/v1/cards/{id}/assign", post(credentials::assign_card))
        .route("/v1/cards/{id}/unassign", post(credentials::unassign_card))
        .route("/v1/cards/{id}/simulate", post(simulate::simulate_card))
        .layer(middleware::from_fn_with_state(state.clone(), require_running));

    // Ungated routes — always available.
    let ungated = Router::new()
        .route("/v1/status", get(status::get_status));

    let app = Router::new()
        .merge(gated)
        .merge(ungated)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("API server listening on {addr}");
    axum::serve(listener, app).await.unwrap();
}
