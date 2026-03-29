use super::{Ac215Packet, header::Ac215Header};

/// A packet that has already been serialized to bytes.
/// Useful for replaying captured or pre-built payloads through the framing layer.
pub struct PreSerialized {
    id: u8,
    data: [u8; 468],
    len: u16,
}

impl PreSerialized {
    pub fn new(id: u8, data: &[u8]) -> Self {
        let len = data.len() as u16;
        let mut buf = [0u8; 468];
        buf[..data.len()].copy_from_slice(data);
        Self { id, data: buf, len }
    }
}

impl Ac215Packet for PreSerialized {
    type Error = std::convert::Infallible;

    const PACKET_ID: Option<u8> = None;

    fn packet_id(&self) -> u8 {
        self.id
    }

    fn into_bytes(self, out: &mut [u8; 468]) -> u16 {
        out[..self.len as usize].copy_from_slice(&self.data[..self.len as usize]);
        self.len
    }

    fn from_bytes(header: &Ac215Header, bytes: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self::new(header.command_id(), bytes))
    }
}
