use std::sync::{Arc, Mutex};

use log::info;
use ractor::{Actor, ActorProcessingErr, ActorRef};

use crate::packet::address::Addressing;
use crate::packet::direction::Ac215PacketDirection;
use crate::packet::header::{Ac215Header, Ac215TransactionId, ChecksumMode, EventFlag};
use crate::server::Channel;
use crate::server::Frame;

use super::message::{CoordinatorMsg, Side, WriterMsg};
use super::pipeline::{ExtraPacket, FrameHandler, FrameHandling, Pipeline};
use super::status::StatusTracker;
use super::transaction::{ResponseRouting, TransactionRewriter};

pub struct CoordinatorActor;

pub struct CoordinatorState {
    panel_primary: ActorRef<WriterMsg>,
    panel_async: ActorRef<WriterMsg>,
    server_primary: ActorRef<WriterMsg>,
    server_async: ActorRef<WriterMsg>,
    pipeline: Pipeline,
    rewriter: TransactionRewriter,
    /// When true, server→panel primary frames are buffered instead of forwarded.
    server_primary_blocked: bool,
    server_primary_buffer: Vec<Frame>,
    status: StatusTracker,
}

pub struct CoordinatorArgs {
    pub panel_primary: ActorRef<WriterMsg>,
    pub panel_async: ActorRef<WriterMsg>,
    pub server_primary: ActorRef<WriterMsg>,
    pub server_async: ActorRef<WriterMsg>,
    pub handlers: Vec<Arc<Mutex<dyn FrameHandler>>>,
    pub status: StatusTracker,
}

impl CoordinatorState {
    fn writer(&self, target: Side, channel: Channel) -> &ActorRef<WriterMsg> {
        match (target, channel) {
            (Side::Panel, Channel::Primary) => &self.panel_primary,
            (Side::Panel, Channel::Async) => &self.panel_async,
            (Side::Server, Channel::Primary) => &self.server_primary,
            (Side::Server, Channel::Async) => &self.server_async,
        }
    }

    fn send_frame(&self, target: Side, frame: Frame) {
        let (header, payload, channel) = frame.into_parts();
        let writer = self.writer(target, channel);
        let _ = writer.send_message(WriterMsg::Write { header, payload });
    }

    fn shutdown_all(&self) {
        let _ = self.panel_primary.send_message(WriterMsg::Shutdown);
        let _ = self.panel_async.send_message(WriterMsg::Shutdown);
        let _ = self.server_primary.send_message(WriterMsg::Shutdown);
        let _ = self.server_async.send_message(WriterMsg::Shutdown);
    }
}

impl Actor for CoordinatorActor {
    type Msg = CoordinatorMsg;
    type State = CoordinatorState;
    type Arguments = CoordinatorArgs;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        args.status.set("coordinator", "relaying");
        Ok(CoordinatorState {
            panel_primary: args.panel_primary,
            panel_async: args.panel_async,
            server_primary: args.server_primary,
            server_async: args.server_async,
            pipeline: Pipeline::new(args.handlers),
            rewriter: TransactionRewriter::new(),
            server_primary_blocked: false,
            server_primary_buffer: Vec::new(),
            status: args.status,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        msg: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            CoordinatorMsg::Frame { from, frame } => {
                handle_frame(from, frame, state);
            }

            CoordinatorMsg::Disconnected { side, channel } => {
                info!("{:?}/{:?} disconnected, shutting down", side, channel);
                state.pipeline.reset();
                state.status.set_detail(
                    "coordinator",
                    "stopped",
                    serde_json::json!({
                        "reason": format!("{side:?}/{channel:?} disconnected"),
                    }),
                );
                state.shutdown_all();
                myself.stop(Some("disconnect".to_string()));
            }

            CoordinatorMsg::SendRequest {
                addressing,
                command_id,
                data,
                event_flag,
                checksum_mode,
                response_tx,
            } => {
                // Impostor: proxy pretending to be the server.
                let mut frame = build_frame(addressing, command_id, data, event_flag, checksum_mode, Channel::Primary);
                let header = state.rewriter.inject_to_panel(frame.header().clone(), response_tx);
                *frame.header_mut() = header;

                let result = state.pipeline.process(FrameHandling::Impostor, Side::Server, frame);
                send_extras(result.extra, state);
                if let Some(frame) = result.frame {
                    state.send_frame(Side::Panel, frame);
                }
            }

            CoordinatorMsg::Send {
                target,
                channel,
                addressing,
                command_id,
                data,
                event_flag,
                checksum_mode,
            } => {
                let frame = build_frame(addressing, command_id, data, event_flag, checksum_mode, channel);
                let result = state.pipeline.process(FrameHandling::Impostor, target, frame);
                send_extras(result.extra, state);
                if let Some(frame) = result.frame {
                    state.send_frame(target, frame);
                }
            }

            CoordinatorMsg::BlockServerPrimary => {
                state.server_primary_blocked = true;
                state.status.set_detail(
                    "coordinator",
                    "blocking",
                    serde_json::json!({
                        "pending_requests": state.rewriter.pending_count(),
                        "buffered_frames": state.server_primary_buffer.len(),
                    }),
                );
            }

            CoordinatorMsg::UnblockServerPrimary => {
                state.server_primary_blocked = false;
                let buffered: Vec<Frame> = state.server_primary_buffer.drain(..).collect();
                for frame in buffered {
                    forward_to_panel(frame, state);
                }
                state.status.set_detail(
                    "coordinator",
                    "tracking",
                    serde_json::json!({
                        "pending_requests": state.rewriter.pending_count(),
                    }),
                );
            }
        }
        Ok(())
    }
}

/// Build a Frame from packet-level params.
fn build_frame(
    addressing: Addressing,
    command_id: u8,
    data: Vec<u8>,
    event_flag: EventFlag,
    checksum_mode: ChecksumMode,
    channel: Channel,
) -> Frame {
    let header = Ac215Header::new(
        addressing.direction,
        addressing.source,
        addressing.destination,
        Ac215TransactionId::zero(),
        data.len() as u16,
        command_id,
        event_flag,
        checksum_mode,
    )
    .expect("failed to build header");
    Frame::new(header, data, channel)
}

/// Infer target side from addressing direction.
fn target_from_addressing(addressing: &Addressing) -> Side {
    match addressing.direction {
        Ac215PacketDirection::ToPanel => Side::Panel,
        Ac215PacketDirection::FromPanel => Side::Server,
    }
}

/// Process extra packets through the pipeline and send them.
/// Extras can themselves produce more extras, which are processed recursively.
fn send_extras(extras: Vec<ExtraPacket>, state: &mut CoordinatorState) {
    for extra in extras {
        let target = target_from_addressing(&extra.addressing);
        let frame = build_frame(
            extra.addressing,
            extra.command_id,
            extra.data,
            extra.event_flag,
            extra.checksum_mode,
            extra.channel,
        );
        let result = state.pipeline.process(FrameHandling::Impostor, target, frame);
        // Recurse for any extras produced by this extra.
        send_extras(result.extra, state);
        if let Some(frame) = result.frame {
            state.send_frame(target, frame);
        }
    }
}

/// Route an incoming frame through the pipeline and rewriter.
fn handle_frame(from: Side, frame: Frame, state: &mut CoordinatorState) {
    let channel = frame.channel();

    // Server→Panel on Primary: goes through the rewriter.
    if from == Side::Server && channel == Channel::Primary {
        let result = state.pipeline.process(FrameHandling::Passthrough, from, frame);
        send_extras(result.extra, state);
        if let Some(frame) = result.frame {
            if state.server_primary_blocked {
                state.server_primary_buffer.push(frame);
            } else {
                forward_to_panel(frame, state);
            }
        }
        return;
    }

    // Panel→Server on Primary: response — resolve through rewriter first,
    // then run through pipeline with the appropriate handling tag.
    if from == Side::Panel && channel == Channel::Primary {
        match state.rewriter.resolve_response(frame) {
            ResponseRouting::ToServer(frame) => {
                // Passthrough: server sent the request, panel responded.
                let result = state.pipeline.process(FrameHandling::Passthrough, from, frame);
                send_extras(result.extra, state);
                if let Some(frame) = result.frame {
                    state.send_frame(Side::Server, frame);
                }
            }
            ResponseRouting::ToProxy(frame, tx) => {
                // Intercepted: proxy sent the request, consuming the response.
                // Run through pipeline for logging, then deliver via oneshot.
                let result = state.pipeline.process(FrameHandling::Intercepted, from, frame);
                send_extras(result.extra, state);
                if let Some(frame) = result.frame {
                    let _ = tx.send(frame);
                }
            }
            ResponseRouting::Unknown(frame) => {
                // No mapping — pass through as normal.
                let result = state.pipeline.process(FrameHandling::Passthrough, from, frame);
                send_extras(result.extra, state);
                if let Some(frame) = result.frame {
                    state.send_frame(Side::Server, frame);
                }
            }
        }
        return;
    }

    // Everything else (async channel): pass through pipeline, no rewriting.
    let result = state.pipeline.process(FrameHandling::Passthrough, from, frame);
    send_extras(result.extra, state);
    if let Some(frame) = result.frame {
        state.send_frame(from.opposite(), frame);
    }
}

/// Rewrite a server request's transaction ID and send it to the panel.
fn forward_to_panel(frame: Frame, state: &mut CoordinatorState) {
    let was_tracking = state.rewriter.is_tracking();
    let (header, payload, channel) = frame.into_parts();
    let header = state.rewriter.forward_to_panel(header);
    state.send_frame(Side::Panel, Frame::new(header, payload, channel));

    if !was_tracking && state.rewriter.is_tracking() {
        state.status.set("coordinator", "tracking");
    }
}
