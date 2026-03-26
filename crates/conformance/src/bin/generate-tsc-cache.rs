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
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;
use tsz_conformance::tsc_results::DiagnosticFingerprint;
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
            "console.log(require.resolve('typescript/lib/tsc.js'))",
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
            &node_semaphore,
        ) {
            Ok(Some((key, entry))) => {
                cache.lock().unwrap().insert(key, entry);
            }
            Ok(None) => {
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

    println!(
        "\r✓ Completed in {:.1}s ({:.0} tests/sec)                              ",
        start.elapsed().as_secs_f64(),
        test_files.len() as f64 / start.elapsed().as_secs_f64()
    );

    println!("  Processed: {}", processed.load(Ordering::SeqCst));
    println!("  Cached: {}", cache.len());
    println!("  Skipped: {}", skipped.load(Ordering::SeqCst));
    println!("  Errors: {}", errors.load(Ordering::SeqCst));

    println!("\n💾 Writing cache to: {}", args.output);
    write_cache(&args.output, &cache)?;
    println!("✓ Cache written with {} entries", cache.len());

    Ok(())
}

fn discover_tests(test_dir: &str, max: usize, filter: Option<&str>) -> Result<Vec<PathBuf>> {
    use tsz_conformance::test_filter::matches_path_filter;
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

        let path_str = path.to_string_lossy();

        if path_str.contains("/fourslash/") || path_str.contains("\\fourslash\\") {
            continue;
        }

        if path_str.ends_with(".d.ts") {
            continue;
        }

        if path
            .extension()
            .is_some_and(|ext| ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx")
        {
            if !matches_path_filter(path, filter) {
                continue;
            }
            files.push(path.to_path_buf());
        }
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
    node_sem: &CountingSemaphore,
) -> Result<Option<(String, TscCacheEntry)>> {
    use std::fs;
    use tsz_conformance::text_decode::{decode_source_text, DecodedSourceText};

    let bytes = fs::read(path)?;
    let decoded = decode_source_text(&bytes);

    let (content, filenames, options, binary_bytes) = match decoded {
        DecodedSourceText::Text(content) => {
            let parsed = tsz_conformance::test_parser::parse_test_file(&content)?;
            if tsz_conformance::test_parser::should_skip_test(&parsed.directives).is_some() {
                return Ok(None);
            }
            (
                Some(content),
                parsed.directives.filenames,
                parsed.directives.options,
                None,
            )
        }
        DecodedSourceText::TextWithOriginalBytes(content, original) => {
            let parsed = tsz_conformance::test_parser::parse_test_file(&content)?;
            if tsz_conformance::test_parser::should_skip_test(&parsed.directives).is_some() {
                return Ok(None);
            }
            (
                Some(content),
                parsed.directives.filenames,
                parsed.directives.options,
                Some(original),
            )
        }
        DecodedSourceText::Binary(bytes) => (None, Vec::new(), HashMap::new(), Some(bytes)),
    };

    let metadata = fs::metadata(path)?;
    let mtime_ms = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    let size = metadata.len();

    let key = tsz_conformance::cache::cache_key(path, test_dir).ok_or_else(|| {
        anyhow::anyhow!(
            "Path {} is not under test dir {}",
            path.display(),
            test_dir.display()
        )
    })?;

    let original_extension = path.extension().and_then(|e| e.to_str());

    // Prepare test dir (Rust-side work — no semaphore needed)
    let prepared = if let Some(content) = &content {
        tsz_wrapper::prepare_test_dir(content, &filenames, &options, original_extension, &[], None)?
    } else if let Some(bytes) = &binary_bytes {
        tsz_wrapper::prepare_binary_test_dir(bytes, original_extension.unwrap_or("ts"), &options)?
    } else {
        return Err(anyhow::anyhow!("No content or binary bytes for test file"));
    };

    let work_dir = prepared.project_dir.as_path();

    // Acquire semaphore before spawning node subprocess to cap memory usage
    node_sem.acquire();

    let output = if tsc_path.starts_with("npx:") {
        Command::new("npx")
            .arg("tsc")
            .arg("--project")
            .arg(work_dir)
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false")
            .current_dir(work_dir)
            .output()
    } else if tsc_path.ends_with(".js") {
        Command::new("node")
            .arg(tsc_path)
            .arg("--project")
            .arg(work_dir)
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false")
            .current_dir(work_dir)
            .output()
    } else {
        Command::new(tsc_path)
            .arg("--project")
            .arg(work_dir)
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false")
            .current_dir(work_dir)
            .output()
    };

    // Release permit immediately after subprocess completes
    node_sem.release();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to run tsc: {}", e));
        }
    };

    // Detect node/tsc startup failures (e.g. MODULE_NOT_FOUND) that produce
    // no TS diagnostics.  Without this check the test silently caches as [].
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success()
        && stderr.contains("Cannot find module")
        && !stderr.contains("error TS")
    {
        return Err(anyhow::anyhow!(
            "tsc startup failure (MODULE_NOT_FOUND): {}",
            stderr
                .lines()
                .find(|l| l.contains("Cannot find module"))
                .unwrap_or("unknown")
        ));
    }

    let result = tsz_wrapper::parse_tsz_output(&output, work_dir, options);

    let mut error_codes = result.error_codes;
    error_codes.sort();
    error_codes.dedup();

    Ok(Some((
        key,
        TscCacheEntry {
            metadata: FileMetadata {
                mtime_ms,
                size,
                typescript_version: Some(tsc_version.to_string()),
            },
            error_codes,
            diagnostic_fingerprints: result.diagnostic_fingerprints,
        },
    )))
}

fn write_cache(path: &str, cache: &HashMap<String, TscCacheEntry>) -> Result<()> {
    use std::fs::File;
    use std::io::BufWriter;

    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, cache)?;
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
