use crate::packet::packets::answer_events_825::AnswerEvents825;
use crate::packet::packets::answer_firm_ver::AnswerFirmVer825Packet;
use crate::server::Frame;

use super::super::message::Side;
use super::super::pipeline::{Disposition, FrameHandler, FrameHandling, HandlerContext};
use super::super::status::StatusTracker;

/// Pipeline handler that watches for firmware version and status packets
/// from the panel and writes a `panel` entry to the status tracker.
///
/// State graph: disconnected → waiting_fw | waiting_evt → connected
pub struct PanelHealthHandler {
    status: StatusTracker,
    firmware: Option<String>,
    bootloader: Option<String>,
    power_flags: Option<Vec<&'static str>>,
}

impl PanelHealthHandler {
    pub fn new(status: StatusTracker) -> Self {
        status.set("panel", "disconnected");
        Self {
            status,
            firmware: None,
            bootloader: None,
            power_flags: None,
        }
    }

    fn publish(&self) {
        let has_fw = self.firmware.is_some();
        let has_evt = self.power_flags.is_some();

        let (state, mut detail) = match (has_fw, has_evt) {
            (true, true) => ("connected", serde_json::json!({
                "firmware": self.firmware,
                "bootloader": self.bootloader,
                "power_flags": self.power_flags,
            })),
            (true, false) => ("waiting_evt", serde_json::json!({
                "firmware": self.firmware,
                "bootloader": self.bootloader,
            })),
            (false, true) => ("waiting_fw", serde_json::json!({
                "power_flags": self.power_flags,
            })),
            (false, false) => {
                self.status.set("panel", "disconnected");
                return;
            }
        };

        self.status.set_detail("panel", state, detail);
    }
}

fn power_flag_names(flags: crate::packet::packets::answer_events_825::PowerFlags) -> Vec<&'static str> {
    use crate::packet::packets::answer_events_825::PowerFlags;
    let mut names = Vec::new();
    if flags.contains(PowerFlags::AUX_POWER) { names.push("aux_power"); }
    if flags.contains(PowerFlags::VIN_STATUS) { names.push("vin_status"); }
    if flags.contains(PowerFlags::VEXP_STATUS) { names.push("vexp_status"); }
    if flags.contains(PowerFlags::READERS_VOLTAGE) { names.push("readers_voltage"); }
    if flags.contains(PowerFlags::SRC_OF_SYSTEM) { names.push("src_of_system"); }
    if flags.contains(PowerFlags::AUX_POWER_BOARD) { names.push("aux_power_board"); }
    if flags.contains(PowerFlags::BATTERY_OK) { names.push("battery_ok"); }
    if flags.contains(PowerFlags::CASE_TAMPER) { names.push("case_tamper"); }
    names
}

impl FrameHandler for PanelHealthHandler {
    fn reset(&mut self) {
        self.firmware = None;
        self.bootloader = None;
        self.power_flags = None;
        self.status.set("panel", "disconnected");
    }

    fn on_frame(
        &mut self,
        _ctx: &mut HandlerContext,
        _handling: FrameHandling,
        from: Side,
        frame: &mut Frame,
    ) -> Disposition {
        if from != Side::Panel {
            return Disposition::Forward;
        }

        let mut changed = false;

        // AnswerFirmVer825 (0xEB) — arrives once after connect.
        if let Some(Ok(pkt)) = frame.parse::<AnswerFirmVer825Packet>() {
            let fw = pkt.firmware.as_str().to_string();
            let bl = pkt.bootloader.as_str().to_string();
            self.firmware = Some(if fw.is_empty() { "(none)".to_string() } else { fw });
            self.bootloader = Some(if bl.is_empty() { "(none)".to_string() } else { bl });
            changed = true;
        }

        // AnswerEvents825 (0xC9) — arrives every poll cycle.
        if let Some(Ok(pkt)) = frame.parse::<AnswerEvents825>() {
            self.power_flags = Some(power_flag_names(pkt.ac825_status.power_flags));
            changed = true;
        }

        if changed {
            self.publish();
        }

        Disposition::Forward
    }
}
