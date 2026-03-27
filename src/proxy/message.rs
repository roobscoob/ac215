use crate::packet::header::Ac215Header;
use crate::server::Channel;

/// Identifies which side of the proxy a frame originated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The real panel.
    Panel,
    /// The server connecting through the proxy.
    Server,
}

/// Messages handled by the coordinator actor.
#[derive(Debug)]
pub enum CoordinatorMsg {
    /// A parsed frame arrived from one side and should be relayed.
    Frame {
        from: Side,
        channel: Channel,
        header: Ac215Header,
        payload: Vec<u8>,
    },
    /// A reader disconnected.
    Disconnected { side: Side, channel: Channel },
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
