//! TSZ Conformance Test Runner
//!
//! High-performance Rust implementation for testing tsz TypeScript compiler.

use clap::Parser;
use tsz_conformance::cli::Args;
use tsz_conformance::runner::Runner;

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

    if let Some(shard_count) = args.plan {
        let plan = tsz_conformance::runner::plan::build_shard_plan(&args, shard_count)?;
        println!("{}", serde_json::to_string(&plan)?);
        return Ok(());
    }

    // Run tests
    let runner = Runner::new(args.clone())?;
    let stats = runner.run().await?;

    // Exit with appropriate code
    if !stats.has_result_bijection() || stats.has_terminal_failure() {
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
    let cache: HashMap<String, tsz_conformance::tsc_results::TscResult> =
        serde_json::from_str(&content)?;

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
