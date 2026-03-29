use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessOutcome {
    Granted,
    Denied,
    Intermediate,
    CodeRecorded,
    ConfigMode,
}

impl AccessOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::Intermediate => "intermediate",
            Self::CodeRecorded => "code_recorded",
            Self::ConfigMode => "config_mode",
        }
    }
}

// ── Event Category (type byte) ──────────────────────────────────────────────

/// High-level event category derived from the event type byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventCategory {
    Recording,
    AccessGranted,
    AccessDenied,
    InputOutput,
    Timezone,
    PcManual,
    Alarm,
    Intermediate,
    System,
    CodeAccepted,
    SdCard,
    Area,
    Configuration,
    Unknown,
}

impl fmt::Display for EventCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Recording => "Recording",
            Self::AccessGranted => "Access Granted",
            Self::AccessDenied => "Access Denied",
            Self::InputOutput => "I/O",
            Self::Timezone => "Timezone",
            Self::PcManual => "PC Manual",
            Self::Alarm => "Alarm",
            Self::Intermediate => "Intermediate",
            Self::System => "System",
            Self::CodeAccepted => "Code Accepted",
            Self::SdCard => "SD Card",
            Self::Area => "Area",
            Self::Configuration => "Configuration",
            Self::Unknown => "Unknown",
        })
    }
}

// ── Event Type ──────────────────────────────────────────────────────────────

/// An event type and subtype pair from the panel's event log.
///
/// The type byte identifies the event category (access granted, I/O change,
/// system alert, etc.) and the subtype provides detail within that category
/// (why access was denied, which card format, etc.).
///
/// Constructed via named constants or `EventType::new()` for unknown values.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventType {
    type_id: u8,
    subtype_id: u8,
}

// ── Construction ────────────────────────────────────────────────────────────

/// Recording / camera events
impl EventType {
    pub const RECORDING_STARTED: Self = Self::of(0x01, 0);
    pub const RECORDING_STOPPED: Self = Self::of(0x02, 0);
    pub const RECORDING_SNAPSHOT: Self = Self::of(0x03, 0);
    pub const MOTION_DETECTION_STARTED: Self = Self::of(0x04, 0);
    pub const MOTION_DETECTION_STOPPED: Self = Self::of(0x05, 0);
    pub const VEHICLE_ACCESS_GRANTED: Self = Self::of(0x06, 0);
    pub const VEHICLE_ACCESS_DENIED: Self = Self::of(0x07, 0);
    pub const TERMINAL_RECORDING_SNAPSHOT: Self = Self::of(0x08, 0);
}

/// Access granted events (type only — combine with access granted subtypes)
impl EventType {
    pub const ACCESS_GRANTED_CARD_WITH_FACILITY: Self = Self::of(0x11, 0);
    pub const ACCESS_GRANTED_CARD: Self = Self::of(0x12, 0);
    pub const ACCESS_GRANTED_PIN: Self = Self::of(0x13, 0);
    pub const ACCESS_RECORDED: Self = Self::of(0x19, 0);
}

/// Access denied events (type only — combine with access denied subtypes)
impl EventType {
    pub const ACCESS_DENIED_CARD_WITH_FACILITY: Self = Self::of(0x21, 0);
    pub const ACCESS_DENIED_CARD: Self = Self::of(0x22, 0);
    pub const ACCESS_DENIED_PIN: Self = Self::of(0x23, 0);
}

/// I/O, siren, input, reader events
impl EventType {
    pub const OUTPUT_OPEN: Self = Self::of(0x31, 0);
    pub const OUTPUT_CLOSED: Self = Self::of(0x32, 0);
    pub const INPUT_OPEN: Self = Self::of(0x33, 0);
    pub const INPUT_CLOSE: Self = Self::of(0x34, 0);
    pub const SIREN_STARTED: Self = Self::of(0x35, 0);
    pub const SIREN_STOPPED: Self = Self::of(0x36, 0);
    pub const INPUT_ARMED: Self = Self::of(0x37, 0);
    pub const INPUT_DISARMED: Self = Self::of(0x38, 0);
    pub const READER_MODE_CHANGED: Self = Self::of(0x3A, 0);
    pub const READER_BELL: Self = Self::of(0x3C, 0);
    pub const DOOR_INTERLOCK: Self = Self::of(0x3D, 0);
}

/// Timezone-driven changes
impl EventType {
    pub const READER_MODE_CHANGED_TIMEZONE: Self = Self::of(0x43, 0);
    pub const READER_MODE_RETURNED_DEFAULT_TIMEZONE: Self = Self::of(0x44, 0);
    pub const TIMED_ANTIPASSBACK_FORGIVE_TIMEZONE: Self = Self::of(0x47, 0);
    pub const DOOR_ANTIPASSBACK_FORGIVE_TIMEZONE: Self = Self::of(0x48, 0);
    pub const GLOBAL_ANTIPASSBACK_FORGIVE_TIMEZONE: Self = Self::of(0x49, 0);
}

/// PC / server manual operations
impl EventType {
    pub const OUTPUT_UNLOCKED_PC_MANUAL: Self = Self::of(0x51, 0);
    pub const OUTPUT_RELOCKED_PC_MANUAL: Self = Self::of(0x52, 0);
    pub const READER_MODE_CHANGED_PC_MANUAL: Self = Self::of(0x53, 0);
    pub const READER_MODE_RETURNED_DEFAULT_PC_MANUAL: Self = Self::of(0x54, 0);
    pub const INPUT_DISARMED_PC_MANUAL: Self = Self::of(0x55, 0);
    pub const INPUT_RETURNED_DEFAULT_PC_MANUAL: Self = Self::of(0x56, 0);
}

/// Alarm and warning events
impl EventType {
    pub const WARNING_CONDITION_STARTED: Self = Self::of(0x61, 0);
    pub const ALARM_STARTED: Self = Self::of(0x63, 0);
    pub const RECORDING_CAMERA_A: Self = Self::of(0x65, 0);
    pub const RECORDING_CAMERA_B: Self = Self::of(0x66, 0);

    pub const DOOR_FORCED_OPEN: Self = Self::of(0x61, 0x02);
    pub const DOOR_HELD_OPEN: Self = Self::of(0x61, 0x04);
}

/// Intermediate ID events
impl EventType {
    pub const INTERMEDIATE_ID_CARD_WITH_FACILITY: Self = Self::of(0x71, 0);
    pub const INTERMEDIATE_ID_CARD: Self = Self::of(0x72, 0);
    pub const INTERMEDIATE_PIN: Self = Self::of(0x73, 0);
}

/// System events
impl EventType {
    pub const SYSTEM_RESET: Self = Self::of(0x81, 0);
    pub const FACTORY_SETTINGS_LOADED: Self = Self::of(0x82, 0);
    pub const FIRMWARE_DOWNLOAD_SUCCEEDED: Self = Self::of(0x83, 0);
    pub const MANUAL_RESET_FACTORY_SETTINGS: Self = Self::of(0x84, 0);
    pub const MODEM_ANSWERED: Self = Self::of(0x85, 0);
    pub const MODEM_DISCONNECTED: Self = Self::of(0x86, 0);
    pub const COMMUNICATION_FAILED: Self = Self::of(0x87, 0);
    pub const COMMUNICATION_RESTORED: Self = Self::of(0x88, 0);
    pub const EVENT_MEMORY_FULL: Self = Self::of(0x89, 0);
    pub const EVENT_MEMORY_LOW: Self = Self::of(0x8A, 0);
    pub const AC_POWER_FAIL: Self = Self::of(0x8C, 0);
    pub const AC_POWER_RESTORED: Self = Self::of(0x8D, 0);
    pub const LOW_BATTERY: Self = Self::of(0x8E, 0);
    pub const BATTERY_OK: Self = Self::of(0x8F, 0);
}

/// Code accepted events
impl EventType {
    pub const CODE_ACCEPTED_WITH_FACILITY: Self = Self::of(0x91, 0);
    pub const CODE_ACCEPTED_CARD: Self = Self::of(0x92, 0);
    pub const CODE_ACCEPTED_PIN: Self = Self::of(0x93, 0);
}

/// SD card / USB events
impl EventType {
    pub const SD_CARD_ALERT: Self = Self::of(0xA0, 0);
    pub const SD_CARD_REMOVED: Self = Self::of(0xA0, 0x01);
    pub const SD_CARD_FAILED: Self = Self::of(0xA0, 0x02);
    pub const DISK_ON_KEY_DISCONNECTED: Self = Self::of(0xA1, 0);
    pub const DISK_ON_KEY_CONNECTED: Self = Self::of(0xA2, 0);
    pub const DISK_ON_KEY_LOW_LEVEL: Self = Self::of(0xA3, 0);
    pub const DISK_ON_KEY_NORMAL_LEVEL: Self = Self::of(0xA4, 0);
    pub const DISK_ON_KEY_ERROR: Self = Self::of(0xA5, 0);
}

/// Area / parking events
impl EventType {
    pub const AREA_IS_FULL: Self = Self::of(0xB0, 0);
    pub const AREA_IS_NOT_FULL: Self = Self::of(0xB1, 0);
}

/// Configuration and panel events
impl EventType {
    pub const CONFIG_MODE_CARD_WITH_FACILITY: Self = Self::of(0xC1, 0);
    pub const CONFIG_MODE_CARD: Self = Self::of(0xC2, 0);
    pub const CONFIG_MODE_PIN: Self = Self::of(0xC3, 0);
    pub const CASE_TAMPER: Self = Self::of(0xC5, 0);
    pub const PANELS_DB_EMPTY: Self = Self::of(0xC6, 0);
    pub const BATTERY_CANNOT_CHARGE: Self = Self::of(0xCC, 0);
    pub const BATTERY_CHARGING: Self = Self::of(0xCD, 0);
    pub const LOW_VOLTAGE: Self = Self::of(0xCE, 0);
    pub const VOLTAGE_OK: Self = Self::of(0xCF, 0);
}

// ── Access Granted subtypes ─────────────────────────────────────────────────

/// Subtype constants for access granted events (types 0x11–0x13).
pub mod access_granted {
    pub const CODE_VALID: u8 = 0x01;
    pub const TIMED_ANTIPASSBACK_SOFT: u8 = 0x02;
    pub const DOOR_ANTIPASSBACK_SOFT: u8 = 0x03;
    pub const GLOBAL_ANTIPASSBACK_SOFT: u8 = 0x04;
    pub const ONLY_FACILITY_CODE: u8 = 0x05;
    pub const FIRST_PERSON_IN_NEW_DAY: u8 = 0x07;
    pub const FIRST_PERSON_IN_CARD: u8 = 0x08;
    pub const SECOND_PERSON_IN_CARD: u8 = 0x09;
    pub const THIRD_PERSON_IN_CARD: u8 = 0x0A;
    pub const SERVER_OVERRIDE: u8 = 0xF1;

    pub fn name(subtype: u8) -> Option<&'static str> {
        Some(match subtype {
            CODE_VALID => "Code Valid",
            TIMED_ANTIPASSBACK_SOFT => "Timed Antipassback (Soft)",
            DOOR_ANTIPASSBACK_SOFT => "Door Antipassback (Soft)",
            GLOBAL_ANTIPASSBACK_SOFT => "Global Antipassback (Soft)",
            ONLY_FACILITY_CODE => "Facility Code Only",
            FIRST_PERSON_IN_NEW_DAY => "First Person In New Day",
            FIRST_PERSON_IN_CARD => "First Person In Card",
            SECOND_PERSON_IN_CARD => "Second Person In Card",
            THIRD_PERSON_IN_CARD => "Third Person In Card",
            SERVER_OVERRIDE => "Server Override",
            _ => return None,
        })
    }
}

// ── Access Denied subtypes ──────────────────────────────────────────────────

/// Subtype constants for access denied events (types 0x21–0x23).
pub mod access_denied {
    pub const UNKNOWN_CODE_PRIMARY: u8 = 0x01;
    pub const WRONG_FACILITY_CODE: u8 = 0x02;
    pub const UNKNOWN_CODE_SECONDARY: u8 = 0x03;
    pub const WRONG_TIMEZONE: u8 = 0x11;
    pub const INVALID_DATE: u8 = 0x12;
    pub const USER_COUNTER_ZERO: u8 = 0x13;
    pub const ACCESS_RECORDED: u8 = 0x19;
    pub const CARD_AND_PIN_WRONG_PIN: u8 = 0x21;
    pub const CARD_AND_PIN_NO_PIN: u8 = 0x22;
    pub const CARD_AND_PIN_NOT_ALLOWED: u8 = 0x23;
    pub const READER_DESKTOP_PRIMARY: u8 = 0x26;
    pub const READER_DESKTOP_SECONDARY: u8 = 0x27;
    pub const READER_DISABLED: u8 = 0x28;
    pub const CODE_TYPE_INVALID: u8 = 0x2A;
    pub const TIMED_ANTIPASSBACK_HARD: u8 = 0x31;
    pub const DOOR_ANTIPASSBACK_HARD: u8 = 0x32;
    pub const GLOBAL_ANTIPASSBACK_HARD: u8 = 0x33;
    pub const DOOR_INTERLOCK: u8 = 0x34;
    pub const WRONG_CARD_UNKNOWN_USER: u8 = 0x35;
    pub const FINGERPRINT_FAILED_KNOWN: u8 = 0x36;
    pub const FINGERPRINT_FAILED_UNKNOWN: u8 = 0x37;
    pub const FINGERPRINT_VERIFY_FAILED_KNOWN: u8 = 0x38;
    pub const FINGERPRINT_VERIFY_FAILED_UNKNOWN: u8 = 0x39;
    pub const DURESS_FINGERPRINT: u8 = 0x3A;
    pub const GROUPS_NOT_EQUAL: u8 = 0x3B;
    pub const CONFIGURATION_MODE: u8 = 0x3C;
    pub const INTERLOCK_CONDITION: u8 = 0x41;
    pub const FULL_AREA_PARKING: u8 = 0x80;
    pub const NO_ACCESS_RIGHTS: u8 = 0xFD;
    pub const UNKNOWN_IN_PANEL: u8 = 0xFE;
    pub const CARD_INACTIVE: u8 = 0xFF;

    pub fn name(subtype: u8) -> Option<&'static str> {
        Some(match subtype {
            UNKNOWN_CODE_PRIMARY => "Unknown Code (Primary Format)",
            WRONG_FACILITY_CODE => "Wrong Facility Code",
            UNKNOWN_CODE_SECONDARY => "Unknown Code (Secondary Format)",
            WRONG_TIMEZONE => "Wrong Timezone",
            INVALID_DATE => "Invalid Date",
            USER_COUNTER_ZERO => "User Counter Zero",
            ACCESS_RECORDED => "Access Recorded",
            CARD_AND_PIN_WRONG_PIN => "Card+PIN: Wrong PIN",
            CARD_AND_PIN_NO_PIN => "Card+PIN: No PIN Entered",
            CARD_AND_PIN_NOT_ALLOWED => "Card+PIN: Not Allowed",
            READER_DESKTOP_PRIMARY => "Reader Desktop Mode (Primary)",
            READER_DESKTOP_SECONDARY => "Reader Desktop Mode (Secondary)",
            READER_DISABLED => "Reader Disabled",
            CODE_TYPE_INVALID => "Code Type Invalid",
            TIMED_ANTIPASSBACK_HARD => "Timed Antipassback (Hard)",
            DOOR_ANTIPASSBACK_HARD => "Door Antipassback (Hard)",
            GLOBAL_ANTIPASSBACK_HARD => "Global Antipassback (Hard)",
            DOOR_INTERLOCK => "Door Interlock",
            WRONG_CARD_UNKNOWN_USER => "Wrong Card / Unknown User ID",
            FINGERPRINT_FAILED_KNOWN => "Fingerprint Failed (Known User)",
            FINGERPRINT_FAILED_UNKNOWN => "Fingerprint Failed (Unknown User)",
            FINGERPRINT_VERIFY_FAILED_KNOWN => "Fingerprint Verify Failed (Known)",
            FINGERPRINT_VERIFY_FAILED_UNKNOWN => "Fingerprint Verify Failed (Unknown)",
            DURESS_FINGERPRINT => "Duress Code (Fingerprint Reader)",
            GROUPS_NOT_EQUAL => "Groups Not Equal (Card+Card Mode)",
            CONFIGURATION_MODE => "Configuration Mode",
            INTERLOCK_CONDITION => "Interlock Condition",
            FULL_AREA_PARKING => "Area Full (Parking)",
            NO_ACCESS_RIGHTS => "No Access Rights",
            UNKNOWN_IN_PANEL => "Unknown In Panel",
            CARD_INACTIVE => "Card Inactive",
            _ => return None,
        })
    }
}

// ── Core implementation ─────────────────────────────────────────────────────

impl EventType {
    const fn of(type_id: u8, subtype_id: u8) -> Self {
        Self {
            type_id,
            subtype_id,
        }
    }

    pub fn new(type_id: u8, subtype_id: u8) -> Self {
        Self::of(type_id, subtype_id)
    }

    pub fn type_id(&self) -> u8 {
        self.type_id
    }

    pub fn subtype_id(&self) -> u8 {
        self.subtype_id
    }

    /// Returns the high-level event category.
    pub fn category(&self) -> EventCategory {
        match self.type_id {
            0x01..=0x08 => EventCategory::Recording,
            0x11..=0x13 | 0x19 => EventCategory::AccessGranted,
            0x21..=0x23 => EventCategory::AccessDenied,
            0x31..=0x3D => EventCategory::InputOutput,
            0x43..=0x49 => EventCategory::Timezone,
            0x51..=0x56 => EventCategory::PcManual,
            0x61..=0x66 => EventCategory::Alarm,
            0x71..=0x73 => EventCategory::Intermediate,
            0x81..=0x8F => EventCategory::System,
            0x91..=0x93 => EventCategory::CodeAccepted,
            0xA0..=0xA5 => EventCategory::SdCard,
            0xB0..=0xB1 => EventCategory::Area,
            0xC1..=0xCF => EventCategory::Configuration,
            _ => EventCategory::Unknown,
        }
    }

    /// Returns a human-readable name for the event type byte.
    pub fn type_name(&self) -> &'static str {
        match self.type_id {
            0x01 => "Recording Started",
            0x02 => "Recording Stopped",
            0x03 => "Recording Snapshot",
            0x04 => "Motion Detection Started",
            0x05 => "Motion Detection Stopped",
            0x06 => "Vehicle Access Granted",
            0x07 => "Vehicle Access Denied",
            0x08 => "Terminal Recording Snapshot",

            0x11 => "Access Granted (Card+Facility)",
            0x12 => "Access Granted (Card)",
            0x13 => "Access Granted (PIN)",
            0x19 => "Access Recorded",

            0x21 => "Access Denied (Card+Facility)",
            0x22 => "Access Denied (Card)",
            0x23 => "Access Denied (PIN)",

            0x31 => "Output Open",
            0x32 => "Output Closed",
            0x33 => "Input Open",
            0x34 => "Input Close",
            0x35 => "Siren Started",
            0x36 => "Siren Stopped",
            0x37 => "Input Armed",
            0x38 => "Input Disarmed",
            0x3A => "Reader Mode Changed",
            0x3C => "Reader Bell",
            0x3D => "Door Interlock",

            0x43 => "Reader Mode Changed (Timezone)",
            0x44 => "Reader Mode Returned Default (Timezone)",
            0x47 => "Timed Antipassback Forgive (Timezone)",
            0x48 => "Door Antipassback Forgive (Timezone)",
            0x49 => "Global Antipassback Forgive (Timezone)",

            0x51 => "Output Unlocked (PC Manual)",
            0x52 => "Output Relocked (PC Manual)",
            0x53 => "Reader Mode Changed (PC Manual)",
            0x54 => "Reader Mode Returned Default (PC Manual)",
            0x55 => "Input Disarmed (PC Manual)",
            0x56 => "Input Returned Default (PC Manual)",

            0x61 => "Warning Condition",
            0x63 => "Alarm Started",
            0x65 => "Recording Camera A",
            0x66 => "Recording Camera B",

            0x71 => "Intermediate ID (Card+Facility)",
            0x72 => "Intermediate ID (Card)",
            0x73 => "Intermediate ID (PIN)",

            0x81 => "System Reset",
            0x82 => "Factory Settings Loaded",
            0x83 => "Firmware Download Succeeded",
            0x84 => "Manual Reset / Factory Settings",
            0x85 => "Modem Answered",
            0x86 => "Modem Disconnected",
            0x87 => "Communication Failed",
            0x88 => "Communication Restored",
            0x89 => "Event Memory Full",
            0x8A => "Event Memory Low",
            0x8C => "AC Power Fail",
            0x8D => "AC Power Restored",
            0x8E => "Low Battery",
            0x8F => "Battery OK",

            0x91 => "Code Accepted (Card+Facility)",
            0x92 => "Code Accepted (Card)",
            0x93 => "Code Accepted (PIN)",

            0xA0 => "SD Card Alert",
            0xA1 => "USB Disk Disconnected",
            0xA2 => "USB Disk Connected",
            0xA3 => "USB Disk Low Level",
            0xA4 => "USB Disk Normal Level",
            0xA5 => "USB Disk Error",

            0xB0 => "Area Is Full",
            0xB1 => "Area Is Not Full",

            0xC1 => "Config Mode (Card+Facility)",
            0xC2 => "Config Mode (Card)",
            0xC3 => "Config Mode (PIN)",
            0xC5 => "Case Tamper",
            0xC6 => "Panel DB Empty",
            0xCC => "Battery Cannot Charge",
            0xCD => "Battery Charging",
            0xCE => "Low Voltage",
            0xCF => "Voltage OK",

            _ => "Unknown",
        }
    }

    /// Returns a human-readable name for the subtype, if one is defined
    /// for this event type.
    pub fn subtype_name(&self) -> Option<&'static str> {
        match self.category() {
            EventCategory::AccessGranted => access_granted::name(self.subtype_id),
            EventCategory::AccessDenied => access_denied::name(self.subtype_id),
            EventCategory::SdCard if self.type_id == 0xA0 => match self.subtype_id {
                0x01 => Some("SD Card Removed"),
                0x02 => Some("SD Card Failed"),
                _ => None,
            },
            EventCategory::Alarm if self.type_id == 0x61 => match self.subtype_id {
                0x02 => Some("Door Forced Open"),
                0x04 => Some("Door Held Open"),
                _ => None,
            },
            _ if self.subtype_id == 0 => None,
            _ => None,
        }
    }

    /// Returns a full human-readable description combining type and subtype.
    pub fn description(&self) -> String {
        match self.subtype_name() {
            Some(sub) => format!("{}: {}", self.type_name(), sub),
            None if self.subtype_id != 0 => {
                format!("{} (sub=0x{:02X})", self.type_name(), self.subtype_id)
            }
            None => self.type_name().to_string(),
        }
    }

    /// Whether this is an access event that carries card credential data
    /// in the event_data bytes.
    pub fn has_card_data(&self) -> bool {
        matches!(
            self.type_id,
            0x11..=0x13 | 0x21..=0x23 | 0x71..=0x73 | 0x91..=0x93 | 0xC1..=0xC3
        )
    }

    /// Whether the event payload contains an `AccessEventData`
    /// (facility + card code layout). True for all 0x_1 credential events:
    /// AccessGranted, AccessDenied, Intermediate, CodeAccepted, ConfigMode.
    pub fn has_access_event_data(&self) -> bool {
        matches!(self.type_id, 0x11 | 0x21 | 0x71 | 0x91 | 0xC1)
    }

    pub fn access_outcome(&self) -> Option<AccessOutcome> {
        match self.type_id {
            0x11..=0x13 | 0x19 => Some(AccessOutcome::Granted),
            0x21..=0x23 => Some(AccessOutcome::Denied),
            0x71..=0x73 => Some(AccessOutcome::Intermediate),
            0x91..=0x93 => Some(AccessOutcome::CodeRecorded),
            0xC1..=0xC3 => Some(AccessOutcome::ConfigMode),
            _ => None,
        }
    }

    /// Whether this is a PIN event that carries keypad digits in the
    /// event_data bytes.
    pub fn has_pin_data(&self) -> bool {
        matches!(self.type_id, 0x13 | 0x23 | 0x73 | 0x93 | 0xC3)
    }

    /// Whether this is an access granted event (any card format).
    pub fn is_access_granted(&self) -> bool {
        self.category() == EventCategory::AccessGranted
    }

    /// Whether this is an access denied event (any card format).
    pub fn is_access_denied(&self) -> bool {
        self.category() == EventCategory::AccessDenied
    }

    /// Whether this is an access event (granted or denied).
    pub fn is_access_event(&self) -> bool {
        self.is_access_granted() || self.is_access_denied()
    }
}

impl fmt::Debug for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.type_name();
        if name == "Unknown" {
            write!(
                f,
                "EventType(0x{:02X}:0x{:02X})",
                self.type_id, self.subtype_id
            )
        } else {
            match self.subtype_name() {
                Some(sub) => write!(f, "EventType({name}: {sub})"),
                None if self.subtype_id != 0 => {
                    write!(f, "EventType({name}, sub=0x{:02X})", self.subtype_id)
                }
                None => write!(f, "EventType({name})"),
            }
        }
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.description())
    }
}
