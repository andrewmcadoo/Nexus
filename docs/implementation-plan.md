# Nexus v0 Implementation Plan

**Target:** Safe multi-file refactoring CLI in Rust
**Stack:** Rust + Codex (via OpenAI API)
**Timeline:** No rush, get it right

---

## What We're Building (v0 Scope)

A focused CLI that does ONE thing well:

```
nexus "rename getUserData to fetchUserProfile across the codebase"
```

1. User describes refactoring task
2. Executor (Codex) proposes patches as `ProposedAction`s
3. Permission Gate prompts user for approval (Allow/Ask/Deny)
4. Tool Gateway applies approved unified diffs
5. Event log records everything for replay/audit

**NOT building for v0:** Router model, research agent, planner agent, BAML, doc graph, workflow graphs, complexity units, batching, context packs, sandbox isolation.

---

## Project Structure

```
nexus/
├── Cargo.toml
├── src/
│   ├── main.rs                 # CLI entry (clap)
│   ├── lib.rs
│   │
│   ├── types/                  # Core data models from schemas
│   │   ├── mod.rs
│   │   ├── action.rs           # ProposedAction, ActionKind, *Details
│   │   ├── event.rs            # RunEvent, TraceContext
│   │   └── settings.rs         # NexusSettings, PermissionMode
│   │
│   ├── executor/               # Codex adapter
│   │   ├── mod.rs
│   │   ├── adapter.rs          # Call Codex, get patches
│   │   └── parser.rs           # Parse Codex output → ProposedActions
│   │
│   ├── permission/             # Permission Gate
│   │   ├── mod.rs
│   │   ├── gate.rs             # Evaluate actions, prompt user
│   │   ├── policy.rs           # Allow/Ask/Deny rules
│   │   └── matcher.rs          # Glob paths, match commands
│   │
│   ├── gateway/                # Tool Gateway
│   │   ├── mod.rs
│   │   ├── patch.rs            # Apply unified diffs (diffy crate)
│   │   └── command.rs          # Execute approved commands
│   │
│   ├── event_log/              # Append-only JSONL
│   │   ├── mod.rs
│   │   ├── writer.rs
│   │   └── reader.rs           # For replay/resume
│   │
│   └── engine.rs               # Main loop: executor → gate → gateway
│
└── tests/
    └── fixtures/               # Test diffs, mock responses
```

---

## Implementation Phases

### Phase 1: Foundation
**Goal:** Types compile, CLI parses args, settings load

- [ ] `cargo init`, set up Cargo.toml with dependencies
- [ ] Derive Rust types from JSON schemas (`types/`)
- [ ] CLI skeleton with clap (`nexus run "task"`, `nexus init`, `nexus policy show`)
- [ ] Load `.nexus/settings.json` with defaults

**Dependencies:**
```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
thiserror = "2"
anyhow = "1"
chrono = { version = "0.4", features = ["serde"] }
```

### Phase 2: Event Log
**Goal:** Append-only JSONL writer/reader

- [ ] `EventLogWriter::append(event)` - atomic writes
- [ ] `EventLogReader::iter()` - parse JSONL
- [ ] Helper functions: `action_proposed()`, `permission_granted()`, `tool_executed()`
- [ ] Tests with tempdir

### Phase 3: Permission Gate
**Goal:** Allow/Ask/Deny logic with interactive prompts

- [ ] `PolicyEngine` - evaluate action against settings rules
- [ ] Path glob matching (`glob` crate)
- [ ] Command prefix matching
- [ ] `UserPrompter` trait + CLI impl with `dialoguer`
- [ ] Session state for "remember" approvals
- [ ] Tests with mock prompter

**Additional deps:**
```toml
glob = "0.3"
dialoguer = "0.11"
console = "0.15"
```

### Phase 4: Tool Gateway
**Goal:** Apply patches, run commands

- [ ] `PatchGateway::apply_diff(diff, base_hashes)` using `diffy`
- [ ] `PatchGateway::validate_diff()` for dry-run
- [ ] File hash verification (sha256)
- [ ] Path traversal protection
- [ ] `CommandGateway::run(argv, timeout)` with subprocess
- [ ] Tests with real file operations

**Additional deps:**
```toml
diffy = "0.4"
sha2 = "0.10"
hex = "0.4"
```

### Phase 5: Executor Adapter
**Goal:** Call Codex via OpenAI API, parse patches

- [ ] `CodexAdapter::new(api_key)` - load from env or config
- [ ] `CodexAdapter::execute(prompt, files)` - call Responses API
- [ ] Build prompt with file contents + refactoring instructions
- [ ] Parse response → extract diffs → `Vec<ProposedAction>`
- [ ] Handle rate limits, retries, API errors
- [ ] Mock adapter for testing

**Integration:** OpenAI API direct (Responses API for Codex)

**Additional deps:**
```toml
reqwest = { version = "0.12", features = ["json"] }
```

### Phase 6: Engine
**Goal:** End-to-end flow

- [ ] `Engine::run(task, files, dry_run)`
- [ ] Loop: executor → for each action → gate → gateway
- [ ] Event logging throughout
- [ ] Summary output

### Phase 7: Polish
**Goal:** Production-ready CLI

- [ ] `nexus replay <run_id>` - show timeline from events
- [ ] `nexus resume <run_id>` - continue from last success
- [ ] Better output formatting (colors, cards)
- [ ] Error recovery, partial failure handling
- [ ] README with examples

---

## Key Files to Modify/Create

| File | Purpose |
|------|---------|
| `Cargo.toml` | Dependencies, metadata |
| `src/types/action.rs` | ProposedAction from `.nexus/schemas/proposed_action.schema.json` |
| `src/types/settings.rs` | NexusSettings from `.nexus/schemas/settings.schema.json` |
| `src/types/event.rs` | RunEvent from `.nexus/schemas/event.schema.json` |
| `src/permission/gate.rs` | Core permission logic from `.nexus/policy.md` |
| `src/gateway/patch.rs` | Unified diff application |
| `src/executor/adapter.rs` | Codex integration |
| `src/engine.rs` | Main orchestration loop |

---

## Verification

After each phase, verify:

1. **Phase 1:** `cargo build` succeeds, `nexus --help` works
2. **Phase 2:** Unit tests pass for event log round-trip
3. **Phase 3:** Integration test: mock action → prompt → approval recorded
4. **Phase 4:** Integration test: apply real diff to temp files
5. **Phase 5:** Integration test: call Codex, get patches (or mock)
6. **Phase 6:** E2E test: full refactoring flow on test repo
7. **Phase 7:** Manual testing of replay/resume

---

## Architecture Decisions

1. **Single crate** for v0 (no workspace overhead)
2. **tokio async** for API calls and user prompts; file ops stay sync
3. **thiserror for library, anyhow for CLI** (idiomatic Rust)
4. **Session approvals in-memory only** (reset on exit, safer default)
5. **Unified diff only** (matches Codex output, simpler than JSON patch)
6. **No workflow graphs** (linear execution: prompt → codex → review → apply)
7. **OpenAI API direct** for Codex (standalone, no CLI dependency)
8. **API key from env** (`OPENAI_API_KEY`) or `.nexus/settings.json`
