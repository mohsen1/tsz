//! TSC Cache Generator using tsc directly
//!
//! Generates the conformance cache by running tsc on each test file.
//! Uses the same `prepare_test_dir` and output parsing as the conformance runner
//! to ensure cache-vs-runner consistency.
//!
//! Architecture: rayon threads handle Rust-side work (file I/O, parsing, setup)
//! while a semaphore caps concurrent node subprocesses to avoid OOM.

use anyhow::Result;
use clap::Parser;
use rayon::prelude::*;
use serde::Serialize;
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
}

#[derive(Debug, Clone, Serialize)]
struct FileMetadata {
    mtime_ms: u64,
    size: u64,
    #[serde(default)]
    typescript_version: Option<String>,
}

enum ProcessOutcome {
    Cached(String, TscCacheEntry),
    Skipped(String, &'static str),
}

#[derive(Serialize)]
struct ConformanceDomain<'a> {
    typescript_version: &'a str,
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

fn resolve_tsc_path() -> Result<String> {
    // Prefer the project-local TypeScript installed in scripts/node_modules.
    // This ensures the cache is generated with the pinned tsc version from
    // scripts/package.json, not a random global tsc (which may be a different
    // major version and produce different diagnostics).
    let scripts_tsc = Path::new("scripts/node_modules/typescript/lib/tsc.js");
    if scripts_tsc.exists() {
        // Canonicalize to absolute path so it works when current_dir is a temp directory
        let abs = scripts_tsc
            .canonicalize()
            .unwrap_or_else(|_| scripts_tsc.to_path_buf());
        return Ok(abs.to_string_lossy().to_string());
    }
    if let Ok(output) = Command::new("node")
        .args([
            "-e",
            "const path=require('path'); const p=require.resolve('typescript/package.json'); console.log(path.join(path.dirname(p),'lib','tsc.js'))",
        ])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if std::path::Path::new(&path).exists() {
                return Ok(path);
            }
        }
    }
    if let Ok(output) = Command::new("which").arg("tsc").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
    }
    Ok("npx:tsc".to_string())
}

fn tsc_command(tsc_path: &str) -> Command {
    if tsc_path.starts_with("npx:") {
        let mut command = Command::new("npx");
        command.arg("tsc");
        command
    } else if tsc_path.ends_with(".js") {
        let mut command = Command::new("node");
        command.arg(tsc_path);
        command
    } else {
        Command::new(tsc_path)
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

fn is_typescript_7_or_newer(version: &str) -> bool {
    version
        .split('.')
        .next()
        .and_then(|major| major.parse::<u32>().ok())
        .is_some_and(|major| major >= 7)
}

fn main() -> Result<()> {
    let args = Args::parse();

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

    let tsc_path = resolve_tsc_path()?;
    let tsc_version = resolve_tsc_version().unwrap_or_else(|_| "unknown".to_string());
    println!("📍 Using tsc: {}", tsc_path);
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
    let processed = AtomicUsize::new(0);
    let errors = AtomicUsize::new(0);
    let skipped = AtomicUsize::new(0);
    let total = test_files.len();
    let verbose = args.verbose;
    let tsc_path_ref = &tsc_path;
    let test_dir_path = Path::new(&args.test_dir)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&args.test_dir));
    let node_semaphore = Arc::new(CountingSemaphore::new(max_node));

    test_files.par_iter().for_each(|path| {
        match process_test_file(
            path,
            &test_dir_path,
            tsc_path_ref,
            tsc_version.as_str(),
            args.timeout,
            &node_semaphore,
        ) {
            Ok(ProcessOutcome::Cached(key, entry)) => {
                cache.lock().unwrap().insert(key, entry);
            }
            Ok(ProcessOutcome::Skipped(key, reason)) => {
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
                if verbose {
                    println!("✗ Error processing {}: {}", path.display(), e);
                }
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

    println!("\n💾 Writing cache to: {}", args.output);
    write_cache(&args.output, &cache)?;
    println!("✓ Cache written with {} entries", cache.len());

    let domain_output = args
        .domain_output
        .unwrap_or_else(|| format!("{}.domain.json", args.output));
    write_domain(
        &domain_output,
        &ConformanceDomain {
            typescript_version: &tsc_version,
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

    for entry in WalkDir::new(test_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();

        if path.is_dir() {
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
    tsc_path: &str,
    tsc_version: &str,
    timeout_secs: u64,
    node_sem: &CountingSemaphore,
) -> Result<ProcessOutcome> {
    use std::fs;
    use tsz_conformance::text_decode::{decode_source_text, DecodedSourceText};

    let bytes = fs::read(path)?;
    let decoded = decode_source_text(&bytes);
    let key = tsz_conformance::cache::cache_key(path, test_dir).ok_or_else(|| {
        anyhow::anyhow!(
            "Path {} is not under test dir {}",
            path.display(),
            test_dir.display()
        )
    })?;

    let (content, filenames, option_variants, option_order, binary_bytes) = match decoded {
        DecodedSourceText::Text(content) => {
            let parsed = tsz_conformance::test_parser::parse_test_file(&content)?;
            if let Some(reason) =
                tsz_conformance::test_parser::should_skip_test_at_path(path, &parsed.directives)
            {
                return Ok(ProcessOutcome::Skipped(key, reason));
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
                return Ok(ProcessOutcome::Skipped(key, reason));
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

    let mut error_codes = std::collections::HashSet::new();
    let mut diagnostic_fingerprints = std::collections::HashSet::new();
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
                None,
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
        let mut command = tsc_command(tsc_path);
        command
            .arg("--project")
            .arg(work_dir)
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false");
        if is_typescript_7_or_newer(tsc_version) {
            command.arg("--singleThreaded");
            command.arg("--stableTypeOrdering").arg("true");
        }
        command.current_dir(work_dir);
        let output = run_command_with_timeout(command, Duration::from_secs(timeout_secs));
        node_sem.release();

        let output = output.map_err(|error| anyhow::anyhow!("Failed to run tsc: {error}"))?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success()
            && stderr.contains("Cannot find module")
            && !stderr.contains("error TS")
        {
            return Err(anyhow::anyhow!(
                "tsc startup failure (MODULE_NOT_FOUND): {}",
                stderr
                    .lines()
                    .find(|line| line.contains("Cannot find module"))
                    .unwrap_or("unknown")
            ));
        }

        let result = tsz_wrapper::parse_tsz_output(&output, work_dir, options.clone());
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
        error_codes.extend(result.error_codes);
        diagnostic_fingerprints.extend(result.diagnostic_fingerprints);
    }

    let mut error_codes: Vec<_> = error_codes.into_iter().collect();
    error_codes.sort_unstable();
    let mut diagnostic_fingerprints: Vec<_> = diagnostic_fingerprints.into_iter().collect();
    diagnostic_fingerprints.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then_with(|| left.file.cmp(&right.file))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.column.cmp(&right.column))
            .then_with(|| left.message_key.cmp(&right.message_key))
    });

    Ok(ProcessOutcome::Cached(
        key,
        TscCacheEntry {
            metadata: FileMetadata {
                mtime_ms,
                size,
                typescript_version: Some(tsc_version.to_string()),
            },
            error_codes,
            diagnostic_fingerprints,
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

fn resolve_tsc_version() -> Result<String> {
    // Read the actual version from the project-local TypeScript installation.
    // This must match the tsc binary resolved by resolve_tsc_path() to ensure
    // the version metadata in cache entries accurately reflects which tsc ran.
    let local_pkg = Path::new("scripts/node_modules/typescript/package.json");
    if local_pkg.exists() {
        if let Ok(content) = std::fs::read_to_string(local_pkg) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(version) = pkg.get("version").and_then(|v| v.as_str()) {
                    return Ok(version.to_string());
                }
            }
        }
    }
    // Fallback: try require.resolve
    let script = r#"
        try {
            const p = require.resolve('typescript/package.json');
            const pkg = JSON.parse(require('fs').readFileSync(p, 'utf8'));
            console.log(pkg.version || 'unknown');
        } catch { console.log('unknown'); }
    "#;
    let output = Command::new("node").args(["-e", script]).output()?;

    if !output.status.success() {
        return Ok("unknown".to_string());
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        Ok("unknown".to_string())
    } else {
        Ok(version)
    }
}
