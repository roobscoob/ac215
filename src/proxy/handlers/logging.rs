use log::debug;

use crate::packet::named_id::NamedPacketId;
use crate::packet::packets::answer_events_825::AnswerEvents825;
use crate::packet::packets::send_log_message::SendLogMessagePacket;
use crate::server::Channel;
use crate::server::Frame;

use super::super::message::Side;
use super::super::pipeline::{Disposition, FrameHandler, FrameHandling, HandlerContext};

pub struct LoggingHandler;

impl FrameHandler for LoggingHandler {
    fn on_frame(
        &mut self,
        _ctx: &mut HandlerContext,
        handling: FrameHandling,
        from: Side,
        frame: &mut Frame,
    ) -> Disposition {
        let header = frame.header();
        let channel = frame.channel();
        let cmd = NamedPacketId(header.command_id());

        const GREY: &str = "\x1b[90m";
        const GREEN: &str = "\x1b[32m";
        const BLUE: &str = "\x1b[34m";
        const YELLOW: &str = "\x1b[33m";
        const CYAN: &str = "\x1b[36m";
        const RESET: &str = "\x1b[0m";

        let channel_color = match channel {
            Channel::Primary => GREEN,
            Channel::Async => BLUE,
        };

        let side_color = match from {
            Side::Server => CYAN,
            Side::Panel => YELLOW,
        };

        let marker = match handling {
            FrameHandling::Passthrough => " ",
            FrameHandling::Impostor => "*",
            FrameHandling::Intercepted => "-",
            FrameHandling::Dropped => "x",
        };

        // Pad visible text first, then inject color codes so alignment isn't broken.
        let cmd_visible = format!("{} ({} bytes)", cmd, frame.payload().len());
        let padding = 50usize.saturating_sub(cmd_visible.len());
        let cmd_colored = format!(
            "{channel_color}{}{RESET} {GREY}({} bytes){RESET}{:padding$}",
            cmd,
            frame.payload().len(),
            "",
        );

        debug!(
            "{GREY}[{RESET}{channel_color}{:<7}{RESET} {GREY}({RESET}{side_color}{}{RESET}{GREY}){RESET}{:<pad$}{GREY}]{RESET} {marker} {cmd_colored} {GREY}| {src:?} -> {dst:?}{RESET}",
            format!("{:?}", channel),
            format!("{:?}", from),
            "",
            pad = 6 - format!("{:?}", from).len(),
            src = header.source(),
            dst = header.destination(),
        );

        if let Some(Ok(log)) = frame.parse::<SendLogMessagePacket>() {
            let SendLogMessagePacket {
                destination,
                severity,
                message,
            } = log;

            debug!(
                "{GREY}[{RESET}{channel_color}{:<7}{RESET} {GREY}({RESET}{side_color}{}{RESET}{GREY}){RESET}{:<pad$}{GREY}]{RESET} {marker} {GREY}LOG to {destination:?} {severity:?}: {message}{RESET}",
                format!("{:?}", channel),
                format!("{:?}", from),
                "",
                pad = 6 - format!("{:?}", from).len(),
            );
        }

        if let Some(Ok(pkt)) = frame.parse::<AnswerEvents825>() {
            for event in pkt.active_events() {
                debug!(
                    "{GREY}[{RESET}{channel_color}{:<7}{RESET} {GREY}({RESET}{side_color}{}{RESET}{GREY}){RESET}{:<pad$}{GREY}]{RESET} {marker} EVT #{evn} {ts} {typ:?} @ {loc:?}",
                    format!("{:?}", channel),
                    format!("{:?}", from),
                    "",
                    pad = 6 - format!("{:?}", from).len(),
                    evn = event.event_number,
                    ts = event.timestamp,
                    typ = event.event_type,
                    loc = event.location,
                );
            }
        }

        Disposition::Forward
    }
}
