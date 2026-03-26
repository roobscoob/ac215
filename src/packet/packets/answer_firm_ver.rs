use core::fmt;

use heapless::String;

use crate::packet::Ac215Packet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McuType {
    StVc,
    StVg,
    Unknown,
}

impl McuType {
    fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::StVc,
            1 => Self::StVg,
            _ => Self::Unknown,
        }
    }

    fn into_byte(self) -> u8 {
        match self {
            Self::StVc => 0,
            Self::StVg => 1,
            Self::Unknown => 0xFF,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ac825ExtensionType {
    D805,
    R805,
    S805,
    P805,
    Unknown(u8),
}

impl Ac825ExtensionType {
    fn from_nibble(val: u8) -> Self {
        match val {
            1 => Self::D805,
            2 => Self::R805,
            3 => Self::S805,
            4 => Self::P805,
            v => Self::Unknown(v),
        }
    }

    fn into_nibble(self) -> u8 {
        match self {
            Self::D805 => 1,
            Self::R805 => 2,
            Self::S805 => 3,
            Self::P805 => 4,
            Self::Unknown(v) => v,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExtensionPanel {
    pub panel_type: Ac825ExtensionType,
    pub double_panel: bool,
    pub firmware: String<5>,
}

fn string_from_bytes<const N: usize>(bytes: &[u8]) -> String<N> {
    let mut s = String::new();
    let text = core::str::from_utf8(bytes).unwrap_or("");
    let trimmed = text.trim_end_matches('\0').trim_end();
    for c in trimmed.chars() {
        if s.push(c).is_err() {
            break;
        }
    }
    s
}

fn string_to_bytes<const N: usize>(s: &String<N>, out: &mut [u8]) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(out.len());
    out[..len].copy_from_slice(&bytes[..len]);
    out[len..].fill(0);
}

/// The response variant, determined by the raw_size byte in the frame header.
///
/// The C# code branches on `P_1[num + 4]` (the raw_size byte):
/// - raw_size 9:       minimal response, bootloader only (8 bytes at offset 0)
/// - raw_size 54:      boot mode — no firmware, ticket at offset 36
/// - raw_size 69..149: old/unsupported firmware, no ticket
/// - raw_size 149:     full response — firmware, extensions, ticket at offset 133
///
/// Since `from_bytes` receives the decrypted payload (after the 6-byte protocol header),
/// the payload length encodes the variant:
/// - raw_size  9 → pad16( 14) =  16 → payload =  10 bytes
/// - raw_size 54 → pad16( 59) =  64 → payload =  58 bytes
/// - raw_size 69 → pad16( 74) =  80 → payload =  74 bytes
/// - raw_size 149→ pad16(154) = 160 → payload = 154 bytes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareStatus {
    /// raw_size 9: panel responded with just a bootloader, no firmware at all.
    Minimal,
    /// raw_size 54: panel is in boot mode, firmware area is empty.
    BootMode,
    /// raw_size 69..149: panel has firmware but it's an old/unsupported version.
    OldFirmware,
    /// raw_size 149: full response with current firmware.
    Current,
}

#[derive(Clone, Debug)]
pub struct AnswerFirmVer825Packet {
    pub status: FirmwareStatus,
    pub firmware: String<16>,
    pub bootloader: String<16>,
    pub event_number: u32,
    pub hardware_type: u8,
    pub extensions: [Option<ExtensionPanel>; 16],
    pub mcu_type: McuType,
    pub double_panel: bool,
    pub ticket: Option<[u8; 16]>,
}

#[derive(Debug)]
pub enum AnswerFirmVerError {
    TooShort { expected: usize, actual: usize },
}

impl fmt::Display for AnswerFirmVerError {
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

fn extract_ticket(bytes: &[u8]) -> Option<[u8; 16]> {
    if bytes.len() < 16 {
        return None;
    }
    let mut t = [0u8; 16];
    t.copy_from_slice(&bytes[..16]);
    if t.iter().all(|&b| b == 0) {
        None
    } else {
        Some(t)
    }
}

impl Ac215Packet for AnswerFirmVer825Packet {
    type Error = AnswerFirmVerError;

    fn packet_id(&self) -> u8 {
        0xEB
    }

    fn into_bytes(self, out: &mut [u8; 467]) -> u16 {
        match self.status {
            FirmwareStatus::Minimal => {
                // raw_size 9: only bootloader, 8 bytes at offset 0
                string_to_bytes(&self.bootloader, &mut out[0..8]);
                4 // data_length = raw_size - 5 = 4
            }
            FirmwareStatus::BootMode => {
                // raw_size 54: firmware area present but empty, bootloader at 16
                string_to_bytes(&self.firmware, &mut out[0..16]);
                string_to_bytes(&self.bootloader, &mut out[16..32]);
                out[32] = (self.event_number >> 16) as u8;
                out[33] = (self.event_number >> 8) as u8;
                out[34] = self.event_number as u8;
                out[35] = self.hardware_type;
                // ticket at offset 36 in boot mode
                match self.ticket {
                    Some(ticket) => out[36..52].copy_from_slice(&ticket),
                    None => out[36..52].fill(0),
                }
                49 // data_length = 54 - 5
            }
            FirmwareStatus::OldFirmware => {
                // raw_size 69..149: firmware + bootloader + basics, no extensions/ticket
                string_to_bytes(&self.firmware, &mut out[0..16]);
                string_to_bytes(&self.bootloader, &mut out[16..32]);
                out[32] = (self.event_number >> 16) as u8;
                out[33] = (self.event_number >> 8) as u8;
                out[34] = self.event_number as u8;
                out[35] = self.hardware_type;
                64 // data_length = 69 - 5
            }
            FirmwareStatus::Current => {
                // raw_size 149: full response
                string_to_bytes(&self.firmware, &mut out[0..16]);
                string_to_bytes(&self.bootloader, &mut out[16..32]);
                out[32] = (self.event_number >> 16) as u8;
                out[33] = (self.event_number >> 8) as u8;
                out[34] = self.event_number as u8;
                out[35] = self.hardware_type;

                for (i, slot) in self.extensions.iter().enumerate() {
                    let base = 36 + i * 6;
                    match slot {
                        Some(ext) => {
                            let double_bits = if ext.double_panel { 0x30 } else { 0x00 };
                            out[base] = ext.panel_type.into_nibble() | double_bits;
                            string_to_bytes(&ext.firmware, &mut out[base + 1..base + 6]);
                        }
                        None => {
                            out[base..base + 6].fill(0);
                        }
                    }
                }

                out[132] = self.mcu_type.into_byte();

                match self.ticket {
                    Some(ticket) => out[133..149].copy_from_slice(&ticket),
                    None => out[133..149].fill(0),
                }

                149
            }
        }
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, Self::Error> {
        let len = bytes.len();

        // Minimal response (raw_size 9, payload 10 bytes):
        // bootloader is 8 bytes at offset 0, nothing else meaningful
        if len < 32 {
            if len < 8 {
                return Err(AnswerFirmVerError::TooShort {
                    expected: 8,
                    actual: len,
                });
            }
            return Ok(Self {
                status: FirmwareStatus::Minimal,
                firmware: String::new(),
                bootloader: string_from_bytes(&bytes[0..8]),
                event_number: 0,
                hardware_type: 0,
                extensions: Default::default(),
                mcu_type: McuType::Unknown,
                double_panel: false,
                ticket: None,
            });
        }

        // All other variants have firmware at 0-15, bootloader at 16-31
        if len < 36 {
            return Err(AnswerFirmVerError::TooShort {
                expected: 36,
                actual: len,
            });
        }

        let firmware: String<16> = string_from_bytes(&bytes[0..16]);
        let bootloader: String<16> = string_from_bytes(&bytes[16..32]);
        let event_number =
            ((bytes[32] as u32) << 16) | ((bytes[33] as u32) << 8) | (bytes[34] as u32);
        let hardware_type = bytes[35] & 0x07;
        let mcu_type = McuType::from_byte(bytes[32]);

        // Boot mode (raw_size 54, payload 58 bytes):
        // No real firmware, ticket at offset 36
        if len < 74 {
            return Ok(Self {
                status: FirmwareStatus::BootMode,
                firmware,
                bootloader,
                event_number,
                hardware_type,
                extensions: Default::default(),
                mcu_type,
                double_panel: false,
                ticket: if len >= 52 {
                    extract_ticket(&bytes[36..52])
                } else {
                    None
                },
            });
        }

        // Old firmware (raw_size 69..148, payload 74..143):
        // Has firmware string but version is old/unsupported, no extensions or ticket
        if len < 144 {
            return Ok(Self {
                status: FirmwareStatus::OldFirmware,
                firmware,
                bootloader,
                event_number,
                hardware_type,
                extensions: Default::default(),
                mcu_type,
                double_panel: false,
                ticket: None,
            });
        }

        // Full response (raw_size 149, payload 154 bytes):
        // Extensions at 36-131, MCU at 132, ticket at 133-148
        let mut extensions: [Option<ExtensionPanel>; 16] = Default::default();
        for i in 0..16 {
            let base = 36 + i * 6;
            let type_byte = bytes[base];
            if type_byte > 0 {
                extensions[i] = Some(ExtensionPanel {
                    panel_type: Ac825ExtensionType::from_nibble(type_byte & 0x0F),
                    double_panel: (type_byte & 0xF0) >> 4 == 3,
                    firmware: string_from_bytes(&bytes[base + 1..base + 6]),
                });
            }
        }

        Ok(Self {
            status: FirmwareStatus::Current,
            firmware,
            bootloader,
            event_number,
            hardware_type,
            extensions,
            mcu_type,
            double_panel: false,
            ticket: extract_ticket(&bytes[133..149]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_response() {
        let payload: &[u8] = &[
            0x61, 0x63, 0x38, 0x32, 0x35, 0x76, 0x30, 0x32, 0x5F, 0x30, 0x39, 0x5F, 0x36, 0x31,
            0x20, 0x20, // firmware: "ac825v02_09_61  "
            0x62, 0x74, 0x6C, 0x5F, 0x61, 0x63, 0x38, 0x32, 0x35, 0x76, 0x5F, 0x30, 0x31, 0x5F,
            0x30, 0x35, // bootloader: "btl_ac825v_01_05"
            0x01, 0x00, 0x00, // event number
            0x10, // hardware type
            // 96 bytes of zeros: 16 extension slots, all empty
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            // MCU type
            0x01, // ticket (16 bytes)
            0x94, 0x7F, 0xEB, 0x1C, 0x9D, 0x56, 0xED, 0xD8, 0x6E, 0x69, 0x8B, 0xF6, 0xD5, 0xB3,
            0xF4, 0x00, // trailing padding
            0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let pkt = AnswerFirmVer825Packet::from_bytes(payload).unwrap();

        assert_eq!(pkt.status, FirmwareStatus::Current);
        assert_eq!(pkt.firmware.as_str(), "ac825v02_09_61");
        assert_eq!(pkt.bootloader.as_str(), "btl_ac825v_01_05");
        assert_eq!(pkt.event_number, 0x010000);
        assert_eq!(pkt.hardware_type, 0x10 & 0x07);
        assert_eq!(pkt.mcu_type, McuType::StVg);
        assert!(pkt.extensions.iter().all(|e| e.is_none()));
        assert!(pkt.ticket.is_some());
        assert_eq!(pkt.ticket.unwrap()[0], 0x94);

        // Round-trip
        let mut out = [0u8; 467];
        let len = pkt.clone().into_bytes(&mut out);
        let pkt2 = AnswerFirmVer825Packet::from_bytes(&out[..len as usize]).unwrap();
        assert_eq!(pkt2.status, FirmwareStatus::Current);
        assert_eq!(pkt2.firmware.as_str(), pkt.firmware.as_str());
        assert_eq!(pkt2.bootloader.as_str(), pkt.bootloader.as_str());
        assert_eq!(pkt2.event_number, pkt.event_number);
        assert_eq!(pkt2.ticket, pkt.ticket);
    }

    #[test]
    fn parse_minimal_response() {
        // raw_size 9: just 8 bytes of bootloader, padded to 10
        let mut payload = [0u8; 10];
        payload[0..8].copy_from_slice(b"btl_0105");

        let pkt = AnswerFirmVer825Packet::from_bytes(&payload).unwrap();

        assert_eq!(pkt.status, FirmwareStatus::Minimal);
        assert_eq!(pkt.firmware.as_str(), "");
        assert_eq!(pkt.bootloader.as_str(), "btl_0105");
        assert_eq!(pkt.event_number, 0);
        assert!(pkt.ticket.is_none());
    }

    #[test]
    fn parse_boot_mode_response() {
        // raw_size 54: firmware area + bootloader + basics + ticket at offset 36
        let mut payload = [0u8; 58];
        // firmware (empty in boot mode)
        // bootloader at 16
        payload[16..32].copy_from_slice(b"btl_ac825v_01_05");
        // event number at 32
        payload[32] = 0x00;
        payload[33] = 0x01;
        payload[34] = 0x00;
        // hardware type at 35
        payload[35] = 0x02;
        // ticket at 36
        payload[36..52].copy_from_slice(&[
            0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
            0x99, 0x00,
        ]);

        let pkt = AnswerFirmVer825Packet::from_bytes(&payload).unwrap();

        assert_eq!(pkt.status, FirmwareStatus::BootMode);
        assert_eq!(pkt.firmware.as_str(), "");
        assert_eq!(pkt.bootloader.as_str(), "btl_ac825v_01_05");
        assert_eq!(pkt.event_number, 0x000100);
        assert_eq!(pkt.hardware_type, 0x02);
        assert!(pkt.extensions.iter().all(|e| e.is_none()));
        assert!(pkt.ticket.is_some());
        assert_eq!(pkt.ticket.unwrap()[0], 0xAA);
    }

    #[test]
    fn parse_old_firmware_response() {
        // raw_size 69..148: has firmware but old version, no extensions or ticket
        let mut payload = [0u8; 74];
        payload[0..16].copy_from_slice(b"ac825v01_03_20\0\0");
        payload[16..32].copy_from_slice(b"btl_ac825v_01_05");
        payload[32] = 0x00;
        payload[33] = 0x00;
        payload[34] = 0x42;
        payload[35] = 0x03;

        let pkt = AnswerFirmVer825Packet::from_bytes(&payload).unwrap();

        assert_eq!(pkt.status, FirmwareStatus::OldFirmware);
        assert_eq!(pkt.firmware.as_str(), "ac825v01_03_20");
        assert_eq!(pkt.bootloader.as_str(), "btl_ac825v_01_05");
        assert_eq!(pkt.event_number, 0x42);
        assert_eq!(pkt.hardware_type, 0x03);
        assert!(pkt.ticket.is_none());
    }

    #[test]
    fn parse_full_with_extensions() {
        let mut payload = [0u8; 154];
        payload[0..16].copy_from_slice(b"ac825v02_09_61\x20\x20");
        payload[16..32].copy_from_slice(b"btl_ac825v_01_05");
        payload[32] = 0x01;
        payload[33] = 0x00;
        payload[34] = 0x00;
        payload[35] = 0x10;

        // Extension slot 0 (hardware ID 2): D805 panel
        payload[36] = 0x01; // type nibble 1 = D805, upper nibble 0 = not double
        payload[37..42].copy_from_slice(b"v1.00");

        // Extension slot 1 (hardware ID 3): R805 double panel
        payload[42] = 0x32; // type nibble 2 = R805, upper nibble 3 = double
        payload[43..48].copy_from_slice(b"v2.10");

        // MCU type
        payload[132] = 0x01; // ST_VG

        // Ticket
        payload[133..149].copy_from_slice(&[
            0x94, 0x7F, 0xEB, 0x1C, 0x9D, 0x56, 0xED, 0xD8, 0x6E, 0x69, 0x8B, 0xF6, 0xD5, 0xB3,
            0xF4, 0xAA,
        ]);

        let pkt = AnswerFirmVer825Packet::from_bytes(&payload).unwrap();

        assert_eq!(pkt.status, FirmwareStatus::Current);

        let ext0 = pkt.extensions[0].as_ref().unwrap();
        assert_eq!(ext0.panel_type, Ac825ExtensionType::D805);
        assert!(!ext0.double_panel);
        assert_eq!(ext0.firmware.as_str(), "v1.00");

        let ext1 = pkt.extensions[1].as_ref().unwrap();
        assert_eq!(ext1.panel_type, Ac825ExtensionType::R805);
        assert!(ext1.double_panel);
        assert_eq!(ext1.firmware.as_str(), "v2.10");

        assert!(pkt.extensions[2].is_none());
        assert!(pkt.ticket.is_some());
    }

    #[test]
    fn too_short_error() {
        let payload = [0u8; 4];
        let err = AnswerFirmVer825Packet::from_bytes(&payload).unwrap_err();
        assert!(matches!(err, AnswerFirmVerError::TooShort { .. }));
    }
}
