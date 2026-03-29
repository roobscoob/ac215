use core::fmt;

/// The physical location on the panel that an event originated from.
///
/// Decoded from the `event_source` byte in an event record.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventLocation {
    Panel,
    Door(u8),
    Reader(u8),
    Voltage(VoltageSource),
    Input(u8),
    Output(u8),
    Unknown(u8),
}

/// Voltage source identifiers (0x31–0x37).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoltageSource {
    Aux,
    Vin,
    Vexp,
    Readers,
    Src,
    AuxPwr,
    Battery,
}

impl EventLocation {
    /// Decode from the raw `event_source` byte.
    pub fn from_byte(b: u8) -> Self {
        match b {
            0x01 => Self::Panel,

            0x11..=0x1A => Self::Door(b - 0x11),
            0x21..=0x2A => Self::Reader(b - 0x21),

            0x31 => Self::Voltage(VoltageSource::Aux),
            0x32 => Self::Voltage(VoltageSource::Vin),
            0x33 => Self::Voltage(VoltageSource::Vexp),
            0x34 => Self::Voltage(VoltageSource::Readers),
            0x35 => Self::Voltage(VoltageSource::Src),
            0x36 => Self::Voltage(VoltageSource::AuxPwr),
            0x37 => Self::Voltage(VoltageSource::Battery),

            0x41..=0x4F => Self::Input(b - 0x41),
            0x51..=0x5F => Self::Output(b - 0x51),

            0x81..=0x91 => Self::Input(b - 0x81 + 15),
            0xA1..=0xAB => Self::Output(b - 0xA1 + 15),

            _ => Self::Unknown(b),
        }
    }

    /// Encode back to the raw `event_source` byte.
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Panel => 0x01,

            Self::Door(n) => 0x11 + n,
            Self::Reader(n) => 0x21 + n,

            Self::Voltage(VoltageSource::Aux) => 0x31,
            Self::Voltage(VoltageSource::Vin) => 0x32,
            Self::Voltage(VoltageSource::Vexp) => 0x33,
            Self::Voltage(VoltageSource::Readers) => 0x34,
            Self::Voltage(VoltageSource::Src) => 0x35,
            Self::Voltage(VoltageSource::AuxPwr) => 0x36,
            Self::Voltage(VoltageSource::Battery) => 0x37,

            Self::Input(n) if n < 15 => 0x41 + n,
            Self::Input(n) => 0x81 + n - 15,

            Self::Output(n) if n < 15 => 0x51 + n,
            Self::Output(n) => 0xA1 + n - 15,

            Self::Unknown(b) => b,
        }
    }
}

impl fmt::Debug for EventLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Panel => write!(f, "Panel"),
            Self::Door(n) => write!(f, "Door {n}"),
            Self::Reader(n) => write!(f, "Reader {n}"),
            Self::Voltage(v) => write!(f, "Voltage({v:?})"),
            Self::Input(n) => write!(f, "Input {n}"),
            Self::Output(n) => write!(f, "Output {n}"),
            Self::Unknown(b) => write!(f, "Unknown(0x{b:02X})"),
        }
    }
}

impl fmt::Display for EventLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel() {
        assert_eq!(EventLocation::from_byte(0x01), EventLocation::Panel);
        assert_eq!(EventLocation::Panel.to_byte(), 0x01);
    }

    #[test]
    fn doors() {
        for i in 1..=10u8 {
            let loc = EventLocation::from_byte(0x10 + i);
            assert_eq!(loc, EventLocation::Door(i - 1));
            assert_eq!(loc.to_byte(), 0x10 + i);
        }
    }

    #[test]
    fn readers() {
        for i in 1..=10u8 {
            let loc = EventLocation::from_byte(0x20 + i);
            assert_eq!(loc, EventLocation::Reader(i - 1));
            assert_eq!(loc.to_byte(), 0x20 + i);
        }
    }

    #[test]
    fn voltage_sources() {
        assert_eq!(
            EventLocation::from_byte(0x31),
            EventLocation::Voltage(VoltageSource::Aux)
        );
        assert_eq!(
            EventLocation::from_byte(0x37),
            EventLocation::Voltage(VoltageSource::Battery)
        );
        assert_eq!(
            EventLocation::Voltage(VoltageSource::Battery).to_byte(),
            0x37
        );
    }

    #[test]
    fn inputs_low() {
        for i in 1..=15u8 {
            let loc = EventLocation::from_byte(0x40 + i);
            assert_eq!(loc, EventLocation::Input(i - 1));
            assert_eq!(loc.to_byte(), 0x40 + i);
        }
    }

    #[test]
    fn inputs_high() {
        for i in 16..=32u8 {
            let loc = EventLocation::from_byte(0x81 + i - 16);
            assert_eq!(loc, EventLocation::Input(i - 1));
            assert_eq!(loc.to_byte(), 0x81 + i - 16);
        }
    }

    #[test]
    fn outputs_low() {
        for i in 1..=15u8 {
            let loc = EventLocation::from_byte(0x50 + i);
            assert_eq!(loc, EventLocation::Output(i - 1));
            assert_eq!(loc.to_byte(), 0x50 + i);
        }
    }

    #[test]
    fn outputs_high() {
        for i in 16..=26u8 {
            let loc = EventLocation::from_byte(0xA1 + i - 16);
            assert_eq!(loc, EventLocation::Output(i - 1));
            assert_eq!(loc.to_byte(), 0xA1 + i - 16);
        }
    }

    #[test]
    fn unknown() {
        let loc = EventLocation::from_byte(0xFF);
        assert_eq!(loc, EventLocation::Unknown(0xFF));
        assert_eq!(loc.to_byte(), 0xFF);
    }
}
