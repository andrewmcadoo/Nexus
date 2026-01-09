use anyhow::{Context, Result};
use clap::Parser;
use std::process::ExitCode;

use nexus::cli::Cli;
use nexus::error::exit_code_from_anyhow;
use nexus::settings::NexusConfig;

/// Program entry point that runs the application and converts its result into a process exit code.
///
/// On success, this returns exit code 0. On error, the error is printed to stderr using debug
/// formatting and a Nexus-specific mapping determines the non-zero exit code returned.
///
/// # Examples
///
/// ```no_run
/// // Invoke the program entry and inspect the returned exit code.
/// let code = my_crate::main();
/// // `code` is `ExitCode::from(0)` on success, or non-zero on failure.
/// ```
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::from(0),
        Err(err) => {
            eprintln!("Error: {err:?}");
            ExitCode::from(exit_code_from_anyhow(&err))
        }
    }
}

/// Starts the application: loads environment and CLI options, initializes logging, loads the Nexus configuration, and either prints a dry-run summary or proceeds to execution.
///
/// This function:
/// - Loads a `.env` file if present.
/// - Parses command-line arguments (CLI), using the CLI log level to initialize logging.
/// - Loads `NexusConfig`, propagating an error with context if loading fails.
/// - If the CLI requested a dry run, prints a summary (task, whether settings were loaded, and whether an API key is available) and returns early.
/// - Otherwise, prints the execution notice (actual execution is pending implementation).
///
/// # Examples
///
/// ```no_run
/// // Call `run()` from the crate root; the example is marked `no_run` because it
/// // parses CLI args and initializes logging.
/// let _ = run();
/// ```
///
/// # Returns
///
/// `Ok(())` on success, `Err` if configuration loading or startup fails (with context "failed to load configuration").
fn run() -> Result<()> {
    // Load .env if present before parsing CLI options.
    dotenvy::dotenv().ok();

    // Parse CLI arguments.
    let cli = Cli::parse();

    // Initialize logging.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(cli.log_level()),
    )
    .init();

    log::info!("Task: {}", cli.task);

    // Load configuration.
    let config = NexusConfig::load().context("failed to load configuration")?;

    log::debug!("Config path: {:?}", config.settings_path);
    log::debug!("Permission mode: {:?}", config.settings.permission_mode);

    if cli.dry_run {
        println!("[DRY RUN] Would execute: {}", cli.task);
        println!("Settings loaded: {}", config.has_settings_file());
        println!("API key available: {}", config.has_api_key());
        return Ok(());
    }

    // TODO: Phase 2+ - Implement actual execution.
    println!("Executing: {}", cli.task);
    println!("(Implementation pending - Phase 2+)");

    Ok(())
}