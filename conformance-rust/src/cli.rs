//! CLI argument parsing using clap
//!
//! Defines all command-line arguments for the conformance runner.

use clap::Parser;

/// TypeScript Conformance Test Runner
///
/// High-performance Rust implementation for testing tsz TypeScript compiler
/// against the official TypeScript test suite.
#[derive(Parser, Debug, Clone)]
#[command(name = "tsz-conformance")]
#[command(about, long_about = None)]
pub struct Args {
    /// Maximum number of tests to run
    #[arg(short = 'm', long, default_value_t = 99999)]
    pub max: usize,

    /// Number of parallel workers
    #[arg(short = 'w', long, default_value_t = num_cpus::get().saturating_sub(1))]
    pub workers: usize,

    /// Filter tests by error code (e.g., 2304 for TS2304)
    #[arg(long)]
    pub error_code: Option<u32>,

    /// Verbose output - show details for each test
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Print test file names while running
    #[arg(long)]
    pub print_test: bool,

    /// Filter pattern for test files
    #[arg(long)]
    pub filter: Option<String>,

    /// Show cache status
    #[arg(long)]
    pub cache_status: bool,

    /// Clear the cache
    #[arg(long)]
    pub cache_clear: bool,

    /// Run all tests (no limit)
    #[arg(long)]
    pub all: bool,

    /// Test directory path
    #[arg(long, default_value = "./TypeScript/tests/cases")]
    pub test_dir: String,

    /// Path to TSC cache JSON file
    #[arg(long, default_value = "./tsc-cache.json")]
    pub cache_file: String,

    /// Path to tsz binary for compilation
    #[arg(long, default_value = "../target/release/tsz")]
    pub tsz_binary: String,
}

impl Args {
    /// Validate arguments and apply any post-processing
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.all {
            // --all flag just means use a very high max
            // No additional validation needed
        }
        Ok(())
    }
}
