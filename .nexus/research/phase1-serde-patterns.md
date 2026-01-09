# Phase 1: Serde JSON Schema to Rust Type Mapping

**Research Date:** 2026-01-08
**Confidence:** High (multiple authoritative sources consulted)
**Scope:** Mapping Nexus JSON schemas to idiomatic Rust types with serde

---

## 1. Summary of Findings

This research addresses five key questions for converting Nexus JSON schemas to Rust types:

| Question | Recommended Solution |
|----------|---------------------|
| `oneOf` with discriminator | Internally tagged enum: `#[serde(tag = "kind")]` |
| `kind`/`details` sibling pattern | **Not** adjacently tagged; use internally tagged + flattened struct |
| Optional fields | `#[serde(default, skip_serializing_if = "Option::is_none")]` |
| `additionalProperties` | `HashMap<String, T>` with `#[serde(flatten)]` |
| Validation approach | Two-phase: serde for structure + `validator` crate for business rules |

**Key Insight:** The Nexus `ProposedAction` schema uses a `kind` discriminator with a separate `details` object whose structure varies by kind. This is best modeled as an **internally tagged enum** where the `kind` field discriminates, and each variant contains a details struct.

---

## 2. Schema Analysis

### 2.1 ProposedAction Schema (`/Users/aj/Desktop/Projects/Nexus/.nexus/schemas/proposed_action.schema.json`)

**Structure:**
- Required fields: `id`, `kind`, `summary`, `details`
- `kind` is an enum discriminator with 8 variants
- `details` structure varies based on `kind` value
- Uses `oneOf` to specify variant-specific constraints
- Contains optional fields with defaults (`risk`, `requires_approval`)
- Uses `additionalProperties` for `base_file_sha256` and `whole_file_content`

**Discriminator Values:**
- `handoff`, `patch`, `command`, `plan_patch`, `agenda_patch`, `file_create`, `file_rename`, `file_delete`

### 2.2 Settings Schema (`/Users/aj/Desktop/Projects/Nexus/.nexus/schemas/settings.schema.json`)

**Structure:**
- Required fields: `permission_mode`, `schema_version`
- Uses `const` for `schema_version` (always `"1.0"`)
- Nested `autopilot` object with all optional fields
- Arrays of arrays for command patterns (`allow_commands`, etc.)

### 2.3 Event Schema (`/Users/aj/Desktop/Projects/Nexus/.nexus/schemas/event.schema.json`)

**Structure:**
- Required fields: `v`, `type`, `time`, `run_id`
- Nested optional objects: `trace`, `actor`, `payload`, `payload_ref`
- `payload` uses `additionalProperties: true` (fully dynamic)
- `actor.agent` uses same enum as ProposedAction

---

## 3. Serde Enum Representation Analysis

### 3.1 The Four Serde Enum Representations

Based on [Serde documentation](https://serde.rs/enum-representations.html):

| Representation | Attribute | JSON Example | Use Case |
|----------------|-----------|--------------|----------|
| **Externally Tagged** | (default) | `{"Variant": {...}}` | Simple cases, no-alloc |
| **Internally Tagged** | `#[serde(tag = "kind")]` | `{"kind": "Variant", ...}` | Java-style, flat objects |
| **Adjacently Tagged** | `#[serde(tag = "t", content = "c")]` | `{"t": "Variant", "c": {...}}` | Haskell-style, tuple variants |
| **Untagged** | `#[serde(untagged)]` | `{...}` (inferred) | Schema-less, polymorphic |

### 3.2 Why Internally Tagged for ProposedAction

The Nexus schema has `kind` as a top-level field alongside `details`. This is NOT adjacently tagged because:

1. **Adjacently tagged** would require: `{"kind": "patch", "details": {...}}` where `details` contains the ENTIRE variant payload
2. **Our schema** has: `{"kind": "patch", "id": "...", "summary": "...", "details": {...}}` where common fields are siblings

The correct approach: **Internally tagged enum for the action type, with shared fields factored into a common struct.**

---

## 4. Concrete Rust Implementations

### 4.1 ProposedAction with Internally Tagged Enum

```rust
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

/// Approval group for batch operations (ADR-012)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalGroup {
    pub id: String,
    pub label: String,
    pub size: u32,
    pub index: u32,
}

/// The main ProposedAction type
/// Uses internally tagged enum with flattened common fields
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

    /// The action kind and its specific details
    #[serde(flatten)]
    pub action: ActionKind,
}

fn default_risk() -> u8 { 1 }
fn default_true() -> bool { true }

/// Internally tagged enum for action kinds
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "details")]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Handoff(HandoffDetails),
    Patch(PatchDetails),
    Command(CommandDetails),
    PlanPatch(PlanPatchDetails),
    AgendaPatch(AgendaPatchDetails),
    FileCreate(FileCreateDetails),
    FileRename(FileRenameDetails),
    FileDelete(FileDeleteDetails),
}
```

**Note:** The `#[serde(tag = "kind", content = "details")]` is adjacently tagged at the enum level, but `#[serde(flatten)]` on the `action` field promotes those fields to the parent struct level.

### 4.2 PatchDetails with ADR-010 Format Discrimination

```rust
/// Patch format discriminator (ADR-010)
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

/// Search/replace block for search_replace format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchReplaceBlock {
    pub file: String,  // repo-relative path
    pub search: String,
    pub replace: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub match_mode: MatchMode,
}

/// Patch action details (supports unified, search_replace, whole_file formats)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchDetails {
    #[serde(default)]
    pub format: PatchFormat,

    // Unified format fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,

    // Search/replace format fields (ADR-010)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_replace_blocks: Option<Vec<SearchReplaceBlock>>,

    // Whole file format fields
    /// Map of file path -> complete new content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whole_file_content: Option<HashMap<String, String>>,

    // Common optional fields
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,

    /// Map of path -> expected sha256 hash (ADR-012 style additionalProperties)
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

/// Helper for skip_serializing_if on Default values
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}
```

### 4.3 Other Action Details

```rust
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

fn default_timeout() -> u32 { 1200 }

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
```

### 4.4 Settings Type

```rust
/// Permission mode enumeration
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    #[default]
    Default,
    AcceptEdits,
    Autopilot,
}

/// Autopilot configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutopilotConfig {
    #[serde(default = "default_max_batch_cu")]
    pub max_batch_cu: u32,
    #[serde(default = "default_max_batch_steps")]
    pub max_batch_steps: u32,
    #[serde(default)]
    pub auto_approve_patches: bool,
    #[serde(default)]
    pub auto_approve_tests: bool,
    #[serde(default)]
    pub auto_handoffs: bool,
}

fn default_max_batch_cu() -> u32 { 40 }
fn default_max_batch_steps() -> u32 { 8 }

/// Nexus settings (matches settings.schema.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusSettings {
    /// Always "1.0" for this schema version
    pub schema_version: String,

    #[serde(default)]
    pub permission_mode: PermissionMode,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny_paths: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_paths_write: Vec<String>,

    /// Commands as argv arrays
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_commands: Vec<Vec<String>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ask_commands: Vec<Vec<String>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny_commands: Vec<Vec<String>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autopilot: Option<AutopilotConfig>,
}
```

### 4.5 Event Type

```rust
use chrono::{DateTime, Utc};

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
```

---

## 5. Serde Attributes Reference

### 5.1 Container Attributes

| Attribute | Purpose | Example |
|-----------|---------|---------|
| `#[serde(tag = "field")]` | Internally tagged enum | `#[serde(tag = "kind")]` |
| `#[serde(tag = "t", content = "c")]` | Adjacently tagged enum | `#[serde(tag = "type", content = "data")]` |
| `#[serde(untagged)]` | Untagged enum (try each variant) | For polymorphic JSON |
| `#[serde(rename_all = "...")]` | Case conversion | `snake_case`, `camelCase` |
| `#[serde(deny_unknown_fields)]` | Strict parsing | For `additionalProperties: false` |

### 5.2 Field Attributes

| Attribute | Purpose | Example |
|-----------|---------|---------|
| `#[serde(default)]` | Use Default::default() if missing | Required for optional fields |
| `#[serde(skip_serializing_if = "...")]` | Omit field conditionally | `"Option::is_none"`, `"Vec::is_empty"` |
| `#[serde(flatten)]` | Inline nested struct/map fields | For additionalProperties capture |
| `#[serde(rename = "...")]` | Rename single field | `#[serde(rename = "type")]` for reserved words |
| `#[serde(with = "...")]` | Custom ser/de module | For complex transformations |

### 5.3 Helper Functions for `skip_serializing_if`

```rust
/// Skip serializing if Option is None
// Use: skip_serializing_if = "Option::is_none"

/// Skip serializing if Vec is empty
// Use: skip_serializing_if = "Vec::is_empty"

/// Skip serializing if value equals Default
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}
// Use: skip_serializing_if = "is_default"

/// Skip serializing if HashMap is empty
// Use: skip_serializing_if = "HashMap::is_empty"
```

---

## 6. Handling `additionalProperties`

### 6.1 Pattern: `additionalProperties: { "type": "T" }`

Use `HashMap<String, T>`:

```rust
/// base_file_sha256 in PatchDetails
#[serde(skip_serializing_if = "Option::is_none")]
pub base_file_sha256: Option<HashMap<String, String>>,

/// whole_file_content in PatchDetails
#[serde(skip_serializing_if = "Option::is_none")]
pub whole_file_content: Option<HashMap<String, String>>,
```

### 6.2 Pattern: Capturing Unknown Fields

Use `#[serde(flatten)]` with `HashMap<String, Value>`:

```rust
#[derive(Serialize, Deserialize)]
pub struct FlexibleObject {
    pub known_field: String,

    /// Captures any additional unknown properties
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
```

### 6.3 Pattern: `additionalProperties: true` (fully dynamic)

Use `serde_json::Value`:

```rust
/// Dynamic payload (like Event.payload)
#[serde(skip_serializing_if = "Option::is_none")]
pub payload: Option<serde_json::Value>,
```

---

## 7. Validation Strategy Recommendation

### 7.1 Two-Phase Validation

**Phase 1: Structural (serde)**
- JSON parsing and type coercion
- Required field presence
- Enum variant validation
- Default value population

**Phase 2: Business Rules (runtime)**
- Path validation (no traversal, no absolute paths)
- Range constraints (risk 0-3, fuzzy_threshold 0.0-1.0)
- Cross-field validation (format requires corresponding fields)
- Semantic validation (file exists, sha256 matches)

### 7.2 Recommended Crates

| Crate | Purpose | When to Use |
|-------|---------|-------------|
| [validator](https://github.com/Keats/validator) | Declarative validation | Field constraints, regex patterns |
| [serde_valid](https://docs.rs/serde_valid) | JSON Schema-based validation | Tighter schema compliance |
| Custom `TryFrom` | Complex validation | Cross-field dependencies |

### 7.3 Implementation Example

```rust
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct PatchDetails {
    #[serde(default)]
    pub format: PatchFormat,

    #[validate(length(max = 1_000_000))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,

    #[validate(range(min = 0.0, max = 1.0))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuzzy_threshold: Option<f64>,

    // ... other fields
}

impl PatchDetails {
    /// Validate cross-field constraints
    pub fn validate_format_consistency(&self) -> Result<(), ValidationError> {
        match self.format {
            PatchFormat::Unified => {
                if self.diff.is_none() {
                    return Err(ValidationError::new("unified format requires diff field"));
                }
            }
            PatchFormat::SearchReplace => {
                if self.search_replace_blocks.is_none() {
                    return Err(ValidationError::new(
                        "search_replace format requires search_replace_blocks field"
                    ));
                }
            }
            PatchFormat::WholeFile => {
                if self.whole_file_content.is_none() {
                    return Err(ValidationError::new(
                        "whole_file format requires whole_file_content field"
                    ));
                }
            }
        }
        Ok(())
    }
}
```

### 7.4 Path Validation with Custom Deserializer

```rust
use serde::{Deserialize, Deserializer};
use std::path::Path;

/// Repository-relative path (validated during deserialization)
#[derive(Debug, Clone, Serialize)]
pub struct RepoPath(String);

impl<'de> Deserialize<'de> for RepoPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        // Validate: no path traversal
        if s.contains("..") {
            return Err(serde::de::Error::custom("path traversal (..) not allowed"));
        }

        // Validate: no absolute paths
        if s.starts_with('/') {
            return Err(serde::de::Error::custom("absolute paths not allowed"));
        }

        // Validate: no control characters
        if s.chars().any(|c| c.is_control()) {
            return Err(serde::de::Error::custom("control characters not allowed in path"));
        }

        Ok(RepoPath(s))
    }
}

impl AsRef<str> for RepoPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
```

---

## 8. Recommendations Summary

### 8.1 ProposedAction Modeling

**Approach:** Use `#[serde(tag = "kind", content = "details")]` on the ActionKind enum, then `#[serde(flatten)]` to promote it into the parent struct.

**Rationale:** This preserves the JSON structure where `kind` and `details` are siblings alongside `id`, `summary`, etc.

### 8.2 Optional Fields

**Pattern:** Always pair `#[serde(default)]` with `#[serde(skip_serializing_if = "...")]`

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub optional_field: Option<String>,

#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub optional_list: Vec<String>,

#[serde(default, skip_serializing_if = "is_default")]
pub optional_with_default: SomeEnum,
```

### 8.3 `additionalProperties`

**For known value types:** `HashMap<String, T>`
**For dynamic/unknown:** `serde_json::Value`
**For capturing extras:** `#[serde(flatten)] extra: HashMap<String, Value>`

### 8.4 Validation

**Recommendation:** Two-phase approach:
1. Serde handles structure and types
2. `validator` crate or custom `Validate` trait handles business rules

**Avoid:** Complex validation in custom deserializers (hard to test, poor error messages)

### 8.5 Crate Dependencies

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
validator = { version = "0.18", features = ["derive"] }
thiserror = "2"
```

---

## 9. Sources

- [Serde Enum Representations](https://serde.rs/enum-representations.html) - Official documentation on tagged enum variants
- [Serde Container Attributes](https://serde.rs/container-attrs.html) - tag, content, flatten attributes
- [Serde Field Attributes](https://serde.rs/field-attrs.html) - default, skip_serializing_if, rename
- [Serde Flatten Attribute](https://serde.rs/attr-flatten.html) - Capturing additional properties
- [Schemars](https://github.com/GREsau/schemars) - JSON Schema generation from Rust types
- [Typify](https://github.com/oxidecomputer/typify) - JSON Schema to Rust type conversion
- [Validator Crate](https://github.com/Keats/validator) - Declarative validation for structs
- [serde_valid](https://docs.rs/serde_valid) - JSON Schema-based validation
- [Serde Error Handling](https://serde.rs/error-handling.html) - Custom error messages

---

## 10. Next Steps

1. Create `src/types/mod.rs` with all type definitions
2. Add unit tests for serialization/deserialization round-trips
3. Add integration tests against `.nexus/test-fixtures/`
4. Implement validation traits
5. Consider using `schemars` to generate JSON Schema from Rust types (validate schema consistency)
