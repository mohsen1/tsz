//! CLI argument parsing using clap
//!
//! Defines all command-line arguments for the conformance runner.

use clap::Parser;

/// Backend mode for running conformance tests.
#[derive(Clone, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum RunMode {
    /// Use `tsz --batch` process pool (existing behavior).
    #[default]
    Cli,
    /// Use `tsz-server --protocol legacy` for cached lib loading.
    Server,
}

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

    /// Number of tests to skip from the beginning (applied after sorting, before --max)
    #[arg(short = 'o', long, default_value_t = 0)]
    pub offset: usize,

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

    /// Print test file contents with line numbers (enables verbose mode)
    #[arg(long)]
    pub print_test_files: bool,

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

    /// Path to tsz binary for compilation.
    /// When omitted (`tsz`), the runner prefers `./.target/dist-fast/tsz` if present.
    #[arg(long, default_value = "tsz")]
    pub tsz_binary: String,

    /// Timeout per test in seconds (0 = no timeout)
    #[arg(long, default_value_t = 20)]
    pub timeout: u64,

    /// Print fingerprint deltas for failed tests (when available).
    #[arg(long)]
    pub print_fingerprints: bool,

    /// Disable batch process pool and fall back to spawning a fresh tsz
    /// process per test (slower but useful for debugging).
    #[arg(long)]
    pub no_batch: bool,

    /// Max compilations per batch worker before recycling (0 = no limit).
    /// Recycling kills the worker process and spawns a fresh one, returning all
    /// accumulated memory (global caches, arena fragmentation) to the OS.
    /// With 4 CI workers, 100 means first recycles happen at ~400 total tests,
    /// keeping peak RSS well under the ~7GB CI runner limit.
    #[arg(long, default_value_t = 100)]
    pub max_compilations_per_worker: usize,

    /// Max RSS (in MB) per batch worker before recycling (0 = no limit).
    /// After each compilation, the worker's resident memory is checked. If it
    /// exceeds this threshold, the worker is killed and respawned. This prevents
    /// individual memory-hungry tests (JSX, JSDoc, large multi-file) from pushing
    /// the total process tree past the CI runner's RAM limit.
    #[arg(long, default_value_t = 1536)]
    pub max_worker_rss_mb: usize,

    /// Backend mode: "cli" (default, tsz --batch) or "server" (tsz-server --protocol legacy).
    #[arg(long, default_value = "cli", value_enum)]
    pub mode: RunMode,

    /// Path to tsz-server binary (used when --mode=server).
    /// Defaults to tsz-server next to the tsz binary.
    #[arg(long)]
    pub server_binary: Option<String>,

    /// Write structured parity diff artifacts for failed tests.
    #[arg(long)]
    pub write_diff_artifacts: bool,

    /// Directory for parity diff artifacts.
    #[arg(long, default_value = "./artifacts/conformance/diffs")]
    pub diff_artifacts_dir: String,
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

    /// Resolve the tsz-server binary path.
    #[allow(dead_code)] // Used once server mode is wired up
    pub fn resolved_server_binary(&self) -> String {
        if let Some(ref bin) = self.server_binary {
            return bin.clone();
        }
        let tsz = &self.tsz_binary;
        if tsz.ends_with("/tsz") || tsz.ends_with("\\tsz") {
            format!("{}-server", tsz)
        } else {
            "tsz-server".to_string()
        }
    }

    /// Check if verbose mode should be enabled (either explicitly or via print_test_files)
    pub fn is_verbose(&self) -> bool {
        self.verbose || self.print_test_files
    }
}

#[cfg(test)]
mod tests {
    use super::Args;
    use clap::Parser;

    fn parse_args(input: &[&str]) -> Args {
        Args::try_parse_from(input).expect("argument parsing should succeed in test")
    }

    #[test]
    fn is_verbose_uses_explicit_verbose_flag() {
        let args = parse_args(&["tsz-conformance", "--verbose"]);
        assert!(args.is_verbose());
        assert!(args.validate().is_ok());
    }

    #[test]
    fn is_verbose_is_enabled_by_print_test_files() {
        let args = parse_args(&["tsz-conformance", "--print-test-files"]);
        assert!(args.is_verbose());
    }

    #[test]
    fn is_verbose_stays_false_when_both_flags_are_off() {
        let args = parse_args(&["tsz-conformance"]);
        assert!(!args.is_verbose());
    }

    #[test]
    fn validate_accepts_all_mode_without_extra_post_processing() {
        let args = parse_args(&["tsz-conformance", "--all"]);
        assert!(args.validate().is_ok());
        assert!(args.is_verbose() == (args.verbose || args.print_test_files));
    }
}
