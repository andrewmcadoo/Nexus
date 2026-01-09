use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use serde_json::json;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use nexus::cli::Cli;
use nexus::error::exit_code_from_anyhow;
use nexus::settings::NexusConfig;

/// Program entry point that runs the application and converts its result into a process exit code.
///
/// On success, this returns exit code 0. On error, the error is printed to stderr using debug
/// formatting and a Nexus-specific mapping determines the non-zero exit code returned.
fn main() -> ExitCode {
    debug_log_probe("main.entry");

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

    // Capture raw args and env before Clap parsing (in case parse exits early).
    let raw_args: Vec<String> = env::args().collect();
    let env_config = env::var("NEXUS_CONFIG").ok();

    debug_log(
        "H4",
        "src/main.rs:run:pre_parse",
        "Pre-parse snapshot",
        json!({
            "raw_args": raw_args,
            "env_config": env_config
        }),
    );

    // Parse CLI arguments.
    let cli = Cli::parse();

    debug_log(
        "H1",
        "src/main.rs:run:cli_parsed",
        "CLI parsed",
        json!({
            "config_arg": cli.config,
            "dry_run": cli.dry_run,
            "verbose": cli.verbose
        }),
    );

    // Initialize logging.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(cli.log_level()))
        .init();

    log::info!("Task: {}", cli.task);

    // Load configuration using explicit CLI path (error if missing).
    let config =
        NexusConfig::load_with_config_path(&cli.config).context("failed to load configuration")?;

    log::debug!("Config path: {:?}", config.settings_path);
    log::debug!("Permission mode: {:?}", config.settings.permission_mode);

    debug_log(
        "H1",
        "src/main.rs:run:config_loaded",
        "Config loaded",
        json!({
            "cli_config_arg": cli.config,
            "settings_path_used": config.settings_path,
            "has_settings_file": config.has_settings_file(),
            "has_api_key": config.has_api_key()
        }),
    );

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

fn debug_log(hypothesis_id: &str, location: &str, message: &str, data: serde_json::Value) {
    const DEBUG_LOG_PATH: &str = "/Users/aj/Desktop/Projects/Nexus/.cursor/debug.log";
    const FALLBACK_PATH: &str = "/tmp/nexus-debug.log";
    const LOCAL_PATH: &str = "/Users/aj/Desktop/Projects/Nexus/debug.log";

    if let Some(parent) = Path::new(DEBUG_LOG_PATH).parent() {
        let _ = fs::create_dir_all(parent);
    }

    let payload = json!({
        "sessionId": "debug-session",
        "runId": "pre-fix",
        "hypothesisId": hypothesis_id,
        "location": location,
        "message": message,
        "data": data,
        "timestamp": Utc::now().timestamp_millis()
    });

    let write_result = OpenOptions::new()
        .create(true)
        .append(true)
        .open(DEBUG_LOG_PATH)
        .and_then(|mut file| writeln!(file, "{}", payload));

    if write_result.is_err() {
        let fallback_result = OpenOptions::new()
            .create(true)
            .append(true)
            .open(FALLBACK_PATH)
            .and_then(|mut file| writeln!(file, "{}", payload));

        if fallback_result.is_err() {
            let _ = OpenOptions::new()
                .create(true)
                .append(true)
                .open(LOCAL_PATH)
                .and_then(|mut file| writeln!(file, "{}", payload));
            eprintln!(
                "debug_log fell back: primary={:?}, tmp={:?}",
                write_result.err(),
                fallback_result.err()
            );
        }
    }
}

fn debug_log_probe(tag: &str) {
    const DEBUG_LOG_PATH: &str = "/Users/aj/Desktop/Projects/Nexus/.cursor/debug.log";
    const FALLBACK_PATH: &str = "/tmp/nexus-debug.log";
    const LOCAL_PATH: &str = "/Users/aj/Desktop/Projects/Nexus/debug.log";

    let payload = format!(
        "{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H0\",\"location\":\"src/main.rs:main\",\"message\":\"probe:{}\",\"timestamp\":{}}}",
        tag,
        Utc::now().timestamp_millis()
    );

    if let Some(parent) = Path::new(DEBUG_LOG_PATH).parent() {
        let _ = fs::create_dir_all(parent);
    }

    let write_result = OpenOptions::new()
        .create(true)
        .append(true)
        .open(DEBUG_LOG_PATH)
        .and_then(|mut file| writeln!(file, "{}", payload));

    if write_result.is_err() {
        let fallback_result = OpenOptions::new()
            .create(true)
            .append(true)
            .open(FALLBACK_PATH)
            .and_then(|mut file| writeln!(file, "{}", payload));

        if fallback_result.is_err() {
            let _ = OpenOptions::new()
                .create(true)
                .append(true)
                .open(LOCAL_PATH)
                .and_then(|mut file| writeln!(file, "{}", payload));
            eprintln!(
                "debug_log_probe fell back: primary={:?}, tmp={:?}",
                write_result.err(),
                fallback_result.err()
            );
        }
    }
}
