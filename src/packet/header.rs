use std::fmt::Debug;

use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{
    crypto::Cipher,
    packet::{Ac215Packet, address::Ac215Address, direction::Ac215PacketDirection},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ac215TransactionId {
    value: u8,
}

impl Ac215TransactionId {
    pub(crate) fn from_byte(byte: u8) -> Self {
        Self { value: byte }
    }

    pub(crate) fn into_byte(self) -> u8 {
        self.value
    }
}

impl Debug for Ac215TransactionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ac215TransactionId({:#02})", self.value)
    }
}

#[derive(Debug)]
pub struct Ac215Header {
    direction: Ac215PacketDirection,
    source: Ac215Address,
    destination: Ac215Address,
    transaction_id: Ac215TransactionId,
    command_id: u8,
    raw_size: u8,
}

#[derive(Debug)]
pub enum ParseError {
    IoError(std::io::Error),

    InvalidMagic([u8; 4]),
    InvalidSourceAddress(u8, u8),
    InvalidDestinationAddress(u8, u8),
    InvalidChecksum { expected: u16, actual: u16 },
}

pub enum ChecksumMode {
    Auto,
    ForceLegacy,
    ForceModern,
}

pub fn checksum_legacy(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

pub fn checksum_modern(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc & 0xFFFF
}

fn pad16(len: u16) -> u16 {
    let rem = len & 0xF;
    if rem != 0 { len - rem + 16 } else { len }
}

fn raw_size_for(data_length: u16, event_flag: bool) -> Option<u8> {
    let overhead: u16 = if event_flag { 12 } else { 6 };
    let encrypted_len = pad16(data_length + overhead);

    if event_flag {
        match encrypted_len {
            256 => Some(251),
            480 => Some(254),
            _ => None,
        }
    } else {
        let size_byte = encrypted_len - 5;
        if size_byte > 250 {
            None
        } else {
            Some(size_byte as u8)
        }
    }
}

impl Ac215Header {
    fn is_event(&self) -> bool {
        self.raw_size == 254 || self.raw_size == 251
    }

    fn encrypted_len(&self) -> usize {
        match self.raw_size {
            254 => 480,
            251 => 256,
            s => pad16(s as u16 + 5) as usize,
        }
    }

    pub fn new(
        direction: Ac215PacketDirection,
        source: Ac215Address,
        destination: Ac215Address,
        transaction_id: Ac215TransactionId,
        data_length: u16,
        command_id: u8,
        event_flag: bool,
    ) -> Option<Self> {
        let raw_size = raw_size_for(data_length, event_flag)?;

        Some(Self {
            direction,
            source,
            destination,
            transaction_id,
            command_id,
            raw_size,
        })
    }

    pub async fn parse_frame<R: AsyncRead + Unpin>(
        direction: Ac215PacketDirection,
        cipher: Cipher,
        reader: &mut R,
        checksum_mode: ChecksumMode,
    ) -> Result<(Self, Vec<u8>), ParseError> {
        // Read magic + length byte atomically
        let mut header = [0u8; 5];

        reader
            .read_exact(&mut header)
            .await
            .map_err(ParseError::IoError)?;

        if header[0..4] != [0xFF, 0xFF, 0xFF, 0x55] {
            return Err(ParseError::InvalidMagic([
                header[0], header[1], header[2], header[3],
            ]));
        }

        let raw_size = header[4];
        let is_event = matches!(
            (direction, raw_size),
            (Ac215PacketDirection::FromPanel, 254) | (Ac215PacketDirection::FromPanel, 251)
        );

        let encrypted_length = match raw_size {
            254 => 480usize,
            251 => 256,
            s => pad16(s as u16 + 5) as usize,
        };

        let mut encrypted_block = vec![0u8; encrypted_length as usize];
        let mut checksum_data = vec![0u8; encrypted_length as usize + 1];

        reader
            .read_exact(&mut encrypted_block)
            .await
            .map_err(ParseError::IoError)?;

        checksum_data[0] = header[4]; // length byte is included in checksum
        checksum_data[1..].copy_from_slice(&encrypted_block);

        cipher.decrypt(&mut encrypted_block);

        let offset = if is_event { 6 } else { 0 };

        let destination =
            Ac215Address::parse(encrypted_block[offset + 0], encrypted_block[offset + 1]).ok_or(
                ParseError::InvalidDestinationAddress(
                    encrypted_block[offset + 0],
                    encrypted_block[offset + 1],
                ),
            )?;

        let source = Ac215Address::parse(encrypted_block[offset + 2], encrypted_block[offset + 3])
            .ok_or(ParseError::InvalidSourceAddress(
                encrypted_block[offset + 2],
                encrypted_block[offset + 3],
            ))?;

        let transaction_id = Ac215TransactionId::from_byte(encrypted_block[offset + 4]);
        let command_id = encrypted_block[offset + 5];

        match checksum_mode {
            ChecksumMode::Auto => {
                if command_id == 66 || command_id == 68 || command_id == 106 {
                    let expected_checksum = checksum_legacy(&checksum_data);
                    let actual_checksum = reader.read_u8().await.map_err(ParseError::IoError)?;

                    if expected_checksum != actual_checksum {
                        return Err(ParseError::InvalidChecksum {
                            expected: expected_checksum as u16,
                            actual: actual_checksum as u16,
                        });
                    }
                } else {
                    let expected_checksum = checksum_modern(&checksum_data);
                    let actual_checksum = reader.read_u16().await.map_err(ParseError::IoError)?;

                    if expected_checksum != actual_checksum {
                        return Err(ParseError::InvalidChecksum {
                            expected: expected_checksum,
                            actual: actual_checksum,
                        });
                    }
                }
            }
            ChecksumMode::ForceLegacy => {
                let expected_checksum = checksum_legacy(&checksum_data);
                let actual_checksum = reader.read_u8().await.map_err(ParseError::IoError)?;

                if expected_checksum != actual_checksum {
                    return Err(ParseError::InvalidChecksum {
                        expected: expected_checksum as u16,
                        actual: actual_checksum as u16,
                    });
                }
            }
            ChecksumMode::ForceModern => {
                let expected_checksum = checksum_modern(&checksum_data);
                let actual_checksum = reader.read_u16().await.map_err(ParseError::IoError)?;

                if expected_checksum != actual_checksum {
                    return Err(ParseError::InvalidChecksum {
                        expected: expected_checksum,
                        actual: actual_checksum,
                    });
                }
            }
        }

        Ok((
            Self {
                direction,
                source,
                destination,
                transaction_id,
                raw_size,
                command_id,
            },
            encrypted_block[offset + 6..].to_vec(),
        ))
    }

    pub fn into_frame<P: Ac215Packet>(
        &self,
        cipher: Cipher,
        payload: P,
        checksum_mode: ChecksumMode,
    ) -> Vec<u8> {
        let encrypted_len = self.encrypted_len();

        let mut frame = Vec::with_capacity(5 + encrypted_len + 2); // magic + length + payload + checksum
        frame.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0x55]);
        frame.push(self.raw_size);

        // write the encrypted payload directly into the frame. we'll encrypt in-place

        if self.is_event() {
            frame.extend_from_slice(&[0u8; 6]);
        }

        frame.extend_from_slice(&self.destination.into_bytes());
        frame.extend_from_slice(&self.source.into_bytes());
        frame.push(self.transaction_id.into_byte());
        frame.push(self.command_id);

        let mut payload_bytes = [0u8; 467];
        let payload_length = payload.into_bytes(&mut payload_bytes);
        frame.extend_from_slice(&payload_bytes[..payload_length as usize]);

        //paddinga (like bazinga)
        frame.resize(5 + encrypted_len, 0);

        // encrypt the payload in-place

        cipher.encrypt(&mut frame[5..]);

        // compute checksum

        let checksum_data = &frame[4..]; // length byte + encrypted payload

        match checksum_mode {
            ChecksumMode::Auto => {
                if self.command_id == 66 || self.command_id == 68 || self.command_id == 106 {
                    let checksum = checksum_legacy(checksum_data);
                    frame.push(checksum);
                } else {
                    let checksum = checksum_modern(checksum_data);
                    frame.extend_from_slice(&checksum.to_be_bytes());
                }
            }
            ChecksumMode::ForceLegacy => {
                let checksum = checksum_legacy(checksum_data);
                frame.push(checksum);
            }
            ChecksumMode::ForceModern => {
                let checksum = checksum_modern(checksum_data);
                frame.extend_from_slice(&checksum.to_be_bytes());
            }
        }

        frame
    }

    pub fn direction(&self) -> Ac215PacketDirection {
        self.direction
    }

    pub fn source(&self) -> Ac215Address {
        self.source
    }

    pub fn destination(&self) -> Ac215Address {
        self.destination
    }

    pub fn transaction_id(&self) -> Ac215TransactionId {
        self.transaction_id
    }

    pub fn command_id(&self) -> u8 {
        self.command_id
    }

    pub fn data_length(&self) -> usize {
        match self.raw_size {
            254 => 480 - 12,
            251 => 256 - 12,
            s => s as usize - 5,
        }
    }
}
