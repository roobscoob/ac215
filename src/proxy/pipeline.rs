use std::sync::{Arc, Mutex};

use crate::packet::Ac215Packet;
use crate::packet::address::Addressing;
use crate::packet::header::{ChecksumMode, EventFlag};
use crate::server::{Channel, Frame};

use super::message::Side;

/// How the proxy is handling this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameHandling {
    /// Normal traffic forwarded between server and panel.
    Passthrough,
    /// Proxy is pretending to be the other side (proxy→panel or proxy→server).
    Impostor,
    /// Proxy is consuming this frame — the other side never sees it.
    Intercepted,
    /// A handler earlier in the pipeline dropped this frame.
    Dropped,
}

/// What a handler wants to do with a frame.
pub enum Disposition {
    /// Forward the frame to its normal destination (possibly after mutation).
    Forward,
    /// Drop the frame — the other side never sees it.
    Drop,
}

/// A packet queued by a handler to be sent before the current frame.
pub struct ExtraPacket {
    pub channel: Channel,
    pub addressing: Addressing,
    pub command_id: u8,
    pub data: Vec<u8>,
    pub event_flag: EventFlag,
    pub checksum_mode: ChecksumMode,
}

/// Context passed to handlers during pipeline processing.
///
/// Handlers can enqueue extra packets via [`send`](HandlerContext::send).
/// These are sent through the coordinator (with proper header construction)
/// before the original frame.
pub struct HandlerContext {
    extra: Vec<ExtraPacket>,
}

impl HandlerContext {
    fn new() -> Self {
        Self { extra: Vec::new() }
    }

    /// Enqueue a packet to be sent before the current frame.
    pub fn send<P: Ac215Packet>(
        &mut self,
        channel: Channel,
        addressing: Addressing,
        event_flag: EventFlag,
        checksum_mode: ChecksumMode,
        packet: P,
    ) {
        let mut buf = [0u8; 468];
        let command_id = packet.packet_id();
        let data_len = packet.into_bytes(&mut buf);
        let data = buf[..data_len as usize].to_vec();

        self.extra.push(ExtraPacket {
            channel,
            addressing,
            command_id,
            data,
            event_flag,
            checksum_mode,
        });
    }
}

/// Trait for frame processing handlers.
///
/// Handlers are called in order. When a handler returns `Drop`, the handling
/// changes to `Dropped` and remaining handlers still see the frame but it
/// will not be forwarded.
///
/// A handler may mutate the frame in place before returning `Forward`.
pub trait FrameHandler: Send {
    fn on_frame(
        &mut self,
        ctx: &mut HandlerContext,
        handling: FrameHandling,
        from: Side,
        frame: &mut Frame,
    ) -> Disposition;
}

/// Result of pipeline processing.
pub struct PipelineResult {
    /// The original frame, if it was not dropped.
    pub frame: Option<Frame>,
    /// Extra packets emitted by handlers, to be sent before the original.
    pub extra: Vec<ExtraPacket>,
}

/// Processes frames through a chain of handlers.
pub struct Pipeline {
    handlers: Vec<Arc<Mutex<dyn FrameHandler>>>,
}

impl Pipeline {
    pub fn new(handlers: Vec<Arc<Mutex<dyn FrameHandler>>>) -> Self {
        Self { handlers }
    }

    /// Run the frame through all handlers.
    pub fn process(
        &mut self,
        handling: FrameHandling,
        from: Side,
        mut frame: Frame,
    ) -> PipelineResult {
        let mut handling = handling;
        let mut ctx = HandlerContext::new();
        for handler in &self.handlers {
            match handler.lock().unwrap().on_frame(&mut ctx, handling, from, &mut frame) {
                Disposition::Forward => continue,
                Disposition::Drop => handling = FrameHandling::Dropped,
            }
        }
        PipelineResult {
            frame: (handling != FrameHandling::Dropped).then_some(frame),
            extra: ctx.extra,
        }
    }
}
