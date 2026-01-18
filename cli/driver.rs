//! CLI Entry Point
//!
//! This module handles the initialization of the command-line interface,
//! parsing of arguments, environment setup (logging), and the subsequent
//! dispatching of control to the core application logic.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;

// Importing the core application logic and error types.
// Adjust these imports based on your actual crate structure.
use crate::config::AppConfig;
use crate::core::run_app;
use crate::error::AppError;

/// The command-line arguments for the application.
#[derive(Parser, Debug)]
#[command(name = "my_app")]
#[command(about = "A brief description of what my_app does.", long_about = None)]
struct CliArgs {
    /// Path to the configuration file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Sets the level of logging verbosity
    ///
    /// -v: Warning
    /// -vv: Info
    /// -vvv: Debug
    /// -vvvv: Trace
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Run strictly, returning error on any warning
    #[arg(long)]
    strict: bool,
}

/// The main entry point for the CLI driver.
///
/// This function is responsible for:
/// 1. Parsing CLI arguments.
/// 2. Initializing the logger.
/// 3. Loading configuration.
/// 4. Spawning the async runtime.
/// 5. Executing the application loop.
pub async fn driver() -> ExitCode {
    let args = CliArgs::parse();

    // Initialize the logger based on verbosity flags.
    // We default to WARN, but allow increasing verbosity up to TRACE.
    let log_level = match args.verbose {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        2 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };

    // Setup env_logger or similar implementation
    env_logger::Builder::new()
        .filter_level(log_level)
        .format_timestamp_secs()
        .init();

    log::debug!("CLI arguments parsed: {:?}", args);

    // Load configuration.
    // We attempt to load the config file if provided, otherwise look in default locations.
    let config = match load_configuration(args.config).await {
        Ok(cfg) => cfg,
        Err(e) => {
            // Log the specific error chain for debugging
            log::error!("Failed to load configuration: {:#?}", e);
            // Return a generic error message to stdout
            eprintln!("Error: Failed to load configuration. {}", e);
            return ExitCode::FAILURE;
        }
    };

    log::info!("Configuration loaded successfully.");

    // Execute the core application logic.
    // We wrap this in a generic catch-all to ensure we can log unexpected crashes.
    match run_app(config).await {
        Ok(_) => {
            log::info!("Application executed successfully.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            // Use anyhow to format the error chain nicely
            log::error!("Application error: {:#}", e);
            
            // Check if the root cause is a specific user error or a system error
            if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                eprintln!("IO Error: {}", io_err);
            } else {
                eprintln!("Error: {}", e);
            }
            
            ExitCode::FAILURE
        }
    }
}

/// Helper function to load configuration from a file path.
///
/// If `path` is None, it attempts to find a default config file (e.g., in `~/.config/app_name`).
async fn load_configuration(path: Option<PathBuf>) -> Result<AppConfig, anyhow::Error> {
    let config_path = if let Some(p) = path {
        p
    } else {
        // Logic to find default config
        get_default_config_path()?
    };

    log::debug!("Loading configuration from: {:?}", config_path);

    // Verify file exists
    if !config_path.exists() {
        return Err(AppError::ConfigNotFound(config_path)).context(format!(
            "Could not find configuration file at {:?}",
            config_path
        ));
    }

    // Read and parse config (assuming AppConfig implements Deserialize)
    let contents = tokio::fs::read_to_string(&config_path)
        .await
        .context("Failed to read configuration file")?;

    let config: AppConfig = serde_json::from_str(&contents)
        .or_else(|_| serde_yaml::from_str(&contents))
        .context("Failed to parse configuration file (JSON or YAML)")?;

    Ok(config)
}

/// Determines the default configuration path based on the OS.
fn get_default_config_path() -> Result<PathBuf, anyhow::Error> {
    // Example standard for Linux/macOS. Adjust for Windows using APPDATA if needed.
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    
    // Constructing a path like ~/.config/my_app/config.json
    let config_dir = home.join(".config").join("my_app");
    Ok(config_dir.join("config.json"))
}

```
