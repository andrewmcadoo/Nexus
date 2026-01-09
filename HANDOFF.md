# Nexus Handoff Document

**Updated:** 2026-01-09
**Project:** Nexus - Safe multi-file refactoring CLI in Rust

---

## Current Session: PR #4 Bug Fixes

### Goal
Fix 15 bugs identified in PR #4 code review on the `feature/phase3-executor` branch. Bugs span security issues, logic errors, code duplication, and maintenance concerns.

### Completed (9/14 tasks)

| Task | File(s) | Description |
|------|---------|-------------|
| Task 2 | `main.rs`, `settings.rs` | Removed hardcoded debug paths and debug_log functions |
| Task 4 | `event_log/writer.rs` | Fixed missing log path in IoError - added `path` field |
| Task 5 | `event_log/writer.rs` | Fixed lock failure masking - distinguish `WouldBlock` |
| Task 8 | `Cargo.toml`, `client.rs` | Migrated `tokio-retry` to `tokio-retry2` |
| Task 9 | `executor/client.rs` | Fixed fallback timeout - use `.expect()` not silent fallback |
| Task 12 | `.gitignore` | Removed `.cursor/debug.log`, added `.cursor/` to gitignore |
| Task 3 | `settings.rs` | Fixed config loader - now uses defaults with warning |
| Task 1 | `executor/adapter.rs` | Fixed run ID mismatch - added `execute_internal()` |
| Task 10 | `executor/parser.rs` | Fixed run_id validation - added path/length checks |

### Remaining (5 tasks)

| Task | File | Description |
|------|------|-------------|
| Task 11 | `parser.rs` | Document JSON parsing behavior (doc comments) |
| Task 13 | `types/settings.rs` | Windows path validation (C:\, UNC, is_control()) |
| Task 14 | `Cargo.toml` | Repository URL casing (Nexus -> nexus) |
| Verification | - | `cargo build && test && clippy && fmt --check` |

### Next Steps (Copy-Paste Ready)

```bash
cd /Users/aj/Desktop/Projects/Nexus

# 1. Task 11: Add doc comments to parser.rs parse_fenced/inline_json_actions
# 2. Task 13: Update validate_path_pattern in types/settings.rs
# 3. Task 14: Fix Cargo.toml line 10 URL casing
# 4. Verify: cargo build && cargo test && cargo clippy && cargo fmt --check
# 5. Commit all fixes
```

### Plan File
`/Users/aj/.claude/plans/glittery-noodling-penguin.md`

---

## Project Overview

Build a Rust CLI that does safe multi-file refactoring:
1. User describes refactoring task
2. Executor (Codex via OpenAI API) proposes patches as `ProposedAction`s
3. Permission Gate prompts user for approval (Allow/Ask/Deny)
4. Tool Gateway applies approved unified diffs
5. Event log records everything for replay/audit

**NOT building for v0:** Router model, research agent, planner agent, BAML, doc graph, workflow graphs, complexity units, batching, context packs, sandbox isolation.

---

## Completed Phases

### Phase 0 - Schema Fixes ✅
- Removed `bypass` from permission_mode
- Removed `ext` fields from all 9 schemas
- Added `maxLength: 1000000` to diff fields
- Created test fixtures directory

### Phase 0.5 - Schema Improvements ✅
Enhanced schemas based on competitive analysis (Aider, Codex CLI, LSP, Semgrep):
- Path validation, file operations, diff formats, fallback matching
- Settings required fields, approval groups, document versioning

### Phase 1 - Foundation ✅
- Cargo project setup with dependencies
- `src/error.rs` with `NexusError` enum
- Rust types from JSON schemas (`src/types/`)
- CLI skeleton with clap
- Settings loader

### Phase 2 - Event Log ✅
- Append-only JSONL event logging
- `EventLogWriter/Reader` with fs2 locks
- Atomic writes, seq assignment, filtering

### Phase 3 - Executor (Current PR #4)
- Codex adapter with HTTP client
- Exponential backoff/jitter retry
- SSE parsing, prompt builder
- Streaming handler
- Integration with event logging

---

## What Worked

1. **Skill evaluation before implementation** - Caught issues early
2. **Codex via MCP for all code** - Clean separation: Claude plans/reviews, Codex writes
3. **`execute_internal()` pattern** - Cleanly solved run_id mismatch
4. **Distinguishing `ErrorKind::WouldBlock`** - Proper lock error handling
5. **Test fixtures upfront** - Ready for integration tests

---

## What Didn't Work / Watch Out For

1. **Schema `oneOf` with sibling `kind`/`details`** - Maps awkwardly to Rust. Use `#[serde(flatten)]` with `#[serde(untagged)]` enum.
2. **JSON Schema `default` doesn't auto-apply in serde** - Need explicit `#[serde(default = "...")]`.
3. **Hardcoded debug paths** - Don't commit dev-specific paths. Removed in Task 2.

---

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI entry point |
| `src/settings.rs` | Config loader |
| `src/error.rs` | Error types |
| `src/event_log/writer.rs` | JSONL event logging |
| `src/executor/adapter.rs` | Codex API adapter |
| `src/executor/client.rs` | HTTP client with retry |
| `src/executor/parser.rs` | Response parsing |
| `docs/implementation-plan.md` | Full 7-phase build plan |

---

## Skills to Load

When resuming, run `/skill-evaluator` or load:
- `rust-idioms`, `rust-testing`
- `codex-coder` (Codex writes ALL code via MCP)
- `security-scan`

---

## Memory Bank

Project memory at: `.claude/memory/`
- `CLAUDE-activeContext.md` - Session state
- `CLAUDE-patterns.md` - Code patterns
- `CLAUDE-decisions.md` - Architecture decisions (ADRs)
- `CLAUDE-troubleshooting.md` - Known issues
