use core::fmt;

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Timelike, Utc};

/// Panel timestamp packed into 5 bytes.
///
/// ```text
/// byte[0]: bits 7-5 = (year-2000) >> 1 & 7, bits 4-0 = day (1-31)
/// byte[1]: bits 7-4 = month (1-12),          bits 3-0 = (year-2000) & 0xF
/// byte[2]: bits 5-0 = seconds (0-59)
/// byte[3]: bits 5-0 = minutes (0-59)
/// byte[4]: bits 4-0 = hours   (0-23)
/// ```
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PackedTimestamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
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

        Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
        }
    }

    /// Encode to the 5-byte packed wire format.
    pub fn encode(&self) -> [u8; 5] {
        let y = (self.year.saturating_sub(2000)) as u8;
        [
            ((y & 0x70) << 1) | (self.day & 0x1F),
            ((self.month & 0x0F) << 4) | (y & 0x0F),
            self.second & 0x3F,
            self.minute & 0x3F,
            self.hour & 0x1F,
        ]
    }

    /// Convert from a chrono `DateTime<Utc>`.
    pub fn from_chrono(dt: DateTime<Utc>) -> Self {
        Self {
            year: dt.year() as u16,
            month: dt.month() as u8,
            day: dt.day() as u8,
            hour: dt.hour() as u8,
            minute: dt.minute() as u8,
            second: dt.second() as u8,
        }
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

        let ts = PackedTimestamp::from_chrono(dt);
        let encoded = ts.encode();
        let decoded = PackedTimestamp::decode(&encoded);
        assert_eq!(ts, decoded);
        assert_eq!(decoded.to_chrono(), Some(dt));
    }

    #[test]
    fn decode_known() {
        // 2026-03-15 10:25:30
        let bytes = [0x2F, 0x3A, 0x1E, 0x19, 0x0A];
        let ts = PackedTimestamp::decode(&bytes);
        assert_eq!(ts.year, 2026);
        assert_eq!(ts.month, 3);
        assert_eq!(ts.day, 15);
        assert_eq!(ts.hour, 10);
        assert_eq!(ts.minute, 25);
        assert_eq!(ts.second, 30);
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
    }
}
