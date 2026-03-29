/// The 24-byte event payload for AC-825 access events.
///
/// This is the parsed form of `event_data[0..24]` (P_1[8..31] in the C#).
/// The 25th byte (event_data[24]) is never read by the server.
///
/// ```text
/// Wire layout (24 bytes):
///
///  0       1       2       3       4       5       6       7
/// ┌───────┬───────┬───────┬───────┬───────┬───────┬───────┬───────┐
/// │parking│format │       │       │       │       │       │       │  AC-825 offset
/// │sub-grp│select │  (unused, always zero)                        │  region
/// ├───────┴───────┴───────┴───────┴───────┴───────┴───────┴───────┤
///  8       9      10      11      12      13      14      15
/// ┌───────┬───────┬───────┬───────┬───────┬───────┬───────┬───────┐
/// │  site code    │                                               │
/// │   (u16 BE)    │           card code bytes                     │
/// ├───────┴───────┤           (14 bytes total,                    │
///  16      17      18      19      20      21      22      23     │
/// ┌───────┬───────┬───────┬───────┬───────┬───────┬───────┬───────┤
/// │                        hex string → integer)                  │
/// └───────┴───────┴───────┴───────┴───────┴───────┴───────┴───────┘
/// ```
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct AccessEventData {
    /// Car parking subgroup. Only meaningful when event subtype ≥ 0x80.
    /// Zero for normal access events.
    pub parking_subgroup: u8,

    /// Card format selector. 0 = primary reader format, non-zero = secondary.
    /// The server uses this to pick `eReader` vs `eReader2` on the reader config.
    pub format_select: u8,

    /// Remaining 6 bytes of the AC-825 offset region. Not read by the server.
    pub _offset_pad: [u8; 6],

    /// Site/facility code. Big-endian u16 at bytes 8–9.
    /// For Wiegand 26-bit: the 8-bit facility code, zero-extended to u16.
    pub site_code: u16,

    /// Raw card code bytes (14 bytes at positions 10–23).
    /// Interpreted as a hex string and parsed as an integer:
    ///   "00000000000000000000000000000445" → 0x445 → 1093
    pub card_code_bytes: [u8; 14],
}

impl AccessEventData {
    pub const SIZE: usize = 25;

    pub fn parse(data: &[u8; Self::SIZE]) -> Self {
        Self {
            parking_subgroup: data[0],
            format_select: data[1],
            _offset_pad: [data[2], data[3], data[4], data[5], data[6], data[7]],
            site_code: u16::from_be_bytes([data[8], data[9]]),
            card_code_bytes: data[10..24].try_into().unwrap(),
        }
    }

    /// Card code as an integer, matching the server's hex-string parse.
    ///
    /// Each of the 14 bytes is formatted as 2-char hex, concatenated,
    /// then parsed as a single hex integer. Equivalent to treating the
    /// 14 bytes as a big-endian u112, but since the high bytes are
    /// almost always zero, it fits comfortably in a u64 for Wiegand.
    pub fn card_code(&self) -> u64 {
        // For Wiegand 26-bit, only the last 2–3 bytes are non-zero.
        // For wider formats (e.g. MIFARE 64-bit), more bytes are used.
        // The server parses into a C# ulong (u64), so we do the same.
        let mut val: u64 = 0;
        for &b in &self.card_code_bytes {
            // This mirrors the hex-string parse: each byte contributes
            // two hex digits, so it's equivalent to val = val * 256 + b
            // ... but only if the total doesn't exceed u64::MAX.
            val = val.checked_shl(8).unwrap_or(0) | b as u64;
        }
        val
    }

    /// Whether this card was read using the primary format.
    pub fn is_primary_format(&self) -> bool {
        self.format_select == 0
    }

    pub fn write(&self, buf: &mut [u8; Self::SIZE]) {
        buf[0] = self.parking_subgroup;
        buf[1] = self.format_select;
        buf[2..8].copy_from_slice(&self._offset_pad);
        buf[8..10].copy_from_slice(&self.site_code.to_be_bytes());
        buf[10..24].copy_from_slice(&self.card_code_bytes);
    }
}

impl core::fmt::Debug for AccessEventData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "AccessEventData(site={}, card={}{})",
            self.site_code,
            self.card_code(),
            if self.is_primary_format() {
                ""
            } else {
                ", secondary"
            },
        )
    }
}

impl core::fmt::Display for AccessEventData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{},{}", self.site_code, self.card_code())
    }
}

/// PIN event data for AC-825 PIN access events (types 0x13, 0x23, 0x73, 0x93).
///
/// PIN digits start at byte 8 (after the AC-825 offset), null-terminated.
/// Each byte encodes digits: if low nibble is 0, only the high nibble is
/// used as a single digit; otherwise the full byte value.
pub struct PinEventData {
    pub pin: String,
}

impl PinEventData {
    pub fn parse(data: &[u8; AccessEventData::SIZE]) -> Self {
        let mut pin = String::new();
        for &b in &data[8..] {
            if b == 0 {
                break;
            }
            if (b & 0x0F) == 0 {
                pin.push_str(&format!("{}", b >> 4));
            } else {
                pin.push_str(&format!("{}", b));
            }
        }
        Self { pin }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_granted_wiegand26() {
        // Event #435498: site=198, card=1093, Reader 3
        let raw: [u8; 25] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // AC-825 offset
            0x00, 0xC6, // site = 198
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04,
            0x45, // card = 1093
            0x00,
        ];
        let ev = AccessEventData::parse(&raw);
        assert_eq!(ev.site_code, 198);
        assert_eq!(ev.card_code(), 1093);
        assert!(ev.is_primary_format());
        assert_eq!(format!("{ev}"), "198,1093");
    }

    #[test]
    fn parse_different_facility() {
        // Event #435385: site=118, card=27089, Reader 2
        let raw: [u8; 25] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x76, // site = 118
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x69,
            0xD1, // card = 27089
            0x00,
        ];
        let ev = AccessEventData::parse(&raw);
        assert_eq!(ev.site_code, 118);
        assert_eq!(ev.card_code(), 27089);
    }

    #[test]
    fn parse_secondary_format_denied() {
        // Event #435402: AccessDenied(Card) sub=0x03 (UnknownCodeSecondaryFormat)
        let raw: [u8; 25] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // format_select=1
            0x00, 0x00, // site = 0
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0x00,
        ];
        let ev = AccessEventData::parse(&raw);
        assert!(!ev.is_primary_format());
        assert_eq!(ev.site_code, 0);
        assert_eq!(ev.card_code(), 0x03FFFFFFFFFFFF);
    }

    #[test]
    fn round_trip() {
        let raw: [u8; 25] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC6, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x45, 0x00,
        ];
        let ev = AccessEventData::parse(&raw);
        let mut out = [0u8; 25];
        ev.write(&mut out);
        assert_eq!(raw, out);
    }

    #[test]
    fn display_debug() {
        let raw: [u8; 25] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC6, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x45, 0x00,
        ];
        let ev = AccessEventData::parse(&raw);
        assert_eq!(format!("{ev:?}"), "AccessEventData(site=198, card=1093)");
    }
}
