use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::AgentRole;

/// Trace information for correlation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraceInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
}

/// Actor information (who caused the event)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Actor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Payload reference for large/external data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadRef {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Run event (append-only log entry)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    /// Schema version (e.g., "nexus/1")
    pub v: String,

    pub run_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,

    /// Event type (e.g., "action.proposed", "permission.granted")
    #[serde(rename = "type")]
    pub event_type: String,

    pub time: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<TraceInfo>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<Actor>,

    /// Dynamic payload (additionalProperties: true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_ref: Option<PayloadRef>,
}

impl RunEvent {
    /// Create a new event with required fields
    pub fn new(run_id: impl Into<String>, event_type: impl Into<String>) -> Self {
        Self {
            v: "nexus/1".to_string(),
            run_id: run_id.into(),
            workflow_id: None,
            node_id: None,
            event_type: event_type.into(),
            time: Utc::now(),
            trace: None,
            actor: None,
            payload: None,
            payload_ref: None,
        }
    }

    /// Add payload to event
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Add actor to event
    pub fn with_actor(mut self, actor: Actor) -> Self {
        self.actor = Some(actor);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_event() {
        let event = RunEvent::new("run-123", "action.proposed");
        assert_eq!(event.v, "nexus/1");
        assert_eq!(event.run_id, "run-123");
        assert_eq!(event.event_type, "action.proposed");
    }

    #[test]
    fn test_serialize_event() {
        let event = RunEvent::new("run-123", "action.proposed");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"v\":\"nexus/1\""));
        assert!(json.contains("\"type\":\"action.proposed\""));
    }
}
