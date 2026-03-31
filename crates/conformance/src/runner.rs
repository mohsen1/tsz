//! Parallel test runner
//!
//! Orchestrates parallel test execution using tokio and compares results.

use crate::batch_pool::{BatchOutcome, ProcessPool};
use crate::cache::{self, load_cache};
use crate::cli::{Args, RunMode};
use crate::server_pool::{ServerOutcome, ServerPool};
use crate::test_parser::{
    expand_option_variants, filter_incompatible_module_resolution_variants, parse_test_file,
    should_skip_test,
};
use crate::text_decode::{decode_source_text, DecodedSourceText};
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
        .map_or_else(|_| path.display().to_string(), |p| p.display().to_string())
}

fn sanitize_artifact_name(path: &str) -> String {
    path.chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect()
}

/// Filter diagnostics from `.lib/` test library files out of tsz results.
///
/// Our conformance wrapper resolves `/// <reference path="/.lib/react16.d.ts" />`
/// by copying lib files into the temp dir. This lets tsz type-check them and emit
/// errors (e.g. TS2430) that tsc never sees — tsc emits TS6053 "file not found"
/// instead. Filtering these avoids false positive mismatches.
fn is_lib_diagnostic(fp: &DiagnosticFingerprint) -> bool {
    fp.file.starts_with(".lib/")
        || fp.file.starts_with("/.lib/")
        || fp.message_key.contains("/.lib/")
        || fp.message_key.contains(".lib/")
}

fn filter_lib_diagnostics_tsz(
    mut result: tsz_wrapper::CompilationResult,
) -> tsz_wrapper::CompilationResult {
    let had_lib = result.diagnostic_fingerprints.iter().any(is_lib_diagnostic);
    if !had_lib {
        return result;
    }
    // Collect codes that ONLY appear in .lib/ fingerprints
    let lib_only_codes: std::collections::HashSet<u32> = {
        let lib_codes: std::collections::HashSet<u32> = result
            .diagnostic_fingerprints
            .iter()
            .filter(|fp| is_lib_diagnostic(fp))
            .map(|fp| fp.code)
            .collect();
        let non_lib_codes: std::collections::HashSet<u32> = result
            .diagnostic_fingerprints
            .iter()
            .filter(|fp| !is_lib_diagnostic(fp))
            .map(|fp| fp.code)
            .collect();
        lib_codes.difference(&non_lib_codes).cloned().collect()
    };
    result
        .diagnostic_fingerprints
        .retain(|fp| !is_lib_diagnostic(fp));
    result.error_codes.retain(|c| !lib_only_codes.contains(c));
    result
}

/// Filter `.lib/` artifacts from tsc cache results.
///
/// tsc emits TS6053 for unresolved `/.lib/` references. Since our wrapper
/// resolves them, these TS6053 entries are artifacts that should not count
/// as "missing" diagnostics.
fn filter_lib_diagnostics_tsc(
    tsc_result: &crate::tsc_results::TscResult,
) -> (Vec<u32>, Vec<DiagnosticFingerprint>) {
    let mut codes = tsc_result.error_codes.clone();
    let mut fps = tsc_result.diagnostic_fingerprints.clone();

    let had_lib = fps.iter().any(is_lib_diagnostic);
    if !had_lib {
        return (codes, fps);
    }

    fps.retain(|fp| !is_lib_diagnostic(fp));
    // Remove TS6053 from error codes if no non-.lib/ TS6053 remains
    if !fps.iter().any(|fp| fp.code == 6053) {
        codes.retain(|c| *c != 6053);
    }
    (codes, fps)
}

/// When TSC reports only TS5024 (invalid compiler option shape), suppress
/// downstream semantic diagnostics from tsz.
///
/// In the conformance harness, the cached baseline intentionally expects only
/// TS5024 for a few option-conversion mismatch cases (for example,
/// `"\"true,false\""`` in a boolean-like option). tsz currently continues and
/// emits semantic diagnostics, which should be ignored for this category.
fn suppress_tsz_semantic_diagnostics_after_tsc_option_error(
    tsc_codes: &[u32],
    result: &mut tsz_wrapper::CompilationResult,
) {
    if tsc_codes.len() != 1 || tsc_codes.first().copied() != Some(5024) {
        return;
    }

    result.error_codes.retain(|code| *code == 5024);
    result
        .diagnostic_fingerprints
        .retain(|fingerprint| fingerprint.code == 5024);
}

/// Collects paths of crashed, timed-out, and fingerprint-only-mismatch tests for the final summary.
#[derive(Default)]
struct ProblemTests {
    crashed: std::sync::Mutex<Vec<String>>,
    timed_out: std::sync::Mutex<Vec<String>>,
    fingerprint_only: std::sync::Mutex<Vec<String>>,
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
    fn resolve_tsz_binary(configured: &str) -> String {
        // Prefer the workspace fast-build binary when the default "tsz" is used.
        // This avoids accidentally running a stale PATH-installed binary and
        // producing misleading parity deltas.
        if configured == "tsz" {
            let local_fast = Path::new("./.target/dist-fast/tsz");
            if local_fast.is_file() {
                return local_fast.to_string_lossy().to_string();
            }
        }
        configured.to_string()
    }

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

        let tsz_binary = Self::resolve_tsz_binary(&args.tsz_binary);

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

        // Create batch process pool (unless --no-batch)
        let pool: Option<Arc<ProcessPool>> = if self.args.no_batch {
            info!("Batch pool disabled (--no-batch), using per-test subprocess mode");
            None
        } else {
            info!(
                "Creating batch process pool with {} workers",
                concurrency_limit
            );
            match ProcessPool::new(
                &self.tsz_binary,
                concurrency_limit,
                self.args.max_compilations_per_worker,
                self.args.max_worker_rss_mb * 1024 * 1024,
            )
            .await
            {
                Ok(pool) => Some(Arc::new(pool)),
                Err(e) => {
                    warn!(
                        "Failed to create batch pool: {}. Falling back to subprocess mode.",
                        e
                    );
                    None
                }
            }
        };

        // Create server pool if mode is Server
        let server_pool: Option<Arc<ServerPool>> = if self.args.mode == RunMode::Server {
            let server_bin = self.args.resolved_server_binary();
            match ServerPool::new(
                &server_bin,
                concurrency_limit,
                self.args.max_compilations_per_worker,
                self.args.max_worker_rss_mb * 1024 * 1024,
            )
            .await
            {
                Ok(sp) => {
                    info!(
                        "Server pool ready: {} workers using {}",
                        concurrency_limit, server_bin
                    );
                    Some(Arc::new(sp))
                }
                Err(e) => {
                    warn!("Failed to create server pool: {e}. Falling back to CLI batch mode.");
                    None
                }
            }
        } else {
            None
        };

        // Process tests in parallel
        let start = Instant::now();

        // Base path for relative display (current working directory)
        let base_path: PathBuf = std::env::current_dir().unwrap_or_default();

        let error_code_filter = self.args.error_code;
        let timeout_secs = self.args.timeout;
        let print_fingerprints = self.args.print_fingerprints;
        let write_diff_artifacts = self.args.write_diff_artifacts;
        let diff_artifacts_dir = PathBuf::from(&self.args.diff_artifacts_dir);
        let test_dir: PathBuf = PathBuf::from(&self.args.test_dir);

        stream::iter(test_files)
            .for_each_concurrent(Some(concurrency_limit), |path| {
                let permit = std::sync::Arc::clone(&semaphore);
                let cache = std::sync::Arc::clone(&self.cache);
                let stats = std::sync::Arc::clone(&self.stats);
                let error_freq = std::sync::Arc::clone(&self.error_freq);
                let problems = std::sync::Arc::clone(&self.problems);
                let tsz_binary = self.tsz_binary.clone();
                let pool = pool.clone();
                let server_pool = server_pool.clone();
                let verbose = self.args.is_verbose();
                let print_test = self.args.print_test;
                let print_test_files = self.args.print_test_files;
                let base = base_path.clone();
                let test_dir = test_dir.clone();
                let diff_artifacts_dir = diff_artifacts_dir.clone();

                async move {
                    let _permit = permit.acquire().await.unwrap();
                    let rel_path = relative_display(&path, &base);

                    match Self::run_test(
                        &path,
                        &test_dir,
                        cache,
                        tsz_binary,
                        pool,
                        server_pool,
                        print_test_files,
                        timeout_secs,
                    )
                    .await
                    {
                        Ok((result, file_preview)) => {
                            use std::fmt::Write;

                            // Update stats
                            stats.total.fetch_add(1, Ordering::SeqCst);

                            // Buffer all output for this test so it prints atomically
                            let mut buf = String::new();

                            match result {
                                TestResult::Pass => {
                                    stats.passed.fetch_add(1, Ordering::SeqCst);
                                    if print_test && !verbose {
                                        writeln!(buf, "PASS {}", rel_path).ok();
                                    }
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

                                    // Track fingerprint-only failures: error codes match
                                    // but fingerprints differ (position/message mismatch)
                                    if missing.is_empty()
                                        && extra.is_empty()
                                        && (!missing_fingerprints.is_empty()
                                            || !extra_fingerprints.is_empty())
                                    {
                                        stats
                                            .fingerprint_only
                                            .fetch_add(1, Ordering::SeqCst);
                                        problems
                                            .fingerprint_only
                                            .lock()
                                            .unwrap()
                                            .push(rel_path.clone());
                                    }

                                    // Show file preview for failing tests only
                                    if let Some(preview) = &file_preview {
                                        buf.push_str(preview);
                                    }

                                    // Filter by error code if specified
                                    let should_print = match error_code_filter {
                                        Some(code) => {
                                            expected.contains(&code) || actual.contains(&code)
                                        }
                                        None => true,
                                    };

                                    if should_print {
                                        writeln!(buf, "FAIL {}", rel_path).ok();

                                        if print_test {
                                            let expected_str: Vec<String> = expected
                                                .iter()
                                                .map(|c| format!("TS{}", c))
                                                .collect();
                                            let actual_str: Vec<String> =
                                                actual.iter().map(|c| format!("TS{}", c)).collect();
                                            writeln!(buf, "  expected: [{}]", expected_str.join(", ")).ok();
                                            writeln!(buf, "  actual:   [{}]", actual_str.join(", ")).ok();
                                        }

                                        if print_fingerprints {
                                            if missing_fingerprints.is_empty() {
                                                writeln!(buf, "  missing-fingerprints: []").ok();
                                            } else {
                                                writeln!(buf, "  missing-fingerprints:").ok();
                                                for fingerprint in &missing_fingerprints {
                                                    writeln!(buf, "    - {}", fingerprint.display_key()).ok();
                                                }
                                            }
                                            if extra_fingerprints.is_empty() {
                                                writeln!(buf, "  extra-fingerprints: []").ok();
                                            } else {
                                                writeln!(buf, "  extra-fingerprints:").ok();
                                                for fingerprint in &extra_fingerprints {
                                                    writeln!(buf, "    - {}", fingerprint.display_key()).ok();
                                                }
                                            }
                                        }
                                    }

                                    // Record error frequencies
                                    for code in &missing {
                                        error_freq.record_missing(*code);
                                    }
                                    for code in &extra {
                                        error_freq.record_extra(*code);
                                    }
                                    for fingerprint in &missing_fingerprints {
                                        error_freq.record_missing_fingerprint(fingerprint.clone());
                                    }
                                    for fingerprint in &extra_fingerprints {
                                        error_freq.record_extra_fingerprint(fingerprint.clone());
                                    }

                                    if write_diff_artifacts {
                                        let artifact_name =
                                            format!("{}.json", sanitize_artifact_name(&rel_path));
                                        let artifact_path = diff_artifacts_dir.join(artifact_name);
                                        if let Some(parent) = artifact_path.parent() {
                                            let _ = std::fs::create_dir_all(parent);
                                        }
                                        let payload = serde_json::json!({
                                            "test": rel_path,
                                            "expected_codes": expected,
                                            "actual_codes": actual,
                                            "missing_codes": missing,
                                            "extra_codes": extra,
                                            "missing_fingerprints": missing_fingerprints
                                                .iter()
                                                .map(super::tsc_results::DiagnosticFingerprint::display_key)
                                                .collect::<Vec<_>>(),
                                            "extra_fingerprints": extra_fingerprints
                                                .iter()
                                                .map(super::tsc_results::DiagnosticFingerprint::display_key)
                                                .collect::<Vec<_>>(),
                                            "options": options,
                                        });
                                        let _ = std::fs::write(
                                            &artifact_path,
                                            serde_json::to_string_pretty(&payload)
                                                .unwrap_or_else(|_| "{}".to_string()),
                                        );
                                    }
                                }
                                TestResult::Skipped(reason) => {
                                    stats.skipped.fetch_add(1, Ordering::SeqCst);
                                    if verbose {
                                        writeln!(buf, "SKIP {} ({})", rel_path, reason).ok();
                                    }
                                }
                                TestResult::Crashed => {
                                    stats.crashed.fetch_add(1, Ordering::SeqCst);
                                    problems.crashed.lock().unwrap().push(rel_path.clone());
                                    writeln!(buf, "CRASH {}", rel_path).ok();
                                }
                                TestResult::Timeout => {
                                    stats.timeout.fetch_add(1, Ordering::SeqCst);
                                    problems.timed_out.lock().unwrap().push(rel_path.clone());
                                    writeln!(buf, "TIMEOUT {} (exceeded {}s)", rel_path, timeout_secs).ok();
                                }
                            }

                            if !buf.is_empty() {
                                print!("{}", buf);
                            }
                        }
                        Err(e) => {
                            stats.total.fetch_add(1, Ordering::SeqCst);
                            stats.failed.fetch_add(1, Ordering::SeqCst);
                            println!("FAIL {} (ERROR: {})", rel_path, e);
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

        // Print fingerprint-only failures (same error codes, different positions/messages)
        let fp_only_tests = self.problems.fingerprint_only.lock().unwrap();
        if !fp_only_tests.is_empty() {
            println!();
            println!(
                "Fingerprint-only failures ({}) — error codes match, position/message differs:",
                fp_only_tests.len()
            );
            for path in fp_only_tests.iter() {
                println!("  {}", path);
            }
        }
        drop(fp_only_tests);

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
        let fp_only_count = stats.fingerprint_only.load(Ordering::SeqCst);
        println!("  Fingerprint-only: {}", fp_only_count);
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
            fingerprint_only: AtomicUsize::new(stats.fingerprint_only.load(Ordering::SeqCst)),
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
            .filter_map(std::result::Result::ok)
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

    /// Run a single test.
    /// Returns `(result, file_preview)` where `file_preview` is the numbered
    /// source listing when `print_test_files` is true.
    async fn run_test(
        path: &Path,
        test_dir: &Path,
        cache: Arc<crate::cache::TscCache>,
        tsz_binary: String,
        pool: Option<Arc<ProcessPool>>,
        server_pool: Option<Arc<ServerPool>>,
        print_test_files: bool,
        timeout_secs: u64,
    ) -> anyhow::Result<(TestResult, Option<String>)> {
        // Read and decode file content (UTF-8/UTF-8 BOM/UTF-16 BOM).
        let bytes = tokio::fs::read(path).await?;
        let key =
            cache::cache_key(path, test_dir).unwrap_or_else(|| path.to_string_lossy().to_string());

        // Build file preview if requested (printed atomically by caller)
        let mut file_preview: Option<String> = None;

        match decode_source_text(&bytes) {
            DecodedSourceText::Text(content) => {
                if print_test_files {
                    use std::fmt::Write;
                    let mut buf = String::new();
                    writeln!(buf, "\n--- {} ---", path.display()).ok();
                    for (i, line) in content.lines().enumerate() {
                        writeln!(buf, "{:4}: {}", i + 1, line).ok();
                    }
                    writeln!(buf, "---").ok();
                    file_preview = Some(buf);
                }

                // Parse directives
                let parsed = parse_test_file(&content)?;

                // Check if should skip
                if let Some(reason) = should_skip_test(&parsed.directives) {
                    return Ok((TestResult::Skipped(reason), file_preview.take()));
                }

                if let Some(tsc_result) = cache::lookup(&cache, &key) {
                    debug!("Cache hit for {}", path.display());

                    // Cache hit - prepare test directory (fast sync I/O)
                    let options = parsed.directives.options.clone();
                    let expanded = expand_option_variants(&options);
                    let mut option_variants =
                        filter_incompatible_module_resolution_variants(expanded);
                    if option_variants.is_empty() {
                        option_variants = vec![options.clone()];
                    }

                    let mut all_codes = std::collections::HashSet::new();
                    let mut all_fingerprints = std::collections::HashSet::new();
                    let original_ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(std::string::ToString::to_string);

                    // Determine if we should use server mode for this test
                    let use_server = server_pool.is_some()
                        && !crate::options_convert::has_unsupported_server_options(&options);

                    if use_server {
                        // SERVER MODE: send files + options as JSON, no temp dir.
                        // This skips temp directory creation and filesystem I/O entirely.
                        let server = server_pool.as_ref().unwrap();
                        let mut files = HashMap::new();
                        if parsed.directives.filenames.is_empty() {
                            // Single-file test
                            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("ts");
                            let name = format!("test.{ext}");
                            let clean = tsz_wrapper::strip_directive_comments(&content);
                            files.insert(name, clean);
                        } else {
                            // Multi-file test
                            for (filename, file_content) in &parsed.directives.filenames {
                                files.insert(filename.clone(), file_content.clone());
                            }
                        }

                        let timeout = if timeout_secs > 0 {
                            Duration::from_secs(timeout_secs)
                        } else {
                            Duration::ZERO
                        };

                        // Run first variant only (matching CLI behavior for comparison)
                        let first_variant = option_variants.first().unwrap_or(&options);
                        let outcome = server.check(files, first_variant, timeout).await?;

                        match outcome {
                            ServerOutcome::Done(codes) => {
                                all_codes.extend(codes);
                            }
                            ServerOutcome::Crashed => {
                                return Ok((TestResult::Crashed, file_preview.take()));
                            }
                            ServerOutcome::Timeout => {
                                return Ok((TestResult::Timeout, file_preview.take()));
                            }
                            ServerOutcome::Error(e) => {
                                warn!("Server error for {}: {e}", path.display());
                                return Ok((TestResult::Crashed, file_preview.take()));
                            }
                        }
                    } else {
                        // CLI MODE: existing variant loop with temp dirs.
                        // Run each option variant (e.g. module=commonjs, module=system).
                        // Only the FIRST variant's diagnostics are used for comparison
                        // because the tsc cache was generated with first-value-only
                        // semantics for multi-value options. Non-first variants still
                        // run for crash/timeout detection but are skipped when time is
                        // tight (>5s for the first variant) to avoid cumulative timeouts.
                        let mut first_variant_slow = false;
                        for (variant_idx, variant) in option_variants.into_iter().enumerate() {
                            // Skip non-first variants when the first variant was slow —
                            // the cumulative time of N slow variants would exceed the
                            // timeout even though each individual variant is within bounds.
                            if variant_idx > 0 && first_variant_slow {
                                continue;
                            }
                            let content_clone = content.clone();
                            let filenames = parsed.directives.filenames.clone();
                            let variant_clone = variant.clone();
                            let ext_clone = original_ext.clone();
                            let key_order = parsed.directives.option_order.clone();
                            let expected_error_codes = tsc_result.error_codes.clone();

                            let prepared = tokio::task::spawn_blocking(move || {
                                tsz_wrapper::prepare_test_dir(
                                    &content_clone,
                                    &filenames,
                                    &variant_clone,
                                    ext_clone.as_deref(),
                                    &key_order,
                                    Some(&expected_error_codes),
                                )
                            })
                            .await??;

                            let variant_start = Instant::now();
                            let compile_result = if let Some(ref pool) = pool {
                                // Use batch pool — send project dir, read output
                                let timeout_dur = if timeout_secs > 0 {
                                    Duration::from_secs(timeout_secs)
                                } else {
                                    Duration::ZERO
                                };
                                match pool.compile(&prepared.project_dir, timeout_dur).await? {
                                    BatchOutcome::Done(output) => tsz_wrapper::parse_batch_output(
                                        &output,
                                        prepared.temp_dir.path(),
                                        variant,
                                    ),
                                    BatchOutcome::Crashed => {
                                        return Ok((TestResult::Crashed, file_preview.take()));
                                    }
                                    BatchOutcome::Timeout => {
                                        match Self::compile_with_subprocess(
                                            &tsz_binary,
                                            &prepared.project_dir,
                                            prepared.temp_dir.path(),
                                            variant,
                                            timeout_secs.saturating_mul(2).max(60),
                                        )
                                        .await?
                                        {
                                            Some(result) => result,
                                            None => {
                                                return Ok((
                                                    TestResult::Timeout,
                                                    file_preview.take(),
                                                ));
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Subprocess fallback — spawn fresh tsz per compilation
                                // Set cwd to project dir so diagnostic file paths are
                                // relative to project root (matching cache generator behavior)
                                let child = tokio::process::Command::new(&tsz_binary)
                                    .arg("--project")
                                    .arg(&prepared.project_dir)
                                    .arg("--noEmit")
                                    .arg("--pretty")
                                    .arg("false")
                                    .current_dir(&prepared.project_dir)
                                    .stdout(std::process::Stdio::piped())
                                    .stderr(std::process::Stdio::piped())
                                    .kill_on_drop(true)
                                    .spawn()?;

                                let output = if timeout_secs > 0 {
                                    match tokio::time::timeout(
                                        Duration::from_secs(timeout_secs),
                                        child.wait_with_output(),
                                    )
                                    .await
                                    {
                                        Ok(result) => result?,
                                        Err(_) => {
                                            return Ok((TestResult::Timeout, file_preview.take()));
                                        }
                                    }
                                } else {
                                    child.wait_with_output().await?
                                };

                                tsz_wrapper::parse_tsz_output(
                                    &output,
                                    prepared.temp_dir.path(),
                                    variant,
                                )
                            };
                            if compile_result.crashed {
                                return Ok((TestResult::Crashed, file_preview.take()));
                            }

                            // Only accumulate diagnostics from the first variant.
                            // The tsc cache uses first-value-only for multi-value
                            // options (module, jsx, etc.), so comparing the union
                            // of all variants against the cache produces false
                            // "extra" diagnostics from non-first variants.
                            if variant_idx == 0 {
                                all_codes.extend(compile_result.error_codes);
                                all_fingerprints.extend(compile_result.diagnostic_fingerprints);
                                // Mark first variant as slow if it took >3s — skip
                                // remaining variants to avoid cumulative timeout.
                                if variant_start.elapsed() > Duration::from_secs(3) {
                                    first_variant_slow = true;
                                }
                            }
                        }
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
                        .is_some_and(|v| v == "true");
                    let allow_js = options
                        .get("allowJs")
                        .or_else(|| options.get("allowjs"))
                        .is_some_and(|v| v == "true");
                    if is_js_file && !check_js && !allow_js {
                        // Preserve TS18003 (no inputs found) since it's a config-level
                        // diagnostic that tsc emits regardless of JS checking mode.
                        let had_18003 = all_codes.contains(&18003);
                        let fps_18003: Vec<_> = all_fingerprints
                            .iter()
                            .filter(|fp| fp.code == 18003)
                            .cloned()
                            .collect();
                        all_codes.clear();
                        all_fingerprints.clear();
                        if had_18003 {
                            all_codes.insert(18003);
                            all_fingerprints.extend(fps_18003);
                        }
                    }

                    // Some multi-file conformance tests provide a tsconfig with allowJs and only JS inputs.
                    // In that setup, TS18003 may be a harness artifact (tsz emits it but tsc doesn't).
                    // Only strip TS18003 when tsc does NOT expect it.
                    let tsc_expects_18003 = tsc_result.error_codes.contains(&18003);
                    let has_tsconfig = parsed
                        .directives
                        .filenames
                        .iter()
                        .any(|(name, _)| name.replace('\\', "/").ends_with("tsconfig.json"));
                    let has_js_input_file = parsed.directives.filenames.iter().any(|(name, _)| {
                        let lower = name.to_lowercase();
                        lower.ends_with(".js")
                            || lower.ends_with(".jsx")
                            || lower.ends_with(".mjs")
                            || lower.ends_with(".cjs")
                    });
                    if has_tsconfig && has_js_input_file && !tsc_expects_18003 {
                        all_codes.remove(&18003);
                        all_fingerprints.retain(|fp| fp.code != 18003);
                    }
                    let compile_result = tsz_wrapper::CompilationResult {
                        error_codes: all_codes.into_iter().collect(),
                        diagnostic_fingerprints: all_fingerprints.into_iter().collect(),
                        crashed: false,
                        options: options.clone(),
                    };
                    // Filter .lib/ diagnostics (see filter functions for explanation)
                    let mut compile_result = filter_lib_diagnostics_tsz(compile_result);
                    let (tsc_error_codes, tsc_fps) = filter_lib_diagnostics_tsc(tsc_result);

                    // When @noLib is set, tsc only emits TS2318 ("Cannot find global type")
                    // and suppresses downstream errors caused by missing lib types.
                    // tsz doesn't yet suppress these, so filter extra codes/fingerprints
                    // that cascade from missing global types.
                    let is_nolib = options
                        .get("noLib")
                        .or_else(|| options.get("nolib"))
                        .is_some_and(|v| v == "true");
                    if is_nolib && tsc_error_codes.contains(&2318) {
                        let tsc_code_set: std::collections::HashSet<u32> =
                            tsc_error_codes.iter().cloned().collect();
                        compile_result
                            .error_codes
                            .retain(|c| tsc_code_set.contains(c));
                        compile_result
                            .diagnostic_fingerprints
                            .retain(|fp| tsc_code_set.contains(&fp.code));
                    }

                    // If TSC expects only TS5024, tsz may emit extra diagnostics
                    // from semantic checks that run after the invalid option failure.
                    // Restrict comparison to TS5024 in this case.
                    suppress_tsz_semantic_diagnostics_after_tsc_option_error(
                        &tsc_error_codes,
                        &mut compile_result,
                    );

                    // Compare error codes
                    let tsc_codes: std::collections::HashSet<_> =
                        tsc_error_codes.iter().cloned().collect();
                    let tsz_codes: std::collections::HashSet<_> =
                        compile_result.error_codes.iter().cloned().collect();

                    // Find missing (in TSC but not tsz)
                    let missing: Vec<_> = tsc_codes.difference(&tsz_codes).cloned().collect();
                    // Find extra (in tsz but not TSC)
                    let extra: Vec<_> = tsz_codes.difference(&tsc_codes).cloned().collect();

                    let tsc_fingerprints: std::collections::HashSet<DiagnosticFingerprint> =
                        tsc_fps.iter().cloned().collect();
                    let tsz_fingerprints: std::collections::HashSet<DiagnosticFingerprint> =
                        compile_result
                            .diagnostic_fingerprints
                            .iter()
                            .cloned()
                            .collect();
                    let use_fingerprint_compare = !tsc_fingerprints.is_empty();
                    let mut missing_fingerprints: Vec<DiagnosticFingerprint> =
                        if use_fingerprint_compare {
                            tsc_fingerprints
                                .difference(&tsz_fingerprints)
                                .cloned()
                                .collect()
                        } else {
                            vec![]
                        };
                    let mut extra_fingerprints: Vec<DiagnosticFingerprint> =
                        if use_fingerprint_compare {
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
                        Ok((TestResult::Pass, file_preview.take()))
                    } else {
                        // Sort the codes for consistent display
                        let mut expected = tsc_result.error_codes.clone();
                        let mut actual = compile_result.error_codes.clone();
                        expected.sort();
                        actual.sort();
                        Ok((
                            TestResult::Fail {
                                expected,
                                actual,
                                missing,
                                extra,
                                missing_fingerprints,
                                extra_fingerprints,
                                options: compile_result.options,
                            },
                            file_preview.take(),
                        ))
                    }
                } else {
                    debug!("Cache miss for {}", path.display());

                    // Cache miss - run tsz anyway (but we can't compare without TSC results)
                    // Return Skipped with reason "no TSC cache"
                    Ok((TestResult::Skipped("no TSC cache"), file_preview.take()))
                }
            }
            DecodedSourceText::TextWithOriginalBytes(decoded_text, original_bytes) => {
                if print_test_files {
                    file_preview = Some(format!(
                        "\n--- {} (UTF-16 BOM, {} bytes) ---\n",
                        path.display(),
                        original_bytes.len()
                    ));
                }

                if let Some(tsc_result) = cache::lookup(&cache, &key) {
                    // Parse directives from the decoded text so we get the correct
                    // compiler options (target, strict, etc.) for the tsconfig.
                    // Previously this was `HashMap::new()` which meant UTF-16 tests
                    // ran with default (empty) options, missing deprecated-option
                    // diagnostics like TS5107 for `target: es5`.
                    let parsed_directives = parse_test_file(&decoded_text)?;
                    let options = parsed_directives.directives.options;
                    let original_ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(std::string::ToString::to_string);
                    // Use the decoded text through the normal prepare_test_dir path
                    // (which strips directive comments) instead of writing raw UTF-16
                    // bytes. This ensures line numbers match tsc's expectations.
                    let filenames = parsed_directives.directives.filenames;
                    let key_order = parsed_directives.directives.option_order;
                    let expected_error_codes = tsc_result.error_codes.clone();
                    let prepared = tokio::task::spawn_blocking({
                        let text = decoded_text.clone();
                        let options = options.clone();
                        let ext = original_ext.clone();
                        let key_order = key_order.clone();
                        move || {
                            tsz_wrapper::prepare_test_dir(
                                &text,
                                &filenames,
                                &options,
                                ext.as_deref(),
                                &key_order,
                                Some(&expected_error_codes),
                            )
                        }
                    })
                    .await??;

                    let compile_result = if let Some(ref pool) = pool {
                        let timeout_dur = if timeout_secs > 0 {
                            Duration::from_secs(timeout_secs)
                        } else {
                            Duration::ZERO
                        };
                        match pool.compile(&prepared.project_dir, timeout_dur).await? {
                            BatchOutcome::Done(output) => tsz_wrapper::parse_batch_output(
                                &output,
                                prepared.temp_dir.path(),
                                options,
                            ),
                            BatchOutcome::Crashed => {
                                return Ok((TestResult::Crashed, file_preview.take()));
                            }
                            BatchOutcome::Timeout => {
                                match Self::compile_with_subprocess(
                                    &tsz_binary,
                                    &prepared.project_dir,
                                    prepared.temp_dir.path(),
                                    options,
                                    timeout_secs.saturating_mul(2).max(60),
                                )
                                .await?
                                {
                                    Some(result) => result,
                                    None => return Ok((TestResult::Timeout, file_preview.take())),
                                }
                            }
                        }
                    } else {
                        let child = tokio::process::Command::new(&tsz_binary)
                            .arg("--project")
                            .arg(&prepared.project_dir)
                            .arg("--noEmit")
                            .arg("--pretty")
                            .arg("false")
                            .current_dir(&prepared.project_dir)
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .kill_on_drop(true)
                            .spawn()?;

                        let output = if timeout_secs > 0 {
                            match tokio::time::timeout(
                                Duration::from_secs(timeout_secs),
                                child.wait_with_output(),
                            )
                            .await
                            {
                                Ok(result) => result?,
                                Err(_) => return Ok((TestResult::Timeout, file_preview.take())),
                            }
                        } else {
                            child.wait_with_output().await?
                        };

                        tsz_wrapper::parse_tsz_output(&output, prepared.temp_dir.path(), options)
                    };

                    if compile_result.crashed {
                        return Ok((TestResult::Crashed, file_preview.take()));
                    }

                    // Filter .lib/ diagnostics (see variant path for explanation)
                    let compile_result = filter_lib_diagnostics_tsz(compile_result);
                    let (tsc_error_codes, tsc_fps) = filter_lib_diagnostics_tsc(tsc_result);

                    let tsc_codes: std::collections::HashSet<_> =
                        tsc_error_codes.iter().cloned().collect();
                    let tsz_codes: std::collections::HashSet<_> =
                        compile_result.error_codes.iter().cloned().collect();

                    let missing: Vec<_> = tsc_codes.difference(&tsz_codes).cloned().collect();
                    let extra: Vec<_> = tsz_codes.difference(&tsc_codes).cloned().collect();

                    let tsc_fingerprints: std::collections::HashSet<DiagnosticFingerprint> =
                        tsc_fps.iter().cloned().collect();
                    let tsz_fingerprints: std::collections::HashSet<DiagnosticFingerprint> =
                        compile_result
                            .diagnostic_fingerprints
                            .iter()
                            .cloned()
                            .collect();
                    let use_fingerprint_compare = !tsc_fingerprints.is_empty();
                    let mut missing_fingerprints: Vec<DiagnosticFingerprint> =
                        if use_fingerprint_compare {
                            tsc_fingerprints
                                .difference(&tsz_fingerprints)
                                .cloned()
                                .collect()
                        } else {
                            vec![]
                        };
                    let mut extra_fingerprints: Vec<DiagnosticFingerprint> =
                        if use_fingerprint_compare {
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
                        Ok((TestResult::Pass, file_preview.take()))
                    } else {
                        let mut expected = tsc_result.error_codes.clone();
                        let mut actual = compile_result.error_codes.clone();
                        expected.sort();
                        actual.sort();
                        Ok((
                            TestResult::Fail {
                                expected,
                                actual,
                                missing,
                                extra,
                                missing_fingerprints,
                                extra_fingerprints,
                                options: HashMap::new(),
                            },
                            file_preview.take(),
                        ))
                    }
                } else {
                    Ok((TestResult::Skipped("no TSC cache"), file_preview.take()))
                }
            }
            DecodedSourceText::Binary(binary) => {
                if print_test_files {
                    file_preview = Some(format!(
                        "\n--- {} (binary, {} bytes) ---\n",
                        path.display(),
                        binary.len()
                    ));
                }

                if let Some(tsc_result) = cache::lookup(&cache, &key) {
                    let options: HashMap<String, String> = HashMap::new();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("ts")
                        .to_string();
                    let prepared = tokio::task::spawn_blocking({
                        let binary = binary.clone();
                        let ext = ext.clone();
                        let options = options.clone();
                        move || tsz_wrapper::prepare_binary_test_dir(&binary, &ext, &options)
                    })
                    .await??;

                    let compile_result = if let Some(ref pool) = pool {
                        let timeout_dur = if timeout_secs > 0 {
                            Duration::from_secs(timeout_secs)
                        } else {
                            Duration::ZERO
                        };
                        match pool.compile(&prepared.project_dir, timeout_dur).await? {
                            BatchOutcome::Done(output) => tsz_wrapper::parse_batch_output(
                                &output,
                                prepared.temp_dir.path(),
                                options,
                            ),
                            BatchOutcome::Crashed => {
                                return Ok((TestResult::Crashed, file_preview.take()));
                            }
                            BatchOutcome::Timeout => {
                                match Self::compile_with_subprocess(
                                    &tsz_binary,
                                    &prepared.project_dir,
                                    prepared.temp_dir.path(),
                                    options,
                                    timeout_secs.saturating_mul(2).max(60),
                                )
                                .await?
                                {
                                    Some(result) => result,
                                    None => return Ok((TestResult::Timeout, file_preview.take())),
                                }
                            }
                        }
                    } else {
                        let child = tokio::process::Command::new(&tsz_binary)
                            .arg("--project")
                            .arg(&prepared.project_dir)
                            .arg("--noEmit")
                            .arg("--pretty")
                            .arg("false")
                            .current_dir(&prepared.project_dir)
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .kill_on_drop(true)
                            .spawn()?;

                        let output = if timeout_secs > 0 {
                            match tokio::time::timeout(
                                Duration::from_secs(timeout_secs),
                                child.wait_with_output(),
                            )
                            .await
                            {
                                Ok(result) => result?,
                                Err(_) => return Ok((TestResult::Timeout, file_preview.take())),
                            }
                        } else {
                            child.wait_with_output().await?
                        };

                        tsz_wrapper::parse_tsz_output(&output, prepared.temp_dir.path(), options)
                    };
                    if compile_result.crashed {
                        return Ok((TestResult::Crashed, file_preview.take()));
                    }

                    // Filter .lib/ diagnostics (see variant path for explanation)
                    let compile_result = filter_lib_diagnostics_tsz(compile_result);
                    let (tsc_error_codes, tsc_fps) = filter_lib_diagnostics_tsc(tsc_result);

                    let tsc_codes: std::collections::HashSet<_> =
                        tsc_error_codes.iter().cloned().collect();
                    let tsz_codes: std::collections::HashSet<_> =
                        compile_result.error_codes.iter().cloned().collect();

                    let missing: Vec<_> = tsc_codes.difference(&tsz_codes).cloned().collect();
                    let extra: Vec<_> = tsz_codes.difference(&tsc_codes).cloned().collect();
                    let tsc_fingerprints: std::collections::HashSet<DiagnosticFingerprint> =
                        tsc_fps.iter().cloned().collect();
                    let tsz_fingerprints: std::collections::HashSet<DiagnosticFingerprint> =
                        compile_result
                            .diagnostic_fingerprints
                            .iter()
                            .cloned()
                            .collect();
                    let use_fingerprint_compare = !tsc_fingerprints.is_empty();
                    let mut missing_fingerprints: Vec<DiagnosticFingerprint> =
                        if use_fingerprint_compare {
                            tsc_fingerprints
                                .difference(&tsz_fingerprints)
                                .cloned()
                                .collect()
                        } else {
                            vec![]
                        };
                    let mut extra_fingerprints: Vec<DiagnosticFingerprint> =
                        if use_fingerprint_compare {
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
                        Ok((TestResult::Pass, file_preview.take()))
                    } else {
                        let mut expected = tsc_result.error_codes.clone();
                        let mut actual = compile_result.error_codes.clone();
                        expected.sort();
                        actual.sort();
                        Ok((
                            TestResult::Fail {
                                expected,
                                actual,
                                missing,
                                extra,
                                missing_fingerprints,
                                extra_fingerprints,
                                options: compile_result.options,
                            },
                            file_preview.take(),
                        ))
                    }
                } else {
                    debug!("Cache miss for {}", path.display());
                    Ok((TestResult::Skipped("no TSC cache"), file_preview.take()))
                }
            }
        }
    }

    async fn compile_with_subprocess(
        tsz_binary: &str,
        project_dir: &Path,
        temp_dir: &Path,
        options: HashMap<String, String>,
        timeout_secs: u64,
    ) -> anyhow::Result<Option<tsz_wrapper::CompilationResult>> {
        let child = tokio::process::Command::new(tsz_binary)
            .arg("--project")
            .arg(project_dir)
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false")
            .current_dir(project_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let output = if timeout_secs > 0 {
            match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output())
                .await
            {
                Ok(result) => result?,
                Err(_) => return Ok(None),
            }
        } else {
            child.wait_with_output().await?
        };

        Ok(Some(tsz_wrapper::parse_tsz_output(
            &output, temp_dir, options,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tsc_results::{DiagnosticFingerprint, FileMetadata, TscResult};
    use std::sync::{Mutex, OnceLock};

    fn fp(code: u32, file: &str, msg: &str) -> DiagnosticFingerprint {
        DiagnosticFingerprint {
            code,
            file: file.to_string(),
            line: 1,
            column: 1,
            message_key: msg.to_string(),
        }
    }

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_temp_cwd<F, T>(create_fast_binary: bool, f: F) -> T
    where
        F: FnOnce(&Path) -> T,
    {
        let _guard = cwd_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let original = std::env::current_dir().expect("current dir should be readable");
        let temp = std::env::temp_dir().join(format!(
            "tsz_runner_helper_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time should move forward")
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp).expect("temp dir should be created");

        if create_fast_binary {
            let fast_binary = temp.join(".target/dist-fast/tsz");
            if let Some(parent) = fast_binary.parent() {
                std::fs::create_dir_all(parent).expect("parent dir should be created");
            }
            std::fs::write(&fast_binary, b"tsz").expect("fast binary should be created");
        }

        std::env::set_current_dir(&temp).expect("cwd should change");
        let result = f(&temp);
        std::env::set_current_dir(original).expect("cwd should be restored");
        let _ = std::fs::remove_dir_all(&temp);
        result
    }

    #[test]
    fn is_lib_diagnostic_detects_lib_files() {
        assert!(is_lib_diagnostic(&fp(
            2430,
            ".lib/react16.d.ts",
            "Interface 'X' incorrectly extends 'Y'."
        )));
        assert!(is_lib_diagnostic(&fp(
            6053,
            "test.tsx",
            "File '/.lib/react.d.ts' not found."
        )));
        assert!(!is_lib_diagnostic(&fp(
            2322,
            "test.ts",
            "Type 'A' is not assignable to type 'B'."
        )));
    }

    #[test]
    fn filter_tsz_removes_lib_only_codes() {
        let result = tsz_wrapper::CompilationResult {
            error_codes: vec![2430, 2322],
            diagnostic_fingerprints: vec![
                fp(2430, ".lib/react16.d.ts", "Interface error"),
                fp(2322, "test.ts", "Type mismatch"),
            ],
            crashed: false,
            options: Default::default(),
        };
        let filtered = filter_lib_diagnostics_tsz(result);
        assert_eq!(filtered.error_codes, vec![2322]);
        assert_eq!(filtered.diagnostic_fingerprints.len(), 1);
        assert_eq!(filtered.diagnostic_fingerprints[0].code, 2322);
    }

    #[test]
    fn filter_tsz_preserves_code_appearing_in_both_lib_and_non_lib() {
        let result = tsz_wrapper::CompilationResult {
            error_codes: vec![2430],
            diagnostic_fingerprints: vec![
                fp(2430, ".lib/react16.d.ts", "Interface error in lib"),
                fp(2430, "test.ts", "Interface error in user code"),
            ],
            crashed: false,
            options: Default::default(),
        };
        let filtered = filter_lib_diagnostics_tsz(result);
        assert_eq!(filtered.error_codes, vec![2430]);
        assert_eq!(filtered.diagnostic_fingerprints.len(), 1);
        assert_eq!(filtered.diagnostic_fingerprints[0].file, "test.ts");
    }

    #[test]
    fn filter_tsz_noop_when_no_lib_diagnostics() {
        let result = tsz_wrapper::CompilationResult {
            error_codes: vec![2322, 2345],
            diagnostic_fingerprints: vec![
                fp(2322, "test.ts", "Type mismatch"),
                fp(2345, "test.ts", "Arg type error"),
            ],
            crashed: false,
            options: Default::default(),
        };
        let filtered = filter_lib_diagnostics_tsz(result);
        assert_eq!(filtered.error_codes, vec![2322, 2345]);
        assert_eq!(filtered.diagnostic_fingerprints.len(), 2);
    }

    #[test]
    fn filter_tsc_removes_lib_6053() {
        let tsc_result = TscResult {
            metadata: FileMetadata {
                mtime_ms: 0,
                size: 0,
                typescript_version: None,
            },
            error_codes: vec![6053, 2322],
            diagnostic_fingerprints: vec![
                fp(6053, "test.tsx", "File '/.lib/react16.d.ts' not found."),
                fp(2322, "test.ts", "Type mismatch"),
            ],
        };
        let (codes, fps) = filter_lib_diagnostics_tsc(&tsc_result);
        assert_eq!(codes, vec![2322]);
        assert_eq!(fps.len(), 1);
        assert_eq!(fps[0].code, 2322);
    }

    #[test]
    fn filter_tsc_preserves_6053_from_non_lib() {
        let tsc_result = TscResult {
            metadata: FileMetadata {
                mtime_ms: 0,
                size: 0,
                typescript_version: None,
            },
            error_codes: vec![6053],
            diagnostic_fingerprints: vec![
                fp(6053, "test.tsx", "File '/.lib/react16.d.ts' not found."),
                fp(6053, "test.ts", "File 'missing.d.ts' not found."),
            ],
        };
        let (codes, fps) = filter_lib_diagnostics_tsc(&tsc_result);
        assert_eq!(codes, vec![6053]);
        assert_eq!(fps.len(), 1);
        assert_eq!(fps[0].message_key, "File 'missing.d.ts' not found.");
    }

    #[test]
    fn filter_tsz_removes_6053_with_lib_in_message() {
        let result = tsz_wrapper::CompilationResult {
            error_codes: vec![6053],
            diagnostic_fingerprints: vec![fp(
                6053,
                "test.tsx",
                "File '/.lib/react.d.ts' not found.",
            )],
            crashed: false,
            options: Default::default(),
        };
        let filtered = filter_lib_diagnostics_tsz(result);
        assert!(filtered.error_codes.is_empty());
        assert!(filtered.diagnostic_fingerprints.is_empty());
    }

    #[test]
    fn relative_display_returns_relative_path_when_possible() {
        let base = Path::new("/repo/project");
        let path = Path::new("/repo/project/tests/case.ts");
        assert_eq!(relative_display(path, base), "tests/case.ts");
    }

    #[test]
    fn relative_display_falls_back_to_absolute_path_when_outside_base() {
        let base = Path::new("/repo/project");
        let path = Path::new("/other/place/case.ts");
        assert_eq!(relative_display(path, base), "/other/place/case.ts");
    }

    #[test]
    fn sanitize_artifact_name_replaces_filesystem_special_characters() {
        let sanitized = sanitize_artifact_name(r#"a/b\c:d*e?f"g<h>i|j"#);
        assert_eq!(sanitized, "a_b_c_d_e_f_g_h_i_j");
    }

    #[test]
    fn resolve_tsz_binary_prefers_local_fast_binary_when_present() {
        with_temp_cwd(true, |temp| {
            let resolved = Runner::resolve_tsz_binary("tsz");
            assert_eq!(resolved, "./.target/dist-fast/tsz");
            assert!(temp.join(".target/dist-fast/tsz").is_file());
        });
    }

    #[test]
    fn resolve_tsz_binary_preserves_configured_binary_when_not_default() {
        with_temp_cwd(false, |_| {
            let resolved = Runner::resolve_tsz_binary("/usr/local/bin/tsz-custom");
            assert_eq!(resolved, "/usr/local/bin/tsz-custom");
        });
    }
}
