# Nexus Handoff Document

**Updated:** 2026-01-08
**Project:** Nexus - Safe multi-file refactoring CLI in Rust

---

## Goal

Build a Rust CLI that does safe multi-file refactoring:
1. User describes refactoring task
2. Executor (Codex via OpenAI API) proposes patches as `ProposedAction`s
3. Permission Gate prompts user for approval (Allow/Ask/Deny)
4. Tool Gateway applies approved unified diffs
5. Event log records everything for replay/audit

**NOT building for v0:** Router model, research agent, planner agent, BAML, doc graph, workflow graphs, complexity units, batching, context packs, sandbox isolation.

---

## Current Progress

### Completed: Phase 0 - Schema Fixes

| Change | Status |
|--------|--------|
| Removed `bypass` from permission_mode | Done |
| Removed `ext` fields from all 9 schemas | Done |
| Added `maxLength: 1000000` to diff fields | Done |
| Created test fixtures directory | Done |

### Completed: Phase 0.5 - Schema Improvements

Based on competitive analysis (Aider, Codex CLI, LSP, Semgrep, ast-grep), enhanced schemas:

| Task | Change | Status |
|------|--------|--------|
| ARCH-1 | Path validation (`$defs/repo_relative_path`) in 4 schemas | Done |
| ARCH-2 | File operations: `file_create`, `file_rename`, `file_delete` | Done |
| ARCH-3 | Diff formats: `unified`, `search_replace`, `whole_file` | Done |
| ARCH-4 | Fallback matching: `fuzzy`, `line_anchor` strategies | Done |
| ARCH-5 | Settings required: `permission_mode`, `schema_version` | Done |
| ARCH-6 | Approval groups for batch approval | Done |
| ARCH-7 | Document versioning in context_pack and event | Done |
| ARCH-8 | Conflict resolution: `fail`, `ours`, `theirs`, `marker` | Done |
| TEST-1 | Test fixtures for all new features (14 files) | Done |

**Full plan:** `docs/schema-improvement-plan.md`

**Test fixtures structure:**
```
.nexus/test-fixtures/
├── actions/
│   ├── valid-patch.json
│   ├── path-traversal-attempt.json
│   ├── denied-command.json
│   ├── file-create-action.json
│   ├── file-rename-action.json
│   ├── file-delete-action.json
│   ├── search-replace-patch.json
│   ├── whole-file-patch.json
│   ├── grouped-approval.json
│   ├── fuzzy-match-patch.json
│   ├── invalid-path-traversal.json
│   └── invalid-absolute-path.json
├── settings/
│   ├── valid-minimal.json
│   ├── valid-full.json
│   ├── invalid-empty.json
│   └── invalid-no-version.json
├── diffs/
│   ├── simple-add.diff
│   ├── multi-file.diff
│   └── conflict.diff
└── events/
    ├── sample-run.jsonl
    └── valid-event.json
```

### Pending: Phase 1-7

Phase 1 (Foundation) is ready to start.

---

## What Worked

1. **Skill evaluation before implementation** - Caught 10 issues in schemas/architecture
2. **Removing bypass mode** - Security by design, no footguns
3. **Removing ext fields** - YAGNI, cleaner types
4. **Test fixtures upfront** - Ready for integration tests
5. **Competitive research before schema finalization** - Found gaps (file ops, diff formats) and validated strengths (risk levels, approval gates)
6. **Parallel sub-agent execution** - Ran ARCH-3/5 in parallel, then ARCH-4/6/7/8 in parallel for speed
7. **Codex via MCP for all code** - Clean separation: Claude plans/reviews, Codex writes

---

## What Didn't Work / Watch Out For

1. **Schema `oneOf` with sibling `kind`/`details`** - Maps awkwardly to Rust. Solution: Use `#[serde(flatten)]` with `#[serde(untagged)]` enum, validate kind↔details match at runtime.

2. **JSON Schema `default` doesn't auto-apply in serde** - Need explicit `#[serde(default = "...")]` annotations.

3. **Some `cwd` fields may need absolute paths** - Path validation pattern blocks absolute paths. May need separate pattern for cwd fields if absolute paths required.

---

## Key Files

| File | Purpose |
|------|---------|
| `docs/implementation-plan.md` | Full 7-phase build plan |
| `docs/schema-improvement-plan.md` | Phase 0.5 schema enhancements |
| `docs/architecture.md` | Original architecture spec |
| `.nexus/schemas/*.json` | JSON schemas (enhanced) |
| `.nexus/policy.md` | Permission policy design |
| `.nexus/test-fixtures/` | Test data for integration tests |

---

## Schema Enhancements Summary

| Schema | New Features |
|--------|--------------|
| `proposed_action.schema.json` | 3 file ops, 3 diff formats, fallback matching, approval groups, conflict resolution, path validation |
| `settings.schema.json` | Required fields, schema_version, path validation |
| `context_pack.schema.json` | Version field, path validation |
| `event.schema.json` | Version pattern `^nexus/[0-9]+$` |
| `exec.result.schema.json` | Path validation |

---

## Next Steps

### Phase 1: Foundation
1. `cargo init --name nexus` in project root
2. Set up Cargo.toml with dependencies:
   ```toml
   serde, serde_json, clap, tokio, thiserror, anyhow, chrono
   ```
3. Create `src/error.rs` with `NexusError` enum
4. Derive Rust types from JSON schemas (`src/types/`)
5. CLI skeleton with clap
6. Settings loader for `.nexus/settings.json`

### Architecture Decisions (Already Made)
- Single crate (no workspace)
- tokio async for API calls, sync for file ops
- thiserror for library, anyhow for CLI
- `tokio::sync::Mutex` for session state
- API key from env only (`OPENAI_API_KEY`)
- No bypass mode

---

## Commands to Resume

```bash
cd /Users/aj/Desktop/Projects/Nexus

# Read the plans
cat docs/implementation-plan.md
cat docs/schema-improvement-plan.md

# Validate schemas
npx ajv-cli validate -s .nexus/schemas/settings.schema.json -d .nexus/test-fixtures/settings/valid-minimal.json

# Start Phase 1
cargo init --name nexus
```

---

## Skills to Load

When resuming, run `/skill-evaluator` or load these directly:
- `rust-idioms`
- `rust-project-structure`
- `rust-testing`
- `codex-coder` (Codex writes ALL code via MCP)
- `security-scan`

---

## Memory Bank

Project memory initialized at: `.claude/memory/`
- `CLAUDE-activeContext.md` - Session state
- `CLAUDE-patterns.md` - Code patterns
- `CLAUDE-decisions.md` - Architecture decisions (9 ADRs)
- `CLAUDE-troubleshooting.md` - Known issues
