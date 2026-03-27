use std::collections::HashMap;

use ractor::ActorRef;
use tokio::net::tcp::OwnedReadHalf;

use crate::crypto::Cipher;
use crate::packet::direction::Ac215PacketDirection;
use crate::packet::header::{Ac215Header, ChecksumMode};
use crate::server::Channel;

use super::message::{CoordinatorMsg, Side};

/// Spawn a reader task that parses frames from a TCP connection and
/// forwards them to the coordinator actor.
///
/// This is a plain tokio task rather than an actor because it is a
/// producer — it generates messages instead of consuming them.
pub fn spawn_reader(
    reader: OwnedReadHalf,
    cipher: Cipher,
    direction: Ac215PacketDirection,
    checksum_mode: ChecksumMode,
    side: Side,
    channel: Channel,
    coordinator: ActorRef<CoordinatorMsg>,
) {
    tokio::spawn(async move {
        let mut reader = reader;
        let no_overrides = HashMap::new();

        loop {
            let result = Ac215Header::parse_frame(
                direction,
                cipher.clone(),
                &mut reader,
                checksum_mode,
                &no_overrides,
            )
            .await;

            match result {
                Ok((header, payload)) => {
                    let msg = CoordinatorMsg::Frame {
                        from: side,
                        channel,
                        header,
                        payload,
                    };
                    if coordinator.send_message(msg).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    println!("[reader] {side:?}/{channel:?} parse error: {e:?}");
                    let _ = coordinator.send_message(CoordinatorMsg::Disconnected {
                        side,
                        channel,
                    });
                    break;
                }
            }
        }
    });
}
