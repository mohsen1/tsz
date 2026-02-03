//! Parallel test runner
//!
//! Orchestrates parallel test execution using tokio and compares results.

use crate::cache::{calculate_test_hash, check_cache_metadata, load_cache};
use crate::cli::Args;
use crate::test_parser::{parse_test_file, should_skip_test};
use crate::tsc_results::{ErrorFrequency, TestResult, TestStats};
use crate::tsz_wrapper;
use anyhow::Context;
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

/// Format a path relative to a base directory for display
fn relative_display(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

/// Test runner
pub struct Runner {
    args: Args,
    tsz_binary: String,
    cache: Arc<crate::cache::TscCache>,
    stats: Arc<TestStats>,
    error_freq: Arc<ErrorFrequency>,
}

impl Runner {
    /// Create a new runner
    pub fn new(args: Args) -> anyhow::Result<Self> {
        // Load cache
        let cache_path = Path::new(&args.cache_file);
        let cache = if cache_path.exists() {
            load_cache(&cache_path)
                .with_context(|| format!("Failed to load cache from {}", args.cache_file))?
        } else {
            warn!("Cache file not found, starting with empty cache");
            HashMap::new()
        };

        info!(
            "Loaded {} cached TSC results",
            cache.len()
        );

        let tsz_binary = args.tsz_binary.clone();

        Ok(Self {
            args,
            tsz_binary,
            cache: Arc::new(cache),
            stats: Arc::new(TestStats::default()),
            error_freq: Arc::new(ErrorFrequency::default()),
        })
    }

    /// Run all tests
    pub async fn run(&self) -> anyhow::Result<TestStats> {
        let test_files = self.discover_tests()?;

        if test_files.is_empty() {
            warn!("No test files found!");
            return Ok(TestStats::default());
        }

        info!("Found {} test files", test_files.len());

        // Set up concurrency control
        let concurrency_limit = self.args.workers;
        let semaphore = Arc::new(Semaphore::new(concurrency_limit));

        // Process tests in parallel
        let start = Instant::now();

        // Base path for relative display (current working directory)
        let base_path: PathBuf = std::env::current_dir().unwrap_or_default();

        stream::iter(test_files)
            .for_each_concurrent(Some(concurrency_limit), |path| {
                let permit = semaphore.clone();
                let cache = self.cache.clone();
                let stats = self.stats.clone();
                let error_freq = self.error_freq.clone();
                let tsz_binary = self.tsz_binary.clone();
                let verbose = self.args.verbose;
                let print_test = self.args.print_test;
                let base = base_path.clone();

                async move {
                    let _permit = permit.acquire().await.unwrap();
                    let rel_path = relative_display(&path, &base);

                    match Self::run_test(&path, cache, tsz_binary, verbose).await {
                        Ok(result) => {
                            // Update stats
                            stats.total.fetch_add(1, Ordering::SeqCst);

                            match result {
                                TestResult::Pass => {
                                    stats.passed.fetch_add(1, Ordering::SeqCst);
                                }
                                TestResult::Fail { expected, actual, missing, extra, options } => {
                                    stats.failed.fetch_add(1, Ordering::SeqCst);
                                    println!("FAIL {}", rel_path);

                                    if print_test {
                                        let expected_str: Vec<String> = expected.iter().map(|c| format!("TS{}", c)).collect();
                                        let actual_str: Vec<String> = actual.iter().map(|c| format!("TS{}", c)).collect();
                                        println!("  expected: [{}]", expected_str.join(", "));
                                        println!("  actual:   [{}]", actual_str.join(", "));
                                        
                                        // Print resolved compiler options
                                        if !options.is_empty() {
                                            let opts_str: Vec<String> = options.iter()
                                                .map(|(k, v)| format!("{}: {}", k, v))
                                                .collect();
                                            println!("  options:  {{{}}}", opts_str.join(", "));
                                        } else {
                                            println!("  options:  {{}}");
                                        }
                                    }

                                    // Record error frequencies
                                    for code in missing {
                                        error_freq.record_missing(code);
                                    }
                                    for code in extra {
                                        error_freq.record_extra(code);
                                    }
                                }
                                TestResult::Skipped(reason) => {
                                    stats.skipped.fetch_add(1, Ordering::SeqCst);
                                    if verbose {
                                        println!("SKIP {} ({})", rel_path, reason);
                                    }
                                }
                                TestResult::Crashed => {
                                    stats.crashed.fetch_add(1, Ordering::SeqCst);
                                    println!("CRASH {} (CRASHED)", rel_path);
                                }
                            }
                        }
                        Err(e) => {
                            stats.total.fetch_add(1, Ordering::SeqCst);
                            stats.failed.fetch_add(1, Ordering::SeqCst);
                            eprintln!("FAIL {} (ERROR: {})", rel_path, e);
                        }
                    }
                }
            })
            .await;

        let elapsed = start.elapsed();

        // Print summary
        let stats = &self.stats;
        let error_freq = &self.error_freq;

        println!();
        println!("{}", "=".repeat(60));
        println!(
            "FINAL RESULTS: {}/{} passed ({:.1}%)",
            stats.passed.load(Ordering::SeqCst),
            stats.total.load(Ordering::SeqCst),
            stats.pass_rate()
        );
        println!(
            "  Skipped: {}",
            stats.skipped.load(Ordering::SeqCst)
        );
        println!(
            "  Crashed: {}",
            stats.crashed.load(Ordering::SeqCst)
        );
        println!("  Time: {:.1}s", elapsed.as_secs_f64());

        // Print top error codes
        let top_errors = error_freq.top_errors(10);
        if !top_errors.is_empty() {
            println!();
            println!("Top Error Code Mismatches:");
            for (code, missing, extra) in top_errors {
                println!(
                    "  TS{}: missing={}, extra={}",
                    code, missing, extra
                );
            }
        }

        println!("{}", "=".repeat(60));

        // Return a summary (note: this is before the final stats are cloned)
        Ok(TestStats {
            total: AtomicUsize::new(stats.total.load(Ordering::SeqCst)),
            passed: AtomicUsize::new(stats.passed.load(Ordering::SeqCst)),
            failed: AtomicUsize::new(stats.failed.load(Ordering::SeqCst)),
            skipped: AtomicUsize::new(stats.skipped.load(Ordering::SeqCst)),
            crashed: AtomicUsize::new(stats.crashed.load(Ordering::SeqCst)),
        })
    }

    /// Discover all test files recursively using walkdir
    fn discover_tests(&self) -> anyhow::Result<Vec<PathBuf>> {
        use walkdir::WalkDir;

        let test_dir = &self.args.test_dir;
        let mut files = Vec::new();

        // Walk directory tree recursively
        for entry in WalkDir::new(test_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Skip directories
            if path.is_dir() {
                continue;
            }

            // Check file extension
            if path.extension().map_or(false, |ext| {
                ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx"
            }) {
                let path_str = path.to_string_lossy();
                
                // Skip fourslash tests (language service tests with special format)
                if path_str.contains("/fourslash/") || path_str.contains("\\fourslash\\") {
                    continue;
                }
                
                // Apply filter pattern if specified
                if let Some(ref filter) = self.args.filter {
                    if !path_str.contains(filter) {
                        continue;
                    }
                }
                files.push(path.to_path_buf());
            }
        }

        // Sort for deterministic order
        files.sort();

        // Apply max limit
        if files.len() > self.args.max {
            files.truncate(self.args.max);
        }

        Ok(files)
    }

    /// Run a single test
    async fn run_test(
        path: &Path,
        cache: Arc<crate::cache::TscCache>,
        tsz_binary: String,
        _verbose: bool,
    ) -> anyhow::Result<TestResult> {
        // CRITICAL PERFORMANCE OPTIMIZATION:
        // Get metadata FIRST (fast syscall) before reading file content
        let metadata = tokio::fs::metadata(path).await?;
        let mtime_ms = metadata
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64;
        let size = metadata.len() as u64;

        // Read file content (only if we need it)
        let content = tokio::fs::read_to_string(path).await?;

        // Parse directives
        let parsed = parse_test_file(&content)?;

        // Check if should skip
        if let Some(reason) = should_skip_test(&parsed.directives) {
            return Ok(TestResult::Skipped(reason));
        }

        // Calculate hash
        let hash = calculate_test_hash(&content, &parsed.directives.options);

        // Check cache (using hash as key, metadata for validation)
        if let Some(tsc_result) = check_cache_metadata(&cache, &hash, mtime_ms, size) {
            debug!("Cache hit for {}", path.display());

            // Cache hit - run tsz and compare
            let compile_result =
                tokio::task::spawn_blocking(move || {
                    tsz_wrapper::compile_test(
                        &content,
                        &parsed.directives.filenames,
                        &parsed.directives.options,
                        &tsz_binary,
                    )
                })
                .await??;

            // Check for crash
            if compile_result.crashed {
                return Ok(TestResult::Crashed);
            }

            // Compare error codes
            let tsc_codes: std::collections::HashSet<_> =
                tsc_result.error_codes.iter().cloned().collect();
            let tsz_codes: std::collections::HashSet<_> =
                compile_result.error_codes.iter().cloned().collect();

            // Find missing (in TSC but not tsz)
            let missing: Vec<_> = tsc_codes.difference(&tsz_codes).cloned().collect();
            // Find extra (in tsz but not TSC)
            let extra: Vec<_> = tsz_codes.difference(&tsc_codes).cloned().collect();

            if missing.is_empty() && extra.is_empty() {
                return Ok(TestResult::Pass);
            } else {
                // Sort the codes for consistent display
                let mut expected = tsc_result.error_codes.clone();
                let mut actual = compile_result.error_codes.clone();
                expected.sort();
                actual.sort();
                return Ok(TestResult::Fail { 
                    expected, 
                    actual, 
                    missing, 
                    extra,
                    options: compile_result.options,
                });
            }
        } else {
            debug!("Cache miss for {}", path.display());

            // Cache miss - run tsz anyway (but we can't compare without TSC results)
            // Return Skipped with reason "no TSC cache"
            return Ok(TestResult::Skipped("no TSC cache"));
        }
    }
}
