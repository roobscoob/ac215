pub mod delivery;
pub mod events;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Deserialize)]
pub struct CreateWebhookRequest {
    url: String,
    signing_key: String,
}

#[derive(Serialize)]
pub struct WebhookResponse {
    id: String,
    url: String,
}

#[derive(Serialize)]
pub struct WebhookListResponse {
    webhooks: Vec<WebhookResponse>,
}

/// POST /v1/webhooks — register a new webhook.
pub async fn create_webhook(
    State(state): State<AppState>,
    Json(req): Json<CreateWebhookRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let id = uuid::Uuid::new_v4().to_string();

    if let Err(e) = state.local_db.insert_webhook(&id, &req.url, &req.signing_key) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
        );
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "ok": true,
            "webhook": { "id": id, "url": req.url }
        })),
    )
}

/// GET /v1/webhooks — list all registered webhooks.
pub async fn list_webhooks(
    State(state): State<AppState>,
) -> (StatusCode, Json<WebhookListResponse>) {
    let webhooks = state.local_db.list_webhooks().unwrap_or_default();
    (
        StatusCode::OK,
        Json(WebhookListResponse {
            webhooks: webhooks
                .into_iter()
                .map(|w| WebhookResponse { id: w.id, url: w.url })
                .collect(),
        }),
    )
}

/// DELETE /v1/webhooks/:id — remove a webhook registration.
pub async fn delete_webhook(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.local_db.delete_webhook(&id) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "ok": false, "error": "webhook not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
        ),
    }
}
