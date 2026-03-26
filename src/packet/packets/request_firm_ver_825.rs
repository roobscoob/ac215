use std::convert::Infallible;

use crate::packet::Ac215Packet;

pub struct RequestFirmVer825Packet;

impl Ac215Packet for RequestFirmVer825Packet {
    type Error = Infallible;

    fn packet_id(&self) -> u8 {
        0x6B
    }

    fn into_bytes(self, out: &mut [u8; 467]) -> u16 {
        0
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self)
    }
}
