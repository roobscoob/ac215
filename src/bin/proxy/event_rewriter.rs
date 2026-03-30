use std::sync::Arc;

use ac215::proxy::handlers::events::MutatorHandle;
use ac215::proxy::handlers::EventsHandler;

use crate::local_db::LocalDb;

/// Registers a mutator on the EventsHandler that rewrites events whose
/// `event_number` has a matching custom event in the local database.
///
/// Returns the `MutatorHandle` — drop it to deregister the rewriter.
pub fn register(
    events_handler: &mut EventsHandler,
    local_db: Arc<LocalDb>,
) -> MutatorHandle {
    events_handler.add_mutator(move |event, _handle| {
        let Ok(Some(custom)) = local_db.get_custom_event(event.event_number) else {
            return;
        };

        event.event_type = custom.event_type;
        event.location = custom.location;
        event.event_data = custom.event_data;
        if let Some(ts) = custom.timestamp {
            event.timestamp = ts;
        }
    })
}
