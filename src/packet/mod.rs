pub mod address;
pub mod direction;
pub mod header;
pub mod packets;
pub mod pre_serialized;
pub mod test;

use header::Ac215Header;

pub trait Ac215Packet: Sized {
    type Error;

    const PACKET_ID: Option<u8>;

    fn packet_id(&self) -> u8;
    fn into_bytes(self, out: &mut [u8; 467]) -> u16;
    fn from_bytes(header: &Ac215Header, bytes: &[u8]) -> Result<Self, Self::Error>;
}
