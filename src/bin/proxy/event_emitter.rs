use std::sync::{Arc, Mutex};

use ac215::event::EventType;
use ac215::packet::address::Addressing;
use ac215::packet::header::{ChecksumMode, EventFlag};
use ac215::packet::packets::ack::AckPacket;
use ac215::packet::packets::manual_output::ManualOutputPacket;
use ac215::packet::packets::nack::NackPacket;
use ac215::packet::packets::send_log_message::SendLogMessagePacket;
use ac215::proxy::handlers::EventsHandler;
use ac215::proxy::handlers::nack::NackHandler;
use ac215::proxy::interceptor::{InterceptorHandle, Transaction};
use ac215::server::Frame;
use log::error;

use crate::custom_event::CustomEvent;
use crate::local_db::LocalDb;

pub enum TransactionError {
    Nack {
        associated_log: Option<SendLogMessagePacket>,
    },
    UnexpectedResponse {
        command: Frame,
    },
}

pub struct EventEmitterTransaction {
    custom_event: Option<CustomEvent>,
    events_handler: Arc<Mutex<EventsHandler>>,
    nack_handler: Arc<Mutex<NackHandler>>,
    local_db: Arc<LocalDb>,
}

impl EventEmitterTransaction {
    pub fn new(
        custom_event: CustomEvent,
        events_handler: Arc<Mutex<EventsHandler>>,
        nack_handler: Arc<Mutex<NackHandler>>,
        local_db: Arc<LocalDb>,
    ) -> Self {
        Self {
            custom_event: Some(custom_event),
            events_handler,
            nack_handler,
            local_db,
        }
    }
}

impl Transaction for EventEmitterTransaction {
    type Result = Result<(), TransactionError>;

    async fn execute(&mut self, handle: &InterceptorHandle) -> Result<(), TransactionError> {
        let custom_event = self
            .custom_event
            .take()
            .expect("transaction already executed");

        let overlay = custom_event.clone();
        let db_event = custom_event;
        let local_db = self.local_db.clone();

        self.events_handler
            .lock()
            .unwrap()
            .add_mutator(move |record, handle| {
                if record.event_type != EventType::OUTPUT_UNLOCKED_PC_MANUAL {
                    return;
                }

                record.event_type = overlay.event_type;
                record.event_data = overlay.event_data;
                record.location = overlay.location;

                if let Some(timestamp) = overlay.timestamp {
                    record.timestamp = timestamp;
                }

                handle.remove();

                // Insert into local DB for persistence and external access.
                if let Err(e) = local_db.save_custom_event(record.event_number, &db_event) {
                    error!("Failed to insert custom event into local DB: {:?}", e);
                }
            });

        let packet = ManualOutputPacket::pulse(&[31], 0, 1);

        let response = handle
            .request(
                Addressing::dual().to_panel(),
                EventFlag::Unset,
                ChecksumMode::Auto,
                packet,
            )
            .await;

        if let Some(Ok(_)) = response.parse::<AckPacket>() {
            return Ok(());
        }

        if let Some(Ok(_)) = response.parse::<NackPacket>() {
            let associated_log = self.nack_handler.lock().unwrap().take_last_log();

            error!("EventEmitter NACKed, associated log: {:?}", associated_log);

            return Err(TransactionError::Nack { associated_log });
        }

        Err(TransactionError::UnexpectedResponse { command: response })
    }

    async fn recover(&self, _handle: &InterceptorHandle) {
        // No recovery needed.
    }
}
