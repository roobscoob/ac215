use ractor::{Actor, ActorProcessingErr, ActorRef};
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;

use crate::crypto::Cipher;

use super::message::WriterMsg;

pub struct WriterActor;

pub struct WriterState {
    writer: OwnedWriteHalf,
    cipher: Cipher,
}

pub struct WriterArgs {
    pub writer: OwnedWriteHalf,
    pub cipher: Cipher,
}

impl Actor for WriterActor {
    type Msg = WriterMsg;
    type State = WriterState;
    type Arguments = WriterArgs;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(WriterState {
            writer: args.writer,
            cipher: args.cipher,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        msg: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match msg {
            WriterMsg::Write { header, payload } => {
                let frame_bytes = header.into_frame(state.cipher.clone(), &payload);
                if let Err(e) = state.writer.write_all(&frame_bytes).await {
                    println!("[writer] Write error: {e}");
                    myself.stop(Some("write error".to_string()));
                }
            }
            WriterMsg::Shutdown => {
                myself.stop(Some("shutdown requested".to_string()));
            }
        }
        Ok(())
    }
}
