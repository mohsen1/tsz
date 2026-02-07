//! TSC Cache Generator using tsc directly
//!
//! Generates the conformance cache by running tsc on each test file.
//! Slower than tsserver but more accurate and reliable.
//! The cache can be stored in GitHub artifacts per TypeScript version.

use anyhow::Result;
use clap::Parser;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(name = "generate-tsc-cache")]
#[command(about = "Generate TSC cache using tsc directly (accurate)", long_about = None)]
struct Args {
    /// Test directory path
    #[arg(long, default_value = "./TypeScript/tests/cases")]
    test_dir: String,

    /// Output cache file path
    #[arg(long, default_value = "./tsc-cache-full.json")]
    output: String,

    /// Maximum number of tests to process (0 = unlimited)
    #[arg(long, default_value_t = 0)]
    max: usize,

    /// Number of parallel workers
    #[arg(long, default_value_t = 8)]
    workers: usize,

    /// Show verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Timeout per file in seconds
    #[arg(long, default_value_t = 60)]
    timeout: u64,
}

#[derive(Debug, Clone, Serialize)]
struct TscCacheEntry {
    metadata: FileMetadata,
    error_codes: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
struct FileMetadata {
    mtime_ms: u64,
    size: u64,
}

/// Test harness-specific directives that should NOT be passed to tsconfig.json
const HARNESS_ONLY_DIRECTIVES: &[&str] = &[
    "filename",
    "allowNonTsExtensions",
    "useCaseSensitiveFileNames",
    "baselineFile",
    "noErrorTruncation",
    "suppressOutputPathCheck",
    "noImplicitReferences",
    "currentDirectory",
    "symlink",
    "link",
    "noTypesAndSymbols",
    "fullEmitPaths",
    "noCheck",
    "nocheck",
    "reportDiagnostics",
    "captureSuggestions",
    "typeScriptVersion",
    "skip",
];

/// List-type compiler options that accept comma-separated values
const LIST_OPTIONS: &[&str] = &[
    "lib",
    "types",
    "typeRoots",
    "rootDirs",
    "moduleSuffixes",
    "customConditions",
];

fn resolve_tsc_path() -> Result<String> {
    // Try to find tsc without npx overhead
    // 1. Check node_modules/.bin/tsc (local install)
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
    // 2. Try `which tsc`
    if let Ok(output) = Command::new("which").arg("tsc").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
    }
    // 3. Fall back to npx (slow)
    Ok("npx:tsc".to_string())
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Set rayon thread pool size
    rayon::ThreadPoolBuilder::new()
        .num_threads(args.workers)
        .build_global()
        .ok();

    // Resolve tsc path once at startup (avoids npx overhead per file)
    let tsc_path = resolve_tsc_path()?;
    println!("📍 Using tsc: {}", tsc_path);

    println!("🔍 Discovering test files in: {}", args.test_dir);
    let test_files = discover_tests(&args.test_dir, args.max)?;
    println!("✓ Found {} test files", test_files.len());

    println!(
        "\n🔨 Processing {} tests with {} workers...",
        test_files.len(),
        args.workers
    );
    let start = Instant::now();

    let cache: Mutex<HashMap<String, TscCacheEntry>> = Mutex::new(HashMap::new());
    let processed = AtomicUsize::new(0);
    let errors = AtomicUsize::new(0);
    let skipped = AtomicUsize::new(0);
    let total = test_files.len();
    let verbose = args.verbose;
    let timeout = args.timeout;
    let tsc_path_ref = &tsc_path;

    // Process tests in parallel
    test_files.par_iter().for_each(|path| {
        match process_test_file(path, timeout, tsc_path_ref) {
            Ok(Some((hash, entry))) => {
                cache.lock().unwrap().insert(hash, entry);
            }
            Ok(None) => {
                skipped.fetch_add(1, Ordering::SeqCst);
            }
            Err(e) => {
                if verbose {
                    eprintln!("✗ Error processing {}: {}", path.display(), e);
                }
                errors.fetch_add(1, Ordering::SeqCst);
            }
        }

        let count = processed.fetch_add(1, Ordering::SeqCst) + 1;
        if count % 100 == 0 {
            let err_count = errors.load(Ordering::SeqCst);
            let skip_count = skipped.load(Ordering::SeqCst);
            eprint!(
                "\r[{}/{}] processed ({} errors, {} skipped)    ",
                count, total, err_count, skip_count
            );
        }
    });

    let cache = cache.into_inner().unwrap();

    println!(
        "\r✓ Completed in {:.1}s ({:.0} tests/sec)                    ",
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

        let path_str = path.to_string_lossy();

        // Skip fourslash tests (language service tests with special format)
        if path_str.contains("/fourslash/") || path_str.contains("\\fourslash\\") {
            continue;
        }

        // Skip .d.ts files
        if path_str.ends_with(".d.ts") {
            continue;
        }

        if path
            .extension()
            .map_or(false, |ext| ext == "ts" || ext == "tsx")
        {
            files.push(path.to_path_buf());
        }
    }

    files.sort();

    if max > 0 && files.len() > max {
        files.truncate(max);
    }

    Ok(files)
}

fn process_test_file(
    path: &Path,
    _timeout_secs: u64,
    tsc_path: &str,
) -> Result<Option<(String, TscCacheEntry)>> {
    use std::fs;

    // Read file content
    let content = fs::read_to_string(path)?;

    // Parse directives
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

    // Create temporary directory for this test
    let temp_dir = tempfile::TempDir::new()?;
    let test_dir = temp_dir.path();

    // Create tsconfig.json with parsed @-directives
    let tsconfig_path = test_dir.join("tsconfig.json");
    let tsconfig_content = serde_json::json!({
        "compilerOptions": convert_options_to_tsconfig(&parsed.directives.options),
        "include": ["*.ts", "*.tsx", "**/*.ts", "**/*.tsx"],
        "exclude": ["node_modules"]
    });
    fs::write(
        &tsconfig_path,
        serde_json::to_string_pretty(&tsconfig_content)?,
    )?;

    // Write test files
    if parsed.directives.filenames.is_empty() {
        // Single-file test
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("ts");
        let main_file = test_dir.join(format!("test.{}", ext));
        fs::write(&main_file, strip_directive_comments(&content))?;
    } else {
        // Multi-file test: write files from @filename directives
        for (filename, file_content) in &parsed.directives.filenames {
            let sanitized = filename
                .replace("..", "_")
                .trim_start_matches('/')
                .to_string();
            let file_path = test_dir.join(&sanitized);

            if !file_path.starts_with(test_dir) {
                continue;
            }

            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&file_path, file_content)?;
        }
    }

    // Run tsc (using pre-resolved path to avoid npx overhead)
    let output = if tsc_path.starts_with("npx:") {
        // Fallback to npx
        Command::new("npx")
            .arg("tsc")
            .arg("--project")
            .arg(test_dir)
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false")
            .current_dir(test_dir)
            .output()
    } else if tsc_path.ends_with(".js") {
        // Direct node invocation of tsc.js
        Command::new("node")
            .arg(tsc_path)
            .arg("--project")
            .arg(test_dir)
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false")
            .current_dir(test_dir)
            .output()
    } else {
        // Direct tsc binary
        Command::new(tsc_path)
            .arg("--project")
            .arg(test_dir)
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false")
            .current_dir(test_dir)
            .output()
    };

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to run tsc: {}", e));
        }
    };

    // Parse error codes from tsc output
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    let mut error_codes = parse_error_codes(&combined);
    error_codes.sort();
    error_codes.dedup();

    Ok(Some((
        hash,
        TscCacheEntry {
            metadata: FileMetadata { mtime_ms, size },
            error_codes,
        },
    )))
}

/// Convert test directive options to tsconfig compiler options JSON
fn convert_options_to_tsconfig(options: &HashMap<String, String>) -> serde_json::Value {
    let mut opts = serde_json::Map::new();

    for (key, value) in options {
        // Skip test harness-specific directives
        let key_lower = key.to_lowercase();
        if HARNESS_ONLY_DIRECTIVES
            .iter()
            .any(|&d| d.to_lowercase() == key_lower)
        {
            continue;
        }

        let json_value = if value == "true" {
            serde_json::Value::Bool(true)
        } else if value == "false" {
            serde_json::Value::Bool(false)
        } else if LIST_OPTIONS
            .iter()
            .any(|&opt| opt.to_lowercase() == key_lower)
        {
            // Parse comma-separated list
            let items: Vec<serde_json::Value> = value
                .split(',')
                .map(|s| serde_json::Value::String(s.trim().to_string()))
                .collect();
            serde_json::Value::Array(items)
        } else if let Ok(num) = value.parse::<i64>() {
            serde_json::Value::Number(num.into())
        } else {
            serde_json::Value::String(value.clone())
        };

        opts.insert(key.clone(), json_value);
    }

    serde_json::Value::Object(opts)
}

/// Parse error codes from tsc output
fn parse_error_codes(text: &str) -> Vec<u32> {
    let mut codes = Vec::new();

    for line in text.lines() {
        // Look for pattern: "error TS1234:" or "TS1234:"
        if let Some(start) = line.find("TS") {
            let rest = &line[start + 2..];
            if let Some(end) = rest.find(':') {
                if let Ok(code) = rest[..end].parse::<u32>() {
                    codes.push(code);
                }
            } else if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                if let Ok(code) = rest[..end].parse::<u32>() {
                    codes.push(code);
                }
            }
        }
    }

    codes
}

/// Strip @ directive comments from test file content
fn strip_directive_comments(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with("//") && trimmed.contains("@") && trimmed.contains(":"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn write_cache(path: &str, cache: &HashMap<String, TscCacheEntry>) -> Result<()> {
    use std::fs::File;
    use std::io::BufWriter;

    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, cache)?;
    Ok(())
}
