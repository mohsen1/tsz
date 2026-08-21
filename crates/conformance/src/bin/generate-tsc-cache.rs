//! TSC Cache Generator using tsc directly
//!
//! Generates the conformance cache by running tsc on each test file.
//! Uses the same `prepare_test_dir` and output parsing as the conformance runner
//! to ensure cache-vs-runner consistency.
//!
//! Architecture: rayon threads handle Rust-side work (file I/O, parsing, setup)
//! while a semaphore caps concurrent node subprocesses to avoid OOM.

use anyhow::{Context, Result};
use clap::Parser;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tsz_conformance::tsc_results::{DiagnosticFingerprint, UnsupportedReason};
use tsz_conformance::tsz_wrapper;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(name = "generate-tsc-cache")]
#[command(about = "Generate TSC cache using tsc directly (accurate)", long_about = None)]
struct Args {
    /// Repository root containing the pinned oracle and TypeScript corpus.
    #[arg(long, default_value = ".")]
    repo_root: String,

    /// Test directory path
    #[arg(long, default_value = "./TypeScript/tests/cases")]
    test_dir: String,

    /// Output cache file path
    #[arg(long, default_value = "./scripts/conformance/tsc-cache-full.json")]
    output: String,

    /// Output path for the exact candidate/runnable/unsupported domain manifest.
    /// Defaults to `<output>.domain.json`.
    #[arg(long)]
    domain_output: Option<String>,

    /// Maximum number of tests to process (0 = unlimited)
    #[arg(long, default_value_t = 0)]
    max: usize,

    /// Number of parallel workers (rayon threads for file I/O and parsing)
    #[arg(long, default_value_t = 0)]
    workers: usize,

    /// Max concurrent node/tsc subprocesses (each uses ~200MB).
    /// Defaults to min(workers, 8) to avoid OOM.
    #[arg(long, default_value_t = 0)]
    max_node_procs: usize,

    /// Show verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Timeout per file in seconds
    #[arg(long, default_value_t = 60)]
    timeout: u64,

    /// Optional substring filter for test file paths
    #[arg(long)]
    filter: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct TscCacheEntry {
    metadata: FileMetadata,
    error_codes: Vec<u32>,
    #[serde(default)]
    diagnostic_fingerprints: Vec<DiagnosticFingerprint>,
    diagnostic_blocks_complete: bool,
    ordinary_exit_statuses: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
struct FileMetadata {
    mtime_ms: u64,
    size: u64,
    #[serde(default)]
    typescript_version: Option<String>,
    source_sha256: String,
}

enum ProcessOutcome {
    Cached(String, TscCacheEntry),
    Skipped(String, &'static str, String),
}

#[derive(Serialize)]
struct ConformanceDomain<'a> {
    schema_version: u32,
    typescript_version: &'a str,
    corpus_commit: &'a str,
    corpus_tree: &'a str,
    candidate_content_sha256: &'a str,
    oracle: &'a Value,
    candidate_count: usize,
    runnable_count: usize,
    unsupported_count: usize,
    skipped_count: usize,
    unsupported: &'a BTreeMap<String, String>,
    skipped: &'a BTreeMap<String, String>,
}

/// Simple counting semaphore (std::sync::Semaphore was removed from std).
struct CountingSemaphore {
    state: Mutex<usize>,
    cvar: Condvar,
}

impl CountingSemaphore {
    fn new(permits: usize) -> Self {
        Self {
            state: Mutex::new(permits),
            cvar: Condvar::new(),
        }
    }

    fn acquire(&self) {
        let mut count = self.state.lock().unwrap();
        while *count == 0 {
            count = self.cvar.wait(count).unwrap();
        }
        *count -= 1;
    }

    fn release(&self) {
        let mut count = self.state.lock().unwrap();
        *count += 1;
        self.cvar.notify_one();
    }
}

fn run_command_with_timeout(mut command: Command, timeout: Duration) -> Result<Output> {
    if timeout.is_zero() {
        return command.output().map_err(Into::into);
    }

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to capture tsc stdout"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to capture tsc stderr"))?;
    let stdout_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            anyhow::bail!("tsc timed out after {} seconds", timeout.as_secs());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| anyhow::anyhow!("tsc stdout reader panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| anyhow::anyhow!("tsc stderr reader panicked"))??;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn main() -> Result<()> {
    let args = Args::parse();
    let repo_root = Path::new(&args.repo_root)
        .canonicalize()
        .with_context(|| format!("cannot canonicalize repository root {}", args.repo_root))?;
    let test_dir_path = Path::new(&args.test_dir)
        .canonicalize()
        .with_context(|| format!("cannot canonicalize test directory {}", args.test_dir))?;
    let corpus = tsz_conformance::corpus::verify_pinned_corpus(&repo_root, &test_dir_path)?;
    let oracle = tsz_conformance::oracle::resolve_verified_oracle(&repo_root)?;
    let oracle_evidence = tsz_conformance::oracle::evidence(&repo_root, &oracle)?;
    let tsc_version = oracle.version()?.to_string();
    if tsc_version != "7.0.2" {
        anyhow::bail!("verified oracle version must be 7.0.2, got {tsc_version}");
    }

    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let workers = if args.workers == 0 {
        num_cpus
    } else {
        args.workers
    };
    let max_node = if args.max_node_procs == 0 {
        workers.min(8)
    } else {
        args.max_node_procs
    };

    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build_global()
        .ok();

    let tsc_path = oracle.binary_path.clone();
    println!("📍 Using verified native tsc: {}", tsc_path.display());
    println!("📍 TypeScript version: {tsc_version}");

    println!("🔍 Discovering test files in: {}", args.test_dir);
    let test_files = discover_tests(&args.test_dir, args.max, args.filter.as_deref())?;
    println!("✓ Found {} test files", test_files.len());

    println!(
        "\n🔨 Processing {} tests ({} rayon threads, {} max node procs)...",
        test_files.len(),
        workers,
        max_node,
    );
    let start = Instant::now();

    let cache: Mutex<HashMap<String, TscCacheEntry>> = Mutex::new(HashMap::new());
    let unsupported: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
    let explicitly_skipped: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
    let observed_sources: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
    let processed = AtomicUsize::new(0);
    let errors = AtomicUsize::new(0);
    let skipped = AtomicUsize::new(0);
    let total = test_files.len();
    let tsc_path_ref = tsc_path.as_path();
    let node_semaphore = Arc::new(CountingSemaphore::new(max_node));

    test_files.par_iter().for_each(|path| {
        let outcome = process_test_file(
            path,
            &test_dir_path,
            tsc_path_ref,
            tsc_version.as_str(),
            args.timeout,
            &node_semaphore,
        );

        match outcome {
            Ok(ProcessOutcome::Cached(key, entry)) => {
                observed_sources
                    .lock()
                    .unwrap()
                    .insert(key.clone(), entry.metadata.source_sha256.clone());
                cache.lock().unwrap().insert(key, entry);
            }
            Ok(ProcessOutcome::Skipped(key, reason, source_sha256)) => {
                observed_sources
                    .lock()
                    .unwrap()
                    .insert(key.clone(), source_sha256);
                let target = if reason == "unsupported by TypeScript 7" {
                    &unsupported
                } else {
                    &explicitly_skipped
                };
                let stable_reason = if reason == "unsupported by TypeScript 7" {
                    UnsupportedReason::TypeScript7Configuration.code()
                } else {
                    reason
                };
                target
                    .lock()
                    .unwrap()
                    .insert(key, stable_reason.to_string());
                skipped.fetch_add(1, Ordering::SeqCst);
            }
            Err(e) => {
                eprintln!("✗ Error processing {}: {e:#}", path.display());
                errors.fetch_add(1, Ordering::SeqCst);
            }
        }

        let count = processed.fetch_add(1, Ordering::SeqCst) + 1;
        if count.is_multiple_of(100) {
            let err_count = errors.load(Ordering::SeqCst);
            let skip_count = skipped.load(Ordering::SeqCst);
            let elapsed = start.elapsed().as_secs_f64();
            let rate = count as f64 / elapsed;
            let remaining = (total - count) as f64 / rate;
            eprint!(
                "\r[{}/{}] {:.0} tests/sec, ETA {:.0}s ({} errors, {} skipped)    ",
                count, total, rate, remaining, err_count, skip_count
            );
        }
    });

    let cache = cache.into_inner().unwrap();
    let unsupported = unsupported.into_inner().unwrap();
    let explicitly_skipped = explicitly_skipped.into_inner().unwrap();
    let observed_sources = observed_sources.into_inner().unwrap();
    let error_count = errors.load(Ordering::SeqCst);

    println!(
        "\r✓ Completed in {:.1}s ({:.0} tests/sec)                              ",
        start.elapsed().as_secs_f64(),
        test_files.len() as f64 / start.elapsed().as_secs_f64()
    );

    println!("  Processed: {}", processed.load(Ordering::SeqCst));
    println!("  Cached: {}", cache.len());
    println!("  Skipped: {}", skipped.load(Ordering::SeqCst));
    println!("  Errors: {error_count}");

    if error_count != 0 {
        anyhow::bail!("refusing to write a partial tsc cache after {error_count} errors");
    }

    let mut candidate_records = Vec::with_capacity(test_files.len());
    for path in &test_files {
        let key = tsz_conformance::cache::cache_key(path, &test_dir_path)
            .with_context(|| format!("candidate escaped test directory: {}", path.display()))?
            .replace('\\', "/");
        let source_sha256 = tsz_conformance::integrity::sha256_bytes(
            &std::fs::read(path)
                .with_context(|| format!("failed to hash candidate {}", path.display()))?,
        );
        if observed_sources.get(&key) != Some(&source_sha256) {
            anyhow::bail!("candidate source changed during oracle generation: {key}");
        }
        let disposition = if cache.contains_key(&key) {
            "runnable".to_string()
        } else if let Some(reason) = unsupported.get(&key) {
            format!("unsupported:{reason}")
        } else if let Some(reason) = explicitly_skipped.get(&key) {
            format!("skipped:{reason}")
        } else {
            anyhow::bail!("processed candidate has no exact disposition: {key}");
        };
        candidate_records.push((key, disposition, source_sha256));
    }
    let candidate_content_sha256 =
        tsz_conformance::integrity::candidate_content_sha256(&candidate_records);

    println!("\n💾 Writing cache to: {}", args.output);
    write_cache(&args.output, &cache)?;
    println!("✓ Cache written with {} entries", cache.len());

    let domain_output = args
        .domain_output
        .unwrap_or_else(|| format!("{}.domain.json", args.output));
    write_domain(
        &domain_output,
        &ConformanceDomain {
            schema_version: 2,
            typescript_version: &tsc_version,
            corpus_commit: &corpus.commit,
            corpus_tree: &corpus.tree,
            candidate_content_sha256: &candidate_content_sha256,
            oracle: &oracle_evidence,
            candidate_count: total,
            runnable_count: cache.len(),
            unsupported_count: unsupported.len(),
            skipped_count: explicitly_skipped.len(),
            unsupported: &unsupported,
            skipped: &explicitly_skipped,
        },
    )?;
    println!("✓ Domain manifest written to {domain_output}");

    Ok(())
}

fn discover_tests(test_dir: &str, max: usize, filter: Option<&str>) -> Result<Vec<PathBuf>> {
    use tsz_conformance::test_filter::{is_conformance_source_file, matches_path_filter};
    let mut files = Vec::new();

    for entry in WalkDir::new(test_dir).follow_links(true) {
        let entry =
            entry.with_context(|| format!("failed to walk conformance corpus {test_dir}"))?;
        let path = entry.path();

        if path.is_dir() {
            continue;
        }

        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("._"))
        {
            continue;
        }

        if !is_conformance_source_file(path) {
            continue;
        }

        if !matches_path_filter(path, filter) {
            continue;
        }

        files.push(path.to_path_buf());
    }

    files.sort();

    if max > 0 && files.len() > max {
        files.truncate(max);
    }

    Ok(files)
}

/// Process a single test file: prepare project dir (shared with runner), run tsc, parse output.
///
/// The `node_sem` semaphore limits concurrent node subprocesses to prevent OOM.
/// Rayon threads do Rust-side work (file read, parse, temp dir setup) without the semaphore,
/// then acquire it only for the subprocess call.
fn process_test_file(
    path: &Path,
    test_dir: &Path,
    tsc_path: &Path,
    tsc_version: &str,
    timeout_secs: u64,
    node_sem: &CountingSemaphore,
) -> Result<ProcessOutcome> {
    use std::fs;
    use tsz_conformance::text_decode::{decode_source_text, DecodedSourceText};

    let bytes = fs::read(path)?;
    let source_sha256 = tsz_conformance::integrity::sha256_bytes(&bytes);
    let decoded = decode_source_text(&bytes);
    let key = tsz_conformance::cache::cache_key(path, test_dir)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Path {} is not under test dir {}",
                path.display(),
                test_dir.display()
            )
        })?
        .replace('\\', "/");

    let (content, filenames, option_variants, option_order, binary_bytes) = match decoded {
        DecodedSourceText::Text(content) => {
            let parsed = tsz_conformance::test_parser::parse_test_file(&content)?;
            if let Some(reason) =
                tsz_conformance::test_parser::should_skip_test_at_path(path, &parsed.directives)
            {
                return Ok(ProcessOutcome::Skipped(key, reason, source_sha256));
            }
            let option_variants =
                tsz_conformance::test_parser::select_ts7_oracle_configurations(&parsed.directives)
                    .expect("TS7 selector succeeded during skip check");
            (
                Some(content),
                parsed.directives.filenames,
                option_variants,
                parsed.directives.option_order,
                None,
            )
        }
        DecodedSourceText::TextWithOriginalBytes(content, original) => {
            let parsed = tsz_conformance::test_parser::parse_test_file(&content)?;
            if let Some(reason) =
                tsz_conformance::test_parser::should_skip_test_at_path(path, &parsed.directives)
            {
                return Ok(ProcessOutcome::Skipped(key, reason, source_sha256));
            }
            let option_variants =
                tsz_conformance::test_parser::select_ts7_oracle_configurations(&parsed.directives)
                    .expect("TS7 selector succeeded during skip check");
            (
                Some(content),
                parsed.directives.filenames,
                option_variants,
                parsed.directives.option_order,
                Some(original),
            )
        }
        DecodedSourceText::Binary(bytes) => (
            None,
            Vec::new(),
            vec![HashMap::new()],
            Vec::new(),
            Some(bytes),
        ),
    };

    let metadata = fs::metadata(path)?;
    let mtime_ms = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    let size = metadata.len();

    let original_extension = path.extension().and_then(|e| e.to_str());

    let mut error_codes = Vec::new();
    let mut diagnostic_fingerprints = Vec::new();
    let mut ordinary_exit_statuses = Vec::new();
    for options in &option_variants {
        // Prepare and compile every configuration selected by the TS7 harness.
        let prepared = if let Some(content) = &content {
            let ts_tests_lib_dir = tsz_wrapper::tests_lib_dir_for_cases_dir(test_dir);
            tsz_wrapper::prepare_test_dir_with_lib_dir(
                content,
                &filenames,
                options,
                original_extension,
                &option_order,
                Some(&ts_tests_lib_dir),
            )?
        } else if let Some(bytes) = &binary_bytes {
            tsz_wrapper::prepare_binary_test_dir(
                bytes,
                original_extension.unwrap_or("ts"),
                options,
            )?
        } else {
            return Err(anyhow::anyhow!("No content or binary bytes for test file"));
        };
        let work_dir = prepared.project_dir.as_path();

        node_sem.acquire();
        let mut command = Command::new(tsc_path);
        command
            .arg("--project")
            .arg(work_dir)
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false");
        command.arg("--singleThreaded");
        command.arg("--stableTypeOrdering").arg("true");
        command.current_dir(work_dir);
        let output = run_command_with_timeout(command, Duration::from_secs(timeout_secs));
        node_sem.release();

        let output = output.map_err(|error| anyhow::anyhow!("Failed to run tsc: {error}"))?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        let result = tsz_wrapper::parse_tsz_output(&output, work_dir, options.clone());
        if !output.status.success()
            && stderr.contains("Cannot find module")
            && result.error_codes.is_empty()
            && result.diagnostic_fingerprints.is_empty()
        {
            return Err(anyhow::anyhow!(
                "tsc startup failure (MODULE_NOT_FOUND): {}",
                stderr
                    .lines()
                    .find(|line| line.contains("Cannot find module"))
                    .unwrap_or("unknown")
            ));
        }

        if output.status.code().is_none()
            || (!output.status.success()
                && result.error_codes.is_empty()
                && result.diagnostic_fingerprints.is_empty())
        {
            return Err(anyhow::anyhow!(
                "tsc exited unsuccessfully without compiler diagnostics (status {}): stdout={:?} stderr={:?}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        if result.crashed || !result.semantic_completion.is_complete() {
            return Err(anyhow::anyhow!(
                "tsc output was not fully covered by the grouped diagnostic parser (status {}): stdout={:?} stderr={:?}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        if result.ordinary_exit_statuses.len() != 1 || result.ordinary_exit_statuses[0] > 2 {
            return Err(anyhow::anyhow!(
                "tsc did not provide one exact ordinary exit status 0/1/2: {}",
                output.status
            ));
        }
        error_codes.extend(result.error_codes);
        diagnostic_fingerprints.extend(result.diagnostic_fingerprints);
        ordinary_exit_statuses.extend(result.ordinary_exit_statuses);
    }

    Ok(ProcessOutcome::Cached(
        key,
        TscCacheEntry {
            metadata: FileMetadata {
                mtime_ms,
                size,
                typescript_version: Some(tsc_version.to_string()),
                source_sha256,
            },
            error_codes,
            diagnostic_fingerprints,
            diagnostic_blocks_complete: true,
            ordinary_exit_statuses,
        },
    ))
}

fn write_cache(path: &str, cache: &HashMap<String, TscCacheEntry>) -> Result<()> {
    use std::fs::File;
    use std::io::BufWriter;

    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let ordered: BTreeMap<_, _> = cache.iter().collect();
    serde_json::to_writer_pretty(writer, &ordered)?;
    Ok(())
}

fn write_domain(path: &str, domain: &ConformanceDomain<'_>) -> Result<()> {
    use std::fs::File;
    use std::io::BufWriter;

    let file = File::create(path)?;
    serde_json::to_writer_pretty(BufWriter::new(file), domain)?;
    Ok(())
}
