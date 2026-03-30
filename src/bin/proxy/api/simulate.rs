use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use log::info;
use serde::Deserialize;

use ac215::event::access_event_data::AccessEventData;
use ac215::event::event_type::EventType;
use ac215::event::location::EventLocation;
use ac215::packet::packets::manual_output::{
    ManualOutputPacket, ManualOutputVariant, OutputOperation, OutputOperationType,
};
use ac215::proxy::handlers::manual_output::{
    ManualOutputTransaction, TransactionError as ManualOutputError,
};

use super::response::ApiResponse;
use crate::AppState;
use crate::custom_event::CustomEvent;
use crate::event_emitter::{EventEmitterTransaction, TransactionError as EmitterError};

#[derive(Deserialize)]
pub struct SimulateOutputOp {
    pub output_id: u8,
    #[serde(default = "default_duration")]
    pub duration: u16,
}

fn default_duration() -> u16 {
    1
}

#[derive(Deserialize)]
pub struct SimulateCardRequest {
    pub reader: u8,
    pub outputs: Vec<SimulateOutputOp>,
}

fn build_custom_event(site_code: i32, card_code: &str, reader: u8) -> CustomEvent {
    let mut card_code_bytes = [0u8; 14];
    if let Ok(val) = card_code.parse::<u64>() {
        card_code_bytes[6..14].copy_from_slice(&val.to_be_bytes());
    }

    let mut event_data = [0u8; 25];
    AccessEventData {
        parking_subgroup: 0,
        format_select: 0,
        _offset_pad: [0; 6],
        site_code: site_code as u16,
        card_code_bytes,
    }
    .write(&mut event_data);

    CustomEvent::new(
        EventType::ACCESS_GRANTED_CARD_WITH_FACILITY,
        EventLocation::Reader(reader),
    )
    .with_data(event_data)
}

/// POST /v1/cards/:id/simulate
pub async fn simulate_card(
    State(state): State<AppState>,
    Path(card_id): Path<i32>,
    Json(req): Json<SimulateCardRequest>,
) -> (StatusCode, Json<ApiResponse>) {
    if req.reader > 9 {
        return ApiResponse::bad_request("reader must be 0-9".to_string());
    }
    if req.outputs.is_empty() {
        return ApiResponse::bad_request("outputs must not be empty".to_string());
    }
    let mut outputs = [OutputOperation::default(); 32];
    for op in &req.outputs {
        if op.output_id >= 32 {
            return ApiResponse::bad_request(format!(
                "output_id {} out of range (0-31)",
                op.output_id
            ));
        }
        if op.duration > 3599 {
            return ApiResponse::bad_request(format!(
                "duration {} exceeds maximum (3599s)",
                op.duration
            ));
        }
        outputs[op.output_id as usize] = OutputOperation {
            op_type: OutputOperationType::OpenMomentarily,
            timer_minutes: (op.duration / 60) as u8,
            timer_seconds: (op.duration % 60) as u8,
        };
    }

    let mut db = state.db.lock().await;

    // Look up the card.
    let card_row = match db
        .query(
            "SELECT iSiteCode, iCardCode, eCardStatus, IdEmpNum FROM tblCard WHERE IdCardNum = @P1",
            &[&card_id],
        )
        .await
    {
        Ok(stream) => match stream.into_row().await {
            Ok(Some(row)) => row,
            Ok(None) => {
                return ApiResponse::bad_request("card not found".to_string());
            }
            Err(e) => {
                return ApiResponse::database_error(e.to_string());
            }
        },
        Err(e) => {
            return ApiResponse::database_error(e.to_string());
        }
    };

    let site_code: i32 = card_row.get("iSiteCode").unwrap_or(0);
    let card_code: &str = card_row.get("iCardCode").unwrap_or("");
    let emp_id: i32 = card_row.get("IdEmpNum").unwrap_or(0);

    if emp_id == 0 {
        return ApiResponse::bad_request("card is not assigned to an employee".to_string());
    }

    // Release the DB lock before running transactions.
    drop(db);

    let custom_event = build_custom_event(site_code, card_code, req.reader);

    info!(
        "API: simulating card {card_id} (site={site_code}, code={card_code}) at reader {}",
        req.reader
    );

    // Transaction 1: ManualOutput — pulse the requested door outputs.
    let packet = ManualOutputPacket {
        variant: ManualOutputVariant::Ac825,
        outputs,
    };
    let mut manual_txn = ManualOutputTransaction::new(
        packet,
        state.nack_handler.clone(),
        state.events_handler.clone(),
    );

    if let Err(e) = state.handle.run_transaction(&mut manual_txn).await {
        return match e {
            ManualOutputError::Nack { associated_log } => ApiResponse::from_nack(associated_log),
            ManualOutputError::UnexpectedResponse { command } => {
                ApiResponse::from_unexpected_response(&command)
            }
        };
    }

    // Transaction 2: EventEmitter
    let mut emitter_txn = EventEmitterTransaction::new(
        custom_event,
        state.events_handler.clone(),
        state.nack_handler.clone(),
        state.local_db.clone(),
    );

    match state.handle.run_transaction(&mut emitter_txn).await {
        Ok(()) => ApiResponse::ok(),
        Err(EmitterError::Nack { associated_log }) => ApiResponse::from_nack(associated_log),
        Err(EmitterError::UnexpectedResponse { command }) => {
            ApiResponse::from_unexpected_response(&command)
        }
    }
}
