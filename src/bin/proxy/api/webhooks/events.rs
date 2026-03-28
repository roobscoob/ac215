use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ac215::packet::packets::answer_events_825::AnswerEvents825;

use super::delivery;
use crate::local_db::LocalDb;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
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
