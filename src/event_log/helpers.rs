//! Convenience factory helpers for building common run events.
//!
//! These helpers centralize event type strings and payload shapes so
//! callers can emit consistent, schema-aligned `RunEvent` records.

use serde_json::json;

use crate::types::{Actor, AgentRole, RunEvent};

fn tool_actor() -> Actor {
    Actor {
        agent: Some(AgentRole::Tool),
        provider: None,
        model: None,
    }
}

fn default_executor_actor() -> Actor {
    Actor {
        agent: Some(AgentRole::Executor),
        provider: Some("openai".to_string()),
        model: Some("codex".to_string()),
    }
}

/// Creates run.started event.
pub fn run_started(run_id: &str, task: &str) -> RunEvent {
    RunEvent::new(run_id, "run.started")
        .with_actor(tool_actor())
        .with_payload(json!({"task": task}))
}

/// Creates run.completed event.
pub fn run_completed(run_id: &str, status: &str, actions_applied: u32) -> RunEvent {
    RunEvent::new(run_id, "run.completed")
        .with_actor(tool_actor())
        .with_payload(json!({"status": status, "actions_applied": actions_applied}))
}

/// Creates action.proposed event.
pub fn action_proposed(
    run_id: &str,
    action_id: &str,
    kind: &str,
    summary: &str,
    actor: Option<Actor>,
) -> RunEvent {
    let actor = actor.unwrap_or_else(default_executor_actor);
    RunEvent::new(run_id, "action.proposed")
        .with_actor(actor)
        .with_payload(json!({
            "action_id": action_id,
            "kind": kind,
            "summary": summary
        }))
}

/// Creates permission.granted event.
pub fn permission_granted(run_id: &str, action_id: &str, scope: &str) -> RunEvent {
    RunEvent::new(run_id, "permission.granted")
        .with_actor(tool_actor())
        .with_payload(json!({"action_id": action_id, "scope": scope}))
}

/// Creates permission.denied event.
pub fn permission_denied(run_id: &str, action_id: &str, reason: &str) -> RunEvent {
    RunEvent::new(run_id, "permission.denied")
        .with_actor(tool_actor())
        .with_payload(json!({"action_id": action_id, "reason": reason}))
}

/// Creates tool.executed event (success).
pub fn tool_executed(run_id: &str, action_id: &str, files_modified: Vec<String>) -> RunEvent {
    RunEvent::new(run_id, "tool.executed")
        .with_actor(tool_actor())
        .with_payload(json!({
            "action_id": action_id,
            "success": true,
            "files_modified": files_modified
        }))
}

/// Creates tool.failed event.
pub fn tool_failed(run_id: &str, action_id: &str, error: &str) -> RunEvent {
    RunEvent::new(run_id, "tool.failed")
        .with_actor(tool_actor())
        .with_payload(json!({"action_id": action_id, "success": false, "error": error}))
}

/// Creates executor.started event.
pub fn executor_started(run_id: &str, task: &str, file_count: usize, model: &str) -> RunEvent {
    let actor = Actor {
        agent: Some(AgentRole::Executor),
        provider: Some("openai".to_string()),
        model: Some(model.to_string()),
    };

    RunEvent::new(run_id, "executor.started")
        .with_actor(actor)
        .with_payload(json!({
            "task": task,
            "file_count": file_count,
            "model": model
        }))
}

/// Creates executor.streaming event.
pub fn executor_streaming(run_id: &str, chunk_size: usize, total_chars: usize) -> RunEvent {
    RunEvent::new(run_id, "executor.streaming")
        .with_actor(default_executor_actor())
        .with_payload(json!({
            "chunk_size": chunk_size,
            "total_chars": total_chars
        }))
}

/// Creates executor.completed event.
pub fn executor_completed(run_id: &str, action_count: usize, duration_ms: u128) -> RunEvent {
    RunEvent::new(run_id, "executor.completed")
        .with_actor(default_executor_actor())
        .with_payload(json!({
            "action_count": action_count,
            "duration_ms": duration_ms,
            "success": true
        }))
}

/// Creates executor.failed event.
pub fn executor_failed(run_id: &str, error: &str, status_code: Option<u16>) -> RunEvent {
    let mut payload = json!({
        "error": error,
        "success": false
    });

    if let Some(status_code) = status_code {
        if let Some(payload) = payload.as_object_mut() {
            payload.insert("status_code".to_string(), json!(status_code));
        }
    }

    RunEvent::new(run_id, "executor.failed")
        .with_actor(default_executor_actor())
        .with_payload(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_tool_actor(actor: &Actor) {
        assert_eq!(actor.agent, Some(AgentRole::Tool));
        assert!(actor.provider.is_none());
        assert!(actor.model.is_none());
    }

    #[test]
    fn test_helper_run_started() {
        let event = run_started("run_001", "rename function");
        assert_eq!(event.v, "nexus/1");
        assert_eq!(event.run_id, "run_001");
        assert_eq!(event.event_type, "run.started");

        let actor = event.actor.as_ref().expect("actor should be set");
        assert_tool_actor(actor);

        assert_eq!(event.payload, Some(json!({"task": "rename function"})));
    }

    #[test]
    fn test_helper_run_completed() {
        let event = run_completed("run_001", "success", 2);
        assert_eq!(event.event_type, "run.completed");

        let actor = event.actor.as_ref().expect("actor should be set");
        assert_tool_actor(actor);

        assert_eq!(
            event.payload,
            Some(json!({"status": "success", "actions_applied": 2}))
        );
    }

    #[test]
    fn test_helper_action_proposed_default_actor() {
        let event = action_proposed("run_001", "act_001", "patch", "Rename function", None);
        assert_eq!(event.event_type, "action.proposed");

        let actor = event.actor.as_ref().expect("actor should be set");
        assert_eq!(actor.agent, Some(AgentRole::Executor));
        assert_eq!(actor.provider.as_deref(), Some("openai"));
        assert_eq!(actor.model.as_deref(), Some("codex"));

        assert_eq!(
            event.payload,
            Some(json!({
                "action_id": "act_001",
                "kind": "patch",
                "summary": "Rename function"
            }))
        );
    }

    #[test]
    fn test_helper_action_proposed_custom_actor() {
        let custom = Actor {
            agent: Some(AgentRole::Reviewer),
            provider: Some("acme".to_string()),
            model: None,
        };

        let event = action_proposed(
            "run_002",
            "act_777",
            "handoff",
            "Request review",
            Some(custom),
        );

        let actor = event.actor.as_ref().expect("actor should be set");
        assert_eq!(actor.agent, Some(AgentRole::Reviewer));
        assert_eq!(actor.provider.as_deref(), Some("acme"));
        assert!(actor.model.is_none());
    }

    #[test]
    fn test_helper_permission_granted() {
        let event = permission_granted("run_001", "act_001", "once");
        assert_eq!(event.event_type, "permission.granted");

        let actor = event.actor.as_ref().expect("actor should be set");
        assert_tool_actor(actor);

        assert_eq!(
            event.payload,
            Some(json!({"action_id": "act_001", "scope": "once"}))
        );
    }

    #[test]
    fn test_helper_permission_denied() {
        let event = permission_denied("run_001", "act_001", "policy");
        assert_eq!(event.event_type, "permission.denied");

        let actor = event.actor.as_ref().expect("actor should be set");
        assert_tool_actor(actor);

        assert_eq!(
            event.payload,
            Some(json!({"action_id": "act_001", "reason": "policy"}))
        );
    }

    #[test]
    fn test_helper_tool_executed() {
        let event = tool_executed(
            "run_001",
            "act_001",
            vec!["src/api.ts".to_string(), "src/lib.rs".to_string()],
        );
        assert_eq!(event.event_type, "tool.executed");

        let actor = event.actor.as_ref().expect("actor should be set");
        assert_tool_actor(actor);

        assert_eq!(
            event.payload,
            Some(json!({
                "action_id": "act_001",
                "success": true,
                "files_modified": ["src/api.ts", "src/lib.rs"]
            }))
        );
    }

    #[test]
    fn test_helper_tool_executed_empty_files() {
        let event = tool_executed("run_001", "act_002", Vec::new());
        assert_eq!(event.event_type, "tool.executed");
        assert_eq!(
            event.payload,
            Some(json!({"action_id": "act_002", "success": true, "files_modified": []}))
        );
    }

    #[test]
    fn test_helper_tool_failed() {
        let event = tool_failed("run_001", "act_001", "boom");
        assert_eq!(event.event_type, "tool.failed");

        let actor = event.actor.as_ref().expect("actor should be set");
        assert_tool_actor(actor);

        assert_eq!(
            event.payload,
            Some(json!({"action_id": "act_001", "success": false, "error": "boom"}))
        );
    }

    #[test]
    fn test_helper_round_trip_serialization() {
        let event = action_proposed("run_003", "act_003", "patch", "Round trip", None);
        let json = serde_json::to_string(&event).expect("serialize event");
        let parsed: RunEvent = serde_json::from_str(&json).expect("deserialize event");

        assert_eq!(parsed.v, event.v);
        assert_eq!(parsed.run_id, event.run_id);
        assert_eq!(parsed.event_type, event.event_type);
        assert_eq!(parsed.time, event.time);
        assert_eq!(parsed.payload, event.payload);

        let parsed_actor = parsed.actor.as_ref().expect("actor should be set");
        let original_actor = event.actor.as_ref().expect("actor should be set");
        assert_eq!(parsed_actor.agent, original_actor.agent);
        assert_eq!(parsed_actor.provider, original_actor.provider);
        assert_eq!(parsed_actor.model, original_actor.model);
    }
}
