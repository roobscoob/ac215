use core::fmt;

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Timelike, Utc};

/// Panel timestamp packed into 5 bytes.
///
/// The panel encodes both the timestamp and the hardware ID of the
/// source panel into these 5 bytes. The hardware ID occupies the
/// otherwise-unused high bits of the seconds, minutes, and hours bytes.
///
/// ```text
/// byte[0]: bits 7-5 = (year-2000) >> 1 & 7
///          bits 4-0 = day (1-31)
/// byte[1]: bits 7-4 = month (1-12)
///          bits 3-0 = (year-2000) & 0xF
/// byte[2]: bits 7-6 = hardware_id bits [6:5]
///          bits 5-0 = seconds (0-59)
/// byte[3]: bits 7-6 = hardware_id bits [4:3]
///          bits 5-0 = minutes (0-59)
/// byte[4]: bits 7-5 = hardware_id bits [2:0]
///          bits 4-0 = hours (0-23)
/// ```
///
/// The server extracts the hardware ID to route events to the correct
/// extension panel. Events with hardware_id=0 are rejected as
/// "event quality is problematic".
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PackedTimestamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    /// Panel hardware ID, encoded in the high bits of bytes 2–4.
    /// Must match the panel's `IdHardware` (typically 1 for the
    /// main AC-825 controller). Events with hardware_id=0 are
    /// dropped by the server.
    ///
    /// This is effectively `Ac215AddressTarget` but annoying
    pub hardware_id: u8,
}

impl PackedTimestamp {
    /// Decode from the 5-byte packed wire format.
    pub fn decode(bytes: &[u8; 5]) -> Self {
        let year = (((bytes[0] & 0xE0) >> 1) as u16) + ((bytes[1] & 0x0F) as u16) + 2000;
        let month = (bytes[1] & 0xF0) >> 4;
        let day = bytes[0] & 0x1F;
        let second = bytes[2] & 0x3F;
        let minute = bytes[3] & 0x3F;
        let hour = bytes[4] & 0x1F;

        let hardware_id =
            ((bytes[2] & 0xC0) >> 1) | ((bytes[3] & 0xC0) >> 3) | ((bytes[4] & 0xE0) >> 5);

        Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            hardware_id,
        }
    }

    /// Encode to the 5-byte packed wire format.
    pub fn encode(&self) -> [u8; 5] {
        let y = (self.year.saturating_sub(2000)) as u8;
        let hw = self.hardware_id;
        [
            ((y & 0x70) << 1) | (self.day & 0x1F),
            ((self.month & 0x0F) << 4) | (y & 0x0F),
            (self.second & 0x3F) | ((hw & 0x60) << 1),
            (self.minute & 0x3F) | ((hw & 0x18) << 3),
            (self.hour & 0x1F) | ((hw & 0x07) << 5),
        ]
    }

    /// Convert from a chrono `DateTime<Utc>` with explicit hardware ID.
    pub fn from_chrono_with_hardware(dt: DateTime<Utc>, hardware_id: u8) -> Self {
        Self {
            year: dt.year() as u16,
            month: dt.month() as u8,
            day: dt.day() as u8,
            hour: dt.hour() as u8,
            minute: dt.minute() as u8,
            second: dt.second() as u8,
            hardware_id,
        }
    }

    /// Convert from a chrono `DateTime<Utc>`.
    ///
    /// Sets hardware_id to 0. Use `from_chrono_with_hardware` or set
    /// the field manually if you need a specific hardware ID.
    pub fn from_chrono(dt: DateTime<Utc>) -> Self {
        Self::from_chrono_with_hardware(dt, 0)
    }

    /// Convert to a chrono `DateTime<Utc>`.
    ///
    /// Returns `None` if the fields don't form a valid date/time.
    pub fn to_chrono(&self) -> Option<DateTime<Utc>> {
        let date = NaiveDate::from_ymd_opt(self.year as i32, self.month as u32, self.day as u32)?;
        let time =
            NaiveTime::from_hms_opt(self.hour as u32, self.minute as u32, self.second as u32)?;
        Some(date.and_time(time).and_utc())
    }
}

impl fmt::Debug for PackedTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second,
        )
    }
}

impl fmt::Display for PackedTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl From<DateTime<Utc>> for PackedTimestamp {
    fn from(dt: DateTime<Utc>) -> Self {
        Self::from_chrono(dt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn round_trip() {
        let dt = NaiveDate::from_ymd_opt(2026, 3, 26)
            .unwrap()
            .and_hms_opt(14, 30, 45)
            .unwrap()
            .and_utc();

        let ts = PackedTimestamp::from_chrono_with_hardware(dt, 1);
        let encoded = ts.encode();
        let decoded = PackedTimestamp::decode(&encoded);
        assert_eq!(ts, decoded);
        assert_eq!(decoded.to_chrono(), Some(dt));
        assert_eq!(decoded.hardware_id, 1);
    }

    #[test]
    fn round_trip_all_hardware_ids() {
        let dt = NaiveDate::from_ymd_opt(2026, 6, 15)
            .unwrap()
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc();

        for hw in 0..=127u8 {
            let ts = PackedTimestamp::from_chrono_with_hardware(dt, hw);
            let encoded = ts.encode();
            let decoded = PackedTimestamp::decode(&encoded);
            assert_eq!(
                decoded.hardware_id, hw,
                "hardware_id {hw} failed round trip"
            );
            assert_eq!(decoded.to_chrono(), Some(dt), "time corrupted for hw={hw}");
        }
    }

    #[test]
    fn decode_known() {
        // 2026-03-15 10:25:30, hardware_id=0
        let bytes = [0x2F, 0x3A, 0x1E, 0x19, 0x0A];
        let ts = PackedTimestamp::decode(&bytes);
        assert_eq!(ts.year, 2026);
        assert_eq!(ts.month, 3);
        assert_eq!(ts.day, 15);
        assert_eq!(ts.hour, 10);
        assert_eq!(ts.minute, 25);
        assert_eq!(ts.second, 30);
        assert_eq!(ts.hardware_id, 0);
    }

    #[test]
    fn decode_with_hardware_id_1() {
        // From the captured pcap: 2026-03-27 21:56:43, hardware_id=1
        // byte[4] = 0x35 = hours 21 (0x15) | hw bit 0 at position 5 (0x20)
        let bytes = [0x3B, 0x3A, 0x2B, 0x38, 0x35];
        let ts = PackedTimestamp::decode(&bytes);
        assert_eq!(ts.year, 2026);
        assert_eq!(ts.month, 3);
        assert_eq!(ts.day, 27);
        assert_eq!(ts.hour, 21);
        assert_eq!(ts.minute, 56);
        assert_eq!(ts.second, 43);
        assert_eq!(ts.hardware_id, 1);
    }

    #[test]
    fn hardware_id_bit_layout() {
        // Verify each bit of hardware_id lands in the right place
        let dt = NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();

        // hw=1 (bit 0) → byte[4] bit 5
        let ts = PackedTimestamp::from_chrono_with_hardware(dt, 1);
        let enc = ts.encode();
        assert_eq!(enc[4] & 0xE0, 0x20);
        assert_eq!(enc[3] & 0xC0, 0x00);
        assert_eq!(enc[2] & 0xC0, 0x00);

        // hw=8 (bit 3) → byte[3] bit 6
        let ts = PackedTimestamp::from_chrono_with_hardware(dt, 8);
        let enc = ts.encode();
        assert_eq!(enc[4] & 0xE0, 0x00);
        assert_eq!(enc[3] & 0xC0, 0x40);
        assert_eq!(enc[2] & 0xC0, 0x00);

        // hw=32 (bit 5) → byte[2] bit 6
        let ts = PackedTimestamp::from_chrono_with_hardware(dt, 32);
        let enc = ts.encode();
        assert_eq!(enc[4] & 0xE0, 0x00);
        assert_eq!(enc[3] & 0xC0, 0x00);
        assert_eq!(enc[2] & 0xC0, 0x40);

        // hw=127 (all bits) → all high bits set
        let ts = PackedTimestamp::from_chrono_with_hardware(dt, 127);
        let enc = ts.encode();
        assert_eq!(enc[4] & 0xE0, 0xE0);
        assert_eq!(enc[3] & 0xC0, 0xC0);
        assert_eq!(enc[2] & 0xC0, 0xC0);
    }

    #[test]
    fn chrono_round_trip() {
        let dt = NaiveDate::from_ymd_opt(2042, 12, 31)
            .unwrap()
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc();

        let ts = PackedTimestamp::from(dt);
        assert_eq!(ts.to_chrono(), Some(dt));
        assert_eq!(ts.hardware_id, 0);
    }
}
