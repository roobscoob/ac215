use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use log::info;
use serde::{Deserialize, Serialize};

use ac215::packet::packets::manual_output::{
    ManualOutputPacket, ManualOutputVariant, OutputOperation, OutputOperationType,
};
use ac215::proxy::handlers::manual_output::{ManualOutputTransaction, TransactionError};

use super::response::ApiResponse;
use crate::AppState;

// --- GET /v1/panel/outputs ---

#[derive(Serialize)]
pub struct OutputEntry {
    output_id: u8,
    is_unlocked: bool,
    is_overridden: bool,
}

#[derive(Serialize)]
pub struct OutputsResponse {
    outputs: Vec<OutputEntry>,
}

pub async fn get_outputs(
    State(state): State<AppState>,
) -> (StatusCode, Json<OutputsResponse>) {
    let handler = state.events_handler.lock().unwrap();
    let outputs = match handler.last_events() {
        Some(events) => {
            let status = &events.ac825_status;
            (0..32u8)
                .filter_map(|i| {
                    let unlocked = status.output_active(i);
                    let overridden = status.output_is_manual(i);
                    if unlocked || overridden {
                        Some(OutputEntry {
                            output_id: i,
                            is_unlocked: unlocked,
                            is_overridden: overridden,
                        })
                    } else {
                        None
                    }
                })
                .collect()
        }
        None => Vec::new(),
    };
    (StatusCode::OK, Json(OutputsResponse { outputs }))
}

// --- POST /v1/panel/outputs ---

#[derive(Deserialize)]
pub struct OutputsRequest {
    operations: Vec<OutputOp>,
}

#[derive(Deserialize)]
struct OutputOp {
    output_id: u8,
    operation: String,
    #[serde(default)]
    duration: u16,
}

pub async fn post_outputs(
    State(state): State<AppState>,
    Json(req): Json<OutputsRequest>,
) -> (StatusCode, Json<ApiResponse>) {
    let mut outputs = [OutputOperation::default(); 32];

    for op in &req.operations {
        if op.output_id >= 32 {
            return ApiResponse::bad_request(format!(
                "output_id {} out of range (0-31)",
                op.output_id
            ));
        }

        if op.duration > 3599 {
            return ApiResponse::bad_request(format!(
                "duration {} exceeds maximum (3599s, 59m 59s)",
                op.duration
            ));
        }

        let op_type = match op.operation.as_str() {
            "pulse" => OutputOperationType::OpenMomentarily,
            "open" => OutputOperationType::OpenPermanently,
            "close" => OutputOperationType::CloseAndReturnToDefault,
            other => {
                return ApiResponse::bad_request(format!("unknown operation: {other}"));
            }
        };

        outputs[op.output_id as usize] = OutputOperation {
            op_type,
            timer_minutes: (op.duration / 60) as u8,
            timer_seconds: (op.duration % 60) as u8,
        };
    }

    let packet = ManualOutputPacket {
        variant: ManualOutputVariant::Ac825,
        outputs,
    };

    info!(
        "API: manual output with {} operations",
        req.operations.len()
    );

    let mut txn = ManualOutputTransaction::new(
        packet,
        state.nack_handler.clone(),
        state.events_handler.clone(),
    );

    match state.handle.run_transaction(&mut txn).await {
        Ok(()) => ApiResponse::ok(),
        Err(TransactionError::Nack { associated_log }) => ApiResponse::from_nack(associated_log),
        Err(TransactionError::UnexpectedResponse { command }) => {
            ApiResponse::from_unexpected_response(&command)
        }
    }
}
