use std::convert::Infallible;

use crate::packet::Ac215Packet;

pub struct RequestEvents825Packet;

impl Ac215Packet for RequestEvents825Packet {
    type Error = Infallible;

    const PACKET_ID: Option<u8> = Some(0x48);

    fn packet_id(&self) -> u8 {
        0x48
    }

    fn into_bytes(self, _out: &mut [u8; 468]) -> u16 {
        0
    }

    fn from_bytes(_header: &crate::packet::header::Ac215Header, _bytes: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self)
    }
}
