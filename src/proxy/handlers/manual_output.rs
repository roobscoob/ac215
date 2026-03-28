use std::sync::{Arc, Mutex};

use log::{error, info};

use crate::event::EventType;
use crate::packet::address::Addressing;
use crate::packet::header::{ChecksumMode, EventFlag};
use crate::packet::packets::ack::AckPacket;
use crate::packet::packets::manual_output::ManualOutputPacket;
use crate::packet::packets::nack::NackPacket;
use crate::packet::packets::send_log_message::SendLogMessagePacket;
use crate::proxy::handlers::EventsHandler;
use crate::proxy::interceptor::{InterceptorHandle, Transaction};
use crate::proxy::pipeline::Disposition;
use crate::server::Frame;

use super::nack::NackHandler;

/// Send a ManualOutput command to the panel and wait for the response.
///
/// Blocks the server→panel primary channel for the duration so the server
/// never sees our request or the panel's response.
pub struct ManualOutputTransaction {
    packet: Option<ManualOutputPacket>,
    nack_handler: Arc<Mutex<NackHandler>>,
    events_handler: Arc<Mutex<EventsHandler>>,
}

impl ManualOutputTransaction {
    pub fn new(
        packet: ManualOutputPacket,
        nack_handler: Arc<Mutex<NackHandler>>,
        events_handler: Arc<Mutex<EventsHandler>>,
    ) -> Self {
        Self {
            packet: Some(packet),
            nack_handler,
            events_handler,
        }
    }
}

pub enum TransactionError {
    Nack {
        associated_log: Option<SendLogMessagePacket>,
    },
    UnexpectedResponse {
        command: Frame,
    },
}

impl Transaction for ManualOutputTransaction {
    type Result = Result<(), TransactionError>;

    async fn execute(&mut self, handle: &InterceptorHandle) -> Result<(), TransactionError> {
        let packet = self.packet.take().expect("transaction already executed");

        self.events_handler
            .lock()
            .unwrap()
            .once(EventType::OUTPUT_UNLOCKED_PC_MANUAL, |_| Disposition::Drop);

        let response = handle
            .request(
                Addressing::dual().to_panel(),
                EventFlag::Unset,
                ChecksumMode::Auto,
                packet,
            )
            .await;

        if let Some(Ok(_)) = response.parse::<AckPacket>() {
            info!("ManualOutput successful");
            return Ok(());
        }

        if let Some(Ok(_)) = response.parse::<NackPacket>() {
            let associated_log = self.nack_handler.lock().unwrap().take_last_log();

            error!("ManualOutput NACKed, associated log: {:?}", associated_log);

            return Err(TransactionError::Nack { associated_log });
        }

        Err(TransactionError::UnexpectedResponse { command: response })
    }

    async fn recover(&self, _handle: &InterceptorHandle) {
        // No recovery needed — ManualOutput is a stateless query.
    }
}
