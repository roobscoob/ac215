use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ac215::event::location::EventLocation;
use ac215::packet::packets::answer_events_825::AnswerEvents825;
use ac215::proxy::handlers::events::AccessEvent;

use super::delivery;
use crate::local_db::LocalDb;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Emit an `access.granted`, `access.denied`, etc. webhook event.
pub fn on_access(store: &Arc<LocalDb>, event: &AccessEvent) {
    let time = event
        .timestamp
        .to_chrono()
        .map(|dt| dt.timestamp_millis() as u64)
        .unwrap();

    delivery::deliver_all(
        store,
        serde_json::json!({
            "time": time,
            "type": "access",
            "data": {
                "outcome": event.outcome.as_str(),
                "location": location_json(event.location),
                "site_code": event.site_code,
                "card_code": event.card_code,
                "format_index": event.format_index,
            }
        }),
    );
}

fn location_json(loc: EventLocation) -> serde_json::Value {
    match loc {
        EventLocation::Panel => serde_json::json!({ "type": "panel" }),
        EventLocation::Door(n) => serde_json::json!({ "type": "door", "index": n }),
        EventLocation::Reader(n) => serde_json::json!({ "type": "reader", "index": n }),
        EventLocation::Voltage(v) => {
            serde_json::json!({ "type": "voltage", "source": format!("{v:?}") })
        }
        EventLocation::Input(n) => serde_json::json!({ "type": "input", "index": n }),
        EventLocation::Output(n) => serde_json::json!({ "type": "output", "index": n }),
        EventLocation::Unknown(b) => serde_json::json!({ "type": "unknown", "raw": b }),
    }
}

/// Diff output states between previous and current status, emitting
/// `output.enabled` / `output.disabled` webhook events for each change.
pub fn diff_outputs(store: &Arc<LocalDb>, prev: Option<&AnswerEvents825>, curr: &AnswerEvents825) {
    let time = now_ms();

    for i in 0..32u8 {
        let was_active = prev.is_some_and(|p| p.ac825_status.output_active(i));
        let is_active = curr.ac825_status.output_active(i);

        if was_active != is_active {
            let event_type = if is_active {
                "output.enabled"
            } else {
                "output.disabled"
            };

            delivery::deliver_all(
                store,
                serde_json::json!({
                    "time": time,
                    "type": event_type,
                    "data": {
                        "output_id": i,
                        "is_overridden": curr.ac825_status.output_is_manual(i),
                    }
                }),
            );
        }
    }
}
