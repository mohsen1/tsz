//! Parallel test runner
//!
//! Orchestrates parallel test execution using tokio and compares results.

use crate::cache::{self, load_cache, load_domain};
use crate::cli::Args;
use crate::test_parser::{
    parse_test_file, select_ts7_oracle_configurations, test_disposition_at_path, TestDirectives,
    TestDisposition,
};
use crate::text_decode::{decode_source_text, DecodedSourceText};
use crate::tsc_results::{
    DiagnosticFingerprint, ErrorFrequency, TestResult, TestResultFail, TestStats, UnsupportedReason,
};
use crate::tsz_wrapper;
use anyhow::Context;
use futures::stream::{self, StreamExt};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

#[path = "runner/helpers.rs"]
mod runner_helpers;
use runner_helpers::*;
#[path = "runner/plan.rs"]
pub mod plan;

/// Collects paths of crashed, timed-out, and fingerprint-only-mismatch tests for the final summary.
#[derive(Default)]
struct ProblemTests {
    crashed: std::sync::Mutex<Vec<String>>,
    timed_out: std::sync::Mutex<Vec<String>>,
    fingerprint_only: std::sync::Mutex<Vec<String>>,
}

enum FreshTextOutcome {
    Complete(tsz_wrapper::CompilationResult),
    Terminal(TestResult),
}

fn directive_non_runnable_result(path: &Path, directives: &TestDirectives) -> Option<TestResult> {
    match test_disposition_at_path(path, directives) {
        TestDisposition::Runnable => None,
        TestDisposition::Unsupported(reason) => Some(TestResult::Unsupported(reason)),
        TestDisposition::Skipped(reason) => Some(TestResult::Skipped(reason)),
    }
}

fn semantic_non_runnable_result(result: &tsz_wrapper::CompilationResult) -> Option<TestResult> {
    (!result.semantic_completion.is_complete()).then_some(TestResult::Unsupported(
        UnsupportedReason::SemanticIncomplete,
    ))
}

fn oracle_diagnostic_evidence_non_runnable(
    result: &crate::tsc_results::TscResult,
) -> Option<TestResult> {
    (!result.diagnostic_blocks_complete
        || result.ordinary_exit_statuses.is_empty()
        || result
            .ordinary_exit_statuses
            .iter()
            .any(|status| *status > 2))
    .then_some(TestResult::Unsupported(
        UnsupportedReason::OracleDiagnosticEvidenceIncomplete,
    ))
}

fn validate_candidate_source_bytes(key: &str, bytes: &[u8], expected: &str) -> anyhow::Result<()> {
    let observed = crate::integrity::sha256_bytes(bytes);
    if observed != expected {
        anyhow::bail!("candidate bytes changed after domain validation: {key}");
    }
    Ok(())
}

/// Test runner
pub struct Runner {
    args: Args,
    tsz_binary: String,
    typescript_lib_dir: PathBuf,
    cache: Arc<crate::cache::TscCache>,
    domain: Arc<crate::cache::ConformanceDomain>,
    stats: Arc<TestStats>,
    error_freq: Arc<ErrorFrequency>,
    problems: Arc<ProblemTests>,
}

impl Runner {
    fn absolutize_binary_path(path: &Path) -> String {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };

        std::fs::canonicalize(&absolute)
            .unwrap_or(absolute)
            .to_string_lossy()
            .to_string()
    }

    fn resolve_tsz_binary(configured: &str) -> String {
        // Prefer the workspace fast-build binary when the default "tsz" is used.
        // This avoids accidentally running a stale PATH-installed binary and
        // producing misleading parity deltas.
        if configured == "tsz" {
            let local_fast = Path::new("./.target/dist-fast/tsz");
            if local_fast.is_file() {
                return Self::absolutize_binary_path(local_fast);
            }
        }
        let configured_path = Path::new(configured);
        if configured_path.components().count() > 1 || configured_path.is_absolute() {
            return Self::absolutize_binary_path(configured_path);
        }
        configured.to_string()
    }

    /// Execute every TS7-selected configuration in selector order. UTF-8 and
    /// UTF-16 inputs share this exact path so decoding cannot change compiler
    /// options or process multiplicity.
    async fn compile_text_variants(
        content: &str,
        directives: &TestDirectives,
        original_ext: Option<&str>,
        ts_tests_lib_dir: &Path,
        typescript_lib_dir: &Path,
        tsz_binary: &str,
        timeout_secs: u64,
    ) -> anyhow::Result<FreshTextOutcome> {
        let option_variants = select_ts7_oracle_configurations(directives)
            .expect("TS7 selector succeeded during skip check");
        let options = option_variants
            .first()
            .expect("TS7 selector returned at least one configuration")
            .clone();
        let mut all_codes = Vec::new();
        let mut all_fingerprints = Vec::new();
        let mut all_exit_statuses = Vec::new();

        for variant in option_variants {
            let content = content.to_string();
            let filenames = directives.filenames.clone();
            let key_order = directives.option_order.clone();
            let original_ext = original_ext.map(str::to_string);
            let ts_tests_lib_dir = ts_tests_lib_dir.to_path_buf();
            let prepared = tokio::task::spawn_blocking(move || {
                tsz_wrapper::prepare_test_dir_with_lib_dir(
                    &content,
                    &filenames,
                    &variant,
                    original_ext.as_deref(),
                    &key_order,
                    Some(&ts_tests_lib_dir),
                )
                .map(|prepared| (prepared, variant))
            })
            .await??;
            let (prepared, variant) = prepared;

            let child = tokio::process::Command::new(tsz_binary)
                .arg("--project")
                .arg(&prepared.project_dir)
                .arg("--noEmit")
                .arg("--pretty")
                .arg("false")
                .env("TSZ_LIB_DIR", typescript_lib_dir)
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
                    Err(_) => return Ok(FreshTextOutcome::Terminal(TestResult::Timeout)),
                }
            } else {
                child.wait_with_output().await?
            };
            let compile_result =
                tsz_wrapper::parse_tsz_output(&output, prepared.temp_dir.path(), variant);
            if compile_result.crashed {
                return Ok(FreshTextOutcome::Terminal(TestResult::Crashed));
            }
            if let Some(result) = semantic_non_runnable_result(&compile_result) {
                return Ok(FreshTextOutcome::Terminal(result));
            }
            all_codes.extend(compile_result.error_codes);
            all_fingerprints.extend(compile_result.diagnostic_fingerprints);
            all_exit_statuses.extend(compile_result.ordinary_exit_statuses);
        }

        Ok(FreshTextOutcome::Complete(tsz_wrapper::CompilationResult {
            error_codes: all_codes,
            diagnostic_fingerprints: all_fingerprints,
            crashed: false,
            semantic_completion: tsz_wrapper::SemanticCompletion::Complete,
            ordinary_exit_statuses: all_exit_statuses,
            options,
        }))
    }

    /// Create a new runner
    pub fn new(args: Args) -> anyhow::Result<Self> {
        // Load cache
        let cache_path = Path::new(&args.cache_file);
        let cache = if cache_path.exists() {
            load_cache(cache_path)
                .with_context(|| format!("Failed to load cache from {}", args.cache_file))?
        } else {
            anyhow::bail!("TSC cache file not found: {}", args.cache_file)
        };
        cache::validate_runnable_evidence(&cache)?;

        let domain = load_domain(Path::new(&args.domain_file))
            .with_context(|| format!("Failed to load domain from {}", args.domain_file))?;

        let repo_root = crate::corpus::repository_root_from_current_dir()?;
        let corpus = crate::corpus::verify_pinned_corpus(&repo_root, Path::new(&args.test_dir))?;
        if domain.schema_version != 2
            || domain.corpus_commit != corpus.commit
            || domain.corpus_tree != corpus.tree
        {
            anyhow::bail!("cache/domain corpus identity does not match the pinned pristine corpus");
        }
        let local_oracle = crate::oracle::resolve_verified_oracle(&repo_root)?;
        crate::oracle::validate_runtime_evidence(&repo_root, &domain.oracle, &local_oracle)?;
        if local_oracle.version()? != domain.typescript_version {
            anyhow::bail!("cache/domain TypeScript version differs from verified native oracle");
        }
        let typescript_lib_dir = local_oracle
            .binary_path
            .parent()
            .context("verified native oracle executable has no library directory")?
            .canonicalize()
            .context("cannot canonicalize verified native oracle library directory")?;

        info!("Loaded {} cached TSC results", cache.len());

        let tsz_binary = Self::resolve_tsz_binary(&args.tsz_binary);

        Ok(Self {
            args,
            tsz_binary,
            typescript_lib_dir,
            cache: Arc::new(cache),
            domain: Arc::new(domain),
            stats: Arc::new(TestStats::default()),
            error_freq: Arc::new(ErrorFrequency::default()),
            problems: Arc::new(ProblemTests::default()),
        })
    }

    /// Run all tests
    pub async fn run(&self) -> anyhow::Result<TestStats> {
        let source_hashes = Arc::new(plan::validate_live_domain(
            &self.args,
            &self.cache,
            &self.domain,
        )?);
        let test_files = plan::discover_tests(&self.args)?;

        if test_files.is_empty() {
            anyhow::bail!("conformance selection is empty");
        }

        self.stats
            .selected
            .store(test_files.len(), Ordering::SeqCst);

        info!("Found {} test files", test_files.len());

        // Set up concurrency control
        let concurrency_limit = self.args.workers;
        let semaphore = Arc::new(Semaphore::new(concurrency_limit));

        info!("Canonical fresh-process mode captures stdout, stderr, and exact exit status");

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
        let timed_tests = Arc::new(std::sync::Mutex::new(Vec::<TimedTest>::new()));
        let fatal_errors = Arc::new(std::sync::Mutex::new(Vec::<(String, String)>::new()));

        stream::iter(test_files)
            .for_each_concurrent(Some(concurrency_limit), |path| {
                let permit = std::sync::Arc::clone(&semaphore);
                let cache = std::sync::Arc::clone(&self.cache);
                let source_hashes = Arc::clone(&source_hashes);
                let stats = std::sync::Arc::clone(&self.stats);
                let error_freq = std::sync::Arc::clone(&self.error_freq);
                let problems = std::sync::Arc::clone(&self.problems);
                let tsz_binary = self.tsz_binary.clone();
                let typescript_lib_dir = self.typescript_lib_dir.clone();
                let verbose = self.args.is_verbose();
                let print_test = self.args.print_test;
                let print_test_files = self.args.print_test_files;
                let base = base_path.clone();
                let test_dir = test_dir.clone();
                let diff_artifacts_dir = diff_artifacts_dir.clone();
                let timed_tests = Arc::clone(&timed_tests);
                let fatal_errors = Arc::clone(&fatal_errors);

                async move {
                    let _permit = permit.acquire().await.unwrap();
                    let rel_path = relative_display(&path, &base);
                    let test_start = Instant::now();

                    match Self::run_test(
                        &path,
                        &test_dir,
                        cache,
                        source_hashes,
                        tsz_binary,
                        typescript_lib_dir,
                        print_test_files,
                        timeout_secs,
                    )
                    .await
                    {
                        Ok((result, file_preview)) => {
                            timed_tests.lock().unwrap().push(TimedTest {
                                file: rel_path.replace('\\', "/"),
                                elapsed_ms: test_start.elapsed().as_millis(),
                            });
                            use std::fmt::Write;

                            // Update stats
                            stats.total.fetch_add(1, Ordering::SeqCst);

                            // Buffer all output for this test so it prints atomically
                            let mut buf = String::new();

                            match result {
                                TestResult::Pass => {
                                    stats.passed.fetch_add(1, Ordering::SeqCst);
                                    if print_test {
                                        writeln!(buf, "PASS {}", rel_path).ok();
                                    }
                                }
                                TestResult::Fail(fail) => {
                                    let TestResultFail {
                                        expected,
                                        actual,
                                        missing,
                                        extra,
                                        missing_fingerprints,
                                        extra_fingerprints,
                                        expected_fingerprints,
                                        actual_fingerprints,
                                        expected_exit_statuses,
                                        actual_exit_statuses,
                                        options,
                                        known_failure,
                                    } = *fail;
                                    stats.failed.fetch_add(1, Ordering::SeqCst);
                                    if known_failure.is_some() {
                                        stats.known_failures.fetch_add(1, Ordering::SeqCst);
                                    }

                                    // Track fingerprint-only failures: error codes match
                                    // but fingerprints differ (position/message mismatch)
                                    if known_failure.is_none()
                                        && missing.is_empty()
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
                                        if let Some(reason) = known_failure {
                                            writeln!(buf, "XFAIL {} ({})", rel_path, reason).ok();
                                        } else {
                                            writeln!(buf, "FAIL {}", rel_path).ok();
                                        }

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
                                            "expected_fingerprints": expected_fingerprints
                                                .iter()
                                                .map(super::tsc_results::DiagnosticFingerprint::display_key)
                                                .collect::<Vec<_>>(),
                                            "actual_fingerprints": actual_fingerprints
                                                .iter()
                                                .map(super::tsc_results::DiagnosticFingerprint::display_key)
                                                .collect::<Vec<_>>(),
                                            "expected_exit_statuses": expected_exit_statuses,
                                            "actual_exit_statuses": actual_exit_statuses,
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
                                    if print_test || verbose {
                                        writeln!(buf, "SKIP {} ({})", rel_path, reason).ok();
                                    }
                                }
                                TestResult::Unsupported(reason) => {
                                    stats.unsupported.fetch_add(1, Ordering::SeqCst);
                                    if print_test || verbose {
                                        writeln!(
                                            buf,
                                            "UNSUPPORTED {} ({})",
                                            rel_path,
                                            reason.code()
                                        )
                                        .ok();
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
                            fatal_errors
                                .lock()
                                .unwrap()
                                .push((rel_path.replace('\\', "/"), format!("{e:#}")));
                        }
                    }
                }
            })
            .await;

        let fatal_errors = fatal_errors.lock().unwrap();
        if let Some((path, error)) = fatal_errors.first() {
            anyhow::bail!(
                "conformance infrastructure failed for {path}: {error} ({} fatal worker errors)",
                fatal_errors.len()
            );
        }
        drop(fatal_errors);

        let elapsed = start.elapsed();

        // Print summary
        let stats = &self.stats;
        let error_freq = &self.error_freq;
        let summary = TestStats {
            selected: AtomicUsize::new(stats.selected.load(Ordering::SeqCst)),
            total: AtomicUsize::new(stats.total.load(Ordering::SeqCst)),
            passed: AtomicUsize::new(stats.passed.load(Ordering::SeqCst)),
            failed: AtomicUsize::new(stats.failed.load(Ordering::SeqCst)),
            skipped: AtomicUsize::new(stats.skipped.load(Ordering::SeqCst)),
            unsupported: AtomicUsize::new(stats.unsupported.load(Ordering::SeqCst)),
            crashed: AtomicUsize::new(stats.crashed.load(Ordering::SeqCst)),
            timeout: AtomicUsize::new(stats.timeout.load(Ordering::SeqCst)),
            known_failures: AtomicUsize::new(stats.known_failures.load(Ordering::SeqCst)),
            fingerprint_only: AtomicUsize::new(stats.fingerprint_only.load(Ordering::SeqCst)),
        };
        if !summary.has_result_bijection() {
            anyhow::bail!(
                "conformance result accounting is not a bijection: selected={} results={}",
                summary.selected.load(Ordering::SeqCst),
                summary.total.load(Ordering::SeqCst)
            );
        }

        // Complete every fallible output before exposing the canonical final
        // summary. A timings error or result-accounting gap therefore cannot
        // leave a parseable-but-incomplete observation behind.
        if let Some(path) = &self.args.timings_file {
            let mut results = timed_tests.lock().unwrap().clone();
            results.sort_by(|a, b| a.file.cmp(&b.file));
            let payload = serde_json::json!({
                "summary": {
                    "total": results.len(),
                    "elapsed_ms": elapsed.as_millis(),
                },
                "results": results
                    .iter()
                    .map(|result| serde_json::json!({
                        "file": &result.file,
                        "elapsed_ms": result.elapsed_ms,
                    }))
                    .collect::<Vec<_>>(),
            });
            if let Some(parent) = Path::new(path).parent() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create timings directory {}", parent.display())
                })?;
            }
            std::fs::write(path, serde_json::to_string(&payload)?)
                .with_context(|| format!("failed to write timings file {path}"))?;
        }

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
        println!("  Candidates: {}", stats.total.load(Ordering::SeqCst));
        println!("  Runnable: {}", stats.runnable());
        println!(
            "  Unsupported: {}",
            stats.unsupported.load(Ordering::SeqCst)
        );
        println!("  Skipped: {}", stats.skipped.load(Ordering::SeqCst));
        println!(
            "  Known failures: {}",
            stats.known_failures.load(Ordering::SeqCst)
        );
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

        Ok(summary)
    }

    /// Run a single test.
    /// Returns `(result, file_preview)` where `file_preview` is the numbered
    /// source listing when `print_test_files` is true.
    async fn run_test(
        path: &Path,
        test_dir: &Path,
        cache: Arc<crate::cache::TscCache>,
        source_hashes: Arc<BTreeMap<String, String>>,
        tsz_binary: String,
        typescript_lib_dir: PathBuf,
        print_test_files: bool,
        timeout_secs: u64,
    ) -> anyhow::Result<(TestResult, Option<String>)> {
        // Read and decode file content (UTF-8/UTF-8 BOM/UTF-16 BOM).
        let bytes = tokio::fs::read(path).await?;
        let key =
            cache::cache_key(path, test_dir).unwrap_or_else(|| path.to_string_lossy().to_string());
        let expected_source_sha256 = source_hashes
            .get(&key)
            .with_context(|| format!("candidate has no preflight source identity: {key}"))?;
        validate_candidate_source_bytes(&key, &bytes, expected_source_sha256)?;
        let cached_result = cache::lookup(&cache, &key);
        let ts_tests_lib_dir = tsz_wrapper::tests_lib_dir_for_cases_dir(test_dir);

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
                if let Some(result) = directive_non_runnable_result(path, &parsed.directives) {
                    return Ok((result, file_preview.take()));
                }

                if let Some(tsc_result) = cached_result {
                    debug!("Cache hit for {}", path.display());
                    if let Some(result) = oracle_diagnostic_evidence_non_runnable(tsc_result) {
                        return Ok((result, file_preview.take()));
                    }

                    let original_ext = path.extension().and_then(|extension| extension.to_str());
                    let compile_result = match Self::compile_text_variants(
                        &content,
                        &parsed.directives,
                        original_ext,
                        &ts_tests_lib_dir,
                        &typescript_lib_dir,
                        &tsz_binary,
                        timeout_secs,
                    )
                    .await?
                    {
                        FreshTextOutcome::Complete(result) => result,
                        FreshTextOutcome::Terminal(result) => {
                            return Ok((result, file_preview.take()));
                        }
                    };
                    let (tsc_error_codes, tsc_fps, tsc_exits) =
                        canonical_tsc_diagnostics(tsc_result);

                    let options_for_fail = compile_result.options.clone();
                    let outcome = compare_diagnostics(
                        &compile_result,
                        &tsc_error_codes,
                        &tsc_fps,
                        &tsc_exits,
                        options_for_fail,
                    );
                    Ok((outcome, file_preview.take()))
                } else {
                    debug!("Cache miss for {}", path.display());
                    anyhow::bail!("missing TSC cache entry for {key}")
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

                let parsed_directives = parse_test_file(&decoded_text)?;
                if let Some(result) =
                    directive_non_runnable_result(path, &parsed_directives.directives)
                {
                    return Ok((result, file_preview.take()));
                }

                if let Some(tsc_result) = cached_result {
                    if let Some(result) = oracle_diagnostic_evidence_non_runnable(tsc_result) {
                        return Ok((result, file_preview.take()));
                    }
                    let original_ext = path.extension().and_then(|extension| extension.to_str());
                    let compile_result = match Self::compile_text_variants(
                        &decoded_text,
                        &parsed_directives.directives,
                        original_ext,
                        &ts_tests_lib_dir,
                        &typescript_lib_dir,
                        &tsz_binary,
                        timeout_secs,
                    )
                    .await?
                    {
                        FreshTextOutcome::Complete(result) => result,
                        FreshTextOutcome::Terminal(result) => {
                            return Ok((result, file_preview.take()));
                        }
                    };

                    let (tsc_error_codes, tsc_fps, tsc_exits) =
                        canonical_tsc_diagnostics(tsc_result);

                    let options_for_fail = compile_result.options.clone();
                    let outcome = compare_diagnostics(
                        &compile_result,
                        &tsc_error_codes,
                        &tsc_fps,
                        &tsc_exits,
                        options_for_fail,
                    );
                    Ok((outcome, file_preview.take()))
                } else {
                    debug!("Cache miss for {}", path.display());
                    anyhow::bail!("missing TSC cache entry for {key}")
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

                if let Some(tsc_result) = cached_result {
                    if let Some(result) = oracle_diagnostic_evidence_non_runnable(tsc_result) {
                        return Ok((result, file_preview.take()));
                    }
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

                    let child = tokio::process::Command::new(&tsz_binary)
                        .arg("--project")
                        .arg(&prepared.project_dir)
                        .arg("--noEmit")
                        .arg("--pretty")
                        .arg("false")
                        .env("TSZ_LIB_DIR", &typescript_lib_dir)
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

                    let compile_result =
                        tsz_wrapper::parse_tsz_output(&output, prepared.temp_dir.path(), options);
                    if compile_result.crashed {
                        return Ok((TestResult::Crashed, file_preview.take()));
                    }
                    if let Some(result) = semantic_non_runnable_result(&compile_result) {
                        return Ok((result, file_preview.take()));
                    }

                    let (tsc_error_codes, tsc_fps, tsc_exits) =
                        canonical_tsc_diagnostics(tsc_result);

                    let options_for_fail = compile_result.options.clone();
                    let outcome = compare_diagnostics(
                        &compile_result,
                        &tsc_error_codes,
                        &tsc_fps,
                        &tsc_exits,
                        options_for_fail,
                    );
                    Ok((outcome, file_preview.take()))
                } else {
                    debug!("Cache miss for {}", path.display());
                    anyhow::bail!("missing TSC cache entry for {key}")
                }
            }
        }
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
            continuations: Vec::new(),
        }
    }

    async fn run_test_with_empty_cache_and_identity(
        source: &[u8],
        expected_source: &[u8],
    ) -> anyhow::Result<TestResult> {
        let temp = tempfile::tempdir().expect("tempdir");
        let test_dir = temp.path().join("cases");
        let path = test_dir.join("compiler/case.ts");
        std::fs::create_dir_all(path.parent().expect("test parent")).expect("create test dir");
        std::fs::write(&path, source).expect("write test source");
        let source_hashes = BTreeMap::from([(
            "compiler/case.ts".to_string(),
            crate::integrity::sha256_bytes(expected_source),
        )]);

        let (result, _preview) = Runner::run_test(
            &path,
            &test_dir,
            Arc::new(HashMap::new()),
            Arc::new(source_hashes),
            "unused-tsz-binary".to_string(),
            temp.path().to_path_buf(),
            false,
            0,
        )
        .await?;
        Ok(result)
    }

    async fn run_test_with_empty_cache(source: &[u8]) -> anyhow::Result<TestResult> {
        run_test_with_empty_cache_and_identity(source, source).await
    }

    #[tokio::test]
    async fn run_test_classifies_ts7_unsupported_before_cache_lookup() {
        let result = run_test_with_empty_cache(b"// @target: es5\nlet value = 1;\n")
            .await
            .expect("unsupported test should not require cache");
        assert_eq!(
            result,
            TestResult::Unsupported(UnsupportedReason::TypeScript7Configuration)
        );
    }

    #[tokio::test]
    async fn run_test_classifies_trace_products_before_cache_lookup() {
        for source in [
            b"// @traceResolution: true\nlet value = 1;\n".as_slice(),
            b"// @filename: tsconfig.json\n{\"compilerOptions\":{\"traceResolution\":true}}\n// @filename: input.ts\nimport 'pkg';\n"
                .as_slice(),
        ] {
            let result = run_test_with_empty_cache(source)
                .await
                .expect("trace product should not require a diagnostic cache row");
            assert_eq!(
                result,
                TestResult::Unsupported(
                    UnsupportedReason::TraceResolutionOutputNotCompared
                )
            );
        }
    }

    #[tokio::test]
    async fn run_test_fails_when_runnable_cache_entry_is_missing() {
        let error = run_test_with_empty_cache(b"let value = 1;\n")
            .await
            .expect_err("runnable cache miss must fail");
        assert!(
            error
                .to_string()
                .contains("missing TSC cache entry for compiler/case.ts"),
            "unexpected error: {error:#}"
        );
    }

    #[tokio::test]
    async fn run_test_rechecks_nonrunnable_bytes_at_the_point_of_use() {
        for source in [
            b"// @skip: tracked\nlet value = 1;\n".as_slice(),
            b"// @target: es5\nlet value = 1;\n".as_slice(),
        ] {
            let error = run_test_with_empty_cache_and_identity(source, b"preflight bytes\n")
                .await
                .expect_err("changed nonrunnable source must fail before classification");
            assert!(error
                .to_string()
                .contains("changed after domain validation"));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shared_text_executor_spawns_every_selected_variant_fresh() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let counter = temp.path().join("invocations.txt");
        let compiler = temp.path().join("fake-tsz.sh");
        std::fs::write(
            &compiler,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$TSZ_LIB_DIR\" >> '{}'\nexit 0\n",
                counter.display()
            ),
        )
        .expect("fake compiler");
        let mut permissions = std::fs::metadata(&compiler)
            .expect("compiler metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&compiler, permissions).expect("executable compiler");
        let directives = TestDirectives {
            options: HashMap::from([("module".to_string(), "node16, esnext".to_string())]),
            option_order: vec!["module".to_string()],
            filenames: Vec::new(),
        };

        let outcome = Runner::compile_text_variants(
            "let value = 1;\n",
            &directives,
            Some("ts"),
            temp.path(),
            temp.path(),
            compiler.to_str().expect("utf8 compiler path"),
            5,
        )
        .await
        .expect("variant execution");
        assert!(matches!(outcome, FreshTextOutcome::Complete(_)));
        let invocations = std::fs::read_to_string(counter).expect("invocation counter");
        assert_eq!(invocations.lines().count(), 2);
        assert!(invocations
            .lines()
            .all(|line| line == temp.path().to_string_lossy()));
    }

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn appledouble_files_are_not_discoverable_tests() {
        assert!(is_appledouble_file(Path::new(
            "TypeScript/tests/cases/._foo.ts"
        )));
        assert!(is_appledouble_file(Path::new("._bar.js")));
        assert!(!is_appledouble_file(Path::new("foo.ts")));
        assert!(!is_appledouble_file(Path::new("dir/regular.js")));
    }

    #[test]
    fn result_timings_override_stale_path_weights() {
        let file = tempfile::NamedTempFile::new().expect("weights file");
        std::fs::write(
            file.path(),
            serde_json::json!({
                "path_weights": {
                    "TypeScript/tests/cases/compiler/foo.ts": 10_000.0
                },
                "results": [{
                    "file": "TypeScript/tests/cases/compiler/foo.ts",
                    "elapsed_ms": 25.0
                }]
            })
            .to_string(),
        )
        .expect("write weights");

        let weights = load_json_weights(file.path()).expect("weights should load");
        let path = Path::new("/repo/TypeScript/tests/cases/compiler/foo.ts");
        let test_dir = Path::new("/repo/TypeScript/tests/cases");

        assert_eq!(historical_path_weight(&weights, path, test_dir), Some(25.0));
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
    fn canonical_tsc_diagnostics_preserves_lib_and_orphan_facts() {
        let tsc_result = TscResult {
            metadata: FileMetadata {
                mtime_ms: 0,
                size: 0,
                typescript_version: None,
                source_sha256: "00".repeat(32),
            },
            error_codes: vec![6053],
            diagnostic_fingerprints: vec![
                fp(6053, "test.tsx", "File '/.lib/react16.d.ts' not found."),
                fp(9999, ".lib/helper.d.ts", "Oracle-only fingerprint."),
            ],
            diagnostic_blocks_complete: true,
            ordinary_exit_statuses: vec![1],
        };
        let (codes, fps, exits) = canonical_tsc_diagnostics(&tsc_result);
        assert_eq!(codes, vec![6053]);
        assert_eq!(fps.len(), 2);
        assert_eq!(
            fps.iter().map(|fp| fp.code).collect::<Vec<_>>(),
            [6053, 9999]
        );
        assert_eq!(fps[0].file, "test.tsx");
        assert_eq!(fps[1].file, ".lib/helper.d.ts");
        assert_eq!(exits, vec![1]);
    }

    #[test]
    fn diagnostic_cache_without_grouped_blocks_is_an_explicit_nonclaim() {
        let tsc_result = TscResult {
            metadata: FileMetadata {
                mtime_ms: 0,
                size: 0,
                typescript_version: Some("7.0.2".to_string()),
                source_sha256: "00".repeat(32),
            },
            error_codes: vec![2322],
            diagnostic_fingerprints: vec![fp(2322, "test.ts", "Mismatch.")],
            diagnostic_blocks_complete: false,
            ordinary_exit_statuses: vec![1],
        };

        assert_eq!(
            oracle_diagnostic_evidence_non_runnable(&tsc_result),
            Some(TestResult::Unsupported(
                UnsupportedReason::OracleDiagnosticEvidenceIncomplete
            ))
        );
    }

    #[test]
    fn runnable_bytes_are_rechecked_at_the_point_of_use() {
        let source = b"let value = 1;\n";
        let tsc_result = TscResult {
            metadata: FileMetadata {
                mtime_ms: 0,
                size: source.len() as u64,
                typescript_version: Some("7.0.2".to_string()),
                source_sha256: crate::integrity::sha256_bytes(source),
            },
            error_codes: Vec::new(),
            diagnostic_fingerprints: Vec::new(),
            diagnostic_blocks_complete: true,
            ordinary_exit_statuses: vec![0],
        };
        validate_candidate_source_bytes(
            "compiler/case.ts",
            source,
            &tsc_result.metadata.source_sha256,
        )
        .expect("exact cached bytes");
        assert!(validate_candidate_source_bytes(
            "compiler/case.ts",
            b"let value = 2;\n",
            &tsc_result.metadata.source_sha256
        )
        .is_err());
    }

    #[test]
    fn copied_lib_input_divergence_is_an_explicit_mismatch() {
        let expected = fp(6053, "test.ts", "File '/.lib/react.d.ts' not found.");
        let actual = fp(2430, ".lib/react.d.ts", "Interface mismatch.");
        let result = compare_diagnostics(
            &compilation(&[2430], vec![actual]),
            &[6053],
            &[expected],
            HashMap::new(),
        );

        match result {
            TestResult::Fail(fail) => {
                assert_eq!(fail.missing, vec![6053]);
                assert_eq!(fail.extra, vec![2430]);
                assert_eq!(fail.missing_fingerprints.len(), 1);
                assert_eq!(fail.extra_fingerprints.len(), 1);
            }
            other => panic!("divergent lib inputs must not pass: {other:?}"),
        }
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
            assert_eq!(
                resolved,
                std::fs::canonicalize(temp.join(".target/dist-fast/tsz"))
                    .expect("fast binary path should canonicalize")
                    .to_string_lossy()
                    .to_string()
            );
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

    #[test]
    fn resolve_tsz_binary_absolutizes_relative_configured_path() {
        with_temp_cwd(false, |temp| {
            let rel = Path::new("bin/tsz-custom");
            std::fs::create_dir_all(temp.join("bin")).expect("bin dir should exist");
            std::fs::write(temp.join(rel), b"").expect("binary placeholder should exist");

            let resolved = Runner::resolve_tsz_binary("bin/tsz-custom");
            assert_eq!(
                resolved,
                std::fs::canonicalize(temp.join(rel))
                    .expect("configured binary path should canonicalize")
                    .to_string_lossy()
                    .to_string()
            );
        });
    }

    fn compilation(
        codes: &[u32],
        fps: Vec<DiagnosticFingerprint>,
    ) -> tsz_wrapper::CompilationResult {
        let ordinary_exit = if codes.is_empty() && fps.is_empty() {
            0
        } else {
            1
        };
        tsz_wrapper::CompilationResult {
            error_codes: codes.to_vec(),
            diagnostic_fingerprints: fps,
            crashed: false,
            semantic_completion: tsz_wrapper::SemanticCompletion::Complete,
            ordinary_exit_statuses: vec![ordinary_exit],
            options: HashMap::new(),
        }
    }

    fn compare_diagnostics(
        compile: &tsz_wrapper::CompilationResult,
        tsc_codes: &[u32],
        tsc_fps: &[DiagnosticFingerprint],
        options: HashMap<String, String>,
    ) -> TestResult {
        let expected_exit = [if tsc_codes.is_empty() && tsc_fps.is_empty() {
            0
        } else {
            1
        }];
        super::compare_diagnostics(compile, tsc_codes, tsc_fps, &expected_exit, options)
    }

    #[test]
    fn batch_semantic_marker_is_unsupported_not_pass_or_crash() {
        let result = tsz_wrapper::parse_batch_output(
            "---TSZ-SEMANTIC-COMPLETION:deferred---\n",
            Path::new("/tmp/tsz-semantic-nonclaim"),
            HashMap::new(),
        );

        assert!(!result.crashed);
        assert!(result.error_codes.is_empty());
        assert_eq!(
            semantic_non_runnable_result(&result),
            Some(TestResult::Unsupported(
                UnsupportedReason::SemanticIncomplete
            ))
        );
    }

    fn assert_fail_codes(
        result: &TestResult,
        expected_codes: &[u32],
        actual_codes: &[u32],
        missing_codes: &[u32],
        extra_codes: &[u32],
    ) {
        match result {
            TestResult::Fail(fail) => {
                assert_eq!(&fail.expected, expected_codes, "expected codes mismatch");
                assert_eq!(&fail.actual, actual_codes, "actual codes mismatch");
                let mut m = fail.missing.clone();
                m.sort_unstable();
                let mut e = fail.extra.clone();
                e.sort_unstable();
                let mut want_m = missing_codes.to_vec();
                want_m.sort_unstable();
                let mut want_e = extra_codes.to_vec();
                want_e.sort_unstable();
                assert_eq!(m, want_m, "missing codes mismatch");
                assert_eq!(e, want_e, "extra codes mismatch");
            }
            other => panic!("expected TestResult::Fail, got {other:?}"),
        }
    }

    #[test]
    fn compare_diagnostics_passes_on_exact_match() {
        let tsc_codes = vec![2304];
        let tsc_fps = vec![fp(2304, "a.ts", "Cannot find name 'foo'.")];
        let compile = compilation(&[2304], vec![fp(2304, "a.ts", "Cannot find name 'foo'.")]);

        let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
        assert_eq!(result, TestResult::Pass);
    }

    #[test]
    fn compare_diagnostics_rejects_top_level_order_election() {
        let first = fp(2304, "a.ts", "first");
        let second = fp(2322, "b.ts", "second");
        let compile = compilation(&[2322, 2304], vec![second.clone(), first.clone()]);
        let result = super::compare_diagnostics(
            &compile,
            &[2304, 2322],
            &[first, second],
            &[1],
            HashMap::new(),
        );
        let TestResult::Fail(fail) = result else {
            panic!("a diagnostic order mismatch must fail");
        };
        assert!(fail.missing.is_empty() && fail.extra.is_empty());
        assert!(fail.missing_fingerprints.is_empty());
        assert!(fail.extra_fingerprints.is_empty());
    }

    #[test]
    fn compare_diagnostics_rejects_wrong_ordinary_exit() {
        let diagnostic = fp(2304, "a.ts", "Cannot find name 'x'.");
        let mut compile = compilation(&[2304], vec![diagnostic.clone()]);
        compile.ordinary_exit_statuses = vec![2];
        let result =
            super::compare_diagnostics(&compile, &[2304], &[diagnostic], &[1], HashMap::new());
        let TestResult::Fail(fail) = result else {
            panic!("wrong compiler exit must fail");
        };
        assert_eq!(fail.expected_exit_statuses, vec![1]);
        assert_eq!(fail.actual_exit_statuses, vec![2]);
    }

    #[test]
    fn compare_diagnostics_detects_missing_code() {
        let tsc_codes = vec![2304, 2322];
        let tsc_fps: Vec<DiagnosticFingerprint> = vec![];
        let compile = compilation(&[2304], vec![]);

        let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
        assert_fail_codes(&result, &[2304, 2322], &[2304], &[2322], &[]);
    }

    #[test]
    fn compare_diagnostics_detects_extra_code() {
        let tsc_codes = vec![2304];
        let tsc_fps: Vec<DiagnosticFingerprint> = vec![];
        let compile = compilation(&[2304, 7027], vec![]);

        let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
        assert_fail_codes(&result, &[2304], &[2304, 7027], &[], &[7027]);
    }

    #[test]
    fn compare_diagnostics_rejects_a_code_only_server_result() {
        let tsc_codes = vec![2304];
        let tsc_fps = vec![fp(2304, "a.ts", "Cannot find name 'foo'.")];
        let compile = compilation(&[2304], vec![]);

        let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
        match result {
            TestResult::Fail(fail) => {
                assert_eq!(fail.missing_fingerprints, tsc_fps);
                assert!(fail.extra_fingerprints.is_empty());
            }
            other => panic!("code-only server output must not pass: {other:?}"),
        }
    }

    #[test]
    fn compare_diagnostics_rejects_code_only_results_on_both_sides() {
        let result =
            compare_diagnostics(&compilation(&[2304], vec![]), &[2304], &[], HashMap::new());

        assert!(matches!(result, TestResult::Fail(_)));
    }

    #[test]
    fn compare_diagnostics_preserves_duplicate_code_multiplicity() {
        let diagnostic = fp(2304, "a.ts", "Cannot find name 'missing'.");
        let result = compare_diagnostics(
            &compilation(&[2304, 2304], vec![diagnostic.clone(), diagnostic.clone()]),
            &[2304],
            std::slice::from_ref(&diagnostic),
            HashMap::new(),
        );

        match result {
            TestResult::Fail(fail) => {
                assert_eq!(fail.extra, vec![2304]);
                assert_eq!(fail.extra_fingerprints, vec![diagnostic]);
            }
            other => panic!("duplicate TSZ code must not pass: {other:?}"),
        }
    }

    #[test]
    fn compare_diagnostics_rejects_wrong_message_and_span() {
        let expected = fp(
            2322,
            "a.ts",
            "Type 'string' is not assignable to type 'number'.",
        );
        let mut actual = fp(2322, "a.ts", "A different message.");
        actual.line = 9;
        actual.column = 4;
        let result = compare_diagnostics(
            &compilation(&[2322], vec![actual.clone()]),
            &[2322],
            std::slice::from_ref(&expected),
            HashMap::new(),
        );
        match result {
            TestResult::Fail(fail) => {
                assert_eq!(fail.missing_fingerprints, vec![expected]);
                assert_eq!(fail.extra_fingerprints, vec![actual]);
            }
            other => panic!("wrong message/span must not pass: {other:?}"),
        }
    }

    #[test]
    fn compare_diagnostics_preserves_duplicate_multiplicity() {
        let diagnostic = fp(2304, "a.ts", "Cannot find name 'missing'.");
        let result = compare_diagnostics(
            &compilation(&[2304], vec![diagnostic.clone(), diagnostic.clone()]),
            &[2304],
            std::slice::from_ref(&diagnostic),
            HashMap::new(),
        );
        match result {
            TestResult::Fail(fail) => {
                assert!(fail.missing_fingerprints.is_empty());
                assert_eq!(fail.extra_fingerprints, vec![diagnostic]);
                assert_eq!(fail.actual_fingerprints.len(), 2);
            }
            other => panic!("duplicate TSZ diagnostic must not pass: {other:?}"),
        }
    }

    #[test]
    fn compare_diagnostics_does_not_condition_tsz_output_on_tsc_codes() {
        let option_error = fp(5024, "", "Compiler option has an invalid value.");
        let semantic_error = fp(2322, "a.ts", "Type mismatch.");
        let result = compare_diagnostics(
            &compilation(
                &[5024, 2322],
                vec![option_error.clone(), semantic_error.clone()],
            ),
            &[5024],
            std::slice::from_ref(&option_error),
            HashMap::new(),
        );
        match result {
            TestResult::Fail(fail) => {
                assert_eq!(fail.extra, vec![2322]);
                assert_eq!(fail.extra_fingerprints, vec![semantic_error]);
            }
            other => panic!("oracle-conditioned TSZ removal returned: {other:?}"),
        }
    }

    #[test]
    fn compare_diagnostics_detects_fingerprint_only_mismatch() {
        // Codes match but fingerprints disagree (e.g. wrong file or message).
        // This is the "fingerprint-only failure" case that dominates the
        // close-to-passing bucket in the conformance dashboard.
        let tsc_codes = vec![2304];
        let tsc_fps = vec![fp(2304, "expected.ts", "Cannot find name 'foo'.")];
        let compile = compilation(
            &[2304],
            vec![fp(2304, "actual.ts", "Cannot find name 'foo'.")],
        );

        let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
        match result {
            TestResult::Fail(fail) => {
                assert!(
                    fail.missing.is_empty() && fail.extra.is_empty(),
                    "codes should match exactly"
                );
                assert_eq!(fail.missing_fingerprints.len(), 1);
                assert_eq!(fail.missing_fingerprints[0].file, "expected.ts");
                assert_eq!(fail.extra_fingerprints.len(), 1);
                assert_eq!(fail.extra_fingerprints[0].file, "actual.ts");
            }
            other => panic!("expected Fail with fingerprint diff, got {other:?}"),
        }
    }

    #[test]
    fn compare_diagnostics_preserves_expected_and_actual_order() {
        let tsc_codes = vec![2345, 2304];
        let tsc_fps: Vec<DiagnosticFingerprint> = vec![];
        let compile = compilation(&[7027, 2304], vec![]);

        let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
        match result {
            TestResult::Fail(fail) => {
                assert_eq!(fail.expected, vec![2345, 2304]);
                assert_eq!(fail.actual, vec![7027, 2304]);
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[test]
    fn compare_diagnostics_preserves_fingerprint_diff_order() {
        let tsc_codes = vec![2322, 2304];
        let tsc_fps = vec![
            fp(2322, "b.ts", "Type mismatch."),
            fp(2304, "a.ts", "Cannot find."),
        ];
        let compile = compilation(&[], vec![fp(9999, "z.ts", "sentinel")]);

        let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
        match result {
            TestResult::Fail(fail) => {
                assert_eq!(
                    fail.missing_fingerprints
                        .iter()
                        .map(|f| (f.code, f.file.clone()))
                        .collect::<Vec<_>>(),
                    vec![(2322, "b.ts".into()), (2304, "a.ts".into())],
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[test]
    fn compare_diagnostics_carries_full_fingerprint_sets_on_fail() {
        let tsc_codes = vec![2304, 2322];
        let tsc_fps = vec![
            fp(2322, "b.ts", "Type mismatch."),
            fp(2304, "a.ts", "Cannot find."),
        ];
        let compile = compilation(
            &[2304, 2322],
            vec![
                fp(2322, "actual.ts", "Type mismatch."),
                fp(2304, "a.ts", "Cannot find."),
            ],
        );

        let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
        match result {
            TestResult::Fail(fail) => {
                assert_eq!(
                    fail.expected_fingerprints
                        .iter()
                        .map(|f| (f.code, f.file.as_str()))
                        .collect::<Vec<_>>(),
                    vec![(2322, "b.ts"), (2304, "a.ts")],
                );
                assert_eq!(
                    fail.actual_fingerprints
                        .iter()
                        .map(|f| (f.code, f.file.as_str()))
                        .collect::<Vec<_>>(),
                    vec![(2322, "actual.ts"), (2304, "a.ts")],
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[test]
    fn compare_diagnostics_threads_options_into_fail() {
        let mut options = HashMap::new();
        options.insert("target".to_string(), "es2020".to_string());
        let tsc_codes = vec![2304];
        let compile = compilation(&[], vec![]);

        let result = compare_diagnostics(&compile, &tsc_codes, &[], options.clone());
        match result {
            TestResult::Fail(fail) => assert_eq!(fail.options, options),
            other => panic!("expected Fail, got {other:?}"),
        }
    }
}
