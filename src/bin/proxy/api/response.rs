use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use ac215::packet::named_id::NamedPacketId;
use ac215::server::Frame;

#[derive(Serialize)]
pub struct ApiResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiErrorBody>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum ApiErrorBody {
    #[serde(rename = "bad_request")]
    BadRequest { message: String },
    #[serde(rename = "database_error")]
    DatabaseError { message: String },
    #[serde(rename = "nack")]
    Nack { associated_logs: Vec<LogPacket> },
    #[serde(rename = "unexpected_packet")]
    UnexpectedPacket { packet: FrameJson },
}

#[derive(Serialize)]
pub struct LogPacket {
    pub destination: Vec<String>,
    pub severity: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct FrameJson {
    pub direction: String,
    pub source: String,
    pub destination: String,
    pub transaction_id: String,
    pub command_id: u8,
    pub command_name: Option<String>,
    pub event_flag: String,
    pub payload: String,
}

impl ApiResponse {
    pub fn ok() -> (StatusCode, Json<Self>) {
        (
            StatusCode::OK,
            Json(Self {
                ok: true,
                error: None,
            }),
        )
    }

    pub fn bad_request(message: String) -> (StatusCode, Json<Self>) {
        (
            StatusCode::BAD_REQUEST,
            Json(Self {
                ok: false,
                error: Some(ApiErrorBody::BadRequest { message }),
            }),
        )
    }

    pub fn database_error(message: String) -> (StatusCode, Json<Self>) {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(Self {
                ok: false,
                error: Some(ApiErrorBody::DatabaseError { message }),
            }),
        )
    }

    pub fn from_nack(
        associated_logs: impl IntoIterator<
            Item = ac215::packet::packets::send_log_message::SendLogMessagePacket,
        >,
    ) -> (StatusCode, Json<Self>) {
        let associated_logs = associated_logs
            .into_iter()
            .map(|log| LogPacket {
                destination: {
                    let mut d = Vec::new();
                    if log
                        .destination
                        .contains(ac215::packet::packets::send_log_message::LogDestination::LOG)
                    {
                        d.push("log".to_string());
                    }
                    if log
                        .destination
                        .contains(ac215::packet::packets::send_log_message::LogDestination::EVENT)
                    {
                        d.push("event".to_string());
                    }
                    d
                },
                severity: format!("{:?}", log.severity),
                message: log.message.to_string(),
            })
            .collect();
        (
            StatusCode::BAD_GATEWAY,
            Json(Self {
                ok: false,
                error: Some(ApiErrorBody::Nack { associated_logs }),
            }),
        )
    }

    pub fn from_unexpected_response(frame: &Frame) -> (StatusCode, Json<Self>) {
        let h = frame.header();
        let cmd = NamedPacketId(h.command_id());
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(Self {
                ok: false,
                error: Some(ApiErrorBody::UnexpectedPacket {
                    packet: FrameJson {
                        direction: format!("{:?}", h.direction()),
                        source: format!("{:?}", h.source()),
                        destination: format!("{:?}", h.destination()),
                        transaction_id: format!("{}", h.transaction_id()),
                        command_id: h.command_id(),
                        command_name: cmd.name().map(|s| s.to_string()),
                        event_flag: format!("{:?}", h.event_flag()),
                        payload: hex::encode(frame.payload()),
                    },
                }),
            }),
        )
    }
}

impl IntoResponse for ApiResponse {
    fn into_response(self) -> axum::response::Response {
        let status = if self.ok {
            StatusCode::OK
        } else {
            StatusCode::BAD_GATEWAY
        };
        (status, Json(self)).into_response()
    }
}
