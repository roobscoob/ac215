/// A wrapper around a raw command ID byte that prints the human-readable
/// AC215 command name in its `Debug` implementation.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NamedPacketId(pub u8);

impl NamedPacketId {
    pub fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            0x10 => "RequestVerHEXFile805Extension",
            0x11 => "DownloadHEXfileAC825Extensions",
            0x12 => "StartProgramAC825Extensions",
            0x20 => "ConfigurationModeAC825",
            0x21 => "UpdateUserDataAndPanelConfigurationAC825",
            0x22 => "OutputsManual",
            0x23 => "InputsManual",
            0x24 => "ReadersManual",
            0x25 => "SounderManual",
            0x26 => "ForgiveAntipass",
            0x30 => "UpdateModemSettings",
            0x32 => "CodeData",
            0x34 => "UserData",
            0x36 => "EventFilter",
            0x37 => "PanelData",
            0x38 => "DoorData",
            0x39 => "LinksPanel",
            0x3A => "TimezoneConfig",
            0x3B => "InputGroups",
            0x3C => "InterlockGroups825",
            0x3D => "OutputGroups",
            0x3E => "Holidays",
            0x3F => "OutputsDefinitions",
            0x40 => "RequestEvents",
            0x42 => "EnterBoot",
            0x43 => "EnterBoot825",
            0x44 => "DownloadFirmware",
            0x45 => "DownloadFirmware825",
            0x46 => "RequestPanelPassword",
            0x48 => "RequestEvents825",
            0x4A => "FullUploadEvents825",
            0x4B => "GetFullUpload",
            0x50 => "AckEvents825",
            0x54 => "GlobalAntipass",
            0x56 => "InputData",
            0x58 => "RefreshGlobalAntip825",
            0x59 => "SendLogSeverity",
            0x5A => "ReaderData",
            0x5B => "CustomReaderData",
            0x62 => "UpdateClock",
            0x64 => "DeletePanelData",
            0x66 => "RequestUsersStatus",
            0x68 => "RequestTime",
            0x6A => "RequestFirmVer",
            0x6B => "RequestFirmVer825",
            0x6C => "FullArea",
            0x80 => "Answer80",
            0x81 => "BootAck",
            0x82 => "Nack",
            0x91 => "AcknowledgeVerHEXFile805Extension",
            0x92 => "AcknowledgeHEXfileAC825Extensions",
            0x93 => "AcknowledgeProgramAC825Extensions",
            0xC5 => "AnswerFirmwareAck",
            0xC6 => "AnswerFirmwareAck825",
            0xC7 => "AnswerPanelPassword",
            0xC9 => "AnswerEvents825",
            0xCB => "ReportIndexEvent",
            0xCC => "ReportEvents",
            0xD0 => "KeepAlive",
            0xD1 => "KeepAliveAck",
            0xD4 => "SendLogMessage",
            0xD7 => "SearchExtensionManual",
            0xD8 => "SetLogSeverity",
            0xDA => "GetLogSeverity",
            0xE7 => "AnswerUsersStatus",
            0xE9 => "AnswerTime",
            0xEB => "AnswerFirmVer",
            0xF1 => "DeleteUserData",
            0xF2 => "DeleteCardData",
            0xF3 => "MassUpdateUserData",
            0xF4 => "MassUpdateCodeData",
            0xF5 => "MassDeleteUserData",
            0xF6 => "MassDeleteCodeData",
            _ => return None,
        })
    }
}

impl std::fmt::Debug for NamedPacketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:02X} {:?}", self.0, self.name())
    }
}

impl std::fmt::Display for NamedPacketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = self.name();
        if let Some(name) = name {
            write!(f, "{}", name)
        } else {
            write!(f, "0x{:02X}", self.0)
        }
    }
}

impl From<u8> for NamedPacketId {
    fn from(id: u8) -> Self {
        Self(id)
    }
}
