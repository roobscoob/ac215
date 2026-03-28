use core::fmt;

use super::event_type::EventType;
use super::location::EventLocation;
use super::timestamp::PackedTimestamp;

/// A single event record from the panel's circular event log.
///
/// ```text
/// [0..2]    event_number   u24 big-endian (000000 = empty slot)
/// [3..7]    timestamp      5 bytes packed (see PackedTimestamp)
/// [8]       event_type     category enum
/// [9]       event_subtype  detail within category
/// [10]      location       source location (see EventLocation)
/// [11..35]  event_data     25 bytes, meaning depends on type
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub event_number: u32,
    pub timestamp: PackedTimestamp,
    pub event_type: EventType,
    pub location: EventLocation,
    pub event_data: [u8; 25],
}

impl EventRecord {
    pub const SIZE: usize = 36;

    /// An empty event slot has event_number == 0.
    pub fn is_empty(&self) -> bool {
        self.event_number == 0
    }

    /// Parse from a 36-byte slice.
    pub fn parse(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= 36);

        let event_number =
            ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | (bytes[2] as u32);

        let timestamp = PackedTimestamp::decode(bytes[3..8].try_into().unwrap());

        let mut event_data = [0u8; 25];
        event_data.copy_from_slice(&bytes[11..36]);

        Self {
            event_number,
            timestamp,
            event_type: EventType::new(bytes[8], bytes[9]),
            location: EventLocation::from_byte(bytes[10]),
            event_data,
        }
    }

    /// Write to a 36-byte slice.
    pub fn write(&self, out: &mut [u8]) {
        assert!(out.len() >= 36);

        out[0] = (self.event_number >> 16) as u8;
        out[1] = (self.event_number >> 8) as u8;
        out[2] = self.event_number as u8;

        out[3..8].copy_from_slice(&self.timestamp.encode());

        out[8] = self.event_type.type_id();
        out[9] = self.event_type.subtype_id();
        out[10] = self.location.to_byte();
        out[11..36].copy_from_slice(&self.event_data);
    }
}

impl fmt::Debug for EventRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            write!(f, "EventRecord(empty)")
        } else {
            write!(
                f,
                "EventRecord(#{} {} {:?} loc={:?})",
                self.event_number, self.timestamp, self.event_type, self.location,
            )
        }
    }
}
