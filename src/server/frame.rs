use crate::packet::{Ac215Packet, header::Ac215Header};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// The connection initiated by the server to the panel's listening port.
    Primary,
    /// The connection initiated by the panel to the server's listening port (port + 1).
    Async,
}

pub struct Frame {
    header: Ac215Header,
    payload: Vec<u8>,
    channel: Channel,
}

impl Frame {
    pub(crate) fn new(header: Ac215Header, payload: Vec<u8>, channel: Channel) -> Self {
        Self {
            header,
            payload,
            channel,
        }
    }

    pub fn header(&self) -> &Ac215Header {
        &self.header
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn channel(&self) -> Channel {
        self.channel
    }

    pub fn parse<P: Ac215Packet>(&self) -> Option<Result<P, P::Error>> {
        if P::PACKET_ID.is_some_and(|id| id != self.header.command_id()) {
            return None;
        }

        Some(P::from_bytes(&self.header, &self.payload))
    }
}
