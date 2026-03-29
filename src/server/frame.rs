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
    pub fn new(header: Ac215Header, payload: Vec<u8>, channel: Channel) -> Self {
        Self {
            header,
            payload,
            channel,
        }
    }

    pub fn header(&self) -> &Ac215Header {
        &self.header
    }

    pub fn header_mut(&mut self) -> &mut Ac215Header {
        &mut self.header
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn payload_mut(&mut self) -> &mut Vec<u8> {
        &mut self.payload
    }

    pub fn channel(&self) -> Channel {
        self.channel
    }

    pub fn into_parts(self) -> (Ac215Header, Vec<u8>, Channel) {
        (self.header, self.payload, self.channel)
    }

    pub fn parse<P: Ac215Packet>(&self) -> Option<Result<P, P::Error>> {
        if P::PACKET_ID.is_some_and(|id| id != self.header.command_id()) {
            return None;
        }

        Some(P::from_bytes(&self.header, &self.payload))
    }

    /// Replace this frame's contents with a new packet.
    ///
    /// Rebuilds the header with the new packet's command ID and data length,
    /// preserving addressing, transaction ID, event flag, and checksum mode.
    pub fn replace<P: Ac215Packet>(&mut self, packet: P) {
        let mut buf = [0u8; 468];
        let command_id = packet.packet_id();
        let data_len = packet.into_bytes(&mut buf);

        let old = &self.header;
        self.header = Ac215Header::new(
            old.direction(),
            old.source(),
            old.destination(),
            old.transaction_id(),
            data_len,
            command_id,
            old.event_flag(),
            old.checksum().into(),
        )
        .expect("failed to rebuild header for replace");

        self.payload.clear();
        self.payload.extend_from_slice(&buf[..data_len as usize]);
    }
}
