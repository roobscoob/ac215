use core::fmt;

use crate::packet::Ac215Packet;

/// Output relay operation type.
///
/// Maps to the C# `OutputOperationType` enum:
///   0 = NoChange
///   1 = CloseAndReturnToDefault
///   2 = OpenMomentarily (pulse for timer duration, then re-lock)
///   3 = OpenPermanently (stay open until explicitly closed)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputOperationType {
    NoChange,
    CloseAndReturnToDefault,
    OpenMomentarily,
    OpenPermanently,
    Unknown(u8),
}

impl OutputOperationType {
    fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::NoChange,
            1 => Self::CloseAndReturnToDefault,
            2 => Self::OpenMomentarily,
            3 => Self::OpenPermanently,
            v => Self::Unknown(v),
        }
    }

    fn into_byte(self) -> u8 {
        match self {
            Self::NoChange => 0,
            Self::CloseAndReturnToDefault => 1,
            Self::OpenMomentarily => 2,
            Self::OpenPermanently => 3,
            Self::Unknown(v) => v,
        }
    }
}

/// A single output relay operation: what to do and for how long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputOperation {
    pub op_type: OutputOperationType,
    pub timer_minutes: u8,
    pub timer_seconds: u8,
}

impl Default for OutputOperation {
    fn default() -> Self {
        Self {
            op_type: OutputOperationType::NoChange,
            timer_minutes: 0,
            timer_seconds: 0,
        }
    }
}

/// Whether this is from an AC-825 (32 outputs) or a legacy panel (16 outputs).
///
/// Determined by payload length:
///   - Legacy: Data[64], 16 outputs × 4 bytes
///   - AC-825: Data[129], 32 outputs × 4 bytes + 1 reserved byte
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualOutputVariant {
    Legacy,
    Ac825,
}

/// CMD 0x22 — Manual Output Operation.
///
/// Controls output relays on the panel. Each output slot is 4 bytes on the wire:
///
/// ```text
/// [0] op_type        (OutputOperationType)
/// [1] reserved       (always 0)
/// [2] timer_minutes
/// [3] timer_seconds
/// ```
///
/// The server sets operations on specific output IDs (1-indexed in the DB,
/// mapped to 0-indexed slots here). Outputs not being changed have OpType::NoChange.
#[derive(Debug, Clone)]
pub struct ManualOutputPacket {
    pub variant: ManualOutputVariant,
    pub outputs: [OutputOperation; 32],
}

#[derive(Debug)]
pub enum ManualOutputError {
    TooShort { expected: usize, actual: usize },
}

impl fmt::Display for ManualOutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { expected, actual } => {
                write!(
                    f,
                    "payload too short: need {} bytes, got {}",
                    expected, actual
                )
            }
        }
    }
}

impl ManualOutputPacket {
    /// Create a new packet that pulses the given 1-indexed output IDs for the specified duration.
    pub fn pulse(output_ids: &[u8], minutes: u8, seconds: u8) -> Self {
        let mut outputs = [OutputOperation::default(); 32];
        for &id in output_ids {
            if id >= 1 && (id as usize) <= 32 {
                outputs[id as usize - 1] = OutputOperation {
                    op_type: OutputOperationType::OpenMomentarily,
                    timer_minutes: minutes,
                    timer_seconds: seconds,
                };
            }
        }
        Self {
            variant: ManualOutputVariant::Ac825,
            outputs,
        }
    }
}

impl Ac215Packet for ManualOutputPacket {
    type Error = ManualOutputError;

    const PACKET_ID: Option<u8> = Some(0x22);

    fn packet_id(&self) -> u8 {
        0x22
    }

    fn into_bytes(self, out: &mut [u8; 468]) -> u16 {
        let count = match self.variant {
            ManualOutputVariant::Legacy => 16,
            ManualOutputVariant::Ac825 => 32,
        };

        for i in 0..count {
            let base = i * 4;
            out[base] = self.outputs[i].op_type.into_byte();
            out[base + 1] = 0; // reserved
            out[base + 2] = self.outputs[i].timer_minutes;
            out[base + 3] = self.outputs[i].timer_seconds;
        }

        match self.variant {
            ManualOutputVariant::Legacy => 64,
            ManualOutputVariant::Ac825 => {
                out[128] = 0; // trailing reserved byte
                129
            }
        }
    }

    fn from_bytes(
        _header: &crate::packet::header::Ac215Header,
        bytes: &[u8],
    ) -> Result<Self, Self::Error> {
        let len = bytes.len();

        let (variant, count) = if len >= 129 {
            (ManualOutputVariant::Ac825, 32)
        } else if len >= 64 {
            (ManualOutputVariant::Legacy, 16)
        } else {
            return Err(ManualOutputError::TooShort {
                expected: 64,
                actual: len,
            });
        };

        let mut outputs = [OutputOperation::default(); 32];
        for i in 0..count {
            let base = i * 4;
            outputs[i] = OutputOperation {
                op_type: OutputOperationType::from_byte(bytes[base]),
                timer_minutes: bytes[base + 2],
                timer_seconds: bytes[base + 3],
            };
        }

        Ok(Self { variant, outputs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::address::Ac215Address;
    use crate::packet::direction::Ac215PacketDirection;
    use crate::packet::header::EventFlag;
    use crate::packet::header::{Ac215Header, Ac215TransactionId};

    fn dummy_header() -> Ac215Header {
        Ac215Header::new(
            Ac215PacketDirection::ToPanel,
            Ac215Address::SERVER,
            Ac215Address::SERVER,
            Ac215TransactionId::from_byte(0),
            0,
            0,
            EventFlag::Unset,
            crate::packet::header::ChecksumMode::Auto,
        )
        .unwrap()
    }

    #[test]
    fn parse_captured_payload() {
        // Captured from real traffic: pulse outputs 1-6 for 4 seconds each
        let payload: &[u8] = &[
            0x02, 0x00, 0x00, 0x04, // output 1: OpenMomentarily, 0:04
            0x02, 0x00, 0x00, 0x04, // output 2: OpenMomentarily, 0:04
            0x02, 0x00, 0x00, 0x04, // output 3
            0x02, 0x00, 0x00, 0x04, // output 4
            0x02, 0x00, 0x00, 0x04, // output 5
            0x02, 0x00, 0x00, 0x04, // output 6
            0x00, 0x00, 0x00, 0x00, // output 7: NoChange
            0x00, 0x00, 0x00, 0x00, // output 8
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 9-10
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 11-12
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 13-14
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 15-16
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 17-18
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 19-20
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 21-22
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 23-24
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 25-26
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 27-28
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 29-30
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 31-32
            0x00, // reserved trailing byte
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // AES padding
        ];

        let pkt = ManualOutputPacket::from_bytes(&dummy_header(), payload).unwrap();

        assert_eq!(pkt.variant, ManualOutputVariant::Ac825);

        for i in 0..6 {
            assert_eq!(pkt.outputs[i].op_type, OutputOperationType::OpenMomentarily);
            assert_eq!(pkt.outputs[i].timer_minutes, 0);
            assert_eq!(pkt.outputs[i].timer_seconds, 4);
        }

        for i in 6..32 {
            assert_eq!(pkt.outputs[i].op_type, OutputOperationType::NoChange);
        }
    }

    #[test]
    fn round_trip() {
        let pkt = ManualOutputPacket::pulse(&[1, 2, 3], 1, 30);

        let mut out = [0u8; 468];
        let len = pkt.clone().into_bytes(&mut out);
        assert_eq!(len, 129);

        let pkt2 = ManualOutputPacket::from_bytes(&dummy_header(), &out[..len as usize]).unwrap();

        for i in 0..3 {
            assert_eq!(
                pkt2.outputs[i].op_type,
                OutputOperationType::OpenMomentarily
            );
            assert_eq!(pkt2.outputs[i].timer_minutes, 1);
            assert_eq!(pkt2.outputs[i].timer_seconds, 30);
        }
        for i in 3..32 {
            assert_eq!(pkt2.outputs[i].op_type, OutputOperationType::NoChange);
        }
    }

    #[test]
    fn too_short() {
        let payload = [0u8; 32];
        let err = ManualOutputPacket::from_bytes(&dummy_header(), &payload).unwrap_err();
        assert!(matches!(err, ManualOutputError::TooShort { .. }));
    }

    #[test]
    fn legacy_variant() {
        let mut payload = [0u8; 64];
        // Output 1: open permanently
        payload[0] = 3;

        let pkt = ManualOutputPacket::from_bytes(&dummy_header(), &payload).unwrap();

        assert_eq!(pkt.variant, ManualOutputVariant::Legacy);
        assert_eq!(pkt.outputs[0].op_type, OutputOperationType::OpenPermanently);
        for i in 1..16 {
            assert_eq!(pkt.outputs[i].op_type, OutputOperationType::NoChange);
        }
        // Upper 16 slots untouched in legacy mode
        for i in 16..32 {
            assert_eq!(pkt.outputs[i].op_type, OutputOperationType::NoChange);
        }
    }
}
