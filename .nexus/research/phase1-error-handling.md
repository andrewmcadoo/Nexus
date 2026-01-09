# Phase 1 Research: Error Handling with thiserror and anyhow

**Date:** 2026-01-08
**Researcher:** research-specialist
**Confidence:** High
**Relevant ADR:** ADR-003 (Use thiserror for library errors, anyhow for CLI main)

---

## 1. Summary of thiserror + anyhow Pattern

The Rust ecosystem has converged on a two-crate pattern for error handling:

| Crate | Purpose | Use Case |
|-------|---------|----------|
| **thiserror** | Derive macro for `std::error::Error` | Library code, typed errors, when callers need to match on variants |
| **anyhow** | Flexible error type with context | Application code, CLI main, when errors are reported not recovered |

**Core Philosophy:**
- Use `thiserror` when callers need to **handle** errors differently based on type
- Use `anyhow` when errors are **reported** to users/operators (not programmatically inspected)
- The two work together: `NexusError` (thiserror) converts to `anyhow::Error` at CLI boundary

**Recommended Dependencies (2025/2026):**
```toml
thiserror = "2"
anyhow = "1"
```

---

## 2. Concrete NexusError Enum

Based on the planned error types and best practices, here is the recommended implementation:

```rust
// src/error.rs
use std::path::PathBuf;
use thiserror::Error;

/// Core error type for Nexus library operations.
///
/// Callers can match on variants to handle specific failure modes.
/// At CLI boundary, these convert to `anyhow::Error` for reporting.
#[derive(Error, Debug)]
pub enum NexusError {
    /// Action blocked by permission policy
    #[error("permission denied: {action}")]
    PermissionDenied {
        action: String,
        #[source]
        reason: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Unified diff or search-replace patch failed to apply
    #[error("patch failed for {path}: {reason}")]
    PatchFailed {
        path: PathBuf,
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Settings file parse or validation error
    #[error("configuration error: {message}")]
    ConfigError {
        message: String,
        path: Option<PathBuf>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Codex API call failed
    #[error("API error: {message}")]
    ApiError {
        message: String,
        status_code: Option<u16>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// File read/write operation failed
    #[error("I/O error: {operation} on {}", path.display())]
    IoError {
        operation: String,
        path: PathBuf,
        #[from]
        #[source]
        source: std::io::Error,
    },

    /// Invalid input (path validation, malformed request, etc.)
    #[error("validation error: {message}")]
    ValidationError {
        message: String,
        field: Option<String>,
    },

    /// JSON serialization/deserialization failed
    #[error("JSON error: {context}")]
    JsonError {
        context: String,
        #[from]
        #[source]
        source: serde_json::Error,
    },

    /// Path security violation (traversal, absolute path, symlink escape)
    #[error("path rejected: {path} - {reason}")]
    PathRejected {
        path: String,
        reason: String,
    },
}

/// Result type alias for Nexus operations
pub type NexusResult<T> = Result<T, NexusError>;
```

### Design Decisions Explained

1. **Structured variants over `#[error(transparent)]`**: Each variant includes contextual fields (path, operation, message) making errors self-documenting.

2. **`Option<Box<dyn Error>>` for source**: Allows attaching underlying errors without requiring them, useful when constructing errors programmatically.

3. **Separate `PathRejected` from `ValidationError`**: Security-related path validation is distinct from general input validation for clearer audit trails.

4. **`IoError` uses `#[from]`**: Single unambiguous source type makes automatic conversion safe.

5. **`JsonError` separated from `ConfigError`**: JSON parsing can fail in many contexts (API responses, event logs), not just config files.

---

## 3. #[from] vs Manual Conversion

### When to Use `#[from]`

Use `#[from]` when:
- There is exactly **one variant** for that source error type
- The conversion is **unambiguous** (one source type maps to one variant)
- The variant contains **only** the source error (plus optional backtrace)

```rust
// GOOD: Single unambiguous source
#[error("I/O error")]
IoError {
    #[from]
    source: std::io::Error,
},

#[error("JSON parsing failed")]
JsonError {
    #[from]
    source: serde_json::Error,
},
```

### When to Use Manual Conversion (map_err)

Use `map_err()` when:
- **Multiple variants** could contain the same source type
- You need to **add context** (file path, operation name)
- The **meaning differs** based on where the error occurred

```rust
// PROBLEM: Can't have #[from] on both!
#[error("failed to read config")]
ConfigReadError(std::io::Error),

#[error("failed to write event log")]
LogWriteError(std::io::Error),

// SOLUTION: Use map_err with context
fn load_config(path: &Path) -> NexusResult<Config> {
    let content = fs::read_to_string(path)
        .map_err(|e| NexusError::IoError {
            operation: "read config".into(),
            path: path.to_path_buf(),
            source: e,
        })?;
    // ...
}

fn write_event(path: &Path, event: &Event) -> NexusResult<()> {
    fs::write(path, serde_json::to_string(event)?)
        .map_err(|e| NexusError::IoError {
            operation: "write event log".into(),
            path: path.to_path_buf(),
            source: e,
        })?;
    Ok(())
}
```

### Rust Limitation

You **cannot** implement `From<T>` multiple times for the same source type `T`. This is a Rust trait coherence rule, not a thiserror limitation:

```rust
// COMPILE ERROR: Conflicting From implementations
impl From<std::io::Error> for NexusError { ... }  // First impl
impl From<std::io::Error> for NexusError { ... }  // Error!
```

---

## 4. Context Propagation with anyhow

### The Context Trait

Import `anyhow::Context` to enable `.context()` and `.with_context()` on `Result` and `Option`:

```rust
use anyhow::{Context, Result};

fn main() -> Result<()> {
    let settings = load_settings()
        .context("failed to load nexus settings")?;

    run_refactoring(&settings)
        .with_context(|| format!("refactoring task '{}' failed", settings.task))?;

    Ok(())
}
```

### `.context()` vs `.with_context()`

| Method | Evaluation | Use When |
|--------|-----------|----------|
| `.context("static message")` | Eager (always evaluates) | Message is a literal or cheap |
| `.with_context(\|\| format!(...))` | Lazy (only on error) | Message involves formatting or computation |

```rust
// GOOD: Static message, no allocation on success
file.read_to_string(&mut buf).context("failed to read file")?;

// GOOD: Lazy evaluation avoids format! on success path
file.read_to_string(&mut buf)
    .with_context(|| format!("failed to read {}", path.display()))?;

// BAD: Allocates even on success
file.read_to_string(&mut buf)
    .context(format!("failed to read {}", path.display()))?;
```

### Context Chaining

Context stacks from most recent to root cause:

```rust
fn load_settings(path: &Path) -> anyhow::Result<Settings> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let settings: Settings = serde_json::from_str(&content)
        .context("invalid JSON in settings file")?;

    validate_settings(&settings)
        .context("settings validation failed")?;

    Ok(settings)
}
```

Output on error:
```
Error: settings validation failed

Caused by:
    0: invalid JSON in settings file
    1: failed to read .nexus/settings.json
    2: No such file or directory (os error 2)
```

### Downcasting Through Context

Adding context preserves the ability to downcast to the original error:

```rust
let result: anyhow::Result<()> = do_something()
    .context("operation failed");

// Can still access original error
if let Some(io_err) = result.unwrap_err().downcast_ref::<std::io::Error>() {
    // Handle specific IO error
}
```

---

## 5. Exit Code Mapping

### Standard Exit Codes (sysexits.h / BSD conventions)

| Code | Constant | Meaning | Nexus Usage |
|------|----------|---------|-------------|
| 0 | `OK` | Success | Task completed |
| 1 | (general) | Generic error | Unspecified failure |
| 64 | `USAGE` | Command line usage error | Invalid CLI arguments |
| 65 | `DATAERR` | Data format error | Invalid JSON, malformed input |
| 66 | `NOINPUT` | Cannot open input | File not found |
| 69 | `UNAVAILABLE` | Service unavailable | API unreachable |
| 70 | `SOFTWARE` | Internal software error | Bug, panic |
| 73 | `CANTCREAT` | Cannot create output file | Write permission denied |
| 74 | `IOERR` | I/O error | Read/write failure |
| 77 | `NOPERM` | Permission denied | Policy blocked action |
| 78 | `CONFIG` | Configuration error | Invalid settings.json |

### Nexus Exit Code Implementation

```rust
// src/exit_codes.rs
use std::process::ExitCode;

/// Exit codes following sysexits.h conventions
pub mod codes {
    pub const OK: u8 = 0;
    pub const GENERAL_ERROR: u8 = 1;
    pub const USAGE: u8 = 64;
    pub const DATAERR: u8 = 65;
    pub const NOINPUT: u8 = 66;
    pub const UNAVAILABLE: u8 = 69;
    pub const SOFTWARE: u8 = 70;
    pub const CANTCREAT: u8 = 73;
    pub const IOERR: u8 = 74;
    pub const NOPERM: u8 = 77;
    pub const CONFIG: u8 = 78;
}

impl From<&NexusError> for u8 {
    fn from(err: &NexusError) -> u8 {
        match err {
            NexusError::PermissionDenied { .. } => codes::NOPERM,
            NexusError::PatchFailed { .. } => codes::DATAERR,
            NexusError::ConfigError { .. } => codes::CONFIG,
            NexusError::ApiError { .. } => codes::UNAVAILABLE,
            NexusError::IoError { operation, .. } => {
                if operation.contains("read") {
                    codes::NOINPUT
                } else {
                    codes::IOERR
                }
            }
            NexusError::ValidationError { .. } => codes::DATAERR,
            NexusError::JsonError { .. } => codes::DATAERR,
            NexusError::PathRejected { .. } => codes::NOPERM,
        }
    }
}

/// Convert anyhow::Error to exit code by inspecting root cause
pub fn exit_code_from_anyhow(err: &anyhow::Error) -> u8 {
    // Try to downcast to NexusError
    if let Some(nexus_err) = err.downcast_ref::<NexusError>() {
        return nexus_err.into();
    }

    // Check for common std errors
    if err.downcast_ref::<std::io::Error>().is_some() {
        return codes::IOERR;
    }

    codes::GENERAL_ERROR
}
```

### CLI Main Pattern

```rust
// src/main.rs
use anyhow::Result;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::from(0),
        Err(e) => {
            eprintln!("Error: {e:?}");
            ExitCode::from(exit_code_from_anyhow(&e))
        }
    }
}

fn run() -> Result<()> {
    let args = Cli::parse();
    let settings = load_settings().context("failed to initialize")?;
    execute_task(&settings, &args).context("task execution failed")?;
    Ok(())
}
```

**Note:** Returning `ExitCode` instead of `Result<()>` from `main()` gives explicit control over exit codes while still using `?` internally via the `run()` helper.

---

## 6. Module Organization Recommendation

### Recommended: Single Crate-Level Error Enum

For Nexus v0 (single crate), use **one error enum** in `src/error.rs`:

```
src/
  error.rs       # NexusError enum + exit codes
  main.rs        # CLI entry, uses anyhow::Result
  lib.rs         # Re-exports error types
  types/         # Data structures (no errors)
  gateway/       # Uses NexusError
  executor/      # Uses NexusError
  permission/    # Uses NexusError
```

**Rationale:**
- v0 scope is limited; single enum keeps code simple
- All modules share common failure modes (IO, validation, config)
- Callers can match on any variant without importing multiple error types
- Easy to refactor to per-module errors later if needed

### When to Split (Future)

Consider per-module errors when:
- Module has **3+ unique failure modes** not shared with other modules
- You want to **hide internal errors** from public API
- Different modules need **different error granularity**

Example future split:
```rust
// src/executor/error.rs
pub enum ExecutorError {
    ApiTimeout { ... },
    RateLimited { ... },
    InvalidResponse { ... },
}

// src/error.rs - aggregate at crate level
pub enum NexusError {
    Executor(#[from] executor::ExecutorError),
    Gateway(#[from] gateway::GatewayError),
    // ...
}
```

### Anti-Pattern: Avoid Dedicated `errors` Module

Do **not** create `src/errors/mod.rs` with all errors. This encourages:
- Overly generic error types
- Losing context about which module an error relates to
- Ball-of-mud error enums

Instead: Keep error definitions **close to the code they serve**.

---

## Sources

### Primary Documentation
- [thiserror crate docs](https://docs.rs/thiserror/latest/thiserror/)
- [anyhow Context trait](https://docs.rs/anyhow/latest/anyhow/trait.Context.html)
- [thiserror GitHub](https://github.com/dtolnay/thiserror)
- [anyhow GitHub](https://github.com/dtolnay/anyhow)

### Exit Codes
- [CLI Exit Codes - Rust CLI Book](https://rust-cli.github.io/book/in-depth/exit-code.html)
- [exitcode crate](https://docs.rs/exitcode)
- [sysexits-rs](https://github.com/sorairolake/sysexits-rs)

### Best Practices
- [Error Handling In Rust - A Deep Dive (Luca Palmieri)](https://lpalmieri.com/posts/error-handling-rust/)
- [Error Type Design (nrc)](https://nrc.github.io/error-docs/error-design/error-type-design.html)
- [Error Handling for Large Rust Projects - GreptimeDB](https://greptime.com/blogs/2024-05-07-error-rust)
- [Rust Error Handling Guide 2025](https://markaicode.com/rust-error-handling-2025-guide/)
- [thiserror Multiple Sources Discussion](https://users.rust-lang.org/t/thiserror-multiple-froms-sources/50863)

### Module Organization
- [Error Handling Ergonomics Discussion](https://users.rust-lang.org/t/error-handling-ergonomics-vs-precision-are-hierarchical-modular-errors-worth-the-boilerplate/126965)
- [Errors in Rust: A Formula](https://jondot.medium.com/errors-in-rust-a-formula-1e3e9a37d207)
