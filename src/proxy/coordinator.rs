use ractor::{Actor, ActorProcessingErr, ActorRef};

use crate::server::Channel;

use super::message::{CoordinatorMsg, Side, WriterMsg};

pub struct CoordinatorActor;

pub struct CoordinatorState {
    panel_primary: ActorRef<WriterMsg>,
    panel_async: ActorRef<WriterMsg>,
    server_primary: ActorRef<WriterMsg>,
    server_async: ActorRef<WriterMsg>,
}

pub struct CoordinatorArgs {
    pub panel_primary: ActorRef<WriterMsg>,
    pub panel_async: ActorRef<WriterMsg>,
    pub server_primary: ActorRef<WriterMsg>,
    pub server_async: ActorRef<WriterMsg>,
}

impl CoordinatorState {
    /// Given the side a frame came FROM, return the writer for the OTHER side
    /// on the same channel.
    fn target(&self, from: Side, channel: Channel) -> &ActorRef<WriterMsg> {
        match (from, channel) {
            (Side::Panel, Channel::Primary) => &self.server_primary,
            (Side::Panel, Channel::Async) => &self.server_async,
            (Side::Server, Channel::Primary) => &self.panel_primary,
            (Side::Server, Channel::Async) => &self.panel_async,
        }
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
        Ok(CoordinatorState {
            panel_primary: args.panel_primary,
            panel_async: args.panel_async,
            server_primary: args.server_primary,
            server_async: args.server_async,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        msg: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            CoordinatorMsg::Frame {
                from,
                channel,
                header,
                payload,
            } => {
                println!(
                    "[proxy] {:?}/{:?} cmd=0x{:02X} {:?}->{:?} ({} bytes)",
                    from,
                    channel,
                    header.command_id(),
                    header.source(),
                    header.destination(),
                    payload.len(),
                );

                let target = state.target(from, channel);
                let _ = target.send_message(WriterMsg::Write { header, payload });
            }
            CoordinatorMsg::Disconnected { side, channel } => {
                println!("[proxy] {:?}/{:?} disconnected, shutting down", side, channel);
                state.shutdown_all();
                myself.stop(Some("disconnect".to_string()));
            }
        }
        Ok(())
    }
}
