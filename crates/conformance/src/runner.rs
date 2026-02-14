//! Parallel test runner
//!
//! Orchestrates parallel test execution using tokio and compares results.

use crate::cache::{self, load_cache};
use crate::cli::Args;
use crate::test_parser::{
    expand_option_variants, filter_incompatible_module_resolution_variants, parse_test_file,
    should_skip_test,
};
use crate::tsc_results::{DiagnosticFingerprint, ErrorFrequency, TestResult, TestStats};
use crate::tsz_wrapper;
use anyhow::Context;
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

/// Format a path relative to a base directory for display
fn relative_display(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

/// Collects paths of crashed and timed-out tests for the final summary.
#[derive(Default)]
struct ProblemTests {
    crashed: std::sync::Mutex<Vec<String>>,
    timed_out: std::sync::Mutex<Vec<String>>,
}

/// Test runner
pub struct Runner {
    args: Args,
    tsz_binary: String,
    cache: Arc<crate::cache::TscCache>,
    stats: Arc<TestStats>,
    error_freq: Arc<ErrorFrequency>,
    problems: Arc<ProblemTests>,
}

impl Runner {
    /// Create a new runner
    pub fn new(args: Args) -> anyhow::Result<Self> {
        // Load cache
        let cache_path = Path::new(&args.cache_file);
        let cache = if cache_path.exists() {
            load_cache(cache_path)
                .with_context(|| format!("Failed to load cache from {}", args.cache_file))?
        } else {
            warn!("Cache file not found, starting with empty cache");
            HashMap::new()
        };

        info!("Loaded {} cached TSC results", cache.len());

        let tsz_binary = args.tsz_binary.clone();

        Ok(Self {
            args,
            tsz_binary,
            cache: Arc::new(cache),
            stats: Arc::new(TestStats::default()),
            error_freq: Arc::new(ErrorFrequency::default()),
            problems: Arc::new(ProblemTests::default()),
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

        let error_code_filter = self.args.error_code;
        let timeout_secs = self.args.timeout;
        let compare_fingerprints = self.args.compare_fingerprints;
        let print_fingerprints = self.args.print_fingerprints;
        let test_dir: PathBuf = PathBuf::from(&self.args.test_dir);

        stream::iter(test_files)
            .for_each_concurrent(Some(concurrency_limit), |path| {
                let permit = semaphore.clone();
                let cache = self.cache.clone();
                let stats = self.stats.clone();
                let error_freq = self.error_freq.clone();
                let problems = self.problems.clone();
                let tsz_binary = self.tsz_binary.clone();
                let verbose = self.args.is_verbose();
                let print_test = self.args.print_test;
                let print_test_files = self.args.print_test_files;
                let base = base_path.clone();
                let test_dir = test_dir.clone();
                let compare_fingerprints = compare_fingerprints;
                let print_fingerprints = print_fingerprints;

                async move {
                    let _permit = permit.acquire().await.unwrap();
                    let rel_path = relative_display(&path, &base);

                    match Self::run_test(
                        &path,
                        &test_dir,
                        cache,
                        tsz_binary,
                        compare_fingerprints,
                        verbose,
                        print_test_files,
                        timeout_secs,
                    )
                    .await
                    {
                        Ok(result) => {
                            // Update stats
                            stats.total.fetch_add(1, Ordering::SeqCst);

                            match result {
                                TestResult::Pass => {
                                    stats.passed.fetch_add(1, Ordering::SeqCst);
                                }
                                TestResult::Fail {
                                    expected,
                                    actual,
                                    missing,
                                    extra,
                                    missing_fingerprints,
                                    extra_fingerprints,
                                    options,
                                } => {
                                    stats.failed.fetch_add(1, Ordering::SeqCst);

                                    // Filter by error code if specified
                                    let should_print = match error_code_filter {
                                        Some(code) => {
                                            expected.contains(&code) || actual.contains(&code)
                                        }
                                        None => true,
                                    };

                                    if should_print {
                                        println!("FAIL {}", rel_path);

                                        if print_test {
                                            let expected_str: Vec<String> = expected
                                                .iter()
                                                .map(|c| format!("TS{}", c))
                                                .collect();
                                            let actual_str: Vec<String> =
                                                actual.iter().map(|c| format!("TS{}", c)).collect();
                                            println!("  expected: [{}]", expected_str.join(", "));
                                            println!("  actual:   [{}]", actual_str.join(", "));

                                            // Print resolved compiler options
                                            if !options.is_empty() {
                                                let opts_str: Vec<String> = options
                                                    .iter()
                                                    .map(|(k, v)| format!("{}: {}", k, v))
                                                    .collect();
                                                println!("  options:  {{{}}}", opts_str.join(", "));
                                            } else {
                                                println!("  options:  {{}}");
                                            }
                                        }

                                        if print_fingerprints {
                                            if missing_fingerprints.is_empty() {
                                                println!("  missing-fingerprints: []");
                                            } else {
                                                println!("  missing-fingerprints:");
                                                for fingerprint in &missing_fingerprints {
                                                    println!("    - {}", fingerprint.display_key());
                                                }
                                            }
                                            if extra_fingerprints.is_empty() {
                                                println!("  extra-fingerprints: []");
                                            } else {
                                                println!("  extra-fingerprints:");
                                                for fingerprint in &extra_fingerprints {
                                                    println!("    - {}", fingerprint.display_key());
                                                }
                                            }
                                        }
                                    }

                                    // Record error frequencies
                                    for code in missing {
                                        error_freq.record_missing(code);
                                    }
                                    for code in extra {
                                        error_freq.record_extra(code);
                                    }
                                    for fingerprint in missing_fingerprints {
                                        error_freq.record_missing_fingerprint(fingerprint);
                                    }
                                    for fingerprint in extra_fingerprints {
                                        error_freq.record_extra_fingerprint(fingerprint);
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
                                    problems.crashed.lock().unwrap().push(rel_path.clone());
                                    println!("CRASH {}", rel_path);
                                }
                                TestResult::Timeout => {
                                    stats.timeout.fetch_add(1, Ordering::SeqCst);
                                    problems.timed_out.lock().unwrap().push(rel_path.clone());
                                    println!(
                                        "⏱️  TIMEOUT {} (exceeded {}s)",
                                        rel_path, timeout_secs
                                    );
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

        // Re-print crashed and timed-out tests for easy visibility
        let crashed_tests = self.problems.crashed.lock().unwrap();
        let timed_out_tests = self.problems.timed_out.lock().unwrap();
        if !crashed_tests.is_empty() {
            println!();
            println!("Crashed tests ({}):", crashed_tests.len());
            for path in crashed_tests.iter() {
                println!("  CRASH {}", path);
            }
        }
        if !timed_out_tests.is_empty() {
            println!();
            println!("Timed out tests ({}):", timed_out_tests.len());
            for path in timed_out_tests.iter() {
                println!("  TIMEOUT {}", path);
            }
        }
        drop(crashed_tests);
        drop(timed_out_tests);

        println!();
        println!("{}", "=".repeat(60));
        let evaluated = stats.evaluated();
        println!(
            "FINAL RESULTS: {}/{} passed ({:.1}%)",
            stats.passed.load(Ordering::SeqCst),
            evaluated,
            stats.pass_rate()
        );
        println!("  Skipped: {}", stats.skipped.load(Ordering::SeqCst));
        println!("  Crashed: {}", stats.crashed.load(Ordering::SeqCst));
        let timeout_count = stats.timeout.load(Ordering::SeqCst);
        if timeout_count > 0 {
            println!(
                "  ⏱️  Timeout: {} (exceeded {}s limit)",
                timeout_count, timeout_secs
            );
        } else {
            println!("  Timeout: 0");
        }
        println!("  Time: {:.1}s", elapsed.as_secs_f64());

        // Print top error codes
        let top_errors = error_freq.top_errors(10);
        if !top_errors.is_empty() {
            println!();
            println!("Top Error Code Mismatches:");
            for (code, missing, extra) in top_errors {
                println!("  TS{}: missing={}, extra={}", code, missing, extra);
            }
        }

        let top_fingerprint_errors = error_freq.top_fingerprint_errors(10);
        if !top_fingerprint_errors.is_empty() {
            println!();
            println!("Top Diagnostic Fingerprint Mismatches:");
            for (fingerprint, missing, extra) in top_fingerprint_errors {
                println!(
                    "  {} (missing={}, extra={})",
                    fingerprint.display_key(),
                    missing,
                    extra
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
            timeout: AtomicUsize::new(stats.timeout.load(Ordering::SeqCst)),
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
            if path
                .extension()
                .is_some_and(|ext| ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx")
            {
                let path_str = path.to_string_lossy();

                // Skip .d.ts files (declaration files, not test sources)
                if path_str.ends_with(".d.ts") || path_str.ends_with(".d.mts") {
                    continue;
                }

                // Skip fourslash tests (language service tests with special format)
                if path_str.contains("/fourslash/") || path_str.contains("\\fourslash\\") {
                    continue;
                }

                // Skip APISample tests - they require /.ts/typescript.d.ts which is a
                // virtual mount in TSC's test harness pointing to built/local/typescript.d.ts
                if path_str.contains("APISample") || path_str.contains("APILibCheck") {
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

        // Apply offset (skip first N tests)
        if self.args.offset > 0 {
            if self.args.offset >= files.len() {
                files.clear();
            } else {
                files = files.split_off(self.args.offset);
            }
        }

        // Apply max limit
        if files.len() > self.args.max {
            files.truncate(self.args.max);
        }

        Ok(files)
    }

    /// Run a single test
    async fn run_test(
        path: &Path,
        test_dir: &Path,
        cache: Arc<crate::cache::TscCache>,
        tsz_binary: String,
        compare_fingerprints: bool,
        _verbose: bool,
        print_test_files: bool,
        timeout_secs: u64,
    ) -> anyhow::Result<TestResult> {
        // Read file content
        // Skip files with invalid UTF-8 (BOM tests, Unicode encoding tests, etc.)
        let bytes = tokio::fs::read(path).await?;
        let content = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => {
                // Skip test files with non-UTF-8 encoding (UTF-16 BOM tests, etc.)
                return Ok(TestResult::Skipped("non-UTF-8 encoding"));
            }
        };

        // Print test file content with line numbers if requested
        if print_test_files {
            println!("\n📄 Test file: {}", path.display());
            println!("{}", "-".repeat(60));
            for (i, line) in content.lines().enumerate() {
                println!("{:4}: {}", i + 1, line);
            }
            println!("{}", "-".repeat(60));
        }

        // Parse directives
        let parsed = parse_test_file(&content)?;

        // Check if should skip
        if let Some(reason) = should_skip_test(&parsed.directives) {
            return Ok(TestResult::Skipped(reason));
        }

        // Look up cache by relative file path
        let key =
            cache::cache_key(path, test_dir).unwrap_or_else(|| path.to_string_lossy().to_string());

        if let Some(tsc_result) = cache::lookup(&cache, &key) {
            debug!("Cache hit for {}", path.display());

            // Cache hit - prepare test directory (fast sync I/O)
            let options = parsed.directives.options.clone();
            let expanded = expand_option_variants(&options);
            let mut option_variants = filter_incompatible_module_resolution_variants(expanded);
            if option_variants.is_empty() {
                option_variants = vec![options.clone()];
            }

            let mut all_codes = std::collections::HashSet::new();
            let mut all_fingerprints = std::collections::HashSet::new();
            for variant in option_variants {
                let content_clone = content.clone();
                let filenames = parsed.directives.filenames.clone();
                let variant_clone = variant.clone();

                let prepared = tokio::task::spawn_blocking(move || {
                    tsz_wrapper::prepare_test_dir(&content_clone, &filenames, &variant_clone)
                })
                .await??;

                // Spawn tsz process with kill_on_drop — ensures cleanup on timeout
                let child = tokio::process::Command::new(&tsz_binary)
                    .arg("--project")
                    .arg(prepared.temp_dir.path())
                    .arg("--noEmit")
                    .arg("--pretty")
                    .arg("false")
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .kill_on_drop(true)
                    .spawn()?;

                // Wait with timeout — child is auto-killed on drop if timeout fires
                let output = if timeout_secs > 0 {
                    match tokio::time::timeout(
                        Duration::from_secs(timeout_secs),
                        child.wait_with_output(),
                    )
                    .await
                    {
                        Ok(result) => result?,
                        Err(_) => return Ok(TestResult::Timeout),
                    }
                } else {
                    child.wait_with_output().await?
                };

                let compile_result =
                    tsz_wrapper::parse_tsz_output(&output, prepared.temp_dir.path(), variant);
                if compile_result.crashed {
                    return Ok(TestResult::Crashed);
                }

                all_codes.extend(compile_result.error_codes);
                all_fingerprints.extend(compile_result.diagnostic_fingerprints);
            }

            // Filter out all error codes for JS files when checkJs is not enabled.
            // In tsc, JS files are only type-checked when checkJs is true;
            // without it, tsc produces no semantic errors for JS files.
            let is_js_file = {
                let p = path.to_string_lossy().to_lowercase();
                p.ends_with(".js")
                    || p.ends_with(".jsx")
                    || p.ends_with(".mjs")
                    || p.ends_with(".cjs")
            };
            let check_js = options
                .get("checkJs")
                .or_else(|| options.get("checkjs"))
                .map(|v| v == "true")
                .unwrap_or(false);
            if is_js_file && !check_js {
                all_codes.clear();
                all_fingerprints.clear();
            }

            let compile_result = tsz_wrapper::CompilationResult {
                error_codes: all_codes.into_iter().collect(),
                diagnostic_fingerprints: all_fingerprints.into_iter().collect(),
                crashed: false,
                options,
            };

            // Compare error codes
            let tsc_codes: std::collections::HashSet<_> =
                tsc_result.error_codes.iter().cloned().collect();
            let tsz_codes: std::collections::HashSet<_> =
                compile_result.error_codes.iter().cloned().collect();

            // Find missing (in TSC but not tsz)
            let missing: Vec<_> = tsc_codes.difference(&tsz_codes).cloned().collect();
            // Find extra (in tsz but not TSC)
            let extra: Vec<_> = tsz_codes.difference(&tsc_codes).cloned().collect();

            let tsc_fingerprints: std::collections::HashSet<DiagnosticFingerprint> =
                tsc_result.diagnostic_fingerprints.iter().cloned().collect();
            let tsz_fingerprints: std::collections::HashSet<DiagnosticFingerprint> = compile_result
                .diagnostic_fingerprints
                .iter()
                .cloned()
                .collect();
            let use_fingerprint_compare = compare_fingerprints && !tsc_fingerprints.is_empty();
            let mut missing_fingerprints: Vec<DiagnosticFingerprint> = if use_fingerprint_compare {
                tsc_fingerprints
                    .difference(&tsz_fingerprints)
                    .cloned()
                    .collect()
            } else {
                vec![]
            };
            let mut extra_fingerprints: Vec<DiagnosticFingerprint> = if use_fingerprint_compare {
                tsz_fingerprints
                    .difference(&tsc_fingerprints)
                    .cloned()
                    .collect()
            } else {
                vec![]
            };
            missing_fingerprints.sort_by_key(|f| {
                (
                    f.code,
                    f.file.clone(),
                    f.line,
                    f.column,
                    f.message_key.clone(),
                )
            });
            extra_fingerprints.sort_by_key(|f| {
                (
                    f.code,
                    f.file.clone(),
                    f.line,
                    f.column,
                    f.message_key.clone(),
                )
            });

            if missing.is_empty()
                && extra.is_empty()
                && (!use_fingerprint_compare
                    || (missing_fingerprints.is_empty() && extra_fingerprints.is_empty()))
            {
                Ok(TestResult::Pass)
            } else {
                // Sort the codes for consistent display
                let mut expected = tsc_result.error_codes.clone();
                let mut actual = compile_result.error_codes.clone();
                expected.sort();
                actual.sort();
                Ok(TestResult::Fail {
                    expected,
                    actual,
                    missing,
                    extra,
                    missing_fingerprints,
                    extra_fingerprints,
                    options: compile_result.options,
                })
            }
        } else {
            debug!("Cache miss for {}", path.display());

            // Cache miss - run tsz anyway (but we can't compare without TSC results)
            // Return Skipped with reason "no TSC cache"
            Ok(TestResult::Skipped("no TSC cache"))
        }
    }
}
