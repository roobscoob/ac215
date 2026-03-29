use core::fmt;

use crate::event::EventRecord;
use crate::packet::Ac215Packet;

// ── Power / voltage / tamper flags (byte 8 of AC-825 status) ────────────────

bitflags::bitflags! {
    /// Hardware status flags from byte 8 of the AC-825 main controller status block.
    ///
    /// ```text
    /// bit 0: AUX_POWER       auxiliary power OK
    /// bit 1: VIN_STATUS       input voltage OK
    /// bit 2: VEXP_STATUS      expansion voltage OK
    /// bit 3: READERS_VOLTAGE  reader power OK
    /// bit 4: SRC_OF_SYSTEM    system power source
    /// bit 5: AUX_POWER_BOARD  aux power board present
    /// bit 6: BATTERY_OK       battery status OK
    /// bit 7: CASE_TAMPER      case tamper switch active
    /// ```
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PowerFlags: u8 {
        const AUX_POWER       = 1 << 0;
        const VIN_STATUS      = 1 << 1;
        const VEXP_STATUS     = 1 << 2;
        const READERS_VOLTAGE = 1 << 3;
        const SRC_OF_SYSTEM   = 1 << 4;
        const AUX_POWER_BOARD = 1 << 5;
        const BATTERY_OK      = 1 << 6;
        const CASE_TAMPER     = 1 << 7;
    }
}

// ── Input state (2-bit enum) ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputState {
    Normal,
    Active,
    Alarm,
    Unknown(u8),
}

impl InputState {
    fn from_bits(val: u8) -> Self {
        match val & 0x03 {
            0 => Self::Normal,
            1 => Self::Active,
            2 => Self::Alarm,
            v => Self::Unknown(v),
        }
    }

    fn into_bits(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Active => 1,
            Self::Alarm => 2,
            Self::Unknown(v) => v & 0x03,
        }
    }
}

// ── AC-825 main controller status (34 bytes) ────────────────────────────────

/// Parsed status of the AC-825 main controller.
///
/// Extracted from the first 34 bytes of the 0xC9 status block.
///
/// ```text
/// [0..3]    Output states            1 bit per output (32 max)
/// [4..7]    Output manual override   1 bit per output
/// [8]       Power/voltage/tamper     PowerFlags bitfield
/// [9..11]   Input states             2 bits per input (12 max)
/// [12..13]  Input armed flags        1 bit per input (LE u16)
/// [14..15]  Input manual override    1 bit per input (LE u16)
/// [16..24]  (reserved)
/// [25..28]  Reader modes             4 bits per reader (2 per byte)
/// [29]      Reader manual override   1 bit per reader
/// [30]      Sounder                  bits 0-2: status, bit 7: manual
/// [31..32]  (reserved)
/// [33]      SD card                  bit 0: present
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Ac825Status {
    pub output_states: u32,
    pub output_manual: u32,
    pub power_flags: PowerFlags,
    pub input_states: [InputState; 12],
    pub input_armed: u16,
    pub input_manual: u16,
    pub reader_modes: [u8; 10],
    pub reader_manual: u16,
    pub sounder_status: u8,
    pub sounder_manual: bool,
    pub sd_card_present: bool,
}

impl Ac825Status {
    pub const SIZE: usize = 34;

    pub fn output_active(&self, output: u8) -> bool {
        output < 32 && (self.output_states >> output) & 1 != 0
    }

    pub fn output_is_manual(&self, output: u8) -> bool {
        output < 32 && (self.output_manual >> output) & 1 != 0
    }

    pub fn input_armed(&self, input: u8) -> bool {
        input < 12 && (self.input_armed >> input) & 1 != 0
    }

    pub fn input_is_manual(&self, input: u8) -> bool {
        input < 12 && (self.input_manual >> input) & 1 != 0
    }

    pub fn reader_manual(&self, reader: u8) -> bool {
        reader < 8 && (self.reader_manual >> reader) & 1 != 0
    }

    fn parse(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= 34);

        let output_states = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let output_manual = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let power_flags = PowerFlags::from_bits_retain(bytes[8]);

        let mut input_states = [InputState::Normal; 12];
        for g in 0..3u8 {
            let byte_val = bytes[9 + g as usize];
            for j in 0..4u8 {
                let idx = (g * 4 + j) as usize;
                if idx < 12 {
                    input_states[idx] = InputState::from_bits(byte_val >> (j * 2));
                }
            }
        }

        let input_armed = u16::from_le_bytes([bytes[12], bytes[13]]);
        let input_manual = u16::from_le_bytes([bytes[14], bytes[15]]);

        let mut reader_modes = [0u8; 10];
        for i in 0..10usize {
            let byte_idx = 25 + (i >> 1);
            let shift = (i & 1) << 2;
            reader_modes[i] = (bytes[byte_idx] >> shift) & 0x07;
        }

        let reader_manual = {
            let mut val = 0u16;
            for i in 0..8u8 {
                if (bytes[29] >> i) & 1 != 0 {
                    val |= 1 << i;
                }
            }
            val
        };

        let sounder_status = bytes[30] & 0x07;
        let sounder_manual = bytes[30] & 0x80 != 0;
        let sd_card_present = bytes[33] & 1 != 0;

        Self {
            output_states,
            output_manual,
            power_flags,
            input_states,
            input_armed,
            input_manual,
            reader_modes,
            reader_manual,
            sounder_status,
            sounder_manual,
            sd_card_present,
        }
    }

    fn write(&self, out: &mut [u8]) {
        assert!(out.len() >= 34);

        // Zero the whole block first (covers reserved bytes 16-24, 31-32)
        out[..34].fill(0);

        // [0..3] output states
        out[0..4].copy_from_slice(&self.output_states.to_le_bytes());

        // [4..7] output manual override
        out[4..8].copy_from_slice(&self.output_manual.to_le_bytes());

        // [8] power/voltage/tamper
        out[8] = self.power_flags.bits();

        // [9..11] input states, 2 bits per input, 4 per byte
        for g in 0..3u8 {
            let mut byte_val = 0u8;
            for j in 0..4u8 {
                let idx = (g * 4 + j) as usize;
                if idx < 12 {
                    byte_val |= self.input_states[idx].into_bits() << (j * 2);
                }
            }
            out[9 + g as usize] = byte_val;
        }

        // [12..13] input armed
        out[12..14].copy_from_slice(&self.input_armed.to_le_bytes());

        // [14..15] input manual override
        out[14..16].copy_from_slice(&self.input_manual.to_le_bytes());

        // [25..28] reader modes, 4 bits per reader, 2 per byte
        for i in 0..10usize {
            let byte_idx = 25 + (i >> 1);
            let shift = (i & 1) << 2;
            out[byte_idx] |= (self.reader_modes[i] & 0x07) << shift;
        }

        // [29] reader manual override, 1 bit per reader
        for i in 0..8u8 {
            if (self.reader_manual >> i) & 1 != 0 {
                out[29] |= 1 << i;
            }
        }

        // [30] sounder (bits 0-2 status, bit 7 manual)
        out[30] = (self.sounder_status & 0x07) | if self.sounder_manual { 0x80 } else { 0 };

        // [33] SD card
        out[33] = if self.sd_card_present { 1 } else { 0 };
    }
}

impl fmt::Debug for Ac825Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Outputs: "1↑ 2↑ 3↑ 4↑ 5↑ 6↑" (↑=active, M=manual)
        let mut outputs = String::new();
        for i in 0..32u8 {
            let active = self.output_active(i);
            let manual = self.output_is_manual(i);
            if active || manual {
                if !outputs.is_empty() {
                    outputs.push(' ');
                }
                outputs.push_str(&format!("{}", i + 1));
                match (active, manual) {
                    (true, true) => outputs.push_str("↑M"),
                    (true, false) => outputs.push('↑'),
                    (false, true) => outputs.push_str("·M"),
                    _ => {}
                }
            }
        }
        if outputs.is_empty() {
            outputs.push_str("(none)");
        }

        // Inputs: "1:act 2:act … 8:alm"
        let mut inputs = String::new();
        for i in 0..12usize {
            let state = &self.input_states[i];
            if *state != InputState::Normal
                || self.input_armed(i as u8)
                || self.input_is_manual(i as u8)
            {
                if !inputs.is_empty() {
                    inputs.push(' ');
                }
                let tag = match state {
                    InputState::Normal => "ok",
                    InputState::Active => "act",
                    InputState::Alarm => "ALM",
                    InputState::Unknown(v) => {
                        inputs.push_str(&format!("{}:?{}", i + 1, v));
                        continue;
                    }
                };
                inputs.push_str(&format!("{}:{}", i + 1, tag));
                if self.input_armed(i as u8) {
                    inputs.push('A');
                }
                if self.input_is_manual(i as u8) {
                    inputs.push('M');
                }
            }
        }
        if inputs.is_empty() {
            inputs.push_str("(all normal)");
        }

        // Readers: "1:card 2:card … 5:free"
        let mode_name = |m: u8| -> &'static str {
            match m {
                0 => "auto",
                1 => "card",
                2 => "card+pin",
                3 => "pin",
                4 => "locked",
                5 => "free",
                _ => "?",
            }
        };
        let mut readers = String::new();
        for i in 0..10usize {
            let mode = self.reader_modes[i];
            let manual = self.reader_manual(i as u8);
            if mode != 0 || manual {
                if !readers.is_empty() {
                    readers.push(' ');
                }
                readers.push_str(&format!("{}:{}", i + 1, mode_name(mode)));
                if manual {
                    readers.push('M');
                }
            }
        }
        if readers.is_empty() {
            readers.push_str("(all auto)");
        }

        f.debug_struct("Ac825Status")
            .field("outputs", &format_args!("{}", outputs))
            .field("inputs", &format_args!("{}", inputs))
            .field("readers", &format_args!("{}", readers))
            .field("power", &self.power_flags)
            .field(
                "sounder",
                &format_args!(
                    "{}{}",
                    self.sounder_status,
                    if self.sounder_manual { "M" } else { "" }
                ),
            )
            .field("sd", &self.sd_card_present)
            .finish()
    }
}

// ── AnswerEvents825 (CMD 0xC9) ──────────────────────────────────────────────

/// CMD 0xC9 — Event log response.
///
/// Sent by the panel in response to CMD 0x48 (RequestEvents825) or CMD 0xD1
/// (KeepAliveAck), and pushed unsolicited on the async channel (1819).
///
/// ```text
/// Bytes 0–321:   Status block (322 bytes)
///   [0..33]      AC-825 main controller (34 bytes)
///   [34..321]    Extension panels (up to 16 × 18 bytes each)
/// Byte  322:     Separator
/// Bytes 323–466: 4 event records × 36 bytes
/// Byte  467:     Trailing padding
/// ```
///
/// Total data payload: 468 bytes (raw_size=254 on wire → 480 encrypted, event buffer frame).
#[derive(Clone)]
pub struct AnswerEvents825 {
    /// Parsed status of the AC-825 main controller.
    pub ac825_status: Ac825Status,
    /// Raw status block bytes for extension panels and passthrough.
    /// Bytes 34–321 of the status block (288 bytes).
    pub extension_status: [u8; 288],
    /// The separator byte (byte 322). Usually 0.
    pub separator: u8,
    /// Four event record slots.
    pub events: [Option<EventRecord>; 4],
    /// Trailing byte (byte 467). Usually 0.
    pub trailing: u8,
}

impl fmt::Debug for AnswerEvents825 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let active_events: Vec<_> = self.events.iter().flatten().collect();
        f.debug_struct("AnswerEvents825")
            .field("ac825_status", &self.ac825_status)
            .field("events", &active_events)
            .finish()
    }
}

#[derive(Debug)]
pub enum AnswerEvents825Error {
    TooShort { expected: usize, actual: usize },
}

impl fmt::Display for AnswerEvents825Error {
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

impl AnswerEvents825 {
    /// The expected data payload length.
    pub const DATA_LENGTH: usize = 468;

    /// The number of event record slots per response.
    pub const EVENT_COUNT: usize = 4;

    /// Offset where the event records begin in the data payload.
    pub const EVENTS_OFFSET: usize = 323;

    /// Size of the full status block (AC-825 + extensions).
    pub const STATUS_BLOCK_SIZE: usize = 322;

    /// Create an empty response (all zeros, no events).
    ///
    /// Useful for responding to polls when nothing has happened.
    pub fn empty() -> Self {
        Self {
            ac825_status: Ac825Status::parse(&[0u8; 34]),
            extension_status: [0u8; 288],
            separator: 0,
            events: [None, None, None, None],
            trailing: 0,
        }
    }

    /// Return only the present events.
    pub fn active_events(&self) -> impl Iterator<Item = &EventRecord> {
        self.events.iter().flatten()
    }

    /// Highest event number in this response, or None if all slots are empty.
    pub fn max_event_number(&self) -> Option<u32> {
        self.events.iter().flatten().map(|e| e.event_number).max()
    }

    /// The full raw status block (322 bytes) for caching/passthrough.
    pub fn status_block(&self) -> [u8; 322] {
        let mut block = [0u8; 322];
        self.ac825_status.write(&mut block[..34]);
        block[34..322].copy_from_slice(&self.extension_status);
        block
    }
}

impl Ac215Packet for AnswerEvents825 {
    type Error = AnswerEvents825Error;

    const PACKET_ID: Option<u8> = Some(0xC9);

    fn packet_id(&self) -> u8 {
        0xC9
    }

    fn into_bytes(self, out: &mut [u8; 468]) -> u16 {
        // Status block: AC-825 (34 bytes) + extensions (288 bytes) = 322 bytes
        self.ac825_status.write(&mut out[0..34]);
        out[34..322].copy_from_slice(&self.extension_status);

        // Separator
        out[322] = self.separator;

        // 4 event records × 36 bytes = 144 bytes at offset 323
        for i in 0..4 {
            let offset = Self::EVENTS_OFFSET + i * EventRecord::SIZE;
            match &self.events[i] {
                Some(record) => record.write(&mut out[offset..offset + EventRecord::SIZE]),
                None => out[offset..offset + EventRecord::SIZE].fill(0),
            }
        }

        // Trailing padding byte
        out[467] = self.trailing;

        468
    }

    fn from_bytes(
        _header: &crate::packet::header::Ac215Header,
        bytes: &[u8],
    ) -> Result<Self, Self::Error> {
        // The data payload should be at least 467 bytes.
        // We accept 467 or 468 (trailing byte may or may not be present).
        if bytes.len() < 467 {
            return Err(AnswerEvents825Error::TooShort {
                expected: 467,
                actual: bytes.len(),
            });
        }

        let ac825_status = Ac825Status::parse(&bytes[0..34]);

        let mut extension_status = [0u8; 288];
        extension_status.copy_from_slice(&bytes[34..322]);

        let separator = bytes[322];

        let mut events: [Option<EventRecord>; 4] = [None, None, None, None];
        for i in 0..4 {
            let offset = Self::EVENTS_OFFSET + i * EventRecord::SIZE;
            if offset + EventRecord::SIZE <= bytes.len() {
                let record = EventRecord::parse(&bytes[offset..offset + EventRecord::SIZE]);
                if record.event_number != 0 {
                    events[i] = Some(record);
                }
            }
        }

        let trailing = if bytes.len() > 467 { bytes[467] } else { 0 };

        Ok(Self {
            ac825_status,
            extension_status,
            separator,
            events,
            trailing,
        })
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventLocation, EventType, PackedTimestamp};
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

    fn captured_payload() -> [u8; 468] {
        let mut payload = [0u8; 468];
        // AC-825 status: outputs 1-6 active + manual
        payload[0] = 0x3F;
        payload[4] = 0x3F;
        // Power: case tamper
        payload[8] = 0x80;
        // Input states: all active (0x55 = 01 01 01 01 per byte)
        payload[9] = 0x55;
        payload[10] = 0x55;
        payload[11] = 0x55;
        // Reader modes: all mode 1 (card only)
        payload[25] = 0x11;
        payload[26] = 0x11;
        payload[27] = 0x11;

        // 4 sequential events starting at byte 323
        for i in 0..4u32 {
            let offset = 323 + (i as usize) * 36;
            let evt_num = 435417 + i;
            payload[offset] = (evt_num >> 16) as u8;
            payload[offset + 1] = (evt_num >> 8) as u8;
            payload[offset + 2] = evt_num as u8;
            // Timestamp: 2026-03-27 21:56:43
            payload[offset + 3] = 0x3B; // day=27, year_hi
            payload[offset + 4] = 0x3A; // month=3, year_lo
            payload[offset + 5] = 0x2B; // seconds=43
            payload[offset + 6] = 0x38; // minutes=56
            payload[offset + 7] = 0x35; // hours=21 (0x15, but captured shows 0x35?)
            // Event type 0x51, source varies
            payload[offset + 8] = 0x51;
            payload[offset + 10] = 0x51 + i as u8;
        }

        payload
    }

    #[test]
    fn parse_captured() {
        let payload = captured_payload();
        let pkt = AnswerEvents825::from_bytes(&dummy_header(), &payload).unwrap();

        // Output states
        assert!(pkt.ac825_status.output_active(0));
        assert!(pkt.ac825_status.output_active(5));
        assert!(!pkt.ac825_status.output_active(6));

        // Output manual
        assert!(pkt.ac825_status.output_is_manual(0));
        assert!(!pkt.ac825_status.output_is_manual(6));

        // Power flags
        assert!(
            pkt.ac825_status
                .power_flags
                .contains(PowerFlags::CASE_TAMPER)
        );
        assert!(
            !pkt.ac825_status
                .power_flags
                .contains(PowerFlags::BATTERY_OK)
        );

        // Input states
        for i in 0..12 {
            assert_eq!(pkt.ac825_status.input_states[i], InputState::Active);
        }

        // Reader modes
        for i in 0..6 {
            assert_eq!(pkt.ac825_status.reader_modes[i], 1);
        }
        for i in 6..10 {
            assert_eq!(pkt.ac825_status.reader_modes[i], 0);
        }

        // SD card
        assert!(!pkt.ac825_status.sd_card_present);

        // Events
        assert_eq!(pkt.active_events().count(), 4);
        let ev0 = pkt.events[0].as_ref().unwrap();
        let ev3 = pkt.events[3].as_ref().unwrap();
        assert_eq!(ev0.event_number, 435417);
        assert_eq!(ev3.event_number, 435420);
        assert_eq!(ev0.event_type, EventType::new(0x51, 0x00));
        assert_eq!(ev0.location, EventLocation::Output(0));
        assert_eq!(ev3.location, EventLocation::Output(3));
        assert_eq!(pkt.max_event_number(), Some(435420));
    }

    #[test]
    fn empty_response() {
        let pkt = AnswerEvents825::empty();

        assert_eq!(pkt.ac825_status.output_states, 0);
        assert_eq!(pkt.ac825_status.input_armed, 0);
        assert_eq!(pkt.active_events().count(), 0);
        assert_eq!(pkt.max_event_number(), None);
    }

    #[test]
    fn round_trip() {
        let payload = captured_payload();
        let pkt = AnswerEvents825::from_bytes(&dummy_header(), &payload).unwrap();

        let mut out = [0u8; 468];
        let len = pkt.clone().into_bytes(&mut out);
        assert_eq!(len, 468);

        // Re-parse from our output (only 467 bytes available in buffer)
        let pkt2 = AnswerEvents825::from_bytes(&dummy_header(), &out).unwrap();

        assert_eq!(pkt2.ac825_status.output_states, 0x3F);
        assert_eq!(pkt2.ac825_status.output_manual, 0x3F);
        assert_eq!(pkt2.events[0].as_ref().unwrap().event_number, 435417);
        assert_eq!(pkt2.events[3].as_ref().unwrap().event_number, 435420);
    }

    #[test]
    fn status_block_encoding() {
        let payload = captured_payload();
        let pkt = AnswerEvents825::from_bytes(&dummy_header(), &payload).unwrap();

        let block = pkt.status_block();
        assert_eq!(block[0], 0x3F);
        assert_eq!(block[4], 0x3F);
        assert_eq!(block[8], 0x80);
        assert_eq!(block[9], 0x55);
    }

    #[test]
    fn event_record_empty() {
        let rec = EventRecord::parse(&[0u8; 36]);
        assert!(rec.is_empty());
    }

    #[test]
    fn too_short() {
        let payload = [0u8; 100];
        let err = AnswerEvents825::from_bytes(&dummy_header(), &payload).unwrap_err();
        assert!(matches!(err, AnswerEvents825Error::TooShort { .. }));
    }
}
