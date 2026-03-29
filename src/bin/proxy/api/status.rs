use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use ac215::proxy::status::StatusEntry;

use crate::AppState;

type JsonResponse = (StatusCode, Json<serde_json::Value>);

/// GET /v1/status
pub async fn get_status(State(state): State<AppState>) -> JsonResponse {
    let mut statuses: HashMap<String, StatusEntry> = state.status.snapshot();

    // Database health check (on-demand, not tracked).
    let db_entry = {
        let mut db = state.db.lock().await;
        match db.simple_query("SELECT 1").await {
            Ok(stream) => {
                let _ = stream.into_results().await;
                StatusEntry {
                    state: "connected".to_string(),
                    since: None,
                    detail: None,
                }
            }
            Err(e) => StatusEntry {
                state: "error".to_string(),
                since: None,
                detail: Some(serde_json::json!({ "error": e.to_string() })),
            },
        }
    };
    statuses.insert("database".to_string(), db_entry);

    (
        StatusCode::OK,
        Json(serde_json::json!({ "ok": true, "components": statuses })),
    )
}
