use std::fmt;

use tokio::sync::oneshot;

use crate::packet::address::Addressing;
use crate::packet::header::{Ac215Header, ChecksumMode, EventFlag};
use crate::server::Channel;
use crate::server::Frame;

/// Identifies which side of the proxy a frame originated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The real panel.
    Panel,
    /// The server connecting through the proxy.
    Server,
}

impl Side {
    pub fn opposite(self) -> Self {
        match self {
            Side::Panel => Side::Server,
            Side::Server => Side::Panel,
        }
    }
}

/// Messages handled by the coordinator actor.
pub enum CoordinatorMsg {
    /// A parsed frame arrived from one side and should be relayed.
    Frame {
        from: Side,
        frame: Frame,
    },
    /// A reader disconnected.
    Disconnected {
        side: Side,
        channel: Channel,
    },
    /// Send a request to the panel on primary and deliver the response via oneshot.
    SendRequest {
        addressing: Addressing,
        command_id: u8,
        data: Vec<u8>,
        event_flag: EventFlag,
        checksum_mode: ChecksumMode,
        response_tx: oneshot::Sender<Frame>,
    },
    /// Fire-and-forget: send a packet to a target.
    Send {
        target: Side,
        channel: Channel,
        addressing: Addressing,
        command_id: u8,
        data: Vec<u8>,
        event_flag: EventFlag,
        checksum_mode: ChecksumMode,
    },
    /// Start buffering server→panel primary frames. The oneshot is completed
    /// once blocking is active **and** all in-flight panel requests have been
    /// resolved (pending count reaches zero).
    BlockServerPrimary {
        ready_tx: oneshot::Sender<()>,
    },
    /// Flush buffered frames and resume normal flow.
    UnblockServerPrimary,
}

impl fmt::Debug for CoordinatorMsg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frame { from, frame } => {
                write!(f, "Frame {{ from: {from:?}, channel: {:?}, .. }}", frame.channel())
            }
            Self::Disconnected { side, channel } => {
                write!(f, "Disconnected {{ side: {side:?}, channel: {channel:?} }}")
            }
            Self::SendRequest { .. } => write!(f, "SendRequest {{ .. }}"),
            Self::Send { target, channel, .. } => {
                write!(f, "Send {{ target: {target:?}, channel: {channel:?}, .. }}")
            }
            Self::BlockServerPrimary { .. } => write!(f, "BlockServerPrimary"),
            Self::UnblockServerPrimary => write!(f, "UnblockServerPrimary"),
        }
    }
}

/// Messages handled by a writer actor.
#[derive(Debug)]
pub enum WriterMsg {
    /// Serialize and write a frame to the TCP connection.
    Write {
        header: Ac215Header,
        payload: Vec<u8>,
    },
    /// Shut down the writer.
    Shutdown,
}
