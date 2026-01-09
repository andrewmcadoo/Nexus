use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent role enumeration (shared across schemas)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Router,
    Researcher,
    Planner,
    Executor,
    Reviewer,
    Tool,
}

/// Creator information for actions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreatedBy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Approval group for batch operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalGroup {
    pub id: String,
    pub label: String,
    pub size: u32,
    pub index: u32,
}

/// The main ProposedAction type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAction {
    pub id: String,
    pub summary: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,

    #[serde(default = "default_risk")]
    pub risk: u8,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_tags: Vec<String>,

    #[serde(default = "default_true")]
    pub requires_approval: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<CreatedBy>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_group: Option<ApprovalGroup>,

    pub kind: ActionKindTag,

    pub details: ActionDetails,
}

/// Provides the default risk level for actions (1).
fn default_risk() -> u8 {
    1
}

/// Provides the boolean value `true` for use as a default.
fn default_true() -> bool {
    true
}

/// Action kind discriminator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKindTag {
    Handoff,
    Patch,
    Command,
    PlanPatch,
    AgendaPatch,
    FileCreate,
    FileRename,
    FileDelete,
}

/// Action details (variant-specific data)
/// Note: Order matters for untagged deserialization - put most specific variants first,
/// and PatchDetails last since it has mostly optional fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActionDetails {
    Handoff(HandoffDetails),
    Command(CommandDetails),
    PlanPatch(PlanPatchDetails),
    AgendaPatch(AgendaPatchDetails),
    FileCreate(FileCreateDetails),
    FileRename(FileRenameDetails),
    FileDelete(FileDeleteDetails),
    Patch(PatchDetails),
}

// --- Patch Format Types ---

/// Patch format discriminator
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatchFormat {
    #[default]
    Unified,
    SearchReplace,
    WholeFile,
}

/// Conflict resolution strategy
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnConflict {
    #[default]
    Fail,
    Ours,
    Theirs,
    Marker,
}

/// Fallback matching strategy
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FallbackStrategy {
    #[default]
    None,
    Fuzzy,
    LineAnchor,
}

/// Match mode for search/replace
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    #[default]
    Exact,
    WhitespaceInsensitive,
}

/// Search/replace block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchReplaceBlock {
    pub file: String,
    pub search: String,
    pub replace: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub match_mode: MatchMode,
}

/// Patch action details
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatchDetails {
    #[serde(default)]
    pub format: PatchFormat,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_replace_blocks: Option<Vec<SearchReplaceBlock>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub whole_file_content: Option<HashMap<String, String>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_file_sha256: Option<HashMap<String, String>>,

    #[serde(default, skip_serializing_if = "is_default")]
    pub on_conflict: OnConflict,

    #[serde(default, skip_serializing_if = "is_default")]
    pub fallback_strategy: FallbackStrategy,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuzzy_threshold: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_confidence: Option<f64>,
}

// --- Other Action Details ---

/// Handoff action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffDetails {
    pub from: AgentRole,
    pub to: AgentRole,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_patch_ref: Option<String>,
}

/// Command action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDetails {
    pub argv: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_s: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_allow: Vec<String>,
    #[serde(default)]
    pub requires_network: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
}

/// Default command timeout in seconds (1200).
fn default_timeout() -> u32 {
    1200
}

/// Plan patch action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanPatchDetails {
    pub plan_id: String,
    pub patch_ref: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub patch_mode: PatchMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatchMode {
    #[default]
    Replace,
    JsonPatch,
}

/// Agenda patch action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgendaPatchDetails {
    pub target_path: String,
    pub diff: String,
}

/// File create action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCreateDetails {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub ignore_if_exists: bool,
}

/// File rename action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRenameDetails {
    pub old_path: String,
    pub new_path: String,
    #[serde(default)]
    pub overwrite: bool,
}

/// File delete action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDeleteDetails {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub ignore_if_missing: bool,
}

/// Checks whether the given value is equal to its type's `Default` value.
/// Used with `serde`'s `skip_serializing_if` to omit default fields.
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_details_defaults() {
        let details = PatchDetails::default();
        assert_eq!(details.format, PatchFormat::Unified);
        assert_eq!(details.on_conflict, OnConflict::Fail);
        assert_eq!(details.fallback_strategy, FallbackStrategy::None);
    }

    #[test]
    fn test_deserialize_patch_action() {
        let json = r#"{
            "id": "action-1",
            "kind": "patch",
            "summary": "Update function name",
            "details": {
                "format": "unified",
                "diff": "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,1 +1,1 @@\n-old\n+new"
            }
        }"#;

        let action: ProposedAction = serde_json::from_str(json).unwrap();
        assert_eq!(action.id, "action-1");
        assert_eq!(action.kind, ActionKindTag::Patch);
        assert_eq!(action.summary, "Update function name");
        assert_eq!(action.risk, 1);
        assert!(action.requires_approval);
        match action.details {
            ActionDetails::Patch(details) => {
                assert_eq!(details.format, PatchFormat::Unified);
                assert_eq!(
                    details.diff.as_deref(),
                    Some("--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,1 +1,1 @@\n-old\n+new")
                );
            }
            _ => panic!("expected patch details"),
        }
    }

    #[test]
    fn test_serialize_roundtrip() {
        let action = ProposedAction {
            id: "action-2".to_string(),
            summary: "Create config".to_string(),
            why: Some("Bootstrap repo".to_string()),
            risk: 2,
            policy_tags: vec!["file_ops".to_string()],
            requires_approval: true,
            created_by: Some(CreatedBy {
                agent: Some(AgentRole::Planner),
                provider: Some("test-provider".to_string()),
                model: Some("test-model".to_string()),
            }),
            approval_group: Some(ApprovalGroup {
                id: "group-1".to_string(),
                label: "Setup".to_string(),
                size: 2,
                index: 0,
            }),
            kind: ActionKindTag::FileCreate,
            details: ActionDetails::FileCreate(FileCreateDetails {
                path: "config.toml".to_string(),
                content: "name = \"nexus\"".to_string(),
                overwrite: false,
                ignore_if_exists: true,
            }),
        };

        let json = serde_json::to_string(&action).unwrap();
        let roundtrip: ProposedAction = serde_json::from_str(&json).unwrap();

        assert_eq!(roundtrip.id, "action-2");
        assert_eq!(roundtrip.kind, ActionKindTag::FileCreate);
        assert_eq!(roundtrip.summary, "Create config");
        assert_eq!(roundtrip.risk, 2);
        assert_eq!(roundtrip.policy_tags, vec!["file_ops".to_string()]);
        assert!(roundtrip.requires_approval);
        assert!(roundtrip.created_by.is_some());
        assert!(roundtrip.approval_group.is_some());
        match roundtrip.details {
            ActionDetails::FileCreate(details) => {
                assert_eq!(details.path, "config.toml");
                assert_eq!(details.content, "name = \"nexus\"");
                assert!(!details.overwrite);
                assert!(details.ignore_if_exists);
            }
            _ => panic!("expected file_create details"),
        }
    }
}
