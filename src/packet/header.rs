use std::fmt::Debug;

use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{
    crypto::Cipher,
    packet::{address::Ac215Address, direction::Ac215PacketDirection},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventFlag {
    #[default]
    Unset,
    Set,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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
    event_flag: EventFlag,
    checksum: Checksum,
}

#[derive(Debug)]
pub enum ParseError {
    IoError(std::io::Error),

    InvalidMagic(u8),
    InvalidSourceAddress(u8, u8),
    InvalidDestinationAddress(u8, u8),
    InvalidChecksum { expected: u16, actual: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checksum {
    Legacy,
    Modern,
}

#[derive(Debug, Clone, Copy)]
pub enum ChecksumMode {
    Auto,
    ForceLegacy,
    ForceModern,
}

impl From<Checksum> for ChecksumMode {
    fn from(checksum: Checksum) -> Self {
        match checksum {
            Checksum::Legacy => ChecksumMode::ForceLegacy,
            Checksum::Modern => ChecksumMode::ForceModern,
        }
    }
}

impl ChecksumMode {
    fn resolve(self, command_id: u8) -> Checksum {
        match self {
            ChecksumMode::Auto => {
                if command_id == 66 || command_id == 68 || command_id == 106 {
                    Checksum::Legacy
                } else {
                    Checksum::Modern
                }
            }
            ChecksumMode::ForceLegacy => Checksum::Legacy,
            ChecksumMode::ForceModern => Checksum::Modern,
        }
    }
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

fn raw_size_for(data_length: u16, event_flag: EventFlag) -> Option<u8> {
    if event_flag == EventFlag::Set {
        let encrypted_len = pad16(data_length + 12);
        match encrypted_len {
            256 => Some(251),
            480 => Some(254),
            _ => None,
        }
    } else {
        if data_length > 250 {
            None
        } else {
            Some(data_length as u8)
        }
    }
}

fn is_event_buffer(raw_size: u8) -> bool {
    raw_size == 251 || raw_size == 254
}

impl Ac215Header {
    pub fn event_flag(&self) -> EventFlag {
        self.event_flag
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
        event_flag: EventFlag,
        checksum_mode: ChecksumMode,
    ) -> Option<Self> {
        Some(Self {
            direction,
            source,
            destination,
            transaction_id,
            command_id,
            raw_size: raw_size_for(data_length, event_flag)?,
            event_flag,
            checksum: checksum_mode.resolve(command_id),
        })
    }

    pub fn checksum(&self) -> Checksum {
        self.checksum
    }

    pub async fn parse_frame<R: AsyncRead + Unpin>(
        direction: Ac215PacketDirection,
        cipher: Cipher,
        reader: &mut R,
        checksum_mode: ChecksumMode,
        checksum_overrides: &std::collections::HashMap<Ac215TransactionId, Checksum>,
    ) -> Result<(Self, Vec<u8>), ParseError> {
        // Read magic sequence one byte at a time
        let magic: [u8; 4] = [0xFF, 0xFF, 0xFF, 0x55];
        for i in 0..4 {
            let byte = reader.read_u8().await.map_err(ParseError::IoError)?;
            if byte != magic[i] {
                return Err(ParseError::InvalidMagic(byte));
            }
        }

        let raw_size = reader.read_u8().await.map_err(ParseError::IoError)?;

        let encrypted_length = match raw_size {
            254 => 480usize,
            251 => 256,
            s => pad16(s as u16 + 5) as usize,
        };

        let mut encrypted_block = vec![0u8; encrypted_length];
        let mut checksum_data = vec![0u8; encrypted_length + 1];

        reader
            .read_exact(&mut encrypted_block)
            .await
            .map_err(ParseError::IoError)?;

        checksum_data[0] = raw_size; // length byte is included in checksum
        checksum_data[1..].copy_from_slice(&encrypted_block);

        cipher.decrypt(&mut encrypted_block);

        let is_event = is_event_buffer(raw_size);
        let offset = if is_event { 6 } else { 0 };
        let event_flag = if is_event {
            EventFlag::Set
        } else {
            EventFlag::Unset
        };

        let destination = Ac215Address::parse(encrypted_block[offset], encrypted_block[offset + 1])
            .ok_or(ParseError::InvalidDestinationAddress(
                encrypted_block[offset],
                encrypted_block[offset + 1],
            ))?;

        let source = Ac215Address::parse(encrypted_block[offset + 2], encrypted_block[offset + 3])
            .ok_or(ParseError::InvalidSourceAddress(
                encrypted_block[offset + 2],
                encrypted_block[offset + 3],
            ))?;

        let transaction_id = Ac215TransactionId::from_byte(encrypted_block[offset + 4]);
        let command_id = encrypted_block[offset + 5];

        let checksum = checksum_overrides
            .get(&transaction_id)
            .copied()
            .unwrap_or_else(|| checksum_mode.resolve(command_id));

        match checksum {
            Checksum::Legacy => {
                let expected_checksum = checksum_legacy(&checksum_data);
                let actual_checksum = reader.read_u8().await.map_err(ParseError::IoError)?;

                if expected_checksum != actual_checksum {
                    return Err(ParseError::InvalidChecksum {
                        expected: expected_checksum as u16,
                        actual: actual_checksum as u16,
                    });
                }
            }
            Checksum::Modern => {
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
                event_flag,
                checksum,
            },
            encrypted_block[offset + 6..].to_vec(),
        ))
    }

    pub fn into_frame(&self, cipher: Cipher, payload: &[u8]) -> Vec<u8> {
        let encrypted_len = self.encrypted_len();

        let mut frame = Vec::with_capacity(5 + encrypted_len + 2);
        frame.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0x55]);
        frame.push(self.raw_size);

        // write the decrypted payload into the frame, we'll encrypt in-place

        if self.event_flag == EventFlag::Set {
            frame.extend_from_slice(&[0u8; 6]); // ghost header
        }

        frame.extend_from_slice(&self.destination.into_bytes());
        frame.extend_from_slice(&self.source.into_bytes());
        frame.push(self.transaction_id.into_byte());
        frame.push(self.command_id);

        frame.extend_from_slice(payload);

        // paddinga (like bazinga)
        frame.resize(5 + encrypted_len, 0);

        // encrypt the payload in-place
        cipher.encrypt(&mut frame[5..]);

        // compute checksum over size byte + encrypted payload
        let checksum_data = &frame[4..];

        match self.checksum {
            Checksum::Legacy => {
                let checksum = checksum_legacy(checksum_data);
                frame.push(checksum);
            }
            Checksum::Modern => {
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
        let overhead = if self.event_flag == EventFlag::Set {
            12
        } else {
            6
        };
        self.encrypted_len() - overhead
    }
}
