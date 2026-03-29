use std::convert::Infallible;

use crate::packet::Ac215Packet;

pub struct RequestFirmVer825Packet;

impl Ac215Packet for RequestFirmVer825Packet {
    type Error = Infallible;

    const PACKET_ID: Option<u8> = Some(0x6B);

    fn packet_id(&self) -> u8 {
        0x6B
    }

    fn into_bytes(self, out: &mut [u8; 468]) -> u16 {
        0
    }

    fn from_bytes(_header: &crate::packet::header::Ac215Header, _bytes: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self)
    }
}
