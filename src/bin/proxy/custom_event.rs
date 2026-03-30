use ac215::event::event_type::EventType;
use ac215::event::location::EventLocation;
use ac215::event::timestamp::PackedTimestamp;

/// A custom event overlay — no event_number (assigned by the panel).
///
/// Stored in the local DB keyed by the base event ID that the panel assigns.
/// When the event flows back through the proxy, the rewriter replaces the
/// panel's event data with these fields. If `timestamp` is `None`, the
/// original panel timestamp is preserved.
#[derive(Debug, Clone)]
pub struct CustomEvent {
    pub event_type: EventType,
    pub location: EventLocation,
    pub timestamp: Option<PackedTimestamp>,
    pub event_data: [u8; 25],
}

impl CustomEvent {
    /// Create a custom event with zeroed event_data and no timestamp override.
    pub fn new(event_type: EventType, location: EventLocation) -> Self {
        Self {
            event_type,
            location,
            timestamp: None,
            event_data: [0u8; 25],
        }
    }

    /// Set a custom timestamp (overrides the panel's timestamp).
    pub fn with_timestamp(mut self, timestamp: PackedTimestamp) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    /// Set the raw event_data payload.
    pub fn with_data(mut self, data: [u8; 25]) -> Self {
        self.event_data = data;
        self
    }

    /// Set event_data from a slice (pads/truncates to 25 bytes).
    pub fn with_data_slice(mut self, data: &[u8]) -> Self {
        let len = data.len().min(25);
        self.event_data[..len].copy_from_slice(&data[..len]);
        self
    }

}
