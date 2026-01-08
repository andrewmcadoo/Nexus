# Schema Improvement Plan

**Date:** 2026-01-08
**Phase:** Pre-Phase 1 (Schema Enhancements)
**Status:** Draft - Awaiting Approval

---

## Executive Summary

This plan addresses gaps identified from competitive analysis of Aider, Codex CLI, LSP, Semgrep, and ast-grep. The improvements are designed to be implemented **before** Phase 1 (Rust CLI foundation) so the generated Rust types include all new capabilities from day one.

### Priority Matrix

| Priority | Change | Impact | Effort |
|----------|--------|--------|--------|
| P0 | Add file operations (create/rename/delete) | Critical for real refactoring | Medium |
| P0 | Add path validation pattern | Security requirement | Low |
| P1 | Add search/replace diff format | Better LLM compatibility | Medium |
| P1 | Add required fields to settings | Prevents invalid configs | Low |
| P2 | Add approval groups | UX improvement | Medium |
| P2 | Add fallback matching strategy | Reliability improvement | Medium |
| P3 | Add document versioning | Future-proofing | Low |
| P3 | Add conflict resolution hint | Git workflow support | Low |

---

## Problem 1: No File Operations

### Current State
The `proposed_action.schema.json` only supports `kind: "patch"` for file changes. There is no way to:
- Create new files
- Rename/move files
- Delete files

### Gap Analysis
LSP WorkspaceEdit supports `CreateFile`, `RenameFile`, `DeleteFile` operations. Aider and Codex CLI both generate these via conventions in their patch formats.

### Proposed Solution

Add three new action kinds to `proposed_action.schema.json`:

```json
{
  "kind": {
    "enum": [
      "handoff",
      "patch",
      "command",
      "plan_patch",
      "agenda_patch",
      "file_create",
      "file_rename",
      "file_delete"
    ]
  }
}
```

#### FileCreateAction Details
```json
{
  "title": "FileCreateAction",
  "properties": {
    "kind": { "const": "file_create" },
    "details": {
      "type": "object",
      "additionalProperties": false,
      "required": ["path", "content"],
      "properties": {
        "path": {
          "type": "string",
          "pattern": "^(?!.*\\.\\.)(?!/).*$",
          "description": "Repo-relative path. No path traversal allowed."
        },
        "content": {
          "type": "string",
          "maxLength": 1000000,
          "description": "Initial file content."
        },
        "encoding": {
          "type": "string",
          "enum": ["utf-8", "base64"],
          "default": "utf-8"
        },
        "overwrite": {
          "type": "boolean",
          "default": false,
          "description": "If true, overwrites existing file. If false, fails if file exists."
        }
      }
    }
  }
}
```

#### FileRenameAction Details
```json
{
  "title": "FileRenameAction",
  "properties": {
    "kind": { "const": "file_rename" },
    "details": {
      "type": "object",
      "additionalProperties": false,
      "required": ["old_path", "new_path"],
      "properties": {
        "old_path": {
          "type": "string",
          "pattern": "^(?!.*\\.\\.)(?!/).*$",
          "description": "Current repo-relative path."
        },
        "new_path": {
          "type": "string",
          "pattern": "^(?!.*\\.\\.)(?!/).*$",
          "description": "Target repo-relative path."
        },
        "overwrite": {
          "type": "boolean",
          "default": false,
          "description": "If true, overwrites target if exists."
        }
      }
    }
  }
}
```

#### FileDeleteAction Details
```json
{
  "title": "FileDeleteAction",
  "properties": {
    "kind": { "const": "file_delete" },
    "details": {
      "type": "object",
      "additionalProperties": false,
      "required": ["path"],
      "properties": {
        "path": {
          "type": "string",
          "pattern": "^(?!.*\\.\\.)(?!/).*$",
          "description": "Repo-relative path to delete."
        },
        "expected_sha256": {
          "type": "string",
          "description": "Optional hash to verify file hasn't changed before delete."
        },
        "recursive": {
          "type": "boolean",
          "default": false,
          "description": "If true and path is directory, delete recursively."
        }
      }
    }
  }
}
```

### Risk Assessment
- **Security:** Path pattern prevents traversal, but runtime validation still needed
- **Risk Level:** File operations inherit action's `risk` field; deletions should default to risk=2

### Implementation Tasks
1. Add `file_create`, `file_rename`, `file_delete` to kind enum
2. Add oneOf entries for each new action type
3. Add path validation pattern to all path fields
4. Update test fixtures with examples
5. Document in policy.md how these map to Allow/Ask/Deny

---

## Problem 2: Single Diff Format

### Current State
`PatchAction.details.format` only allows `"unified"`. While ADR-007 accepted this for v0, LLMs trained on different formats may struggle.

### Gap Analysis
- **Aider:** Uses `<<<<<<< SEARCH / ======= / >>>>>>> REPLACE` blocks
- **Codex CLI:** Uses `*** Begin Patch` / `*** End Patch` markers
- **Cline/RooCode:** Uses search/replace with fuzzy matching

Unified diff requires exact context lines, which LLMs often get wrong (off-by-one, whitespace issues).

### Proposed Solution

Expand format enum and add format-specific fields:

```json
{
  "format": {
    "type": "string",
    "enum": ["unified", "search_replace", "whole_file"],
    "default": "unified"
  },
  "search_replace_blocks": {
    "type": "array",
    "items": {
      "type": "object",
      "additionalProperties": false,
      "required": ["search", "replace"],
      "properties": {
        "search": {
          "type": "string",
          "description": "Exact text to find (or regex if match_mode=regex)"
        },
        "replace": {
          "type": "string",
          "description": "Replacement text"
        },
        "file": {
          "type": "string",
          "description": "Target file path"
        },
        "match_mode": {
          "type": "string",
          "enum": ["exact", "fuzzy", "regex"],
          "default": "exact"
        },
        "match_occurrence": {
          "type": "integer",
          "minimum": 1,
          "default": 1,
          "description": "Which occurrence to replace (1=first, 2=second, etc.)"
        }
      }
    },
    "description": "Used when format=search_replace"
  },
  "whole_file_content": {
    "type": "string",
    "maxLength": 1000000,
    "description": "Complete file content. Used when format=whole_file."
  }
}
```

### Format Selection Logic (Runtime)
1. If `format=unified` -> use `diff` field, apply with diffy
2. If `format=search_replace` -> use `search_replace_blocks`, apply sequentially
3. If `format=whole_file` -> use `whole_file_content`, replace entire file

### Compatibility Note
- `unified` remains default (Codex native output)
- `search_replace` enables Aider-style prompts
- `whole_file` is escape hatch for small files

### Implementation Tasks
1. Expand format enum
2. Add `search_replace_blocks` array definition
3. Add `whole_file_content` field
4. Add conditional validation (format=X requires field Y)
5. Update test fixtures with all three formats

---

## Problem 3: No Path Traversal Validation in Schema

### Current State
Path fields have `type: "string"` with no pattern validation. Comment says "Path traversal checked at runtime."

### Gap Analysis
Schema-level validation catches issues earlier and provides better error messages. JSON Schema supports `pattern` for regex validation.

### Proposed Solution

Add path pattern to all path fields across all schemas:

```json
{
  "path": {
    "type": "string",
    "pattern": "^(?!.*\\.\\.)(?!/)(?!.*[\\x00-\\x1f]).*$",
    "description": "Repo-relative path. Must not contain '..' or start with '/'."
  }
}
```

Pattern breakdown:
- `(?!.*\\.\\.)` - No `..` anywhere
- `(?!/)` - Must not start with `/` (absolute path)
- `(?!.*[\\x00-\\x1f])` - No control characters

### Files to Update
1. `proposed_action.schema.json` - PatchAction.details.files[], file operation paths
2. `context_pack.schema.json` - refs[].path
3. `exec.result.schema.json` - blocker.suggested_ctx.paths[], files_read[], files_write[]
4. `settings.schema.json` - deny_paths[], allow_paths_write[]

### Implementation Tasks
1. Define `$defs/repo_relative_path` pattern in each schema
2. Reference it for all path fields
3. Add test fixtures with invalid paths (should fail validation)

---

## Problem 4: No Fallback Matching Strategy

### Current State
When applying patches, if exact context match fails, the operation fails.

### Gap Analysis
- **Aider:** Uses difflib for fuzzy matching
- **RooCode:** Falls back to searching for unique lines
- **Cline:** Uses similarity scoring

### Proposed Solution

Add fallback strategy field to PatchAction:

```json
{
  "fallback_strategy": {
    "type": "string",
    "enum": ["none", "fuzzy", "line_anchor"],
    "default": "none",
    "description": "What to try if exact match fails"
  },
  "fuzzy_threshold": {
    "type": "number",
    "minimum": 0.0,
    "maximum": 1.0,
    "default": 0.8,
    "description": "Minimum similarity ratio for fuzzy matching (0.0-1.0)"
  }
}
```

Strategies:
- `none` - Fail immediately (current behavior, safest)
- `fuzzy` - Use edit distance to find best match above threshold
- `line_anchor` - Find unique anchor lines, apply relative to them

### Risk Consideration
Fuzzy matching increases risk of wrong-location patches. Consider:
- Auto-elevating risk level when fuzzy match is used
- Requiring user confirmation for fuzzy-matched patches
- Logging match confidence in events

### Implementation Tasks
1. Add `fallback_strategy` and `fuzzy_threshold` to PatchAction.details
2. Add `match_confidence` field to capture actual match quality
3. Document risk implications in policy.md

---

## Problem 5: Settings Has No Required Fields

### Current State
`settings.schema.json` has no `required` array. An empty object `{}` is valid.

### Gap Analysis
This allows invalid/incomplete configurations to pass validation.

### Proposed Solution

Add sensible required fields and defaults:

```json
{
  "required": ["permission_mode"],
  "properties": {
    "permission_mode": {
      "type": "string",
      "enum": ["default", "acceptEdits", "autopilot"],
      "default": "default"
    },
    "schema_version": {
      "type": "string",
      "const": "1.0",
      "description": "Schema version for migration support"
    }
  }
}
```

Rationale for required fields:
- `permission_mode` - Core setting that determines behavior
- `schema_version` - Enables future migrations

### Implementation Tasks
1. Add `required: ["permission_mode"]` to settings.schema.json
2. Add `schema_version` field with const value
3. Update test fixtures

---

## Problem 6: No Approval Groups

### Current State
Each action is approved/denied individually. No way to group related actions.

### Gap Analysis
LSP has `needsConfirmation` with annotation linking. This allows "approve all file renames" as one decision.

### Proposed Solution

Add approval group field to ProposedAction:

```json
{
  "approval_group": {
    "type": "object",
    "additionalProperties": false,
    "properties": {
      "id": {
        "type": "string",
        "description": "Group identifier (actions with same ID can be approved together)"
      },
      "label": {
        "type": "string",
        "description": "Human-readable group description"
      },
      "size": {
        "type": "integer",
        "minimum": 1,
        "description": "Total actions in this group (for progress display)"
      }
    }
  }
}
```

Usage:
- Executor emits actions with same `approval_group.id`
- Permission Gate shows: "Approve 5 file renames? [Y/n/review each]"
- User can approve group or review individually

### Implementation Tasks
1. Add `approval_group` to ProposedAction properties
2. Update Permission Gate logic (Phase 3) to handle groups
3. Add test fixtures with grouped actions

---

## Problem 7: No Document Versioning

### Current State
No way to track schema or document versions.

### Gap Analysis
LSP tracks `TextDocumentIdentifier.version`. This enables optimistic concurrency.

### Proposed Solution

Add version tracking to context_pack and event schemas:

```json
// In context_pack.schema.json
{
  "version": {
    "type": "integer",
    "minimum": 0,
    "description": "Monotonic version number, incremented on each change"
  }
}

// In event.schema.json
{
  "v": {
    "type": "string",
    "pattern": "^nexus/[0-9]+$",
    "description": "Event schema version (e.g., nexus/1)"
  }
}
```

### Implementation Tasks
1. Add `version` field to context_pack
2. Enforce version pattern on event.v field
3. Document version semantics

---

## Problem 8: No Conflict Resolution Hint

### Current State
No guidance for handling conflicts when applying patches.

### Gap Analysis
Git-based tools need to know whether to:
- Fail on conflict
- Auto-resolve (theirs/ours)
- Create conflict markers

### Proposed Solution

Add conflict handling field to PatchAction:

```json
{
  "on_conflict": {
    "type": "string",
    "enum": ["fail", "ours", "theirs", "marker"],
    "default": "fail",
    "description": "How to handle merge conflicts"
  }
}
```

Strategies:
- `fail` - Abort patch application (safest, current behavior)
- `ours` - Keep our (base) version on conflict
- `theirs` - Keep patch version on conflict
- `marker` - Insert conflict markers for manual resolution

### Implementation Tasks
1. Add `on_conflict` to PatchAction.details
2. Document behavior in policy.md
3. Consider risk elevation for non-fail strategies

---

## Strengths to Preserve

These existing features are **better than competitors** and must be preserved:

| Feature | Location | Why Keep |
|---------|----------|----------|
| Risk levels (0-3) | ProposedAction.risk | More granular than binary approval |
| SHA256 verification | PatchAction.base_file_sha256 | Prevents stale patches |
| Approval gates | ProposedAction.requires_approval | Explicit opt-out required |
| Event tracing | Event.trace.{correlation_id, span_id} | Better than any analyzed tool |
| Policy tags | ProposedAction.policy_tags | Enables custom policy rules |
| Argv-only commands | CommandAction.argv | Security (no shell injection) |

---

## Implementation Order

### Phase 0.5: Schema Enhancements (Before Phase 1)

Execute in this order due to dependencies:

```
1. [ARCH-1] Add path validation pattern ($defs)
   - No dependencies
   - Enables all other path changes

2. [ARCH-2] Add file operations (create/rename/delete)
   - Depends on: ARCH-1 (path pattern)
   - High impact for refactoring scenarios

3. [ARCH-3] Add search/replace diff format
   - No dependencies
   - Enables better LLM compatibility

4. [ARCH-4] Add fallback matching strategy
   - Depends on: ARCH-3 (format field)
   - Reliability improvement

5. [ARCH-5] Add settings required fields + version
   - No dependencies
   - Low risk, quick win

6. [ARCH-6] Add approval groups
   - No dependencies
   - UX improvement

7. [ARCH-7] Add document versioning
   - No dependencies
   - Future-proofing

8. [ARCH-8] Add conflict resolution hint
   - No dependencies
   - Git workflow support

9. [TEST-1] Update test fixtures
   - Depends on: All ARCH tasks
   - Validates all changes
```

### Estimated Effort

| Task | Tokens (sizing) | Agent |
|------|-----------------|-------|
| ARCH-1 | ~10k (Atomic) | system-architect |
| ARCH-2 | ~30k (Focused) | system-architect |
| ARCH-3 | ~25k (Focused) | system-architect |
| ARCH-4 | ~15k (Atomic) | system-architect |
| ARCH-5 | ~10k (Atomic) | system-architect |
| ARCH-6 | ~15k (Atomic) | system-architect |
| ARCH-7 | ~10k (Atomic) | system-architect |
| ARCH-8 | ~10k (Atomic) | system-architect |
| TEST-1 | ~40k (Focused) | tests-qa-engineer |

**Total:** ~165k tokens across 9 tasks

---

## Acceptance Criteria

### Schema Changes
- [ ] All path fields have traversal-blocking pattern
- [ ] File create/rename/delete actions validated by schema
- [ ] Three diff formats supported (unified, search_replace, whole_file)
- [ ] Settings requires permission_mode
- [ ] Approval group field exists on ProposedAction
- [ ] Version fields added where appropriate

### Test Fixtures
- [ ] Valid examples for each new action kind
- [ ] Invalid path examples (should fail validation)
- [ ] Examples of each diff format
- [ ] Grouped approval example

### Documentation
- [ ] policy.md updated with new action types
- [ ] ADRs written for significant decisions

---

## Risk Assessment

| Change | Risk | Mitigation |
|--------|------|------------|
| File operations | Medium - Potential for unintended deletions | Require explicit approval, verify SHA |
| Fuzzy matching | Medium - Wrong location patches | Default to `none`, log confidence |
| Search/replace format | Low - Additive change | Keep unified as default |
| Path patterns | Low - May reject edge cases | Test thoroughly, document escaping |

---

## Open Questions

1. **Should file operations be separate kinds or part of patch?**
   - Decision: Separate kinds (clearer semantics, easier approval)

2. **Should fuzzy matching require elevated approval?**
   - Recommendation: Yes, auto-elevate to requires_approval=true

3. **What about binary files?**
   - Recommendation: Add `encoding: "base64"` option to file_create
   - Defer full binary support to post-v0

---

## Next Steps

1. Review and approve this plan
2. Execute ARCH-1 through ARCH-8 in order
3. Execute TEST-1 to validate changes
4. Update CLAUDE-decisions.md with new ADRs
5. Proceed to Phase 1 (Rust CLI foundation)
