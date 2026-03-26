use std::convert::Infallible;

use crate::packet::Ac215Packet;

/// KeepAlive (0xD0) — heartbeat sent by the panel.
pub struct KeepAlivePacket;

impl Ac215Packet for KeepAlivePacket {
    type Error = Infallible;

    fn packet_id(&self) -> u8 {
        0xD0
    }

    fn into_bytes(self, _out: &mut [u8; 467]) -> u16 {
        0
    }

    fn from_bytes(_bytes: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self)
    }
}

/// KeepAliveAck (0xD1) — server's response to the keepalive.
pub struct KeepAliveAckPacket;

impl Ac215Packet for KeepAliveAckPacket {
    type Error = Infallible;

    fn packet_id(&self) -> u8 {
        0xD1
    }

    fn into_bytes(self, _out: &mut [u8; 467]) -> u16 {
        0
    }

    fn from_bytes(_bytes: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self)
    }
}
