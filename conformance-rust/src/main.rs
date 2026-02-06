//! TSZ Conformance Test Runner
//!
//! High-performance Rust implementation for testing tsz TypeScript compiler.

mod cache;
mod cli;
mod runner;
mod test_parser;
mod tsc_results;
mod tsz_wrapper;

use clap::Parser;
use std::sync::atomic::Ordering;

use cli::Args;
use runner::Runner;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "tsz_conformance=info,warn".to_string()),
        )
        .init();

    // Parse CLI arguments
    let args = Args::parse();
    args.validate()?;

    // Handle cache commands
    if args.cache_status {
        return handle_cache_status(&args.cache_file);
    }

    if args.cache_clear {
        return handle_cache_clear(&args.cache_file);
    }

    // Run tests
    let runner = Runner::new(args.clone())?;
    let stats = runner.run().await?;

    // Exit with appropriate code
    if stats.failed.load(Ordering::SeqCst) > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// Handle cache status command
fn handle_cache_status(cache_path: &str) -> anyhow::Result<()> {
    use std::collections::HashMap;

    let path = std::path::Path::new(cache_path);
    if !path.exists() {
        println!("Cache file not found: {}", cache_path);
        return Ok(());
    }

    let content = std::fs::read_to_string(path)?;
    let cache: HashMap<String, tsc_results::TscResult> = serde_json::from_str(&content)?;

    println!("TSC Cache Status");
    println!("  File: {}", cache_path);
    println!("  Entries: {}", cache.len());

    Ok(())
}

/// Handle cache clear command
fn handle_cache_clear(cache_path: &str) -> anyhow::Result<()> {
    let path = std::path::Path::new(cache_path);
    if path.exists() {
        std::fs::remove_file(path)?;
        println!("Cache cleared: {}", cache_path);
    } else {
        println!("Cache file not found: {}", cache_path);
    }

    Ok(())
}
