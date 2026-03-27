use core::fmt;

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};

use crate::packet::Ac215Packet;

/// Packed date/time format used in AC-825 event records.
///
/// The panel encodes timestamps into 5 bytes:
///   byte[0]: bits 7-5 = year high (3 bits, shifted >>1), bits 4-0 = day (1-31)
///   byte[1]: bits 7-4 = month (1-12), bits 3-0 = year low (4 bits)
///   byte[2]: bits 5-0 = second (0-59)
///   byte[3]: bits 5-0 = minute (0-59)
///   byte[4]: bits 4-0 = hour (0-23)
///
/// Year = ((byte[0] & 0xE0) >> 1) + (byte[1] & 0x0F) + 2000
pub fn decode_event_datetime(bytes: &[u8; 5]) -> Option<DateTime<Utc>> {
    let year_hi = ((bytes[0] & 0xE0) >> 1) as i32;
    let year_lo = (bytes[1] & 0x0F) as i32;
    let year = year_hi + year_lo + 2000;
    let month = ((bytes[1] & 0xF0) >> 4) as u32;
    let day = (bytes[0] & 0x1F) as u32;
    let hour = (bytes[4] & 0x1F) as u32;
    let minute = (bytes[3] & 0x3F) as u32;
    let second = (bytes[2] & 0x3F) as u32;

    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let time = NaiveTime::from_hms_opt(hour, minute, second)?;
    Some(date.and_time(time).and_utc())
}

pub fn encode_event_datetime(dt: DateTime<Utc>) -> [u8; 5] {
    use chrono::{Datelike, Timelike};

    let year_offset = (dt.year() - 2000) as u8;
    let year_hi = (year_offset & 0x70) << 1; // upper 3 bits into byte[0] bits 7-5
    let year_lo = year_offset & 0x0F; // lower 4 bits into byte[1] bits 3-0
    let month = dt.month() as u8;
    let day = dt.day() as u8;
    let hour = dt.hour() as u8;
    let minute = dt.minute() as u8;
    let second = dt.second() as u8;

    [
        year_hi | day,
        (month << 4) | year_lo,
        second & 0x3F,
        minute & 0x3F,
        hour & 0x1F,
    ]
}

/// A single 32-byte event record from the panel.
///
/// Layout within the 32-byte event data:
///   [0-4]: packed datetime (5 bytes, see `decode_event_datetime`)
///   [5]:   event type (eEventCategory)
///   [6]:   event sub-type
///   [7]:   event source (reader/input/output number)
///   [8]:   additional data
///   [9]:   card data flag (0 = card present)
///   [10..]: card/credential data (variable interpretation)
#[derive(Clone)]
pub struct EventRecord {
    pub timestamp: Option<DateTime<Utc>>,
    pub event_type: u8,
    pub event_sub_type: u8,
    pub event_source: u8,
    pub raw: [u8; 32],
}

impl fmt::Debug for EventRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventRecord")
            .field("timestamp", &self.timestamp)
            .field("event_type", &self.event_type)
            .field("event_sub_type", &self.event_sub_type)
            .field("event_source", &self.event_source)
            .finish()
    }
}

impl EventRecord {
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let dt_bytes: [u8; 5] = [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4]];
        Self {
            timestamp: decode_event_datetime(&dt_bytes),
            event_type: bytes[5],
            event_sub_type: bytes[6],
            event_source: bytes[7],
            raw: *bytes,
        }
    }

    pub fn into_bytes(&self) -> [u8; 32] {
        self.raw
    }
}

/// An event slot from the AnswerEvents825 response.
///
/// Each slot consists of a 3-byte event number and a 32-byte event record.
#[derive(Debug, Clone)]
pub struct EventSlot {
    pub event_number: u32,
    pub record: EventRecord,
}

/// Status data for the main AC-825 panel (34 bytes) and extension panels (18 bytes each).
///
/// The full status block is 322 bytes:
///   [0-33]:   main panel status (34 bytes)
///   [34-51]:  extension panel 1 status (18 bytes, hardware ID 2)
///   [52-69]:  extension panel 2 status (18 bytes, hardware ID 3)
///   ...up to 16 extensions
#[derive(Clone)]
pub struct PanelStatusBlock {
    pub raw: [u8; 322],
}

impl PanelStatusBlock {
    /// The 34-byte status block for the main AC-825 controller.
    pub fn main_panel_status(&self) -> &[u8] {
        &self.raw[0..34]
    }

    /// The 18-byte status block for an extension panel by hardware ID (2-17).
    /// Returns None if the hardware ID is out of range.
    pub fn extension_status(&self, hardware_id: u8) -> Option<&[u8]> {
        if !(2..=17).contains(&hardware_id) {
            return None;
        }
        let offset = (hardware_id as usize - 2) * 18 + 34;
        let end = offset + 18;
        if end <= self.raw.len() {
            Some(&self.raw[offset..end])
        } else {
            None
        }
    }

    /// The hardware type byte for an extension panel (at offset 17 within its 18-byte block).
    pub fn extension_hardware_type(&self, hardware_id: u8) -> Option<u8> {
        self.extension_status(hardware_id).map(|s| s[17])
    }
}

impl fmt::Debug for PanelStatusBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PanelStatusBlock")
            .field("main_panel", &format_args!("[{} bytes]", 34))
            .finish()
    }
}

/// AnswerEvents825 response (command 0xC9 / 201).
///
/// This is the panel's response to RequestEvents825. It contains both
/// real-time status data and up to 4 buffered event records.
///
/// Payload layout (472 bytes from the event buffer frame):
///   [0-321]:   panel status block (322 bytes)
///   [322]:     reserved/separator byte
///   [323-466]: 4 event slots, each 36 bytes:
///              [0-2]:  event number (3 bytes, big-endian)
///              [3-34]: event data (32 bytes)
///              [35]:   padding
#[derive(Debug, Clone)]
pub struct AnswerEvents825Packet {
    pub status: PanelStatusBlock,
    pub events: [Option<EventSlot>; 4],
}

#[derive(Debug)]
pub enum AnswerEventsError {
    TooShort { expected: usize, actual: usize },
}

impl fmt::Display for AnswerEventsError {
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

const STATUS_SIZE: usize = 322;
const EVENT_OFFSET: usize = 323;
const EVENT_SLOT_SIZE: usize = 36;
const EVENT_NUM_SIZE: usize = 3;
const EVENT_DATA_SIZE: usize = 32;
const EVENT_COUNT: usize = 4;
const MIN_PAYLOAD_SIZE: usize = EVENT_OFFSET + EVENT_COUNT * EVENT_SLOT_SIZE;

impl Ac215Packet for AnswerEvents825Packet {
    type Error = AnswerEventsError;

    const PACKET_ID: Option<u8> = Some(0xC9);

    fn packet_id(&self) -> u8 {
        0xC9
    }

    fn into_bytes(self, out: &mut [u8; 467]) -> u16 {
        out[0..STATUS_SIZE].copy_from_slice(&self.status.raw);
        out[STATUS_SIZE] = 0; // separator

        for (i, slot) in self.events.iter().enumerate() {
            let base = EVENT_OFFSET + i * EVENT_SLOT_SIZE;
            match slot {
                Some(ev) => {
                    out[base] = (ev.event_number >> 16) as u8;
                    out[base + 1] = (ev.event_number >> 8) as u8;
                    out[base + 2] = ev.event_number as u8;
                    out[base + EVENT_NUM_SIZE..base + EVENT_NUM_SIZE + EVENT_DATA_SIZE]
                        .copy_from_slice(&ev.record.into_bytes());
                    out[base + EVENT_NUM_SIZE + EVENT_DATA_SIZE] = 0; // padding
                }
                None => {
                    out[base..base + EVENT_SLOT_SIZE].fill(0);
                }
            }
        }

        MIN_PAYLOAD_SIZE as u16
    }

    fn from_bytes(_header: &crate::packet::header::Ac215Header, bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() < MIN_PAYLOAD_SIZE {
            return Err(AnswerEventsError::TooShort {
                expected: MIN_PAYLOAD_SIZE,
                actual: bytes.len(),
            });
        }

        let mut status_raw = [0u8; STATUS_SIZE];
        status_raw.copy_from_slice(&bytes[0..STATUS_SIZE]);

        let mut events: [Option<EventSlot>; 4] = Default::default();
        for i in 0..EVENT_COUNT {
            let base = EVENT_OFFSET + i * EVENT_SLOT_SIZE;

            // Event number: 3 bytes big-endian
            let event_num_bytes = &bytes[base..base + EVENT_NUM_SIZE];
            if event_num_bytes.iter().all(|&b| b == 0) {
                continue;
            }
            let event_number = ((event_num_bytes[0] as u32) << 16)
                | ((event_num_bytes[1] as u32) << 8)
                | (event_num_bytes[2] as u32);

            // Event data: 32 bytes
            let data_start = base + EVENT_NUM_SIZE;
            let data_end = data_start + EVENT_DATA_SIZE;
            let event_data = &bytes[data_start..data_end];

            // Skip if event data is all 0xFF (empty) or all 0xDD (invalid) or all 0x00
            if event_data.iter().all(|&b| b == 0xFF)
                || event_data.iter().all(|&b| b == 0xDD)
                || event_data.iter().all(|&b| b == 0x00)
            {
                continue;
            }

            let mut record_bytes = [0u8; 32];
            record_bytes.copy_from_slice(event_data);

            events[i] = Some(EventSlot {
                event_number,
                record: EventRecord::from_bytes(&record_bytes),
            });
        }

        Ok(Self {
            status: PanelStatusBlock { raw: status_raw },
            events,
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike, Timelike};

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
    fn datetime_round_trip() {
        let dt = NaiveDate::from_ymd_opt(2026, 3, 26)
            .unwrap()
            .and_hms_opt(14, 30, 45)
            .unwrap()
            .and_utc();

        let encoded = encode_event_datetime(dt);
        let decoded = decode_event_datetime(&encoded).unwrap();
        assert_eq!(dt, decoded);
    }

    #[test]
    fn datetime_decode() {
        // Year: ((0x40 & 0xE0) >> 1) + (0x3A & 0x0F) = 32 + 10 = 42 → 2042
        // Day: 0x40 & 0x1F = 0
        // That's invalid, let's use a real example:
        // 2026-03-15 10:25:30
        // year_offset = 26, year_hi = (26 & 0x70) << 1 = (0x10) << 1 = 0x20
        // year_lo = 26 & 0x0F = 0x0A
        // byte[0] = 0x20 | 15 = 0x2F
        // byte[1] = (3 << 4) | 0x0A = 0x3A
        // byte[2] = 30 = 0x1E
        // byte[3] = 25 = 0x19
        // byte[4] = 10 = 0x0A
        let bytes = [0x2F, 0x3A, 0x1E, 0x19, 0x0A];
        let dt = decode_event_datetime(&bytes).unwrap();
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 15);
        assert_eq!(dt.hour(), 10);
        assert_eq!(dt.minute(), 25);
        assert_eq!(dt.second(), 30);
    }

    #[test]
    fn parse_empty_response() {
        // All zeros — status block present, no events
        let payload = [0u8; MIN_PAYLOAD_SIZE];
        let pkt = AnswerEvents825Packet::from_bytes(&dummy_header(), &payload).unwrap();
        assert!(pkt.events.iter().all(|e| e.is_none()));
    }

    #[test]
    fn parse_with_one_event() {
        let mut payload = [0u8; MIN_PAYLOAD_SIZE];

        // Event slot 0 at offset 323
        // Event number: 0x000042 = 66
        payload[323] = 0x00;
        payload[324] = 0x00;
        payload[325] = 0x42;

        // Event data (32 bytes starting at 326):
        // Encode 2026-03-15 10:25:30
        payload[326] = 0x2F; // date byte 0
        payload[327] = 0x3A; // date byte 1
        payload[328] = 0x1E; // second
        payload[329] = 0x19; // minute
        payload[330] = 0x0A; // hour
        payload[331] = 193; // event type (access granted)
        payload[332] = 0x01; // event sub-type
        payload[333] = 0x03; // event source (reader 3)
        payload[334] = 0x01; // additional data

        let pkt = AnswerEvents825Packet::from_bytes(&dummy_header(), &payload).unwrap();

        let ev = pkt.events[0].as_ref().unwrap();
        assert_eq!(ev.event_number, 66);
        assert_eq!(ev.record.event_type, 193);
        assert_eq!(ev.record.event_sub_type, 0x01);
        assert_eq!(ev.record.event_source, 0x03);

        let ts = ev.record.timestamp.unwrap();
        assert_eq!(ts.year(), 2026);
        assert_eq!(ts.month(), 3);
        assert_eq!(ts.day(), 15);
        assert_eq!(ts.hour(), 10);
        assert_eq!(ts.minute(), 25);
        assert_eq!(ts.second(), 30);

        // Other slots should be empty
        assert!(pkt.events[1].is_none());
        assert!(pkt.events[2].is_none());
        assert!(pkt.events[3].is_none());
    }

    #[test]
    fn round_trip() {
        let mut payload = [0u8; MIN_PAYLOAD_SIZE];
        payload[323] = 0x00;
        payload[324] = 0x01;
        payload[325] = 0x00;
        payload[326] = 0x2F;
        payload[327] = 0x3A;
        payload[328] = 0x1E;
        payload[329] = 0x19;
        payload[330] = 0x0A;
        payload[331] = 193;
        payload[332] = 0x01;
        payload[333] = 0x03;
        payload[334] = 0xFF; // make it non-uniform

        let pkt = AnswerEvents825Packet::from_bytes(&dummy_header(), &payload).unwrap();
        assert!(pkt.events[0].is_some());

        let mut out = [0u8; 467];
        let len = pkt.clone().into_bytes(&mut out);
        let pkt2 = AnswerEvents825Packet::from_bytes(&dummy_header(), &out[..len as usize]).unwrap();

        let ev1 = pkt.events[0].as_ref().unwrap();
        let ev2 = pkt2.events[0].as_ref().unwrap();
        assert_eq!(ev1.event_number, ev2.event_number);
        assert_eq!(ev1.record.event_type, ev2.record.event_type);
        assert_eq!(ev1.record.raw, ev2.record.raw);
    }

    #[test]
    fn too_short() {
        let err = AnswerEvents825Packet::from_bytes(&dummy_header(), &[0u8; 100]).unwrap_err();
        assert!(matches!(err, AnswerEventsError::TooShort { .. }));
    }
}
