# Clap 4 CLI Design Patterns Research

**Research Date:** 2026-01-08
**Topic:** Clap 4 Derive Macro Best Practices for Nexus CLI
**Scope:** Single-command CLI with optional flags, argument validation, help text, environment variables
**Confidence:** High (multiple authoritative sources, official documentation consulted)

---

## 1. Summary of Clap 4 Derive Patterns

### Key Findings

1. **Derive is the recommended approach** for modern Rust CLIs. The derive style provides clean, maintainable code where you describe your CLI as a Rust struct, and clap's proc-macros handle parsing automatically.

2. **Current stable version:** v4.5.54 (as of January 2026). Enable with:
   ```toml
   clap = { version = "4", features = ["derive", "env"] }
   ```

3. **Cargo.toml metadata integration:** Use `#[command(version, about)]` to automatically read name, version, and description from `Cargo.toml`.

4. **Type-driven validation:** Clap validates inputs automatically based on Rust types. Parse into validated types rather than validating strings (the "parse, don't validate" philosophy).

5. **Help generation:** Doc comments (`///`) automatically become help text. Blank lines separate short help (`-h`) from long help (`--help`).

### Sources

- [Clap Official Documentation](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html)
- [Clap GitHub Repository](https://github.com/clap-rs/clap) (15.9k stars, 468k dependents)
- [Rain's Rust CLI Recommendations](https://rust-cli-recommendations.sunshowers.io/handling-arguments.html)

---

## 2. Nexus CLI Struct Design

### Recommended Pattern: Single Command with Optional Flags

For Nexus's use case (primary positional argument with optional flags), a simple struct-based design is ideal:

```rust
use clap::Parser;
use std::path::PathBuf;

/// Safe multi-file refactoring CLI
///
/// Nexus takes a refactoring task description and uses Codex to propose
/// changes, then prompts for approval before applying patches.
#[derive(Parser, Debug)]
#[command(name = "nexus")]
#[command(version, about, long_about = None)]
#[command(author = "AJ")]
pub struct Cli {
    /// The refactoring task to execute
    ///
    /// Describe what you want to refactor in natural language.
    /// Examples:
    ///   "rename getUserData to fetchUserProfile"
    ///   "extract the validation logic into a separate module"
    #[arg(value_name = "TASK")]
    pub task: String,

    /// Path to configuration file
    ///
    /// Defaults to .nexus/settings.json in the current directory
    #[arg(short, long, value_name = "FILE", env = "NEXUS_CONFIG")]
    pub config: Option<PathBuf>,

    /// Preview changes without applying them
    #[arg(long)]
    pub dry_run: bool,

    /// Enable verbose output for debugging
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}
```

### Usage Examples

This struct supports all the requested invocations:

```bash
# Primary usage
nexus "rename getUserData to fetchUserProfile"

# With config override
nexus --config .nexus/settings.json "refactor task"

# Dry run mode
nexus --dry-run "preview changes"

# Verbose output (-v, -vv, -vvv for increasing levels)
nexus --verbose "task with debug output"
nexus -vvv "maximum verbosity"

# Combined
nexus -v --dry-run --config custom.json "complex task"

# Via environment variable
NEXUS_CONFIG=/path/to/config.json nexus "task"
```

### Why Not Subcommands?

For Nexus v0, subcommands are unnecessary:

1. **Primary use case is simple:** Single positional argument (the task).
2. **Future extensibility:** Can add subcommands later without breaking existing usage by wrapping in `Option<Commands>`.
3. **Less cognitive overhead:** Users don't need to remember command names.

However, the implementation plan mentions `nexus init` and `nexus policy show`. If these are needed, here's how to add optional subcommands:

```rust
#[derive(Parser, Debug)]
#[command(name = "nexus", version, about)]
pub struct Cli {
    /// Global configuration file
    #[arg(short, long, value_name = "FILE", env = "NEXUS_CONFIG", global = true)]
    pub config: Option<PathBuf>,

    /// Verbose mode
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Execute a refactoring task
    Run {
        /// The refactoring task description
        task: String,

        /// Preview without applying
        #[arg(long)]
        dry_run: bool,
    },

    /// Initialize Nexus configuration
    Init,

    /// Show current policy rules
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum PolicyAction {
    /// Display current policy
    Show,
}
```

**Recommendation:** Start with the simple struct (no subcommands). The primary invocation `nexus "task"` is cleaner. Add subcommands in v0.1 if needed.

---

## 3. Argument Validation Examples

### Path Validation (File Exists)

Clap 4 uses `value_parser` for custom validation. Here's how to validate that a config file exists:

```rust
use std::path::PathBuf;
use std::fs;

/// Validate that a path points to an existing file
fn validate_file_exists(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);
    match fs::metadata(&path) {
        Ok(meta) if meta.is_file() => Ok(path),
        Ok(_) => Err(format!("'{}' exists but is not a file", s)),
        Err(_) => Err(format!("file not found: '{}'", s)),
    }
}

#[derive(Parser, Debug)]
pub struct Cli {
    /// Path to configuration file (must exist)
    #[arg(short, long, value_parser = validate_file_exists)]
    pub config: Option<PathBuf>,
}
```

### Path Validation (Parent Directory Exists)

For output files where the parent directory must exist:

```rust
fn validate_parent_exists(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);
    if let Some(parent) = path.parent() {
        if parent.as_os_str().is_empty() || parent.exists() {
            return Ok(path);
        }
        return Err(format!("parent directory does not exist: '{}'", parent.display()));
    }
    Ok(path)
}
```

### Non-Empty String Validation

For the task argument, ensure it's not empty or whitespace-only:

```rust
fn validate_non_empty(s: &str) -> Result<String, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        Err("task description cannot be empty".to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

#[derive(Parser, Debug)]
pub struct Cli {
    /// The refactoring task to execute
    #[arg(value_name = "TASK", value_parser = validate_non_empty)]
    pub task: String,
}
```

### Permission Mode Validation (Enum)

For constrained values like permission modes from your settings schema:

```rust
use clap::ValueEnum;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    Autopilot,
}

#[derive(Parser, Debug)]
pub struct Cli {
    /// Permission mode for automatic approvals
    #[arg(long, value_enum, default_value_t = PermissionMode::Default)]
    pub permission_mode: PermissionMode,
}
```

This generates help text showing `[possible values: default, accept-edits, autopilot]`.

---

## 4. Help Text Best Practices

### Doc Comment Conventions

```rust
/// Short description shown with -h
///
/// Extended description shown with --help.
/// Can span multiple paragraphs.
///
/// Blank lines create paragraph breaks.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli { ... }
```

### Clap Text Processing

By default, clap:
- Strips leading/trailing whitespace from lines
- Replaces newlines within paragraphs with spaces (for terminal rewrapping)
- Uses first paragraph for `-h`, entire comment for `--help`
- Removes trailing period from first sentence

To preserve exact formatting, use `#[command(verbatim_doc_comment)]`.

### Recommended Patterns

1. **First line:** Imperative verb, no period. "Initialize configuration" not "Initializes configuration."

2. **Value names:** Use `value_name` for semantic clarity:
   ```rust
   #[arg(value_name = "TASK")]  // Shows as <TASK> in help
   #[arg(value_name = "FILE")]  // Shows as <FILE> in help
   ```

3. **Examples in long help:**
   ```rust
   /// Execute a refactoring task
   ///
   /// Examples:
   ///   nexus "rename getUserData to fetchUserProfile"
   ///   nexus "extract validation into separate module"
   #[arg(value_name = "TASK")]
   pub task: String,
   ```

4. **Environment variable visibility:**
   When using `env`, clap shows `[env: NEXUS_CONFIG=]` in help automatically.

### Complete Help Example

```rust
/// Safe multi-file refactoring CLI
///
/// Nexus uses Codex to propose refactoring changes, prompts for
/// approval, then applies patches safely with full audit logging.
#[derive(Parser, Debug)]
#[command(name = "nexus")]
#[command(version, about)]
#[command(after_help = "For more info: https://github.com/user/nexus")]
pub struct Cli {
    /// The refactoring task to execute
    ///
    /// Describe what you want to change in natural language.
    /// Be specific about what to rename, move, or restructure.
    #[arg(value_name = "TASK")]
    pub task: String,

    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", env = "NEXUS_CONFIG")]
    pub config: Option<PathBuf>,

    /// Preview changes without applying them
    ///
    /// When enabled, shows what would be changed but doesn't
    /// modify any files. Useful for reviewing before committing.
    #[arg(long)]
    pub dry_run: bool,

    /// Increase output verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}
```

Generated help output:

```
Safe multi-file refactoring CLI

Usage: nexus [OPTIONS] <TASK>

Arguments:
  <TASK>  The refactoring task to execute

Options:
  -c, --config <FILE>  Path to configuration file [env: NEXUS_CONFIG=]
      --dry-run        Preview changes without applying them
  -v, --verbose...     Increase output verbosity (-v, -vv, -vvv)
  -h, --help           Print help (see more with '--help')
  -V, --version        Print version

For more info: https://github.com/user/nexus
```

---

## 5. Environment Variable Integration

### Enabling the Feature

Add `env` to features in `Cargo.toml`:

```toml
clap = { version = "4", features = ["derive", "env"] }
```

### Basic Usage

```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
pub struct Cli {
    /// Configuration file path
    #[arg(short, long, env = "NEXUS_CONFIG")]
    pub config: Option<PathBuf>,

    /// OpenAI API key for Codex
    #[arg(long, env = "OPENAI_API_KEY", hide_env_values = true)]
    pub api_key: Option<String>,
}
```

### Key Attributes

| Attribute | Purpose |
|-----------|---------|
| `env = "VAR_NAME"` | Read from environment variable as fallback |
| `hide_env_values = true` | Don't show value in help (for secrets) |
| `hide_env = true` | Don't mention env var in help at all |

### Priority Order

1. Command-line argument (highest)
2. Environment variable
3. Default value (lowest)

### Complete Environment Integration for Nexus

```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "nexus", version, about)]
pub struct Cli {
    /// The refactoring task to execute
    #[arg(value_name = "TASK")]
    pub task: String,

    /// Path to configuration file
    ///
    /// Defaults to .nexus/settings.json in current directory
    #[arg(
        short,
        long,
        value_name = "FILE",
        env = "NEXUS_CONFIG",
        default_value = ".nexus/settings.json"
    )]
    pub config: PathBuf,

    /// OpenAI API key for Codex integration
    ///
    /// Can also be set in settings.json
    #[arg(
        long,
        env = "OPENAI_API_KEY",
        hide_env_values = true,  // Don't show key in --help
    )]
    pub api_key: Option<String>,

    /// Working directory (defaults to current directory)
    #[arg(
        short = 'C',
        long,
        value_name = "DIR",
        env = "NEXUS_WORKDIR",
    )]
    pub workdir: Option<PathBuf>,

    /// Preview changes without applying them
    #[arg(long, env = "NEXUS_DRY_RUN")]
    pub dry_run: bool,

    /// Verbosity level
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}
```

### Integration with dotenv

If you want to support `.env` files, load dotenv before parsing:

```rust
use clap::Parser;

fn main() {
    // Load .env file if present (before clap parses)
    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    // ...
}
```

Add to `Cargo.toml`:
```toml
dotenvy = "0.15"
```

---

## 6. Recommended Implementation

### Final Nexus CLI Struct

```rust
// src/cli.rs

use clap::Parser;
use std::path::PathBuf;

/// Validate that a file exists and is readable
fn validate_config_path(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);

    // Allow non-existent paths (will use defaults)
    if !path.exists() {
        return Ok(path);
    }

    match std::fs::metadata(&path) {
        Ok(meta) if meta.is_file() => Ok(path),
        Ok(_) => Err(format!("'{}' is not a file", s)),
        Err(e) => Err(format!("cannot access '{}': {}", s, e)),
    }
}

/// Validate task is non-empty
fn validate_task(s: &str) -> Result<String, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        Err("task description cannot be empty".into())
    } else {
        Ok(trimmed.to_string())
    }
}

/// Safe multi-file refactoring CLI
///
/// Nexus takes a refactoring task description, uses Codex to propose
/// changes, prompts for approval, then applies patches with full
/// audit logging.
#[derive(Parser, Debug)]
#[command(name = "nexus")]
#[command(version, about)]
#[command(
    after_help = "Examples:\n  \
        nexus \"rename getUserData to fetchUserProfile\"\n  \
        nexus --dry-run \"extract validation logic\"\n  \
        nexus -v --config custom.json \"refactor task\""
)]
pub struct Cli {
    /// The refactoring task to execute
    ///
    /// Describe the refactoring in natural language. Be specific
    /// about what to rename, move, extract, or restructure.
    #[arg(value_name = "TASK", value_parser = validate_task)]
    pub task: String,

    /// Path to configuration file
    #[arg(
        short,
        long,
        value_name = "FILE",
        env = "NEXUS_CONFIG",
        default_value = ".nexus/settings.json",
        value_parser = validate_config_path,
    )]
    pub config: PathBuf,

    /// Preview changes without applying them
    ///
    /// Shows proposed patches and what would change, but doesn't
    /// modify any files. Use to review before committing.
    #[arg(long, env = "NEXUS_DRY_RUN")]
    pub dry_run: bool,

    /// Increase output verbosity
    ///
    /// Use -v for info, -vv for debug, -vvv for trace
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

impl Cli {
    /// Determine log level from verbosity count
    pub fn log_level(&self) -> &'static str {
        match self.verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        // Validates clap configuration at test time
        Cli::command().debug_assert();
    }

    #[test]
    fn test_basic_parse() {
        let cli = Cli::parse_from(["nexus", "rename foo to bar"]);
        assert_eq!(cli.task, "rename foo to bar");
        assert!(!cli.dry_run);
        assert_eq!(cli.verbose, 0);
    }

    #[test]
    fn test_all_flags() {
        let cli = Cli::parse_from([
            "nexus",
            "--dry-run",
            "-vvv",
            "--config",
            "custom.json",
            "my task",
        ]);
        assert!(cli.dry_run);
        assert_eq!(cli.verbose, 3);
        assert_eq!(cli.config, PathBuf::from("custom.json"));
    }
}
```

### main.rs Integration

```rust
// src/main.rs

use clap::Parser;

mod cli;

use cli::Cli;

fn main() -> anyhow::Result<()> {
    // Load .env if present
    dotenvy::dotenv().ok();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize logging based on verbosity
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(cli.log_level())
    ).init();

    log::info!("Task: {}", cli.task);
    log::debug!("Config: {:?}", cli.config);
    log::debug!("Dry run: {}", cli.dry_run);

    if cli.dry_run {
        println!("[DRY RUN] Would execute: {}", cli.task);
        return Ok(());
    }

    // TODO: Implement engine.run()
    println!("Executing: {}", cli.task);

    Ok(())
}
```

### Cargo.toml Dependencies

```toml
[dependencies]
clap = { version = "4", features = ["derive", "env"] }
anyhow = "1"
dotenvy = "0.15"
env_logger = "0.11"
log = "0.4"
```

---

## 7. Sources

### Official Documentation
- [Clap Derive Tutorial](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html) - Complete guide to derive macros
- [Clap API Reference](https://docs.rs/clap/latest/clap/) - Full API documentation
- [Clap GitHub Repository](https://github.com/clap-rs/clap) - v4.5.54, 15.9k stars

### Community Resources
- [Rain's Rust CLI Recommendations](https://rust-cli-recommendations.sunshowers.io/handling-arguments.html) - Subcommand structure patterns
- [Shuttle Clap Guide](https://www.shuttle.dev/blog/2023/12/08/clap-rust) - Practical examples
- [Hemaks Production CLI Tutorial](https://hemaks.org/posts/building-production-ready-cli-tools-in-rust-with-clap-from-zero-to-hero/) - Best practices

### Clap Discussions
- [validator equivalent in Clap 4](https://github.com/clap-rs/clap/discussions/5402) - Custom validation migration
- [PathBuf value_parser](https://github.com/clap-rs/clap/discussions/5221) - Path handling patterns
- [Environment variable integration](https://github.com/clap-rs/clap/issues/814) - ENV feature design

### Additional References
- [Getting started with application configuration in Rust](https://www.perrygeo.com/getting-started-with-application-configuration-in-rust.html) - ENV + clap integration
- [Toolshelf CLI Guide](https://toolshelf.tech/blog/build-rust-cli-tool-with-clap-guide/) - Step-by-step tutorial
