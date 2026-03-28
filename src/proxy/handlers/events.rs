use log::info;

use crate::event::{EventRecord, EventType};
use crate::packet::packets::answer_events_825::AnswerEvents825;
use crate::server::Frame;

use super::super::message::Side;
use super::super::pipeline::{Disposition, FrameHandler, FrameHandling, HandlerContext};

type EventCallback = Box<dyn FnOnce(&EventRecord) -> Disposition + Send>;

/// Called whenever a new AnswerEvents825 is received.
/// Arguments: (previous status, new status).
type StatusChangeCallback = Box<dyn Fn(Option<&AnswerEvents825>, &AnswerEvents825) + Send>;

struct PendingCallback {
    event_type: EventType,
    callback: EventCallback,
}

pub struct EventsHandler {
    last_events: Option<AnswerEvents825>,
    pending: Vec<PendingCallback>,
    on_status: Option<StatusChangeCallback>,
}

impl EventsHandler {
    pub fn new() -> Self {
        Self {
            last_events: None,
            pending: Vec::new(),
            on_status: None,
        }
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

        self.last_events = Some(pkt);

        Disposition::Forward
    }
}
