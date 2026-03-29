use std::convert::Infallible;

use crate::packet::Ac215Packet;

/// Nack (0x82) — empty negative acknowledgement packet.
pub struct NackPacket;

impl Ac215Packet for NackPacket {
    type Error = Infallible;

    const PACKET_ID: Option<u8> = Some(0x82);

    fn packet_id(&self) -> u8 {
        0x82
    }

    fn into_bytes(self, _out: &mut [u8; 468]) -> u16 {
        0
    }

    fn from_bytes(_header: &crate::packet::header::Ac215Header, _bytes: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self)
    }
}
