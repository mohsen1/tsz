//! CLI argument parsing using clap
//!
//! Defines all command-line arguments for the conformance runner.

use clap::Parser;

/// Backend mode for running conformance tests.
#[derive(Clone, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum RunMode {
    /// Spawn one fresh TSZ process and capture both stdout and stderr.
    #[default]
    Fresh,
    /// Noncanonical pooled transport; retained only for performance harness work.
    Batch,
    /// Noncanonical server transport; retained only for performance harness work.
    Server,
}

/// Strategy for assigning tests to conformance shards.
#[derive(Clone, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ShardStrategy {
    /// Stable path hash. Keeps historical behavior and maximizes assignment stability.
    #[default]
    Hash,
    /// Greedy weighted packing using historical timings when available.
    Weighted,
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

    /// Round-robin shard spec, formatted as index/count after sorting and filtering
    #[arg(long)]
    pub shard: Option<String>,

    /// Emit a JSON plan for N conformance shards, then exit.
    #[arg(long, value_name = "N")]
    pub plan: Option<usize>,

    /// Shard assignment strategy.
    #[arg(long, default_value = "hash", value_enum)]
    pub shard_strategy: ShardStrategy,

    /// JSON file with historical conformance test weights.
    #[arg(long)]
    pub shard_weights: Option<String>,

    /// Write per-test timing data as JSON for future weighted shard planning.
    #[arg(long)]
    pub timings_file: Option<String>,

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

    /// Exact candidate-domain manifest paired with the TSC cache.
    #[arg(long, default_value = "./scripts/conformance/conformance-domain.json")]
    pub domain_file: String,

    /// Path to tsz binary for compilation.
    /// When omitted (`tsz`), the runner prefers `./.target/dist-fast/tsz` if present.
    #[arg(long, default_value = "tsz")]
    pub tsz_binary: String,

    /// Timeout per test in seconds (0 = no timeout)
    #[arg(long, default_value_t = 90)]
    pub timeout: u64,

    /// Print fingerprint deltas for failed tests (when available).
    #[arg(long)]
    pub print_fingerprints: bool,

    /// Backend mode. Only fresh mode is canonical and may produce parity claims.
    #[arg(long, default_value = "fresh", value_enum)]
    pub mode: RunMode,

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
        if self.mode != RunMode::Fresh {
            anyhow::bail!(
                "--mode {:?} is a noncanonical performance transport and cannot score conformance",
                self.mode
            );
        }
        if self.all {
            // --all flag just means use a very high max
            // No additional validation needed
        }
        if let Some(count) = self.plan {
            if count == 0 {
                anyhow::bail!("--plan count must be greater than zero");
            }
            if self.shard.is_some() {
                anyhow::bail!("--plan cannot be combined with --shard");
            }
        }
        Ok(())
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
    fn default_timeout_covers_slow_full_suite_fixtures() {
        let args = parse_args(&["tsz-conformance"]);
        assert_eq!(args.timeout, 90);
        assert_eq!(args.mode, super::RunMode::Fresh);
    }

    #[test]
    fn pooled_and_server_modes_are_explicit_nonclaims() {
        for mode in ["batch", "server"] {
            let args = parse_args(&["tsz-conformance", "--mode", mode]);
            assert!(args.validate().is_err());
        }
    }

    #[test]
    fn validate_accepts_all_mode_without_extra_post_processing() {
        let args = parse_args(&["tsz-conformance", "--all"]);
        assert!(args.validate().is_ok());
        assert!(args.is_verbose() == (args.verbose || args.print_test_files));
    }

    #[test]
    fn validate_accepts_positive_plan_count() {
        let args = parse_args(&["tsz-conformance", "--plan", "4"]);
        assert_eq!(args.plan, Some(4));
        assert!(args.validate().is_ok());
    }

    #[test]
    fn validate_rejects_zero_plan_count() {
        let args = parse_args(&["tsz-conformance", "--plan", "0"]);
        assert!(args.validate().is_err());
    }

    #[test]
    fn validate_rejects_plan_with_selected_shard() {
        let args = parse_args(&["tsz-conformance", "--plan", "4", "--shard", "0/4"]);
        assert!(args.validate().is_err());
    }
}
