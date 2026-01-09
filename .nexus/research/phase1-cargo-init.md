# Research Report: Cargo Init Best Practices for Rust CLI Projects

**Research Date:** 2026-01-08
**Scope:** Idiomatic Cargo.toml configuration for Nexus CLI (Rust 2024 edition)
**Confidence:** High (multiple authoritative sources agree)

---

## Summary of Findings

Rust 1.85.0 (released February 20, 2025) stabilized the Rust 2024 edition, which is now the default for new projects created with `cargo new`. Key changes in Edition 2024 include resolver v3 (MSRV-aware by default), new prelude additions, and updated Cargo.toml conventions. For a CLI project like Nexus, the recommended approach is to use Edition 2024 with an explicit `rust-version` field to document MSRV, dual MIT/Apache-2.0 licensing for maximum ecosystem compatibility, and comprehensive metadata for crates.io discoverability.

---

## Research Questions Answered

### 1. What's the idiomatic way to initialize a Rust CLI project in 2025/2026?

**Recommendation:** Use `cargo init --name nexus` (or `cargo new nexus`) which automatically:
- Sets `edition = "2024"` (current default)
- Creates `src/main.rs` for binary target
- Initializes Git repository

For existing directories (like Nexus), `cargo init` is preferred over `cargo new`.

**Best Practice:** Keep `main.rs` thin, delegate business logic to `lib.rs` for testability. This pattern is documented in the [Rust CLI book](https://rust-cli.github.io/book/tutorial/setup.html).

**Source:** [Creating a New Project - The Cargo Book](https://doc.rust-lang.org/cargo/guide/creating-a-new-project.html)

---

### 2. What Cargo.toml fields are required vs recommended for CLIs?

#### Required Fields

| Field | Notes |
|-------|-------|
| `name` | Package identifier; must be alphanumeric, `-`, or `_` |
| `version` | SemVer format (x.y.z); defaults to `0.0.0` if omitted |

**Note:** As of recent Cargo versions, `authors` is no longer required (previously was mandatory).

#### Required for Publishing (crates.io)

| Field | Notes |
|-------|-------|
| `description` | Plain text (not Markdown); must not be empty |
| `license` or `license-file` | SPDX expression or path to license file |

#### Recommended for CLI Projects

| Field | Purpose |
|-------|---------|
| `edition` | Explicitly set even though cargo adds it (clarity) |
| `rust-version` | Document MSRV for users and tooling |
| `repository` | Link to source code |
| `readme` | Auto-detected if `README.md` exists |
| `keywords` | Up to 5 search terms (max 20 chars each) |
| `categories` | Up to 5 crates.io category slugs |
| `default-run` | Specify main binary if multiple exist |

**Source:** [The Manifest Format - The Cargo Book](https://doc.rust-lang.org/cargo/reference/manifest.html)

---

### 3. Best practices for edition, rust-version, and MSRV

#### Edition

**Recommendation:** Use `edition = "2024"` for new projects.

Rust 2024 is stable as of Rust 1.85.0 (February 2025). Key Edition 2024 features:
- Resolver v3 (MSRV-aware) is the default
- Async closures (`async || {}`)
- New prelude additions (`Future`, `IntoFuture`)
- Reserved `gen` keyword

**Source:** [Announcing Rust 1.85.0 and Rust 2024](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/)

#### rust-version (MSRV)

**Recommendation:** Set `rust-version = "1.85"` since Edition 2024 requires Rust 1.85+.

The `rust-version` field:
- Documents minimum supported Rust version
- Enables resolver v3 MSRV-aware dependency resolution
- Helps `cargo add` select compatible dependency versions
- Format: SemVer without range operators (e.g., `"1.85"` or `"1.85.0"`)

**MSRV Policy Options:**

| Policy | Description |
|--------|-------------|
| Latest only | Always require latest stable (simplest, most features) |
| N-2 releases | Support current + 2 previous versions (~3 months runway) |
| Time-based | Support versions from past 6-12 months |

For a new CLI project like Nexus, **Latest only** or **Edition minimum** (1.85 for Edition 2024) is reasonable.

**Source:** [Best (community) practices for MSRV](https://users.rust-lang.org/t/best-community-practices-for-msrv/119566), [MSRV-Aware Resolver RFC](https://rust-lang.github.io/rfcs/3537-msrv-resolver.html)

#### CI Verification

When supporting an MSRV:
- Test on MSRV in CI (compilation check minimum)
- Test on latest stable (full test suite)
- Use `cargo-msrv` to verify MSRV accuracy

**Source:** [cargo-msrv](https://github.com/foresterre/cargo-msrv)

---

### 4. License field conventions for MIT/Apache-2.0 dual licensing

**Recommendation:** Use `license = "MIT OR Apache-2.0"` with both license files.

#### Why Dual License?

The Rust project itself is dual-licensed MIT/Apache-2.0. This provides:
- **MIT**: Maximum permissiveness, GPL v2 compatible
- **Apache-2.0**: Patent protections, explicit contribution terms

Apache-only is incompatible with GPLv2, so dual licensing is the Rust ecosystem convention.

#### Implementation

```toml
license = "MIT OR Apache-2.0"
```

Include both files in repository root:
- `LICENSE-MIT`
- `LICENSE-APACHE`

**Note:** The conventional shorthand `MIT/Apache-2.0` (using `/` instead of `OR`) is also accepted by crates.io but the SPDX standard uses `OR`.

**Sources:**
- [Rust API Guidelines - Necessities](https://rust-lang.github.io/api-guidelines/necessities.html)
- [The Manifest Format - The Cargo Book](https://doc.rust-lang.org/cargo/reference/manifest.html)

---

### 5. Package metadata best practices

#### description

- Plain text, not Markdown
- Do not start with the crate name
- First sentence should stand alone as a summary
- Wrap at 80 columns for multi-line

**Example:**
```toml
description = "Safe multi-file refactoring CLI for AI-assisted code transformations"
```

#### repository

Full URL to source code repository:
```toml
repository = "https://github.com/yourusername/nexus"
```

#### keywords

Up to 5 keywords, each max 20 characters:
```toml
keywords = ["refactoring", "cli", "code-generation", "diff", "llm"]
```

#### categories

Up to 5 slugs from the [crates.io categories list](https://crates.io/category_slugs). Relevant for Nexus:

| Category Slug | Description |
|---------------|-------------|
| `command-line-utilities` | Applications to run at the command line |
| `development-tools` | Tools for developers |
| `text-processing` | Text manipulation (diffs, patches) |

**Recommendation:**
```toml
categories = ["command-line-utilities", "development-tools"]
```

**Source:** [crates.io Categories](https://crates.io/categories)

---

## Cargo.toml Field Ordering

Per the [Rust Style Guide](https://doc.rust-lang.org/style-guide/cargo.html):

1. `[package]` section first
2. Within `[package]`: `name` and `version` first, then alphabetically, with `description` last
3. Blank line between sections
4. No blank lines within sections

---

## Recommended Cargo.toml for Nexus

```toml
[package]
name = "nexus"
version = "0.1.0"
authors = ["AJ <your-email@example.com>"]
categories = ["command-line-utilities", "development-tools"]
edition = "2024"
keywords = ["refactoring", "cli", "code-generation", "diff", "llm"]
license = "MIT OR Apache-2.0"
readme = "README.md"
repository = "https://github.com/yourusername/nexus"
rust-version = "1.85"
description = "Safe multi-file refactoring CLI for AI-assisted code transformations"

[dependencies]
anyhow = "1"
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["full"] }

[profile.release]
lto = true
strip = true
```

### Notes on Dependencies

| Crate | Version | Notes |
|-------|---------|-------|
| `serde` | `1` | Stable API; use `derive` feature |
| `serde_json` | `1` | Stable API |
| `clap` | `4` | Use `derive` feature for declarative CLI |
| `tokio` | `1` | LTS releases available (1.43.x, 1.47.x) |
| `thiserror` | `2` | Major version 2 released; for library errors |
| `anyhow` | `1` | For application error handling |
| `chrono` | `0.4` | Enable `serde` feature for serialization |

### Release Profile Recommendations

For CLI distribution:
- `lto = true`: Link-time optimization (10-20% faster, smaller binary)
- `strip = true`: Remove symbols (smaller binary)

**Source:** [Build Configuration - The Rust Performance Book](https://nnethercote.github.io/perf-book/build-configuration.html)

---

## Additional Recommendations

### 1. Commit Cargo.lock

For applications (not libraries), commit `Cargo.lock` to version control. This ensures reproducible builds.

### 2. Project Structure

```
nexus/
  src/
    main.rs      # Thin entry point
    lib.rs       # Core logic (testable)
    error.rs     # NexusError enum (thiserror)
    types/       # Data structures from schemas
    ...
  tests/         # Integration tests
  LICENSE-MIT
  LICENSE-APACHE
  README.md
  Cargo.toml
  Cargo.lock
```

### 3. Optional: cargo-deny

Consider using [cargo-deny](https://crates.io/crates/cargo-deny) to audit dependencies for:
- License compatibility
- Security advisories
- Duplicate dependencies

---

## Sources

1. [The Manifest Format - The Cargo Book](https://doc.rust-lang.org/cargo/reference/manifest.html)
2. [Cargo.toml conventions - Rust Style Guide](https://doc.rust-lang.org/style-guide/cargo.html)
3. [Announcing Rust 1.85.0 and Rust 2024 - Rust Blog](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/)
4. [Rust API Guidelines - Necessities](https://rust-lang.github.io/api-guidelines/necessities.html)
5. [Command Line Applications in Rust - Project Setup](https://rust-cli.github.io/book/tutorial/setup.html)
6. [Best practices for MSRV - Rust Forum](https://users.rust-lang.org/t/best-community-practices-for-msrv/119566)
7. [MSRV-Aware Resolver RFC](https://rust-lang.github.io/rfcs/3537-msrv-resolver.html)
8. [cargo-msrv - GitHub](https://github.com/foresterre/cargo-msrv)
9. [Build Configuration - Rust Performance Book](https://nnethercote.github.io/perf-book/build-configuration.html)
10. [State of the Crates 2025](https://ohadravid.github.io/posts/2024-12-state-of-the-crates/)
11. [crates.io Category Slugs](https://crates.io/category_slugs)
