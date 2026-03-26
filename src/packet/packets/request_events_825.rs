use std::convert::Infallible;

use crate::packet::Ac215Packet;

pub struct RequestEvents825Packet;

impl Ac215Packet for RequestEvents825Packet {
    type Error = Infallible;

    fn packet_id(&self) -> u8 {
        0x48
    }

    fn into_bytes(self, _out: &mut [u8; 467]) -> u16 {
        0
    }

    fn from_bytes(_bytes: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self)
    }
}
