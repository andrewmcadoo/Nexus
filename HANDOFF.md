# Nexus Handoff Document

**Updated:** 2026-01-09
**Project:** Nexus - Safe multi-file refactoring CLI in Rust
**GitHub:** andrewmcadoo/Nexus

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

### Completed: Phase 0 & 0.5 - Schema Work
- All schema fixes and improvements done
- Test fixtures created

### Completed: Phase 1 - CLI Foundation
- Branch: `feature/phase1-foundation`
- PR #1 merged
- Clap CLI, error types, settings loader, Rust types from schemas

### Completed: Phase 2 - Event Logging
- Branch: `feature/phase1-foundation` (same branch)
- PR #3: **Open with bug reviews - NEEDS FIXES**
- Append-only JSONL event log with file locking
- Reader/Writer with shared/exclusive locks
- Helper functions for event creation

### Just Shipped: Phase 3 - Executor Module
- Branch: `feature/phase3-executor`
- PR #4: Draft PR created
- HTTP client with retry logic and streaming
- Response parser for unified diffs and search/replace
- Prompt builder for API requests
- Event logging integration

---

## IMMEDIATE TODO: Fix PR #3 Bug Reviews

### Critical Issues

#### 1. Hardcoded Debug Paths (CRITICAL)
**Files:** `src/main.rs:103-148`, `src/settings.rs:232-277`

**Problem:** `debug_log` functions use hardcoded paths:
```rust
const DEBUG_LOG_PATH: &str = "/Users/aj/Desktop/Projects/Nexus/.cursor/debug.log";
```
These fail on other machines and CI.

**Fix Options:**
- Option A: Use `NEXUS_DEBUG_LOG` env var with `std::env::temp_dir()` fallback
- Option B: Guard with `#[cfg(feature = "debug-logging")]` feature flag
- Option C: Remove entirely (it's temporary debugging code)

#### 2. Debug Log File Committed (MEDIUM)
**File:** `.cursor/debug.log`

**Problem:** Debug log with session data committed to repo.

**Fix:**
```bash
echo ".cursor/debug.log" >> .gitignore
git rm --cached .cursor/debug.log
```

#### 3. Config Loading Breaks Without File (MEDIUM)
**File:** `src/main.rs:69-72`

**Problem:** Changed from `NexusConfig::load()` to `load_with_config_path(&cli.config)` which errors if file doesn't exist instead of falling back to defaults.

**Fix:** Check if path exists, fall back to `load()` if not:
```rust
let config = if cli.config.exists() {
    NexusConfig::load_with_config_path(&cli.config)?
} else {
    NexusConfig::load()?
};
```

### Minor Issues

#### 4. EventLogWriter Uses Empty PathBuf in Errors
**File:** `src/event_log/writer.rs:131-137`

**Problem:** Error handlers use `PathBuf::new()` (empty) instead of actual path.

**Fix:** Add `path: PathBuf` field to `EventLogWriter` struct, use `self.path.clone()` in error mappings.

#### 5. Run ID Length Validation Off-by-6
**File:** `src/event_log/mod.rs:59-64`

**Problem:** Allows 255 chars but filename is `{run_id}.jsonl` (+6 chars).

**Fix:** Change limit from 255 to 249 characters.

#### 6. Writer Fails on Corrupted Lines
**File:** `src/event_log/writer.rs:102-103`

**Problem:** `scan_max_event_seq` fails on malformed JSON, unlike reader which skips.

**Fix:** Skip corrupted lines with warning, continue scanning (match reader behavior).

#### 7. Non-Executable Doctests on Private Helpers
**File:** `src/types/action.rs:65-87, 233-244, 302-315`

**Problem:** Doc examples on private functions can't be tested.

**Fix:** Remove `/// # Examples` blocks or convert to `//` comments.

---

## What Worked

1. **Skill evaluation before implementation** - Caught issues early
2. **Codex via MCP for all code** - Clean separation: Claude plans/reviews, Codex writes
3. **Parallel sub-agent execution** - Speed up multi-task work
4. **Ship skill for commits** - Consistent workflow with security scans
5. **Separate branches per phase** - Clean PR separation

---

## What Didn't Work / Watch Out For

1. **Schema `oneOf` with sibling `kind`/`details`** - Maps awkwardly to Rust. Use `#[serde(flatten)]` with `#[serde(untagged)]` enum.

2. **JSON Schema `default` doesn't auto-apply in serde** - Need explicit `#[serde(default = "...")]` annotations.

3. **Hardcoded developer paths** - Debug logging committed with absolute paths. Always use env vars or temp dirs.

4. **Config loading semantic change** - `load_with_config_path` has different semantics than `load()`. Check file existence first.

---

## Key Files

| File | Purpose |
|------|---------|
| `docs/implementation-plan.md` | Full 7-phase build plan |
| `docs/plans/phase2-implementation-plan.md` | Phase 2 detailed plan |
| `src/event_log/` | Event logging module |
| `src/executor/` | Codex executor module (Phase 3) |
| `src/cli.rs` | CLI argument parsing |
| `src/settings.rs` | Config loading |
| `src/error.rs` | Error types and exit codes |
| `tests/event_log.rs` | Event log integration tests |
| `tests/executor.rs` | Executor integration tests |

---

## Branch Status

| Branch | PR | Status |
|--------|-----|--------|
| `main` | - | Base |
| `feature/phase1-foundation` | PR #3 | **Needs bug fixes** |
| `feature/phase3-executor` | PR #4 | Draft, ready for review |

---

## Next Steps

1. **Fix PR #3 bugs** (on `feature/phase1-foundation` branch)
   - Remove/fix hardcoded debug paths
   - Add `.cursor/debug.log` to gitignore
   - Fix config loading fallback
   - Fix EventLogWriter path in errors
   - Adjust run_id length limit
   - Make writer skip corrupted lines
   - Fix/remove private helper doctests

2. **Get PR #3 merged**

3. **Continue Phase 4** - Permission Gate
   - Interactive prompts with crossterm
   - Allow/Deny/Ask modes
   - Policy matching

---

## Commands to Resume

```bash
cd /Users/aj/Desktop/Projects/Nexus
git checkout feature/phase1-foundation
git status

# Check test status
cargo test

# Run clippy
cargo clippy -- -D warnings

# After fixes, ship
# /ship
```

---

## Skills to Load

When resuming, run `/skill-evaluator` or load:
- `rust-idioms`
- `rust-testing`
- `codex-coder` (Codex writes ALL code via MCP)
- `security-scan`
- `ship`

---

## Memory Bank

Project memory at: `.claude/memory/`
- `CLAUDE-activeContext.md` - Session state
- `CLAUDE-patterns.md` - Code patterns
- `CLAUDE-decisions.md` - Architecture decisions
- `CLAUDE-troubleshooting.md` - Known issues
