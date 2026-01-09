# Phase 1 Research: Tokio Async Runtime Setup for CLI

**Date:** 2026-01-08
**Researcher:** research-specialist
**Confidence:** High (multiple authoritative sources agree)

---

## Executive Summary

For Nexus CLI, the recommended approach is:
- **Runtime flavor:** `current_thread` for simplicity (single async task at a time)
- **Feature flags:** Minimal set (`rt`, `macros`, `signal`) to reduce binary size
- **File I/O:** Use `std::fs` directly (no `spawn_blocking` needed for small files)
- **Shutdown:** `tokio::signal::ctrl_c()` with graceful cancellation

---

## 1. Runtime Flavor Recommendation

### Decision: Use `current_thread` Runtime

**Rationale:**

| Factor | Multi-Thread | Current-Thread | Winner for Nexus |
|--------|--------------|----------------|------------------|
| Use case | Concurrent background tasks | Sequential operations | Current-Thread |
| Binary size | Larger (work-stealing impl) | Smaller | Current-Thread |
| Complexity | Thread pool management | Single-threaded | Current-Thread |
| Nexus pattern | One API call at a time | Matches | Current-Thread |

Per ADR-002, Nexus makes HTTP calls to OpenAI API sequentially (one refactoring request at a time). There's no need for parallel async operations, making `current_thread` the ideal choice.

**Source:** [Tokio Runtime Documentation](https://docs.rs/tokio/latest/tokio/runtime/index.html) states:
> "The current-thread scheduler provides a single-threaded future executor. All tasks will be created and executed on the current thread."

**When to reconsider:** If Nexus later needs to make multiple concurrent API calls (e.g., parallel chunk processing), switch to `multi_thread`.

---

## 2. Mixing Sync File I/O with Async API Calls

### Recommendation: Use `std::fs` Directly

For Nexus's use case (reading/writing source files during refactoring), **you do not need `spawn_blocking`**.

**Why direct `std::fs` is acceptable:**

1. **File operations are fast** - Reading/writing source files (KB-scale) completes in microseconds
2. **Sequential workflow** - Nexus reads file, sends to API, waits for response, writes file
3. **No async benefit** - Async file I/O only helps with concurrent operations or slow storage
4. **Simplicity** - No additional complexity or thread pool management

**When to use `spawn_blocking`:**

| Operation | Use `spawn_blocking`? |
|-----------|----------------------|
| Read small source file (<1MB) | No |
| Write diff to file | No |
| Process large file (>10MB) | Yes |
| Scan entire codebase for files | Consider it |
| CPU-intensive diff calculation | Yes |

**Pattern for direct sync I/O in async context:**

```rust
use std::fs;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    // Sync file read - OK because it's fast
    let content = fs::read_to_string("src/lib.rs")?;

    // Async API call - benefits from async
    let response = make_api_call(&content).await?;

    // Sync file write - OK because it's fast
    fs::write("src/lib.rs", &response.new_content)?;

    Ok(())
}
```

**Source:** [Tokio Bridging with Sync Code](https://tokio.rs/tokio/topics/bridging) notes:
> "In some cases, you may need to run a small portion of synchronous code."

**When `spawn_blocking` IS needed:**

```rust
use tokio::task;

// For CPU-intensive or slow blocking operations
let result = task::spawn_blocking(move || {
    // This runs on a dedicated thread pool
    expensive_diff_calculation(&large_file)
}).await?;
```

**Warning from docs:** Tasks spawned with `spawn_blocking` cannot be aborted because they are not async. Plan accordingly for shutdown scenarios.

---

## 3. Async Main with Clap Integration

### Concrete Example

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "nexus")]
#[command(about = "Safe multi-file refactoring CLI")]
#[command(version)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Refactoring instruction (when no subcommand)
    #[arg(trailing_var_arg = true)]
    instruction: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize .nexus directory
    Init,
    /// Show event log
    Log {
        /// Number of entries to show
        #[arg(short, default_value = "10")]
        count: usize,
    },
    /// Replay a previous refactoring
    Replay {
        /// Event ID to replay
        event_id: String,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Clap parsing is synchronous - happens before async context
    // This is fine and idiomatic

    match cli.command {
        Some(Commands::Init) => {
            // Mostly sync file operations
            init_nexus_directory()?;
        }
        Some(Commands::Log { count }) => {
            // Sync file read
            show_event_log(count)?;
        }
        Some(Commands::Replay { event_id }) => {
            // May involve async API calls
            replay_event(&event_id).await?;
        }
        None => {
            // Main flow: refactoring with API calls
            let instruction = cli.instruction.join(" ");
            if instruction.is_empty() {
                anyhow::bail!("Please provide a refactoring instruction");
            }
            run_refactoring(&instruction, cli.verbose).await?;
        }
    }

    Ok(())
}
```

**Key points:**
- `#[tokio::main(flavor = "current_thread")]` - Explicit runtime selection
- Clap parsing happens synchronously before async runtime
- `anyhow::Result<()>` for ergonomic error handling (per ADR-003)
- Commands can mix sync and async operations naturally

---

## 4. Graceful Shutdown (Ctrl+C Handling)

### Pattern for CLI Applications

For Nexus, graceful shutdown means:
1. Catch Ctrl+C during long API calls
2. Cancel in-flight requests cleanly
3. Ensure no partial file writes

**Basic Pattern (Sufficient for v0):**

```rust
use tokio::signal;
use tokio::select;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Run main logic with Ctrl+C handling
    select! {
        result = run_main_logic(&cli) => {
            result?;
        }
        _ = signal::ctrl_c() => {
            eprintln!("\nInterrupted. Cleaning up...");
            // Any cleanup logic here
        }
    }

    Ok(())
}

async fn run_main_logic(cli: &Cli) -> anyhow::Result<()> {
    // Main application logic
    // API calls, file operations, etc.
    Ok(())
}
```

**Advanced Pattern with CancellationToken:**

For more complex scenarios (multiple concurrent operations), use `tokio_util::sync::CancellationToken`:

```rust
use tokio::signal;
use tokio_util::sync::CancellationToken;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let token = CancellationToken::new();
    let token_clone = token.clone();

    // Spawn shutdown listener
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        eprintln!("\nShutdown requested...");
        token_clone.cancel();
    });

    // Run main logic with cancellation support
    run_with_cancellation(token).await
}

async fn run_with_cancellation(token: CancellationToken) -> anyhow::Result<()> {
    // Check for cancellation at safe points
    if token.is_cancelled() {
        return Ok(());
    }

    // Or use select! for interruptible operations
    tokio::select! {
        result = make_api_call() => {
            let response = result?;
            // Process response...
        }
        _ = token.cancelled() => {
            eprintln!("Operation cancelled");
            return Ok(());
        }
    }

    Ok(())
}
```

**Recommendation for Nexus v0:** Start with the basic `select!` pattern. It's sufficient for:
- Interrupting a single API call
- Clean exit without partial state

Add `CancellationToken` if you later need:
- Multiple spawned tasks
- Fine-grained cancellation control
- Cleanup callbacks

**Source:** [Tokio Graceful Shutdown Guide](https://tokio.rs/tokio/topics/shutdown)

---

## 5. Recommended Cargo.toml Feature Flags

### Minimal Configuration for Nexus

```toml
[dependencies]
# Async runtime - minimal features for CLI
tokio = { version = "1.43", features = ["rt", "macros", "signal"] }

# Only if using CancellationToken for advanced shutdown
# tokio-util = { version = "0.7", features = ["sync"] }
```

### Feature Flag Breakdown

| Feature | Purpose | Needed for Nexus? |
|---------|---------|-------------------|
| `rt` | Current-thread runtime | **Yes** |
| `macros` | `#[tokio::main]` attribute | **Yes** |
| `signal` | Ctrl+C handling | **Yes** |
| `rt-multi-thread` | Multi-threaded runtime | No |
| `sync` | Channels, mutexes | Maybe (for session state) |
| `time` | Timeouts, delays | Maybe (for API timeouts) |
| `net` | TCP/UDP networking | No (reqwest handles this) |
| `fs` | Async file operations | No (using std::fs) |
| `io-util` | AsyncRead/Write utilities | No |
| `full` | Everything | No (bloats binary) |

### If Using reqwest for HTTP

```toml
[dependencies]
tokio = { version = "1.43", features = ["rt", "macros", "signal"] }
reqwest = { version = "0.12", features = ["json"] }
```

Note: `reqwest` brings its own tokio dependency. The feature flags will be merged, but explicitly declaring them ensures clarity.

### Binary Size Optimization

For release builds, add to `Cargo.toml`:

```toml
[profile.release]
opt-level = "z"      # Optimize for size
lto = true           # Link-time optimization
codegen-units = 1    # Better optimization
panic = "abort"      # Smaller binary (no unwinding)
strip = true         # Strip symbols
```

**Source:** [Tokio Feature Flags](https://lib.rs/crates/tokio/features)

---

## 6. Complete Working Example

Here's a minimal but complete async CLI structure for Nexus:

```rust
// src/main.rs
use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use tokio::select;
use tokio::signal;

#[derive(Parser)]
#[command(name = "nexus")]
#[command(about = "Safe multi-file refactoring CLI")]
struct Cli {
    /// Refactoring instruction
    instruction: Vec<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let instruction = cli.instruction.join(" ");

    if instruction.is_empty() {
        anyhow::bail!("Usage: nexus <refactoring instruction>");
    }

    // Run with Ctrl+C handling
    select! {
        result = run_refactoring(&instruction) => {
            result
        }
        _ = signal::ctrl_c() => {
            eprintln!("\nInterrupted.");
            Ok(())
        }
    }
}

async fn run_refactoring(instruction: &str) -> Result<()> {
    // 1. Read settings (sync - fast)
    let settings_path = ".nexus/settings.json";
    let settings_content = fs::read_to_string(settings_path)
        .context("Failed to read settings")?;

    // 2. Call API (async - slow, benefits from async)
    let response = call_codex_api(instruction).await
        .context("API call failed")?;

    // 3. Apply patches (sync - fast)
    for patch in response.patches {
        apply_patch(&patch)?;
    }

    // 4. Log event (sync - fast)
    log_event(&response)?;

    Ok(())
}

async fn call_codex_api(instruction: &str) -> Result<ApiResponse> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .context("OPENAI_API_KEY not set")?;

    let client = reqwest::Client::new();

    let response = client
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(&api_key)
        .json(&serde_json::json!({
            "model": "codex",
            "input": instruction
        }))
        .send()
        .await?
        .json::<ApiResponse>()
        .await?;

    Ok(response)
}

fn apply_patch(patch: &Patch) -> Result<()> {
    // Sync file operations - perfectly fine
    let content = fs::read_to_string(&patch.file_path)?;
    let new_content = patch.apply(&content)?;
    fs::write(&patch.file_path, new_content)?;
    Ok(())
}

fn log_event(response: &ApiResponse) -> Result<()> {
    // Sync append to log file
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(".nexus/events.jsonl")?;

    writeln!(file, "{}", serde_json::to_string(response)?)?;
    Ok(())
}
```

---

## 7. Decision Summary

| Question | Answer | Rationale |
|----------|--------|-----------|
| Runtime flavor? | `current_thread` | Sequential operations, smaller binary |
| Sync file I/O? | Use `std::fs` directly | Fast operations, no benefit from async |
| `spawn_blocking`? | Not needed for v0 | Only for >10MB files or CPU work |
| Ctrl+C handling? | `tokio::select!` + `signal::ctrl_c()` | Simple, sufficient for single-task CLI |
| Feature flags? | `rt`, `macros`, `signal` | Minimal footprint |
| `tokio-util`? | Only if advanced shutdown needed | CancellationToken for multi-task scenarios |

---

## Sources

1. [Tokio Runtime Documentation](https://docs.rs/tokio/latest/tokio/runtime/index.html) - Official runtime flavor comparison
2. [Tokio Feature Flags](https://lib.rs/crates/tokio/features) - Complete feature flag reference
3. [Bridging with Sync Code](https://tokio.rs/tokio/topics/bridging) - Official guide for mixing sync/async
4. [Graceful Shutdown](https://tokio.rs/tokio/topics/shutdown) - Official shutdown patterns
5. [spawn_blocking Documentation](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) - When and how to use
6. [CancellationToken](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html) - Advanced cancellation
7. [Tokio GitHub Discussion #7091](https://github.com/tokio-rs/tokio/discussions/7091) - Multi-thread vs current-thread performance
8. [Command Line Applications in Rust - Signal Handling](https://rust-cli.github.io/book/in-depth/signals.html) - CLI best practices

---

## Next Steps

1. Add tokio dependency with minimal features to `Cargo.toml`
2. Structure `main.rs` with `#[tokio::main(flavor = "current_thread")]`
3. Integrate clap CLI parsing (synchronous, before async runtime)
4. Add `select!` with `signal::ctrl_c()` for graceful shutdown
5. Keep file I/O synchronous with `std::fs`
6. Make HTTP client calls with `reqwest` (async)
