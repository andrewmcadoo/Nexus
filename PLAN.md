# Plan: Fix 3 Remaining PR #4 Code Review Bugs

## Overview

Fix three bugs identified in PR #4 code review for the Phase 3 Executor module:
1. Retry-After header is parsed but ignored in retry logic
2. Public parsing methods bypass run_id validation (security issue)
3. dry_run option in ExecuteOptions is never checked

---

## Task 1: Respect Retry-After Header in Retry Logic

- **Agent**: backend-api-engineer
- **Scope**: `src/executor/client.rs`
- **Dependencies**: None
- **Token Budget**: ~30k (Focused - 1 file, clear scope)

### Instructions

Fix the retry logic to respect the `Retry-After` header when present.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/executor/client.rs` (full file, 464 lines)
- `/Users/aj/Desktop/Projects/Nexus/src/error.rs` (NexusError::RateLimited definition)

**Problem Analysis:**

The current code at lines 203-207 correctly parses the `Retry-After` header and stores it in `NexusError::RateLimited`, but the retry strategy at lines 262-270 uses fixed exponential backoff, completely ignoring this value.

```rust
// Line 203-207: Retry-After is parsed and stored
let retry_after = parse_retry_after(response.headers());
if status == StatusCode::TOO_MANY_REQUESTS {
    return Err(RetryError::transient(NexusError::RateLimited {
        retry_after,
    }));
}

// Line 262-270: Fixed backoff ignores retry_after
fn build_retry_strategy(max_retries: usize) -> impl Iterator<Item = Duration> {
    // TODO(PR#4): Consider using Retry-After header values...
    ExponentialBackoff::from_millis(RETRY_BASE_MILLIS)
        .factor(RETRY_FACTOR)
        .max_delay(Duration::from_secs(RETRY_MAX_SECS))
        .map(apply_jitter)
        .take(max_retries)
}
```

**Required Changes:**

The `tokio-retry2` crate does not support dynamic delays per-error. We need a different approach:

1. **Option A (Recommended)**: Implement custom retry loop that checks error type and uses `retry_after` duration when `NexusError::RateLimited` is returned.

2. Change `send_with_retry()` to use a manual retry loop instead of `Retry::spawn()`:

```rust
async fn send_with_retry(
    &self,
    request: &ChatCompletionRequest,
) -> Result<reqwest::Response, NexusError> {
    let mut attempts = 0;
    let max_attempts = self.max_retries + 1; // +1 for initial attempt

    loop {
        match self.send_request(request).await {
            Ok(response) => return Ok(response),
            Err(RetryError::Permanent(e)) => return Err(e),
            Err(RetryError::Transient { err, retry_after: _ }) => {
                attempts += 1;
                if attempts >= max_attempts {
                    return Err(err);
                }

                // Use Retry-After from RateLimited error if present
                let delay = match &err {
                    NexusError::RateLimited { retry_after: Some(secs) } => {
                        Duration::from_secs(*secs)
                    }
                    _ => calculate_backoff_delay(attempts),
                };

                tokio::time::sleep(delay).await;
            }
        }
    }
}
```

3. Extract the exponential backoff calculation to a helper function:

```rust
fn calculate_backoff_delay(attempt: usize) -> Duration {
    let base = Duration::from_millis(RETRY_BASE_MILLIS);
    let factor = RETRY_FACTOR.pow(attempt.saturating_sub(1) as u32);
    let delay = base.saturating_mul(factor as u32);
    let capped = delay.min(Duration::from_secs(RETRY_MAX_SECS));
    apply_jitter(capped)
}
```

4. Remove or update the `build_retry_strategy()` function and associated TODO comment.

**Patterns to Follow:**
- Keep jitter application for thundering herd prevention
- Respect max 30 second cap even for Retry-After values
- Log when using Retry-After value (optional, for observability)

**Acceptance Criteria:**
- [ ] When server returns 429 with `Retry-After: 60`, client waits ~60 seconds (not 100ms)
- [ ] When server returns 429 without Retry-After, use exponential backoff as before
- [ ] Other transient errors (timeout, 5xx) still use exponential backoff
- [ ] Max retries limit is still respected
- [ ] Jitter is still applied to calculated backoffs (but NOT to explicit Retry-After)
- [ ] All existing tests pass
- [ ] Add new test for Retry-After header being respected

---

## Task 2: Add run_id Validation to Public Parsing Methods

- **Agent**: backend-api-engineer
- **Scope**: `src/executor/parser.rs`
- **Dependencies**: None
- **Token Budget**: ~20k (Atomic - single file, simple fix)

### Instructions

Add `validate_run_id()` call at the start of both public parsing methods to prevent path traversal attacks.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/executor/parser.rs` (full file, 680 lines)

**Problem Analysis:**

The `parse()` method correctly calls `validate_run_id()` on line 44, but the public methods it delegates to (`parse_unified_diffs` and `parse_search_replace`) do not validate the run_id themselves:

```rust
// Line 43-57: parse() validates, then calls the public methods
pub fn parse(&self, response: &str, run_id: &str) -> Result<Vec<ProposedAction>, NexusError> {
    self.validate_run_id(run_id)?;  // <-- Validated here
    let mut actions = self.parse_unified_diffs(response, run_id);  // <-- No validation inside
    // ...
}

// Line 59-63: PUBLIC method - no validation!
pub fn parse_unified_diffs(&self, response: &str, run_id: &str) -> Vec<ProposedAction> {
    let normalized = normalize_line_endings(response);
    let diffs = self.collect_unified_diffs(&normalized);
    self.build_patch_actions_from_diffs(diffs, run_id)  // run_id used in action IDs
}

// Line 65-69: PUBLIC method - no validation!
pub fn parse_search_replace(&self, response: &str, run_id: &str) -> Vec<ProposedAction> {
    let normalized = normalize_line_endings(response);
    let blocks = self.collect_search_replace_blocks(&normalized);
    self.build_search_replace_actions(blocks, run_id)  // run_id used in action IDs
}
```

**Security Impact:**

External callers could bypass `parse()` and call these methods directly with malicious run_ids like `../../../etc/passwd`, which would be embedded in action IDs and potentially used in file paths elsewhere.

**Required Changes:**

1. Change the return type of both public methods to `Result<Vec<ProposedAction>, NexusError>`

2. Add `validate_run_id()` call at the start of each:

```rust
pub fn parse_unified_diffs(&self, response: &str, run_id: &str) -> Result<Vec<ProposedAction>, NexusError> {
    self.validate_run_id(run_id)?;
    let normalized = normalize_line_endings(response);
    let diffs = self.collect_unified_diffs(&normalized);
    Ok(self.build_patch_actions_from_diffs(diffs, run_id))
}

pub fn parse_search_replace(&self, response: &str, run_id: &str) -> Result<Vec<ProposedAction>, NexusError> {
    self.validate_run_id(run_id)?;
    let normalized = normalize_line_endings(response);
    let blocks = self.collect_search_replace_blocks(&normalized);
    Ok(self.build_search_replace_actions(blocks, run_id))
}
```

3. Update callers in `parse()` to handle the Result:

```rust
pub fn parse(&self, response: &str, run_id: &str) -> Result<Vec<ProposedAction>, NexusError> {
    self.validate_run_id(run_id)?;

    let mut actions = self.parse_unified_diffs(response, run_id)?;  // Add ?
    if !actions.is_empty() {
        return Ok(actions);
    }

    actions = self.parse_search_replace(response, run_id)?;  // Add ?
    if !actions.is_empty() {
        return Ok(actions);
    }

    self.parse_json_actions(response)
}
```

4. Update existing tests to handle the new Result return type.

**Patterns to Follow:**
- Existing `validate_run_id()` logic (lines 117-130) - reuse exactly
- Return `NexusError::InvalidRunId` for validation failures
- Validate at API boundary (public methods)

**Acceptance Criteria:**
- [ ] `parse_unified_diffs()` returns `Result<Vec<ProposedAction>, NexusError>`
- [ ] `parse_search_replace()` returns `Result<Vec<ProposedAction>, NexusError>`
- [ ] Both methods call `validate_run_id()` before processing
- [ ] Malicious run_ids like `../etc/passwd` are rejected with `InvalidRunId` error
- [ ] All existing tests updated and passing
- [ ] Add new tests for path traversal rejection on public methods

---

## Task 3: Implement dry_run Check or Remove Field

- **Agent**: backend-api-engineer
- **Scope**: `src/executor/mod.rs`, `src/executor/adapter.rs`
- **Dependencies**: None
- **Token Budget**: ~25k (Focused - 2 files, clear scope)

### Instructions

The `dry_run` field in `ExecuteOptions` is defined but never checked. Either implement the check or remove the field.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/src/executor/mod.rs` (ExecuteOptions struct, line 48)
- `/Users/aj/Desktop/Projects/Nexus/src/executor/adapter.rs` (execute methods)

**Problem Analysis:**

```rust
// mod.rs line 46-54
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteOptions {
    pub dry_run: bool,  // <-- Defined but never used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    // ...
}

// adapter.rs - execute_internal() and execute_streaming_internal() ignore dry_run
async fn execute_internal(
    &self,
    task: &str,
    files: &[FileContext],
    options: &ExecuteOptions,  // options.dry_run is never checked
    run_id: &str,
) -> Result<Vec<ProposedAction>, NexusError> {
    let request = self.build_request(task, files, options);
    let stream = self.client.chat_completion_stream(request).await?;  // Always calls API
    // ...
}
```

**Decision: Implement dry_run**

The dry_run feature is useful for:
- Testing action parsing without API calls
- Previewing what would be sent to the API
- Cost estimation

**Required Changes:**

1. Add check at the start of `execute_internal()`:

```rust
async fn execute_internal(
    &self,
    task: &str,
    files: &[FileContext],
    options: &ExecuteOptions,
    run_id: &str,
) -> Result<Vec<ProposedAction>, NexusError> {
    if options.dry_run {
        // In dry_run mode, return empty actions without calling API
        // The caller can inspect the request via logging or other means
        return Ok(Vec::new());
    }

    let request = self.build_request(task, files, options);
    // ... rest of implementation
}
```

2. Add same check to `execute_streaming_internal()`:

```rust
async fn execute_streaming_internal(
    &self,
    task: &str,
    files: &[FileContext],
    options: &ExecuteOptions,
    run_id: &str,
    on_chunk: Box<dyn Fn(StreamChunk) + Send>,
) -> Result<Vec<ProposedAction>, NexusError> {
    if options.dry_run {
        // Signal completion without any actions
        on_chunk(StreamChunk::Done);
        return Ok(Vec::new());
    }

    let request = self.build_request(task, files, options);
    // ... rest of implementation
}
```

3. Update `execute_with_logging()` to log dry_run mode:

```rust
pub async fn execute_with_logging(
    &self,
    task: &str,
    files: &[FileContext],
    options: ExecuteOptions,
    writer: &mut EventLogWriter,
) -> Result<Vec<ProposedAction>, NexusError> {
    let run_id = generate_run_id();
    let started_at = Instant::now();

    // Log that execution started (include dry_run status)
    let started = helpers::executor_started(&run_id, task, files.len(), &self.model);
    writer.append(&started)?;

    if options.dry_run {
        // Log dry run completion with 0 actions
        let completed = helpers::executor_completed(&run_id, 0, 0);
        writer.append(&completed)?;
        writer.sync()?;
        return Ok(Vec::new());
    }

    // ... rest of implementation
}
```

**Alternative: Remove dry_run field**

If dry_run is deemed unnecessary for v0:
1. Remove `dry_run` field from `ExecuteOptions`
2. Update any code that constructs `ExecuteOptions` (tests, etc.)
3. Document that dry_run is not supported

**Recommendation:** Implement dry_run. It's a low-cost feature that adds testing/debugging value.

**Patterns to Follow:**
- Return empty `Vec<ProposedAction>` for dry_run (no fake actions)
- Signal `StreamChunk::Done` in streaming mode
- Log dry_run executions to event log for audit trail

**Acceptance Criteria:**
- [ ] `execute()` returns empty Vec when `dry_run: true`
- [ ] `execute_streaming()` calls `on_chunk(StreamChunk::Done)` and returns empty Vec when `dry_run: true`
- [ ] `execute_with_logging()` logs start and completion events even in dry_run mode
- [ ] No API calls are made when `dry_run: true`
- [ ] All existing tests pass
- [ ] Add new tests verifying dry_run behavior

---

## Task 4: Run Tests and Verify

- **Agent**: tests-qa-engineer
- **Scope**: Full test suite
- **Dependencies**: Tasks 1, 2, 3
- **Token Budget**: ~20k (Focused - verification only)

### Instructions

After all fixes are applied, run the full test suite and verify the changes work correctly.

**Context Files to Read:**
- `/Users/aj/Desktop/Projects/Nexus/tests/executor.rs` (existing executor tests)

**Required Actions:**

1. Run the full test suite:
   ```bash
   cd /Users/aj/Desktop/Projects/Nexus
   cargo test --all
   ```

2. Run clippy to check for any new warnings:
   ```bash
   cargo clippy --all-targets --all-features
   ```

3. Run fmt check:
   ```bash
   cargo fmt --check
   ```

4. If any tests fail, report the failures and work with backend-api-engineer to fix.

**Acceptance Criteria:**
- [ ] All tests pass (should be 104+ tests)
- [ ] No clippy warnings
- [ ] Code is formatted correctly
- [ ] Build succeeds with no warnings

---

## Execution Order

1. **Task 1, 2, 3** (parallel - no dependencies between them)
   - All three bugs are in separate files with no shared code paths
   - Can be fixed simultaneously

2. **Task 4** (after Tasks 1-3)
   - Requires all fixes to be in place before verification

---

## Summary

| Task | Bug | Severity | Files | LOC Estimate |
|------|-----|----------|-------|--------------|
| 1 | Retry-After ignored | Medium | client.rs | ~30 lines changed |
| 2 | Validation bypass | High (Security) | parser.rs | ~15 lines changed |
| 3 | dry_run ignored | Low | mod.rs, adapter.rs | ~20 lines changed |
| 4 | Verification | - | tests | ~5 commands |

**Total Estimated Changes:** ~65 lines of code
**Risk Level:** Low - isolated changes in well-tested module
