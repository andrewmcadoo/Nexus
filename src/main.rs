use anyhow::{Context, Result};
use clap::Parser;
use std::process::ExitCode;

use nexus::cli::Cli;
use nexus::error::exit_code_from_anyhow;
use nexus::settings::NexusConfig;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::from(0),
        Err(err) => {
            eprintln!("Error: {err:?}");
            ExitCode::from(exit_code_from_anyhow(&err))
        }
    }
}

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
