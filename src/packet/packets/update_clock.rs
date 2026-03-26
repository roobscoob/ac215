use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Timelike, Utc};

use crate::packet::Ac215Packet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateClockPacket {
    pub year: u8,
    pub month: u8,
    pub day_of_week: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[derive(Debug)]
pub enum UpdateClockError {
    TooShort { expected: usize, actual: usize },
}

impl core::fmt::Display for UpdateClockError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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

impl UpdateClockPacket {
    pub fn from_chrono(dt: DateTime<Utc>) -> Self {
        Self {
            year: (dt.year() - 2000) as u8,
            month: dt.month() as u8,
            day_of_week: dt.weekday().num_days_from_sunday() as u8,
            day: dt.day() as u8,
            hour: dt.hour() as u8,
            minute: dt.minute() as u8,
            second: dt.second() as u8,
        }
    }

    pub fn into_chrono(self) -> Option<DateTime<Utc>> {
        let date = NaiveDate::from_ymd_opt(2000 + self.year as i32, self.month as u32, self.day as u32)?;
        let time = NaiveTime::from_hms_opt(self.hour as u32, self.minute as u32, self.second as u32)?;
        Some(date.and_time(time).and_utc())
    }
}

impl Ac215Packet for UpdateClockPacket {
    type Error = UpdateClockError;

    fn packet_id(&self) -> u8 {
        0x62
    }

    fn into_bytes(self, out: &mut [u8; 467]) -> u16 {
        out[0] = self.year;
        out[1] = self.month;
        out[2] = self.day_of_week;
        out[3] = self.day;
        out[4] = self.hour;
        out[5] = self.minute;
        out[6] = self.second;
        7
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() < 7 {
            return Err(UpdateClockError::TooShort {
                expected: 7,
                actual: bytes.len(),
            });
        }

        Ok(Self {
            year: bytes[0],
            month: bytes[1],
            day_of_week: bytes[2],
            day: bytes[3],
            hour: bytes[4],
            minute: bytes[5],
            second: bytes[6],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let pkt = UpdateClockPacket {
            year: 26,
            month: 3,
            day_of_week: 4,
            day: 26,
            hour: 14,
            minute: 30,
            second: 45,
        };

        let mut out = [0u8; 467];
        let len = pkt.into_bytes(&mut out);
        assert_eq!(len, 7);
        assert_eq!(&out[..7], &[26, 3, 4, 26, 14, 30, 45]);

        let pkt2 = UpdateClockPacket::from_bytes(&out[..len as usize]).unwrap();
        assert_eq!(pkt2, pkt);
    }

    #[test]
    fn chrono_round_trip() {
        let dt = NaiveDate::from_ymd_opt(2026, 3, 26)
            .unwrap()
            .and_hms_opt(14, 30, 45)
            .unwrap()
            .and_utc();

        let pkt = UpdateClockPacket::from_chrono(dt);
        assert_eq!(pkt.year, 26);
        assert_eq!(pkt.month, 3);
        assert_eq!(pkt.day_of_week, 4); // Thursday
        assert_eq!(pkt.day, 26);
        assert_eq!(pkt.hour, 14);
        assert_eq!(pkt.minute, 30);
        assert_eq!(pkt.second, 45);

        let dt2 = pkt.into_chrono().unwrap();
        assert_eq!(dt, dt2);
    }

    #[test]
    fn too_short() {
        let err = UpdateClockPacket::from_bytes(&[0u8; 3]).unwrap_err();
        assert!(matches!(err, UpdateClockError::TooShort { .. }));
    }

    #[test]
    fn invalid_date_returns_none() {
        let pkt = UpdateClockPacket {
            year: 26,
            month: 13, // invalid
            day_of_week: 0,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        };
        assert!(pkt.into_chrono().is_none());
    }
}
