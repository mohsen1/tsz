//! TSC Cache Generator
//!
//! Generates tsc-cache.json by running TSC on all test files.
//!
//! Usage:
//!   cargo run --bin generate-tsc-cache -- --test-dir ./TypeScript/tests/cases/conformance --output ./tsc-cache.json

use anyhow::Result;
use clap::Parser;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use walkdir::WalkDir;
use rayon::prelude::*;

#[derive(Parser, Debug)]
#[command(name = "generate-tsc-cache")]
#[command(about = "Generate TSC cache for conformance testing", long_about = None)]
struct Args {
    /// Test directory path
    #[arg(long, default_value = "./TypeScript/tests/cases/conformance")]
    test_dir: String,

    /// Output cache file path
    #[arg(long, default_value = "./tsc-cache.json")]
    output: String,

    /// TSC binary path
    #[arg(long, default_value = "tsc")]
    tsc_binary: String,

    /// Maximum number of tests to process (0 = unlimited)
    #[arg(long, default_value_t = 0)]
    max: usize,

    /// Number of parallel workers
    #[arg(long, default_value_t = num_cpus::get())]
    workers: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TscCacheEntry {
    metadata: FileMetadata,
    error_codes: Vec<u32>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct FileMetadata {
    mtime_ms: u64,
    size: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!("🔍 Discovering test files in: {}", args.test_dir);
    let test_files = discover_tests(&args.test_dir, args.max)?;
    println!("✓ Found {} test files", test_files.len());

    println!("\n🔨 Running TSC on {} tests with {} workers...", test_files.len(), args.workers);
    let start = Instant::now();

    let cache = generate_cache(&test_files, &args.tsc_binary, args.workers)?;

    let elapsed = start.elapsed();
    println!("✓ Completed in {:.1}s ({:.0} tests/sec)", elapsed.as_secs_f64(), test_files.len() as f64 / elapsed.as_secs_f64());

    println!("\n💾 Writing cache to: {}", args.output);
    write_cache(&args.output, &cache)?;
    println!("✓ Cache written successfully");

    Ok(())
}

/// Discover all test files recursively
fn discover_tests(test_dir: &str, max: usize) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in WalkDir::new(test_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if path.is_dir() {
            continue;
        }

        if path.extension().map_or(false, |ext| {
            ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx"
        }) {
            files.push(path.to_path_buf());
        }
    }

    files.sort();

    if max > 0 && files.len() > max {
        files.truncate(max);
    }

    Ok(files)
}

/// Generate cache by running TSC on each test
fn generate_cache(
    test_files: &[PathBuf],
    tsc_binary: &str,
    workers: usize,
) -> Result<HashMap<String, TscCacheEntry>> {
    use rayon::prelude::*;
    use std::sync::Mutex;
    use indicatif::ProgressBar;
    use indicatif::ProgressStyle;

    let cache = Mutex::new(HashMap::new());
    let bar = ProgressBar::new(test_files.len() as u64);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
    );

    // Set up Rayon thread pool
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;

    pool.install(|| {
        test_files.par_iter().for_each(|path| {
            match process_test_file(path, tsc_binary) {
                Ok(Some(entry)) => {
                    cache.lock().unwrap().insert(entry.0, entry.1);
                }
                Ok(None) => {
                    // Test was skipped (e.g., @noCheck)
                }
                Err(e) => {
                    eprintln!("✗ Error processing {}: {}", path.display(), e);
                }
            }
            bar.inc(1);
        });
    });

    bar.finish();

    Ok(cache.into_inner()?)
}
fn process_test_file(
    path: &Path,
    tsc_binary: &str,
) -> Result<Option<(String, TscCacheEntry)>> {
    use std::fs;

    // Read file content
    let content = fs::read_to_string(path)?;

    // Parse directives (reuse existing parser)
    let parsed = tsz_conformance::test_parser::parse_test_file(&content)?;

    // Check if should skip
    if tsz_conformance::test_parser::should_skip_test(&parsed.directives).is_some() {
        return Ok(None);
    }

    // Get file metadata
    let metadata = fs::metadata(path)?;
    let mtime_ms = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    let size = metadata.len() as u64;

    // Calculate hash
    let hash = tsz_conformance::cache::calculate_test_hash(&content, &parsed.directives.options);

    // Run TSC and extract error codes
    let error_codes = run_tsc_and_extract_errors(path, tsc_binary, &parsed.directives)?;

    Ok(Some((
        hash,
        TscCacheEntry {
            metadata: FileMetadata { mtime_ms, size },
            error_codes,
        },
    )))
}

/// Run TSC and extract error codes from output
fn run_tsc_and_extract_errors(
    test_file: &Path,
    tsc_binary: &str,
    directives: &tsz_conformance::test_parser::TestDirectives,
) -> Result<Vec<u32>> {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    
    // Use atomic counter to ensure unique temp directory per invocation
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique_id = COUNTER.fetch_add(1, Ordering::SeqCst);

    // Create a unique temp directory for this specific invocation
    let temp_dir = std::env::temp_dir();
    let dir_path = temp_dir.join(format!("tsz-conformance-{}-{}", std::process::id(), unique_id));
    fs::create_dir_all(&dir_path)?;

    // Write test file
    let test_path = dir_path.join("test.ts");
    std::fs::write(&test_path, std::fs::read_to_string(test_file)?)?;

    // Write additional files from @filename directives
    for (filename, file_content) in &directives.filenames {
        let file_path = dir_path.join(filename);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, file_content)?;
    }

    // Create tsconfig.json
    let tsconfig_path = dir_path.join("tsconfig.json");
    let tsconfig_content = create_tsconfig(directives);
    std::fs::write(&tsconfig_path, tsconfig_content)?;

    // Run TSC (use shell to handle commands like "npx tsc")
    let output = if tsc_binary.contains(' ') {
        // Command with arguments - use shell
        Command::new("sh")
            .arg("-c")
            .arg(format!("{} --project {} --noEmit", tsc_binary, dir_path.display()))
            .current_dir(&dir_path)
            .output()?
    } else {
        // Simple binary name
        Command::new(tsc_binary)
            .arg("--project")
            .arg(&dir_path)
            .arg("--noEmit")
            .current_dir(&dir_path)
            .output()?
    };

    // Parse error codes from stdout (TSC outputs errors to stdout)
    let error_codes = parse_tsc_output(&String::from_utf8_lossy(&output.stdout));

    // Clean up temp directory
    let _ = fs::remove_dir_all(&dir_path);

    Ok(error_codes)
}

/// Create tsconfig.json content from directives
fn create_tsconfig(directives: &tsz_conformance::test_parser::TestDirectives) -> String {
    let mut opts = serde_json::json!({});

    if let Some(strict) = directives.options.get("strict") {
        opts["strict"] = serde_json::Value::Bool(strict == "true");
    }

    if let Some(target) = directives.options.get("target") {
        opts["target"] = serde_json::Value::String(target.clone());
    }

    if let Some(module) = directives.options.get("module") {
        opts["module"] = serde_json::Value::String(module.clone());
    }

    let tsconfig = serde_json::json!({
        "compilerOptions": opts,
        "include": ["./**/*.ts", "./**/*.tsx"],
        "exclude": ["node_modules"]
    });

    serde_json::to_string_pretty(&tsconfig).unwrap_or_default()
}

/// Parse TSC output to extract error codes
fn parse_tsc_output(output: &str) -> Vec<u32> {
    use regex::Regex;
    use once_cell::sync::Lazy;

    static ERROR_CODE_RE: Lazy<Regex> = Lazy::new(|| {
        // Match "error TS" followed by digits
        Regex::new(r"error TS(\d+)").unwrap()
    });

    let mut codes = Vec::new();

    for line in output.lines() {
        // TSC error format: "file.ts(line,col): error TS2526: message"
        for cap in ERROR_CODE_RE.captures_iter(line) {
            if let Ok(code) = cap[1].parse::<u32>() {
                codes.push(code);
            }
        }
    }

    codes.sort();
    codes.dedup();
    codes
}

/// Write cache to JSON file
fn write_cache(path: &str, cache: &HashMap<String, TscCacheEntry>) -> Result<()> {
    use std::io::BufWriter;
    use std::fs::File;

    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, cache)?;
    Ok(())
}
