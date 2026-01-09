# Phase 2: Event Log Implementation Plan

**Created:** 2026-01-08
**Updated:** 2026-01-08 (Security review fixes applied)
**Goal:** Implement append-only JSONL event logging with atomic writes, cross-platform file locking, and replay/resume capabilities.

---

## Executive Summary

Phase 2 builds the event logging foundation for Nexus. The event log is critical infrastructure that enables:
- **Auditability** - Complete record of what happened during refactoring
- **Replay** - `nexus replay <run_id>` to view timeline of operations
- **Resume** - `nexus resume <run_id>` to continue after crash/interrupt
- **Debugging** - Trace issues back to specific events

### Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Format | JSONL (JSON Lines) | Streamable, human-readable, append-friendly |
| Atomicity | `O_APPEND` + `BufWriter` + `sync_data()` | Prevents partial writes, configurable durability |
| Locking | `fs2` crate (advisory locks) | Cross-platform, OS-managed lifecycle, no stale locks |
| Storage | `.nexus/runs/<run_id>.jsonl` | One file per run, easy to find/delete |
| Durability | Sync on checkpoints, not every write | Balance between safety and performance |

---

## Security Considerations

This plan includes security measures identified during skill-evaluator review:

| Risk | Mitigation | Location |
|------|------------|----------|
| **Path Traversal** | `validate_run_id()` rejects `/`, `\`, `..` | Task 2 |
| **File Permissions** | Unix: 0o600 (owner-only) | Task 3 |
| **Error Handling** | `InvalidRunId` error before file ops | Task 6 |

**Why This Matters:**
- `run_id` is user-controlled (CLI args, resume command)
- Without validation: `nexus resume "../../../etc/cron.d/malicious"` could write anywhere
- With validation: Rejected immediately with clear error message

---

## Research Summary

### 1. JSONL Best Practices

**Key Findings:**
- Each line is a self-contained JSON object terminated by `\n`
- Always terminate last line with newline for Unix tool compatibility
- Use `event_seq` (monotonic counter) for ordering, not just timestamps
- Handle malformed lines gracefully (skip and log, don't abort)

**Nexus Convention:**
```json
{"v":"nexus/1","run_id":"run_001","event_seq":1,"type":"run.started","time":"2026-01-08T12:00:00Z",...}
```

### 2. Atomic File Writes in Rust

**Key Findings:**
- `OpenOptions::new().append(true).create(true)` uses `O_APPEND` flag
- `O_APPEND` guarantees single `write()` calls are atomic (up to ~4KB PIPE_BUF)
- JSONL events are typically <1KB, well within atomic write bounds
- `BufWriter` batches small writes for performance
- `sync_data()` flushes OS cache to disk for durability

**Recommended Pattern:**
```rust
let file = OpenOptions::new()
    .append(true)
    .create(true)
    .open(path)?;
let mut writer = BufWriter::new(file);

// Write event
serde_json::to_writer(&mut writer, &event)?;
writer.write_all(b"\n")?;

// At checkpoints (not every write)
writer.flush()?;
writer.get_ref().sync_data()?;
```

### 3. Cross-Platform File Locking

**Key Findings:**
- `fs2` crate is the de-facto standard (used by Cargo)
- Lock the log file directly, not a separate `.lock` file
- OS automatically releases locks on process crash (no stale lock cleanup needed)
- Use `try_lock_exclusive()` for non-blocking writer access
- Use `lock_shared()` for concurrent reader access

**Locking Strategy:**
| Actor | Lock Type | Behavior |
|-------|-----------|----------|
| EventLogWriter | Exclusive | Fail immediately if locked |
| EventLogReader | Shared | Multiple readers allowed |

### 4. Replay/Resume Patterns

**Key Findings:**
- Use `action_id` (deterministic, based on file+content hash) for idempotency
- `event_seq` enables gap detection and ordering
- Resume = rebuild state from log, find incomplete actions, continue
- "Incomplete" = `action.proposed` without terminal event (`tool.executed`, `tool.failed`, `permission.denied`)

**State Reconstruction Algorithm:**
1. Load all events for `run_id`
2. Verify `event_seq` has no gaps
3. Build `HashMap<action_id, ActionState>`
4. Filter for non-terminal states
5. Re-execute pending actions

### 5. Rust JSONL Patterns

**Key Findings:**
- Use `serde_json::to_writer()` directly to BufWriter (avoids string allocation)
- Use `BufReader::lines()` for streaming reads (memory efficient)
- Return `Iterator<Item = Result<Event>>` for flexible error handling
- `filter_map` pattern for graceful malformed line skipping

---

## Module Structure

```
src/
  event_log/
    mod.rs           # Module exports, EventLogPath helper
    writer.rs        # EventLogWriter - append events atomically
    reader.rs        # EventLogReader - stream/filter events
    helpers.rs       # Convenience functions for common event types
```

---

## Implementation Tasks

### Task 1: Add Dependencies

**Agent:** backend-api-engineer
**Scope:** `Cargo.toml`
**Dependencies:** None

**Instructions:**
Add the `fs2` crate for cross-platform file locking:

```toml
[dependencies]
fs2 = "0.4"

[dev-dependencies]
tempfile = "3"
```

**Acceptance Criteria:**
- [ ] `fs2 = "0.4"` added to dependencies
- [ ] `tempfile = "3"` added to dev-dependencies
- [ ] `cargo build` succeeds

---

### Task 2: Create event_log Module Structure

**Agent:** backend-api-engineer
**Scope:** `src/event_log/mod.rs`, `src/lib.rs`
**Dependencies:** Task 1

**Instructions:**
Create the event_log module with the following structure:

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/lib.rs`
- `/Users/aj/Desktop/Projects/Nexus/src/types/event.rs`

**Requirements:**

1. Create `src/event_log/mod.rs` with:
   - Module declarations for `writer`, `reader`, `helpers`
   - `EventLogPath` struct to manage log file paths
   - Re-exports for public API

2. `EventLogPath` design:
   ```rust
   /// Internal helper for managing event log file paths.
   /// Not exposed in public API.
   pub(crate) struct EventLogPath {
       base_dir: PathBuf,  // .nexus/runs/
   }

   impl EventLogPath {
       pub fn new(project_root: &Path) -> Self;

       /// Returns path to log file for given run_id.
       /// Validates run_id to prevent path traversal attacks.
       /// Returns error if run_id contains '/', '\', or '..'
       pub fn for_run(&self, run_id: &str) -> Result<PathBuf, NexusError>;

       pub fn ensure_dir(&self) -> io::Result<()>;

       /// Validates run_id contains no path traversal characters.
       fn validate_run_id(run_id: &str) -> Result<(), NexusError> {
           if run_id.contains('/') || run_id.contains('\\') || run_id.contains("..") {
               return Err(NexusError::InvalidRunId(run_id.to_string()));
           }
           // Also reject empty or whitespace-only run_ids
           if run_id.trim().is_empty() {
               return Err(NexusError::InvalidRunId(run_id.to_string()));
           }
           Ok(())
       }
   }
   ```

   **Security Note:** The `run_id` parameter is user-controlled (from CLI args or resume commands). Path traversal validation is mandatory to prevent writing to arbitrary filesystem locations.

3. Update `src/lib.rs` to export the module:
   ```rust
   pub mod event_log;
   pub use event_log::{EventLogWriter, EventLogReader};
   ```

**Acceptance Criteria:**
- [ ] `src/event_log/mod.rs` exists with module declarations
- [ ] `EventLogPath` is `pub(crate)` (not in public API)
- [ ] `EventLogPath::for_run("run_123")` returns `Ok(.nexus/runs/run_123.jsonl)`
- [ ] `EventLogPath::for_run("../etc/passwd")` returns `Err(InvalidRunId)`
- [ ] `EventLogPath::for_run("")` returns `Err(InvalidRunId)`
- [ ] `EventLogPath::ensure_dir()` creates `.nexus/runs/` if missing
- [ ] Module exported from `lib.rs`
- [ ] `cargo build` succeeds

---

### Task 3: Implement EventLogWriter

**Agent:** backend-api-engineer
**Scope:** `src/event_log/writer.rs`
**Dependencies:** Task 2

**Instructions:**
Implement the append-only event writer with atomic writes and file locking.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/types/event.rs` (RunEvent struct)
- `/Users/aj/Desktop/Projects/Nexus/src/error.rs` (NexusError)
- `/Users/aj/Desktop/Projects/Nexus/.nexus/test-fixtures/events/sample-run.jsonl` (example format)

**Requirements:**

1. **EventLogWriter struct:**
   ```rust
   pub struct EventLogWriter {
       writer: BufWriter<File>,
       event_seq: u64,  // Monotonic counter
   }
   ```

2. **Constructor:**
   ```rust
   impl EventLogWriter {
       /// Opens log file for run_id, creates if not exists
       /// Acquires exclusive lock (fails immediately if locked)
       /// Reads existing file to determine starting event_seq
       pub fn open(path: &Path) -> Result<Self, NexusError>;
   }
   ```

   **Performance Note:** Opening an existing log file requires scanning all events to find the maximum `event_seq`. This is O(n) where n is the number of events. For typical runs (<1000 events), this takes <10ms. If this becomes a bottleneck in future versions, consider storing `event_seq` in a separate `.meta` file.

   **Convention:** `event_seq` starts at 1 (not 0) for human readability in logs.

3. **Write method:**
   ```rust
   /// Appends event to log, assigns next event_seq
   /// Does NOT sync to disk (call sync() for durability)
   pub fn append(&mut self, event: &mut RunEvent) -> Result<(), NexusError>;
   ```

4. **Sync method:**
   ```rust
   /// Flushes buffer and syncs to disk
   /// Call at checkpoints (command complete, before exit)
   pub fn sync(&mut self) -> Result<(), NexusError>;
   ```

5. **Drop implementation:**
   - Flush buffer (ignore errors in drop)
   - Release exclusive lock via `fs2::FileExt::unlock()`

6. **Error handling:**
   - Lock acquisition failure: Return `NexusError::EventLogLocked`
   - IO errors: Wrap in `NexusError::Io`
   - Serialization errors: Wrap in `NexusError::Serialization`

7. **Patterns to follow:**
   - Use `OpenOptions::new().append(true).create(true)`
   - Use `fs2::FileExt::try_lock_exclusive()` for non-blocking lock
   - Use `serde_json::to_writer()` directly to BufWriter
   - Always append `\n` after each JSON object
   - Use `sync_data()` not `sync_all()` (metadata sync not needed)

8. **Unix file permissions (security):**
   ```rust
   #[cfg(unix)]
   use std::os::unix::fs::OpenOptionsExt;

   let mut opts = OpenOptions::new();
   opts.append(true).create(true);

   #[cfg(unix)]
   opts.mode(0o600);  // Owner read/write only

   let file = opts.open(path)?;
   ```

   **Rationale:** Event logs may contain sensitive information (file paths, action summaries). Restricting to owner-only access (0o600) prevents other users on shared systems from reading logs.

**Acceptance Criteria:**
- [ ] `EventLogWriter::open()` creates file if missing
- [ ] `EventLogWriter::open()` fails with `EventLogLocked` if already locked
- [ ] `append()` writes valid JSONL (one JSON object per line)
- [ ] `append()` increments `event_seq` automatically (starting from 1)
- [ ] `sync()` persists data to disk
- [ ] Lock released on drop
- [ ] On Unix: new files created with 0o600 permissions
- [ ] Unit tests pass

---

### Task 4: Implement EventLogReader

**Agent:** backend-api-engineer
**Scope:** `src/event_log/reader.rs`
**Dependencies:** Task 2

**Instructions:**
Implement the event log reader with streaming iteration and filtering.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/types/event.rs` (RunEvent struct)
- `/Users/aj/Desktop/Projects/Nexus/src/error.rs` (NexusError)

**Requirements:**

1. **EventLogReader struct:**
   ```rust
   pub struct EventLogReader {
       reader: BufReader<File>,
   }
   ```

2. **Constructor:**
   ```rust
   impl EventLogReader {
       /// Opens log file for reading with shared lock
       /// Shared lock allows multiple readers, blocks if writer has exclusive lock
       pub fn open(path: &Path) -> Result<Self, NexusError>;
   }
   ```

3. **Iterator method:**
   ```rust
   /// Returns iterator over events, parsing each line
   /// Malformed lines yield Err, caller decides to skip or abort
   pub fn iter(&mut self) -> impl Iterator<Item = Result<RunEvent, NexusError>> + '_;
   ```

4. **Load all method:**
   ```rust
   /// Loads all events into memory (for resume/replay operations)
   /// Skips malformed lines with warning to stderr
   pub fn load_all(&mut self) -> Result<Vec<RunEvent>, NexusError>;
   ```

5. **Filter helpers:**
   ```rust
   /// Filter events by run_id
   pub fn filter_by_run<'a>(
       events: impl Iterator<Item = Result<RunEvent, NexusError>> + 'a,
       run_id: &'a str,
   ) -> impl Iterator<Item = Result<RunEvent, NexusError>> + 'a;

   /// Filter events by event_type
   pub fn filter_by_type<'a>(
       events: impl Iterator<Item = Result<RunEvent, NexusError>> + 'a,
       event_type: &'a str,
   ) -> impl Iterator<Item = Result<RunEvent, NexusError>> + 'a;
   ```

6. **Error handling:**
   - File not found: Return `NexusError::EventLogNotFound`
   - Parse errors: Include line number in error message
   - IO errors: Wrap in `NexusError::Io`

7. **Patterns to follow:**
   - Use `BufReader::lines()` for streaming
   - Use `fs2::FileExt::lock_shared()` for concurrent reader access
   - Handle empty lines gracefully (skip)
   - Use `enumerate()` to track line numbers for error messages

**Acceptance Criteria:**
- [ ] `EventLogReader::open()` acquires shared lock
- [ ] `iter()` streams events without loading full file
- [ ] `load_all()` returns Vec of all valid events
- [ ] Malformed lines don't crash iterator
- [ ] Filter helpers work correctly
- [ ] Unit tests pass

---

### Task 5: Implement Helper Functions

**Agent:** backend-api-engineer
**Scope:** `src/event_log/helpers.rs`
**Dependencies:** Task 3

**Instructions:**
Implement convenience functions for creating common event types.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/types/event.rs` (RunEvent, Actor)
- `/Users/aj/Desktop/Projects/Nexus/.nexus/test-fixtures/events/sample-run.jsonl` (event types)

**Requirements:**

Create factory functions that return properly structured `RunEvent` instances:

```rust
/// Creates run.started event
pub fn run_started(run_id: &str, task: &str) -> RunEvent;

/// Creates run.completed event
pub fn run_completed(run_id: &str, status: &str, actions_applied: u32) -> RunEvent;

/// Creates action.proposed event
pub fn action_proposed(
    run_id: &str,
    action_id: &str,
    kind: &str,
    summary: &str,
    actor: Option<Actor>,
) -> RunEvent;

/// Creates permission.granted event
pub fn permission_granted(run_id: &str, action_id: &str, scope: &str) -> RunEvent;

/// Creates permission.denied event
pub fn permission_denied(run_id: &str, action_id: &str, reason: &str) -> RunEvent;

/// Creates tool.executed event (success)
pub fn tool_executed(
    run_id: &str,
    action_id: &str,
    files_modified: Vec<String>,
) -> RunEvent;

/// Creates tool.failed event
pub fn tool_failed(run_id: &str, action_id: &str, error: &str) -> RunEvent;
```

**Patterns:**
- Use `RunEvent::new(run_id, event_type)` constructor
- Use `.with_payload()` builder method for payload
- Use `serde_json::json!()` macro for payload construction
- Set `actor` appropriately (tool events use `agent: "tool"`)

**Acceptance Criteria:**
- [ ] All 7 helper functions implemented
- [ ] Generated events match format in `sample-run.jsonl`
- [ ] Events serialize/deserialize correctly (round-trip test)
- [ ] Unit tests for each helper

---

### Task 5.1: Add Doc Tests (Optional Enhancement)

**Agent:** backend-api-engineer
**Scope:** `src/event_log/writer.rs`, `src/event_log/reader.rs`
**Dependencies:** Tasks 3, 4
**Priority:** Low (enhancement, not blocking)

**Instructions:**
Add `/// # Examples` doc tests for public API methods to improve documentation and ensure examples stay in sync with implementation.

**Requirements:**

Add doc tests to:
```rust
/// Opens a new event log for writing.
///
/// # Examples
///
/// ```
/// use nexus::event_log::EventLogWriter;
/// use tempfile::TempDir;
///
/// let dir = TempDir::new().unwrap();
/// let path = dir.path().join("test.jsonl");
/// let writer = EventLogWriter::open(&path).unwrap();
/// ```
pub fn open(path: &Path) -> Result<Self, NexusError>;
```

Add similar examples for:
- `EventLogWriter::append()`
- `EventLogWriter::sync()`
- `EventLogReader::open()`
- `EventLogReader::iter()`
- `EventLogReader::load_all()`

**Acceptance Criteria:**
- [ ] Doc tests compile and pass (`cargo test --doc`)
- [ ] Examples demonstrate typical usage patterns
- [ ] Examples handle errors appropriately

---

### Task 6: Add Error Variants

**Agent:** backend-api-engineer
**Scope:** `src/error.rs`
**Dependencies:** None (can run in parallel with Task 2)

**Instructions:**
Add event log specific error variants to NexusError.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/error.rs`

**Requirements:**

Add these variants to `NexusError`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum NexusError {
    // ... existing variants ...

    #[error("invalid run_id: {0}")]
    InvalidRunId(String),

    #[error("event log is locked by another process")]
    EventLogLocked,

    #[error("event log not found: {0}")]
    EventLogNotFound(PathBuf),

    #[error("event log corrupted at line {line}: {message}")]
    EventLogCorrupted { line: usize, message: String },

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
```

Update `exit_code()` method:
- `InvalidRunId` -> 64 (EX_USAGE - command line usage error)
- `EventLogLocked` -> 75 (EX_TEMPFAIL - temporary failure, retry)
- `EventLogNotFound` -> 66 (EX_NOINPUT)
- `EventLogCorrupted` -> 65 (EX_DATAERR)
- `Serialization` -> 65 (EX_DATAERR)

**Security Note:** `InvalidRunId` prevents path traversal attacks. Any run_id containing `/`, `\`, or `..` is rejected before file operations occur.

**Acceptance Criteria:**
- [ ] All 5 error variants added
- [ ] Exit codes mapped correctly
- [ ] Error messages are clear and actionable
- [ ] `cargo build` succeeds

---

### Task 7: Integration Tests

**Agent:** tests-qa-engineer
**Scope:** `tests/event_log.rs`
**Dependencies:** Tasks 3, 4, 5, 6

**Instructions:**
Write comprehensive integration tests for the event log module.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/tests/integration.rs` (existing test patterns)
- `/Users/aj/Desktop/Projects/Nexus/.nexus/test-fixtures/events/sample-run.jsonl`

**Requirements:**

1. **Writer tests:**
   ```rust
   #[test]
   fn test_writer_creates_file()

   #[test]
   fn test_writer_appends_valid_jsonl()

   #[test]
   fn test_writer_increments_event_seq()

   #[test]
   fn test_writer_exclusive_lock()

   #[test]
   fn test_writer_sync_persists_data()
   ```

2. **Reader tests:**
   ```rust
   #[test]
   fn test_reader_iterates_events()

   #[test]
   fn test_reader_handles_malformed_lines()

   #[test]
   fn test_reader_load_all()

   #[test]
   fn test_reader_filter_by_run()

   #[test]
   fn test_reader_filter_by_type()
   ```

3. **Round-trip tests:**
   ```rust
   #[test]
   fn test_write_then_read_roundtrip()

   #[test]
   fn test_multiple_events_ordering()
   ```

4. **Concurrent access tests:**
   ```rust
   #[test]
   fn test_multiple_readers_concurrent()

   #[test]
   fn test_writer_blocks_writer()
   ```

5. **Helper tests:**
   ```rust
   #[test]
   fn test_helper_run_started()

   #[test]
   fn test_helper_action_proposed()
   // ... test each helper
   ```

**Patterns:**
- Use `tempfile::TempDir` for isolated test directories
- Use `#[cfg(test)]` and `mod tests` for organization
- Clean up after each test
- Test both success and error paths

**Acceptance Criteria:**
- [ ] All tests pass with `cargo test`
- [ ] Tests cover happy path and error cases
- [ ] Concurrent access tests verify locking behavior
- [ ] Round-trip tests verify serialization integrity
- [ ] Test coverage for all public API methods

---

### Task 8: Add Test Fixture Validation

**Agent:** tests-qa-engineer
**Scope:** `tests/event_log.rs`
**Dependencies:** Task 7

**Instructions:**
Add tests that validate against the existing test fixture.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/.nexus/test-fixtures/events/sample-run.jsonl`

**Requirements:**

```rust
#[test]
fn test_parse_sample_fixture() {
    // Read .nexus/test-fixtures/events/sample-run.jsonl
    // Parse each line as RunEvent
    // Verify all 5 events parse successfully
    // Verify event types match expected
}

#[test]
fn test_fixture_event_ordering() {
    // Verify events are in chronological order
    // Verify event_type sequence: run.started -> action.proposed ->
    //   permission.granted -> tool.executed -> run.completed
}
```

**Acceptance Criteria:**
- [ ] Fixture file parses without errors
- [ ] All event types recognized
- [ ] Ordering verified

---

## Execution Order

```
Task 1: Add Dependencies (no deps)
    |
    v
Task 2: Module Structure (after Task 1)
    |
    +---> Task 6: Error Variants (parallel, no deps)
    |
    v
Task 3: EventLogWriter (after Task 2)
    |
Task 4: EventLogReader (after Task 2, parallel with Task 3)
    |
    v
Task 5: Helper Functions (after Task 3)
    |
    +---> Task 5.1: Doc Tests (optional, after Tasks 3, 4)
    |
    v
Task 7: Integration Tests (after Tasks 3, 4, 5, 6)
    |
    v
Task 8: Fixture Validation (after Task 7)
```

**Parallel Execution Groups:**
1. Task 1, Task 6 (can run in parallel)
2. Task 3, Task 4 (can run in parallel after Task 2)
3. Task 5, Task 5.1 (Task 5.1 is optional enhancement)
4. Task 7, Task 8 (sequential, after all implementation)

---

## Verification Checklist

After Phase 2 completion:

1. **Build Verification:**
   - [ ] `cargo build` succeeds with no warnings
   - [ ] `cargo clippy` passes
   - [ ] `cargo fmt --check` passes

2. **Test Verification:**
   - [ ] `cargo test` - all tests pass
   - [ ] Event log tests specifically pass
   - [ ] Fixture validation passes

3. **Manual Testing:**
   - [ ] Create new log file, write events, verify JSONL format
   - [ ] Read log file back, verify all events parsed
   - [ ] Test concurrent writer detection (run two processes)
   - [ ] Test crash recovery (kill writer, verify no corruption)

4. **Integration Points:**
   - [ ] `EventLogWriter` usable from main CLI
   - [ ] `EventLogReader` supports future `nexus replay` command
   - [ ] Helper functions match expected event schema

---

## Dependencies Added

```toml
[dependencies]
fs2 = "0.4"  # Cross-platform file locking

[dev-dependencies]
tempfile = "3"  # Temporary directories for tests
```

No other new dependencies needed. Existing `serde`, `serde_json`, `chrono` are sufficient.

---

## Files Created/Modified

| File | Action | Purpose |
|------|--------|---------|
| `Cargo.toml` | Modified | Add fs2 dependency |
| `src/lib.rs` | Modified | Export event_log module |
| `src/error.rs` | Modified | Add EventLog* error variants |
| `src/event_log/mod.rs` | Created | Module structure, EventLogPath |
| `src/event_log/writer.rs` | Created | EventLogWriter implementation |
| `src/event_log/reader.rs` | Created | EventLogReader implementation |
| `src/event_log/helpers.rs` | Created | Event factory functions |
| `tests/event_log.rs` | Created | Integration tests |

---

## Architecture Notes

### Why Not a Database?

For v0, a simple JSONL file per run is optimal:
- No external dependencies (SQLite, etc.)
- Human-readable and debuggable
- Easy to backup/restore
- Sufficient for CLI tool scale (thousands of events, not millions)

### Future Considerations (Not in v0)

- **Log Rotation:** Not needed for v0 (each run is separate file)
- **Compression:** Could add `.jsonl.gz` for archived runs
- **Indexing:** Could add `.jsonl.idx` for fast lookup by action_id
- **Centralized Logging:** Could send events to external service

### Storage Location

Events stored in project directory: `.nexus/runs/<run_id>.jsonl`

This keeps logs with the project, making them:
- Easy to find
- Included in project backups
- Deletable with project

---

## Research Reports

Full research findings archived in this plan. Key sources:
- Gemini 2.5 Pro analysis of JSONL best practices
- Gemini 2.5 Pro analysis of Rust atomic file writes
- Gemini 2.5 Pro analysis of cross-platform file locking
- Gemini 2.5 Pro analysis of replay/resume patterns
- Gemini 2.5 Pro analysis of Rust JSONL crate ecosystem

---

## Ready for Implementation

This plan is complete and ready for execution. Each task has:
- Clear scope and dependencies
- Context files to read
- Specific requirements
- Acceptance criteria

Estimated implementation time: 2-3 hours for experienced Rust developer.
