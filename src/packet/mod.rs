pub mod address;
pub mod direction;
pub mod header;
pub mod test;
pub mod packets;

pub trait Ac215Packet: Sized {
    type Error;

    fn packet_id(&self) -> u8;
    fn into_bytes(self, out: &mut [u8; 467]) -> u16;
    fn from_bytes(bytes: &[u8]) -> Result<Self, Self::Error>;
}
