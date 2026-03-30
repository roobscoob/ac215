use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ac215::event::location::EventLocation;
use ac215::packet::packets::answer_events_825::AnswerEvents825;
use ac215::proxy::handlers::events::AccessEvent;
use log::warn;
use tokio::sync::Mutex as AsyncMutex;

use super::delivery;
use crate::db::DbClient;
use crate::local_db::LocalDb;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Emit an `access.granted`, `access.denied`, etc. webhook event.
/// Enriches the payload with card and (optionally) employee data from the DB.
pub fn on_access(store: &Arc<LocalDb>, db: &Arc<AsyncMutex<DbClient>>, event: &AccessEvent) {
    let time = event
        .timestamp
        .to_chrono()
        .map(|dt| dt.timestamp_millis() as u64)
        .unwrap();

    let store = store.clone();
    let db = db.clone();
    let site_code = event.site_code;
    let card_code = event.card_code;
    let outcome = event.outcome.as_str().to_string();
    let location = location_json(event.location);
    let format_index = event.format_index;

    tokio::spawn(async move {
        let card = lookup_card_with_employee(&db, site_code as i32, &card_code.to_string()).await;

        let mut data = serde_json::json!({
            "outcome": outcome,
            "location": location,
            "site_code": site_code,
            "card_code": card_code,
            "format_index": format_index,
        });

        if let Some(card) = card {
            data["card"] = card;
        }

        delivery::deliver_all(
            &store,
            serde_json::json!({
                "time": time,
                "type": "access",
                "data": data,
            }),
        );
    });
}

/// Look up a card (with optional employee) by site_code + card_code from SQL Server.
async fn lookup_card_with_employee(
    db: &Arc<AsyncMutex<DbClient>>,
    site_code: i32,
    card_code: &str,
) -> Option<serde_json::Value> {
    let mut db = db.lock().await;
    let row = match db
        .query(
            "SELECT c.IdCardNum, c.iSiteCode, c.iCardCode, c.eCardStatus, c.IdEmpNum,
                    e.tFirstName AS emp_first, e.tLastName AS emp_last,
                    e.IdDepartment AS emp_dept, e.IdAccessGroup AS emp_ag,
                    e.bMasterUser AS emp_master, e.tNotes AS emp_notes
             FROM tblCard c
             LEFT JOIN tblEmployees e ON c.IdEmpNum = e.iEmployeeNum
             WHERE c.iSiteCode = @P1 AND c.iCardCode = @P2",
            &[&site_code, &card_code],
        )
        .await
    {
        Ok(stream) => match stream.into_row().await {
            Ok(row) => row,
            Err(e) => {
                warn!("webhook card lookup failed: {e}");
                return None;
            }
        },
        Err(e) => {
            warn!("webhook card lookup failed: {e}");
            return None;
        }
    };

    let row = row?;

    let emp_id: i32 = row.get("IdEmpNum").unwrap_or(0);
    let employee = if emp_id != 0 {
        Some(serde_json::json!({
            "id": emp_id,
            "first_name": row.get::<&str, _>("emp_first").unwrap_or(""),
            "last_name": row.get::<&str, _>("emp_last").unwrap_or(""),
            "department": row.get::<i32, _>("emp_dept").unwrap_or(0),
            "access_group": row.get::<i32, _>("emp_ag").unwrap_or(0),
            "master_user": row.get::<bool, _>("emp_master").unwrap_or(false),
            "notes": row.get::<&str, _>("emp_notes").unwrap_or(""),
        }))
    } else {
        None
    };

    let status = match row.get::<u8, _>("eCardStatus").unwrap_or(0) {
        1 => "active",
        _ => "unassigned",
    };

    let mut card = serde_json::json!({
        "id": row.get::<i32, _>("IdCardNum").unwrap_or(0),
        "site_code": row.get::<i32, _>("iSiteCode").unwrap_or(0),
        "card_code": row.get::<&str, _>("iCardCode").unwrap_or(""),
        "status": status,
        "employee_id": emp_id,
    });

    if let Some(employee) = employee {
        card["employee"] = employee;
    }

    Some(card)
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
