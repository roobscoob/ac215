use std::fmt::Debug;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ac215Address {
    upper: u8,
    lower: u8,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ac215AddressMode {
    Single,
    Dual,
}

pub enum Ac215AddressTarget {
    Broadcast,
    MainController,
    ExtensionBoard { slot: u8, r#type: u8 },
}

impl Ac215Address {
    pub const SERVER: Self = Self {
        upper: 0x01,
        lower: 0x01,
    };

    pub fn panel_main_controller(mode: Ac215AddressMode) -> Self {
        let upper = match mode {
            Ac215AddressMode::Single => 0x02,
            Ac215AddressMode::Dual => 0x03,
        };

        Self { upper, lower: 0x01 }
    }

    pub fn panel_extension_board(mode: Ac215AddressMode, slot: u8, r#type: u8) -> Self {
        assert!((1..=4).contains(&(mode as u8)));
        assert!((2..=5).contains(&slot));
        assert!(matches!(r#type, 0x02 | 0x03));

        let upper = ((mode as u8) << 4) | r#type;

        Self { upper, lower: slot }
    }

    pub fn parse(upper: u8, lower: u8) -> Option<Self> {
        match (lower, upper) {
            (0x01, 0x01) => Some(Self { upper, lower }),
            (0x01 | 0xFF, 0x02 | 0x03) => Some(Self { upper, lower }),
            (2..=5, _)
                if (1..=4).contains(&(upper >> 4)) && matches!(upper & 0x0F, 0x02 | 0x03) =>
            {
                Some(Self { upper, lower })
            }
            _ => None,
        }
    }

    pub fn into_bytes(&self) -> [u8; 2] {
        [self.upper, self.lower]
    }

    pub fn is_server(&self) -> bool {
        *self == Self::SERVER
    }

    pub fn is_panel(&self) -> bool {
        !self.is_server()
    }

    pub fn address_mode(&self) -> Option<Ac215AddressMode> {
        if self.is_server() {
            return None;
        }

        let nibble = self.upper & 0x0F;

        match nibble {
            0x02 => Some(Ac215AddressMode::Single),
            0x03 => Some(Ac215AddressMode::Dual),
            _ => unreachable!(),
        }
    }

    pub fn target(&self) -> Option<Ac215AddressTarget> {
        if self.is_server() {
            return None;
        }

        if self.lower == 0xFF {
            return Some(Ac215AddressTarget::Broadcast);
        }

        match (self.lower, self.upper >> 4) {
            (0x01, 0x00) => Some(Ac215AddressTarget::MainController),
            (2..=5, 1..=4) => Some(Ac215AddressTarget::ExtensionBoard {
                slot: self.lower,
                r#type: self.upper >> 4,
            }),
            _ => unreachable!(),
        }
    }
}

impl Debug for Ac215Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_server() {
            write!(f, "Server")
        } else {
            let mode = self.address_mode().unwrap();
            let target = self.target().unwrap();

            match (mode, target) {
                (Ac215AddressMode::Single, Ac215AddressTarget::MainController) => {
                    write!(f, "Panel (Single Addressing, Main Controller)")
                }
                (Ac215AddressMode::Dual, Ac215AddressTarget::MainController) => {
                    write!(f, "Panel (Dual Addressing, Main Controller)")
                }
                (Ac215AddressMode::Single, Ac215AddressTarget::Broadcast) => {
                    write!(f, "Panel (Single Addressing, Broadcast)")
                }
                (Ac215AddressMode::Dual, Ac215AddressTarget::Broadcast) => {
                    write!(f, "Panel (Dual Addressing, Broadcast)")
                }
                (Ac215AddressMode::Single, Ac215AddressTarget::ExtensionBoard { slot, r#type }) => {
                    write!(
                        f,
                        "Panel (Single Addressing, Extension Board Slot {}, Type {})",
                        slot, r#type
                    )
                }
                (Ac215AddressMode::Dual, Ac215AddressTarget::ExtensionBoard { slot, r#type }) => {
                    write!(
                        f,
                        "Panel (Dual Addressing, Extension Board Slot {}, Type {})",
                        slot, r#type
                    )
                }
            }
        }
    }
}
