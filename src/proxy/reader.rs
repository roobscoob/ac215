use std::collections::HashMap;

use log::{trace, warn};
use ractor::ActorRef;
use tokio::net::tcp::OwnedReadHalf;

use crate::crypto::Cipher;
use crate::packet::direction::Ac215PacketDirection;
use crate::packet::header::{Ac215Header, ChecksumMode, ParseError};
use crate::server::Channel;
use crate::server::Frame;

use super::message::{CoordinatorMsg, Side};
use super::recording_reader::RecordingReader;

const GARBAGE_FLUSH_THRESHOLD: usize = 64;

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
        let mut reader = RecordingReader::new(reader);
        let no_overrides = HashMap::new();
        let mut garbage_buf: Vec<u8> = Vec::new();

        loop {
            let result = Ac215Header::parse_frame(
                direction,
                cipher.clone(),
                &mut reader,
                checksum_mode,
                &no_overrides,
            )
            .await;

            let raw_bytes = reader.take_recorded();

            match result {
                Ok((header, payload)) => {
                    if !garbage_buf.is_empty() {
                        trace!(
                            "{side:?}/{channel:?} discarding {} bytes of garbage: {garbage_buf:02X?}",
                            garbage_buf.len(),
                        );
                        garbage_buf.clear();
                    }

                    trace!(
                        "{side:?}/{channel:?} raw ({} bytes): {raw_bytes:02X?}",
                        raw_bytes.len(),
                    );

                    let msg = CoordinatorMsg::Frame {
                        from: side,
                        frame: Frame::new(header, payload, channel),
                    };
                    if coordinator.send_message(msg).is_err() {
                        break;
                    }
                }
                Err(ParseError::InvalidMagic(_)) => {
                    garbage_buf.extend_from_slice(&raw_bytes);
                    if garbage_buf.len() >= GARBAGE_FLUSH_THRESHOLD {
                        trace!(
                            "{side:?}/{channel:?} discarding {} bytes of garbage: {garbage_buf:02X?}",
                            garbage_buf.len(),
                        );
                        garbage_buf.clear();
                    }
                }
                Err(e) => {
                    warn!("{side:?}/{channel:?} parse error: {e:?}");
                    let _ =
                        coordinator.send_message(CoordinatorMsg::Disconnected { side, channel });
                    break;
                }
            }
        }
    });
}
