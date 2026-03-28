use log::{error, trace};

use crate::packet::address::Addressing;
use crate::packet::header::{ChecksumMode, EventFlag};
use crate::packet::packets::nack::NackPacket;
use crate::packet::packets::send_log_message::SendLogMessagePacket;
use crate::server::Channel;
use crate::server::Frame;

use super::super::message::Side;
use super::super::pipeline::{Disposition, FrameHandler, FrameHandling, HandlerContext};

pub struct NackHandler {
    last_log: Option<SendLogMessagePacket>,
}

impl NackHandler {
    pub fn new() -> Self {
        Self { last_log: None }
    }

    /// Take the last intercepted nack log, if any.
    pub fn take_last_log(&mut self) -> Option<SendLogMessagePacket> {
        self.last_log.take()
    }
}

impl FrameHandler for NackHandler {
    fn on_frame(
        &mut self,
        ctx: &mut HandlerContext,
        handling: FrameHandling,
        from: Side,
        frame: &mut Frame,
    ) -> Disposition {
        if from != Side::Panel {
            return Disposition::Forward;
        }

        if handling != FrameHandling::Passthrough {
            return Disposition::Forward;
        }

        // Check for Nack packet (0x82)
        if frame.parse::<NackPacket>().is_some_and(|r| r.is_ok()) {
            let reason = self.last_log.take();
            if let Some(log) = reason {
                error!(
                    "Encountered NACK, Associated log: [{:?} {:?}] \"{}\"",
                    log.destination, log.severity, log.message
                );

                if handling == FrameHandling::Passthrough {
                    trace!("This NACK was not previously intercepted - replaying log");

                    ctx.send(
                        Channel::Async,
                        Addressing::dual().from_panel(),
                        EventFlag::Unset,
                        ChecksumMode::Auto,
                        log,
                    );
                }
            } else {
                error!("Encountered NACK, no associated log");
            }
            return Disposition::Forward;
        }

        // Check for PrepareNackResponse log message
        let Some(Ok(log)) = frame.parse::<SendLogMessagePacket>() else {
            return Disposition::Forward;
        };

        if !log.message.contains("PrepareNackResponse") {
            return Disposition::Forward;
        }

        trace!("intercepted nack log: {}", log.message);
        self.last_log = Some(log);

        Disposition::Drop
    }
}
