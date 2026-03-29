use core::fmt;

use heapless::String;

use crate::packet::Ac215Packet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSeverity {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
    Fatal,
    Disable,
    NotAvailable,
    Unknown(u8),
}

impl LogSeverity {
    fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Trace,
            1 => Self::Debug,
            2 => Self::Info,
            3 => Self::Warning,
            4 => Self::Error,
            5 => Self::Fatal,
            6 => Self::Disable,
            7 => Self::NotAvailable,
            v => Self::Unknown(v),
        }
    }

    fn into_byte(self) -> u8 {
        match self {
            Self::Trace => 0,
            Self::Debug => 1,
            Self::Info => 2,
            Self::Warning => 3,
            Self::Error => 4,
            Self::Fatal => 5,
            Self::Disable => 6,
            Self::NotAvailable => 7,
            Self::Unknown(v) => v,
        }
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LogDestination: u8 {
        const LOG   = 0x01;
        const EVENT = 0x02;
    }
}

/// SendLogMessage packet (command 0xD4 / 212).
///
/// Sent from the panel to the server. Contains a log message with
/// severity and destination flags.
///
/// Payload layout:
///   [0]:    destination flags (1=Log, 2=Event, 3=both)
///   [1]:    severity (0=Trace..5=Fatal, 6=Disable, 7=NotAvailable)
///   [2..]:  message string (ASCII, null-padded)
#[derive(Debug, Clone)]
pub struct SendLogMessagePacket {
    pub destination: LogDestination,
    pub severity: LogSeverity,
    pub message: String<461>,
}

#[derive(Debug)]
pub enum SendLogMessageError {
    TooShort { expected: usize, actual: usize },
}

impl fmt::Display for SendLogMessageError {
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

impl Ac215Packet for SendLogMessagePacket {
    type Error = SendLogMessageError;

    const PACKET_ID: Option<u8> = Some(0xD4);

    fn packet_id(&self) -> u8 {
        0xD4
    }

    fn into_bytes(self, out: &mut [u8; 468]) -> u16 {
        out[0] = self.destination.bits();
        out[1] = self.severity.into_byte();
        let msg_bytes = self.message.as_bytes();
        let len = msg_bytes.len().min(465);
        out[2..2 + len].copy_from_slice(&msg_bytes[..len]);
        out[2 + len..].fill(0);
        (2 + len) as u16
    }

    fn from_bytes(_header: &crate::packet::header::Ac215Header, bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() < 2 {
            return Err(SendLogMessageError::TooShort {
                expected: 2,
                actual: bytes.len(),
            });
        }

        let destination = LogDestination::from_bits_truncate(bytes[0]);
        let severity = LogSeverity::from_byte(bytes[1]);

        let msg_raw = &bytes[2..];
        let mut message = String::new();
        let text = core::str::from_utf8(msg_raw).unwrap_or("");
        let trimmed = text.trim_end_matches(|c: char| c == '\0' || c <= '\x03');
        for c in trimmed.chars() {
            if message.push(c).is_err() {
                break;
            }
        }

        Ok(Self {
            destination,
            severity,
            message,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::address::Ac215Address;
    use crate::packet::direction::Ac215PacketDirection;
    use crate::packet::header::{Ac215Header, Ac215TransactionId};
    use crate::packet::header::EventFlag;

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
    fn parse_log_message() {
        let mut payload = [0u8; 32];
        payload[0] = 0x03; // LOG | EVENT
        payload[1] = 0x02; // Info
        payload[2..14].copy_from_slice(b"Hello, panel");

        let pkt = SendLogMessagePacket::from_bytes(&dummy_header(), &payload).unwrap();

        assert_eq!(pkt.destination, LogDestination::LOG | LogDestination::EVENT);
        assert_eq!(pkt.severity, LogSeverity::Info);
        assert_eq!(pkt.message.as_str(), "Hello, panel");
    }

    #[test]
    fn round_trip() {
        let pkt = SendLogMessagePacket {
            destination: LogDestination::EVENT,
            severity: LogSeverity::Warning,
            message: {
                let mut s = String::new();
                s.push_str("test message").unwrap();
                s
            },
        };

        let mut out = [0u8; 468];
        let len = pkt.clone().into_bytes(&mut out);

        let pkt2 = SendLogMessagePacket::from_bytes(&dummy_header(), &out[..len as usize]).unwrap();
        assert_eq!(pkt2.destination, LogDestination::EVENT);
        assert_eq!(pkt2.severity, LogSeverity::Warning);
        assert_eq!(pkt2.message.as_str(), "test message");
    }

    #[test]
    fn too_short() {
        let err = SendLogMessagePacket::from_bytes(&dummy_header(), &[0x01]).unwrap_err();
        assert!(matches!(err, SendLogMessageError::TooShort { .. }));
    }

    #[test]
    fn empty_message() {
        let payload = [0x01, 0x00]; // LOG, Trace, no message
        let pkt = SendLogMessagePacket::from_bytes(&dummy_header(), &payload).unwrap();
        assert_eq!(pkt.message.as_str(), "");
    }
}
