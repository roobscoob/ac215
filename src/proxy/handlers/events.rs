use crate::event::access_event_data::AccessEventData;
use crate::event::event_type::AccessOutcome;
use crate::event::location::EventLocation;
use crate::event::timestamp::PackedTimestamp;
use crate::event::{EventRecord, EventType};
use crate::packet::packets::answer_events_825::AnswerEvents825;
use crate::server::Frame;

use super::super::message::Side;
use super::super::pipeline::{Disposition, FrameHandler, FrameHandling, HandlerContext};
use super::super::status::StatusTracker;

type EventCallback = Box<dyn FnOnce(&EventRecord) -> Disposition + Send>;

/// Called whenever a new AnswerEvents825 is received.
/// Arguments: (previous status, new status).
type StatusChangeCallback = Box<dyn Fn(Option<&AnswerEvents825>, &AnswerEvents825) + Send>;

type AccessCallback = Box<dyn Fn(&AccessEvent) + Send>;

/// Parsed access event with credential data extracted.
#[derive(Debug, Clone)]
pub struct AccessEvent {
    pub timestamp: PackedTimestamp,
    pub outcome: AccessOutcome,
    pub location: EventLocation,
    pub site_code: u16,
    pub card_code: u64,
    pub format_index: u8,
}

struct PendingCallback {
    event_type: EventType,
    callback: EventCallback,
}

pub struct EventsHandler {
    last_events: Option<AnswerEvents825>,
    pending: Vec<PendingCallback>,
    on_status: Option<StatusChangeCallback>,
    on_access: Option<AccessCallback>,
    status: Option<StatusTracker>,
}

impl EventsHandler {
    pub fn new() -> Self {
        Self {
            last_events: None,
            pending: Vec::new(),
            on_status: None,
            on_access: None,
            status: None,
        }
    }

    /// Attach a status tracker for reporting handler state.
    pub fn set_status_tracker(&mut self, status: StatusTracker) {
        status.set("handler.events", "no_status");
        self.status = Some(status);
    }

    /// Get the last received events packet, if any.
    pub fn last_events(&self) -> Option<&AnswerEvents825> {
        self.last_events.as_ref()
    }

    /// Register a one-shot callback that fires when an event with the given
    /// type arrives. The callback receives the event record and returns a
    /// `Disposition` controlling whether the frame is forwarded or dropped.
    /// Register a callback that fires on every new status packet.
    /// Receives the previous status (if any) and the new one.
    pub fn on_status(
        &mut self,
        callback: impl Fn(Option<&AnswerEvents825>, &AnswerEvents825) + Send + 'static,
    ) {
        self.on_status = Some(Box::new(callback));
    }

    /// Register a callback that fires for every access event (granted, denied,
    /// intermediate, code accepted, or config mode) that carries card credentials.
    pub fn on_access(&mut self, callback: impl Fn(&AccessEvent) + Send + 'static) {
        self.on_access = Some(Box::new(callback));
    }

    pub fn once(
        &mut self,
        event_type: EventType,
        callback: impl FnOnce(&EventRecord) -> Disposition + Send + 'static,
    ) {
        self.pending.push(PendingCallback {
            event_type,
            callback: Box::new(callback),
        });
    }
}

impl FrameHandler for EventsHandler {
    fn reset(&mut self) {
        self.last_events = None;
        self.pending.clear();
        if let Some(ref status) = self.status {
            status.set("handler.events", "no_status");
        }
    }

    fn on_frame(
        &mut self,
        _ctx: &mut HandlerContext,
        handling: FrameHandling,
        from: Side,
        frame: &mut Frame,
    ) -> Disposition {
        if from != Side::Panel || handling != FrameHandling::Passthrough {
            return Disposition::Forward;
        }

        let Some(Ok(mut pkt)) = frame.parse::<AnswerEvents825>() else {
            return Disposition::Forward;
        };

        for slot in &mut pkt.events {
            let Some(event) = slot else { continue };

            if let Some(idx) = self
                .pending
                .iter()
                .position(|p| p.event_type == event.event_type)
            {
                let pending = self.pending.swap_remove(idx);
                if let Disposition::Drop = (pending.callback)(event) {
                    *slot = None;
                }
            }
        }

        // Repack: shift all Some slots to the front.
        pkt.events.sort_by_key(|s| s.is_none());

        frame.replace(pkt.clone());

        if let Some(cb) = &self.on_status {
            cb(self.last_events.as_ref(), &pkt);
        }

        if let Some(cb) = &self.on_access {
            for event in pkt
                .active_events()
                .filter(|e| e.event_type.has_access_event_data())
            {
                let data = AccessEventData::parse(&event.event_data);
                cb(&AccessEvent {
                    timestamp: event.timestamp,
                    outcome: event.event_type.access_outcome().unwrap(),
                    location: event.location,
                    site_code: data.site_code,
                    card_code: data.card_code(),
                    format_index: if data.is_primary_format() { 0 } else { 1 },
                });
            }
        }

        self.last_events = Some(pkt.clone());

        if let Some(ref status) = self.status {
            let active: Vec<u8> = (0..32u8).filter(|&i| pkt.ac825_status.output_active(i)).collect();
            let overridden: Vec<u8> = (0..32u8).filter(|&i| pkt.ac825_status.output_is_manual(i)).collect();
            status.set_detail(
                "handler.events",
                "has_status",
                serde_json::json!({
                    "active_outputs": active,
                    "overridden_outputs": overridden,
                }),
            );
        }

        Disposition::Forward
    }
}
