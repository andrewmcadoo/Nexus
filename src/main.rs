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
fn run() -> Result<()> {
    // Load .env if present before parsing CLI options.
    dotenvy::dotenv().ok();

    // Parse CLI arguments.
    let cli = Cli::parse();

    // Initialize logging.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(cli.log_level()))
        .init();

    log::info!("Task: {}", cli.task);

    // Load configuration using explicit CLI path (error if missing).
    let config =
        NexusConfig::load_with_config_path(&cli.config).context("failed to load configuration")?;

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
