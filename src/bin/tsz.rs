use anyhow::{Context, Result};
use clap::Parser;
use std::io::IsTerminal;

use wasm::cli::args::CliArgs;
use wasm::cli::{driver, reporter::Reporter, watch};

fn main() -> Result<()> {
    // Initialize tracing with RUST_LOG environment variable support
    // Use RUST_LOG=debug for detailed tracing, RUST_LOG=trace for everything
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into())
        )
        .init();

    let args = CliArgs::parse();
    let args = CliArgs::parse();
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;

    if args.watch {
        return watch::run(&args, &cwd);
    }

    let result = driver::compile(&args, &cwd)?;

    if !result.diagnostics.is_empty() {
        let color = std::io::stdout().is_terminal();
        let mut reporter = Reporter::new(color);
        let output = reporter.render(&result.diagnostics);
        if !output.is_empty() {
            eprintln!("{output}");
        }
    }

    let has_errors = result
        .diagnostics
        .iter()
        .any(|diag| diag.category == wasm::checker::types::diagnostics::DiagnosticCategory::Error);

    if has_errors {
        std::process::exit(1);
    }

    Ok(())
}
