use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusEntry {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

/// Thread-safe, clonable status tracker.
///
/// Components call `set` / `set_detail` to update their state. The API
/// endpoint calls `snapshot` to read the whole map at once.
#[derive(Clone)]
pub struct StatusTracker {
    inner: Arc<RwLock<HashMap<String, StatusEntry>>>,
}

impl StatusTracker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Update a component's state (no detail).
    pub fn set(&self, component: &str, state: &str) {
        let entry = StatusEntry {
            state: state.to_string(),
            since: Some(now_ms()),
            detail: None,
        };
        self.inner
            .write()
            .unwrap()
            .insert(component.to_string(), entry);
    }

    /// Update a component's state with a detail payload.
    pub fn set_detail(&self, component: &str, state: &str, detail: serde_json::Value) {
        let entry = StatusEntry {
            state: state.to_string(),
            since: Some(now_ms()),
            detail: Some(detail),
        };
        self.inner
            .write()
            .unwrap()
            .insert(component.to_string(), entry);
    }

    /// Read the current state of all components.
    pub fn snapshot(&self) -> HashMap<String, StatusEntry> {
        self.inner.read().unwrap().clone()
    }
}
