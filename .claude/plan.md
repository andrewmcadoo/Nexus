# Plan: Fix PR #3 Bug Reviews

## Overview

Fix 7 verified bugs from PR #3 code review on the `feature/phase1-foundation` branch. Bugs range from CRITICAL (hardcoded paths) to LOW (doctest issues). After fixes, verify with `cargo test` and `cargo clippy -- -D warnings`.

---

## Task 1: Remove Debug Logging Functions (CRITICAL)

- **Agent**: backend-api-engineer
- **Scope**: `src/main.rs`, `src/settings.rs`
- **Dependencies**: None
- **Token Budget**: ~30k (Focused - 2 files, clear scope)

### Instructions

Remove temporary debug logging code that contains hardcoded absolute paths breaking portability.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/main.rs` (lines 103-191)
- `/Users/aj/Desktop/Projects/Nexus/src/settings.rs` (lines 232-277)

**Requirements:**
1. Remove the `debug_log` function from `src/main.rs` (lines 103-148)
2. Remove the `debug_log_probe` function from `src/main.rs` (lines 150-191)
3. Remove all calls to `debug_log` and `debug_log_probe` in `src/main.rs`:
   - Line 20: `debug_log_probe("main.entry");`
   - Lines 40-48: `debug_log(...)` call
   - Lines 53-62: `debug_log(...)` call
   - Lines 77-87: `debug_log(...)` call
4. Remove the `debug_log` function from `src/settings.rs` (lines 232-277)
5. Remove all calls to `debug_log` in `src/settings.rs`:
   - Lines 91-99: `debug_log(...)` call in `load_settings`
   - Lines 103-111: `debug_log(...)` call in `load_settings`
   - Lines 124-132: `debug_log(...)` call in `load_settings_with_preference`
   - Lines 136-143: `debug_log(...)` call in `load_settings_with_preference`
   - Lines 161-169: `debug_log(...)` call in `load_from_file`
   - Lines 189-197: `debug_log(...)` call in `load_from_file`
6. Remove unused imports that were only needed for debug logging:
   - `src/main.rs`: Remove `chrono::Utc`, `serde_json::json`, `std::fs::{self, OpenOptions}`, `std::io::Write`, `std::path::Path` if no longer needed
   - `src/settings.rs`: Remove `chrono::Utc`, `std::fs::OpenOptions`, `std::io::Write` if no longer needed

**Patterns to Follow:**
- Keep existing `log::info!`, `log::debug!` calls (they use env_logger, not the removed debug_log)
- Do not add any replacement logging - this was temporary debug code

**Acceptance Criteria:**
- [ ] No `debug_log` or `debug_log_probe` functions exist in codebase
- [ ] No calls to these functions exist
- [ ] No hardcoded paths like `/Users/aj/` exist in source code
- [ ] `cargo build` succeeds
- [ ] `cargo clippy -- -D warnings` passes (no unused import warnings)

---

## Task 2: Fix Config Loading to Fall Back to Defaults (HIGH)

- **Agent**: backend-api-engineer
- **Scope**: `src/settings.rs`, `src/main.rs`
- **Dependencies**: Task 1 (debug logging removal simplifies the code)
- **Token Budget**: ~30k (Focused - 2 files, clear logic change)

### Instructions

Fix config loading so that missing config file falls back to defaults instead of erroring.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/settings.rs` (focus on `load_settings_with_preference`)
- `/Users/aj/Desktop/Projects/Nexus/src/main.rs` (line 70-72 - call site)

**Problem:**
Currently `load_with_config_path` errors if the config file doesn't exist. This breaks normal operation when no config file is present.

**Requirements:**
1. Modify `load_settings_with_preference` in `src/settings.rs` to:
   - If config file exists: load it (current behavior)
   - If config file does NOT exist: fall back to `NexusSettings::default()` with `None` for `settings_path`
   - Do NOT error on missing file
2. Update the docstring for `load_with_config_path` to document the fallback behavior

**Patterns to Follow (from CLAUDE-patterns.md):**
```rust
pub fn load_settings(project_root: &Path) -> NexusSettings {
    let path = project_root.join(".nexus/settings.json");
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => NexusSettings::default(),  // Silent fallback
    }
}
// Never auto-create settings file
```

**Acceptance Criteria:**
- [ ] Running `nexus` without a config file uses defaults (no error)
- [ ] Running `nexus` with an existing config file loads it
- [ ] Running `nexus` with an invalid JSON config file errors (NOT silently fallback)
- [ ] `cargo test` passes
- [ ] Documentation reflects new behavior

---

## Task 3: Add debug.log to .gitignore and Remove from Git Cache (MEDIUM)

- **Agent**: devops-infra-engineer
- **Scope**: `.gitignore`, git operations
- **Dependencies**: None (can run in parallel with Task 1)
- **Token Budget**: ~10k (Atomic - single file + git command)

### Instructions

Add debug.log to .gitignore and remove it from git tracking if committed.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/.gitignore`

**Requirements:**
1. Add entries to `.gitignore`:
   ```
   # Debug logs
   debug.log
   .cursor/debug.log
   ```
2. Check if `.cursor/debug.log` exists in the repo and remove from git cache:
   ```bash
   git rm --cached .cursor/debug.log 2>/dev/null || true
   git rm --cached debug.log 2>/dev/null || true
   ```
3. Do NOT delete the actual files (just untrack them)

**Patterns to Follow:**
- Keep existing `.gitignore` organization with comment headers
- Place debug entries in a logical section

**Acceptance Criteria:**
- [ ] `.gitignore` contains `debug.log` and `.cursor/debug.log` entries
- [ ] `git status` shows `.gitignore` as modified
- [ ] If debug files were tracked, they are now untracked
- [ ] Debug files are NOT deleted from disk

---

## Task 4: Fix Empty PathBuf in EventLogWriter Errors (LOW)

- **Agent**: backend-api-engineer
- **Scope**: `src/event_log/writer.rs`
- **Dependencies**: None
- **Token Budget**: ~20k (Atomic - single file, 3 locations)

### Instructions

Fix EventLogWriter to use actual file path in error messages instead of empty `PathBuf::new()`.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/event_log/writer.rs` (lines 130-160)

**Problem:**
Lines 135, 147, 156 use `PathBuf::new()` in `NexusError::IoError` which provides no useful context for debugging.

**Requirements:**
1. Store the log file path in `EventLogWriter` struct:
   ```rust
   pub struct EventLogWriter {
       writer: BufWriter<File>,
       event_seq: u64,
       path: PathBuf,  // Add this field
   }
   ```
2. Update `open()` to store the path
3. Update error handling in `append()` (line 135) to use `self.path.clone()`
4. Update error handling in `sync()` (lines 147, 156) to use `self.path.clone()`

**Patterns to Follow:**
- Match existing `IoError` pattern used in `open_file` and `scan_max_event_seq`

**Acceptance Criteria:**
- [ ] `EventLogWriter` stores path
- [ ] All `IoError` in writer.rs use actual path
- [ ] No `PathBuf::new()` in error paths
- [ ] Existing tests pass
- [ ] `cargo clippy -- -D warnings` passes

---

## Task 5: Fix Run ID Length Validation (OFF-BY-6) (LOW)

- **Agent**: backend-api-engineer
- **Scope**: `src/event_log/mod.rs`
- **Dependencies**: None
- **Token Budget**: ~10k (Atomic - single line change)

### Instructions

Fix run_id length validation to account for `.jsonl` extension (6 characters).

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/event_log/mod.rs` (line 60)

**Problem:**
`run_id.len() > 255` allows 255 chars, but filename is `{run_id}.jsonl`. Most filesystems limit filenames to 255 bytes total. A 255-char run_id produces a 261-char filename.

**Requirements:**
1. Change line 60 from `run_id.len() > 255` to `run_id.len() > 249`
2. Update the error message to explain the limit:
   ```rust
   "run_id exceeds 249 characters (filename limit with .jsonl extension)"
   ```
3. Update test in `test_event_log_path_rejects_overlong`:
   - `let ok = "a".repeat(249);` (was 255)
   - `let too_long = "a".repeat(250);` (was 256)

**Acceptance Criteria:**
- [ ] `run_id` max length is 249 characters
- [ ] Error message explains the reason
- [ ] Test uses correct boundary values
- [ ] All tests pass

---

## Task 6: Handle Corrupted Lines in Writer's scan_max_event_seq (LOW)

- **Agent**: backend-api-engineer
- **Scope**: `src/event_log/writer.rs`
- **Dependencies**: None
- **Token Budget**: ~20k (Focused - single function change + test)

### Instructions

Fix `scan_max_event_seq` to skip corrupted JSON lines instead of failing.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/event_log/writer.rs` (line 103)
- `/Users/aj/Desktop/Projects/Nexus/src/event_log/reader.rs` (for pattern reference - reader already skips bad lines)

**Problem:**
Line 103 uses `serde_json::from_str(&line)?` which propagates errors on invalid JSON. This prevents opening a log file that has any corrupted lines. The reader handles this gracefully by logging warnings and skipping bad lines.

**Requirements:**
1. Change `scan_max_event_seq` to skip invalid JSON lines with a warning:
   ```rust
   let value: serde_json::Value = match serde_json::from_str(&line) {
       Ok(v) => v,
       Err(e) => {
           log::warn!("Skipping corrupted line in event log: {}", e);
           continue;
       }
   };
   ```
2. Add test for corrupted line handling:
   ```rust
   #[test]
   fn test_writer_handles_corrupted_line_on_reopen() {
       let dir = TempDir::new().unwrap();
       let path = dir.path().join("test.jsonl");

       // Write valid line, corrupted line, valid line
       std::fs::write(&path,
           "{\"event_seq\":1}\n\
            not valid json\n\
            {\"event_seq\":3}\n"
       ).unwrap();

       let writer = EventLogWriter::open(&path).unwrap();
       assert_eq!(writer.next_seq(), 4);
   }
   ```

**Patterns to Follow (from reader.rs):**
```rust
// Reader skips bad lines:
if let Ok(event) = serde_json::from_str::<RunEvent>(&line) {
    // process
}
```

**Acceptance Criteria:**
- [ ] Corrupted lines are skipped with log warning
- [ ] Valid lines after corruption are still processed
- [ ] `next_seq` correctly reflects max seq from valid lines
- [ ] New test covers corrupted line scenario
- [ ] All tests pass

---

## Task 7: Remove Doctests from Private Functions (LOW)

- **Agent**: backend-api-engineer
- **Scope**: `src/types/action.rs`
- **Dependencies**: None
- **Token Budget**: ~15k (Atomic - documentation-only changes)

### Instructions

Remove doc examples from private functions that cannot compile as doctests.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/types/action.rs` (lines 65-87, 233-244, 302-315)

**Problem:**
Private functions (`default_risk`, `default_true`, `default_timeout`, `is_default`) have `# Examples` doc blocks. Rust doc tests compile as external crate code, so they cannot access private functions.

**Requirements:**
1. Remove the `# Examples` section from `default_risk()` (lines 69-73)
2. Remove the `# Examples` section from `default_true()` (lines 82-86)
3. Remove the `# Examples` section from `default_timeout()` (lines 237-242)
4. Remove the `# Examples` section from `is_default()` (lines 306-312)
5. Keep the non-example documentation (function descriptions)

**Before:**
```rust
/// Provides the default risk level for actions.
///
/// The default risk level is 1.
///
/// # Examples
///
/// ```
/// assert_eq!(default_risk(), 1);
/// ```
fn default_risk() -> u8 {
```

**After:**
```rust
/// Provides the default risk level for actions.
///
/// The default risk level is 1.
fn default_risk() -> u8 {
```

**Patterns to Follow (from CLAUDE-patterns.md):**
> Private functions Cannot Have Doc Examples. Only put `# Examples` doc blocks on `pub` items.

**Acceptance Criteria:**
- [ ] No `# Examples` sections on private functions
- [ ] Function descriptions preserved
- [ ] `cargo test --doc` passes
- [ ] `cargo doc` builds without warnings

---

## Task 8: Verification and Testing

- **Agent**: tests-qa-engineer
- **Scope**: Full project
- **Dependencies**: Tasks 1-7 (all fixes complete)
- **Token Budget**: ~30k (Bounded - full test suite + clippy)

### Instructions

Verify all fixes pass tests and linting.

**Requirements:**
1. Run `cargo test` - all tests must pass
2. Run `cargo clippy -- -D warnings` - no warnings allowed
3. Run `cargo doc` - verify documentation builds
4. Run `cargo build --release` - verify release build
5. Verify no hardcoded paths remain:
   ```bash
   grep -r "/Users/aj" src/ --include="*.rs" || echo "No hardcoded paths found"
   ```

**Acceptance Criteria:**
- [ ] `cargo test` passes (104+ tests)
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo doc` builds without warnings
- [ ] `cargo build --release` succeeds
- [ ] No `/Users/aj` strings in source code

---

## Execution Order

1. **Task 1** (Remove debug logging) - Start immediately
2. **Task 3** (gitignore) - Parallel with Task 1
3. **Task 2** (Config fallback) - After Task 1
4. **Task 4** (PathBuf fix) - Parallel with Task 2
5. **Task 5** (Run ID length) - Parallel with Task 2
6. **Task 6** (Corrupted lines) - Parallel with Task 2
7. **Task 7** (Doctests) - Parallel with Task 2
8. **Task 8** (Verification) - After all fixes

```
Task 1 ─────────────────────┬─> Task 2 ─┬─> Task 8
                            │           │
Task 3 ─────────────────────┤           │
                            │   Task 4 ─┤
                            │   Task 5 ─┤
                            │   Task 6 ─┤
                            │   Task 7 ─┘
```

---

## Summary

| Task | Priority | Agent | Files | Est. Tokens |
|------|----------|-------|-------|-------------|
| 1 | CRITICAL | backend-api-engineer | main.rs, settings.rs | 30k |
| 2 | HIGH | backend-api-engineer | settings.rs, main.rs | 30k |
| 3 | MEDIUM | devops-infra-engineer | .gitignore | 10k |
| 4 | LOW | backend-api-engineer | writer.rs | 20k |
| 5 | LOW | backend-api-engineer | mod.rs | 10k |
| 6 | LOW | backend-api-engineer | writer.rs | 20k |
| 7 | LOW | backend-api-engineer | action.rs | 15k |
| 8 | VERIFY | tests-qa-engineer | full project | 30k |

**Total Estimated Tokens:** ~165k across 8 tasks
