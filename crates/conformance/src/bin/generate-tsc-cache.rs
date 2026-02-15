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
use tsz_conformance::tsc_results::DiagnosticFingerprint;
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
    "traceResolution",
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

// filter logic lives in tsz_conformance::test_filter

fn main() -> Result<()> {
    let args = Args::parse();

    // Set rayon thread pool size
    rayon::ThreadPoolBuilder::new()
        .num_threads(args.workers)
        .build_global()
        .ok();

    // Resolve tsc path once at startup (avoids npx overhead per file)
    let tsc_path = resolve_tsc_path()?;
    let tsc_version = resolve_tsc_version().unwrap_or_else(|_| "unknown".to_string());
    println!("📍 Using tsc: {}", tsc_path);
    println!("📍 TypeScript version: {tsc_version}");

    println!("🔍 Discovering test files in: {}", args.test_dir);
    let test_files = discover_tests(&args.test_dir, args.max, args.filter.as_deref())?;
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
    let test_dir_path = Path::new(&args.test_dir)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&args.test_dir));

    // Process tests in parallel
    test_files.par_iter().for_each(|path| {
        match process_test_file(
            path,
            &test_dir_path,
            timeout,
            tsc_path_ref,
            tsc_version.as_str(),
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

fn process_test_file(
    path: &Path,
    test_dir: &Path,
    _timeout_secs: u64,
    tsc_path: &str,
    tsc_version: &str,
) -> Result<Option<(String, TscCacheEntry)>> {
    use std::fs;

    // Read and decode file content (UTF-8/UTF-8 BOM/UTF-16 BOM).
    let bytes = fs::read(path)?;
    let decoded = tsz_conformance::text_decode::decode_source_text(&bytes);

    let (content, filenames, options, binary_bytes) = match decoded {
        tsz_conformance::text_decode::DecodedSourceText::Text(content) => {
            // Parse directives
            let parsed = tsz_conformance::test_parser::parse_test_file(&content)?;

            // Check if should skip
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
        tsz_conformance::text_decode::DecodedSourceText::Binary(bytes) => {
            (None, Vec::new(), HashMap::new(), Some(bytes))
        }
    };

    // Get file metadata
    let metadata = fs::metadata(path)?;
    let mtime_ms = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    let size = metadata.len();

    // Cache key is relative file path from test directory
    let key = tsz_conformance::cache::cache_key(path, test_dir).ok_or_else(|| {
        anyhow::anyhow!(
            "Path {} is not under test dir {}",
            path.display(),
            test_dir.display()
        )
    })?;

    // Create temporary directory for this test
    let temp_dir = tempfile::TempDir::new()?;
    let work_dir = temp_dir.path();

    // Create tsconfig.json with parsed @-directives unless test provides its own.
    let has_tsconfig_file = filenames
        .iter()
        .any(|(name, _)| name.replace('\\', "/").ends_with("tsconfig.json"));
    if !has_tsconfig_file {
        let tsconfig_path = work_dir.join("tsconfig.json");
        let tsconfig_content = serde_json::json!({
            "compilerOptions": convert_options_to_tsconfig(&options),
            "include": ["*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
            "exclude": ["node_modules"]
        });
        fs::write(
            &tsconfig_path,
            serde_json::to_string_pretty(&tsconfig_content)?,
        )?;
    }

    // Write test files
    if filenames.is_empty() {
        // Single-file test
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("ts");
        let main_file = work_dir.join(format!("test.{}", ext));
        if let Some(content) = content {
            fs::write(&main_file, strip_directive_comments(&content))?;
        } else if let Some(bytes) = binary_bytes {
            fs::write(&main_file, bytes)?;
        }
    } else {
        // Multi-file test: write files from @filename directives
        for (filename, file_content) in &filenames {
            let sanitized = filename
                .replace("..", "_")
                .trim_start_matches('/')
                .to_string();
            let file_path = work_dir.join(&sanitized);

            if !file_path.starts_with(work_dir) {
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
            .arg(work_dir)
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false")
            .current_dir(work_dir)
            .output()
    } else if tsc_path.ends_with(".js") {
        // Direct node invocation of tsc.js
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
        // Direct tsc binary
        Command::new(tsc_path)
            .arg("--project")
            .arg(work_dir)
            .arg("--noEmit")
            .arg("--pretty")
            .arg("false")
            .current_dir(work_dir)
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

    let diagnostic_fingerprints = parse_diagnostic_fingerprints(&combined, work_dir);
    let mut error_codes: Vec<u32> = diagnostic_fingerprints.iter().map(|d| d.code).collect();
    if error_codes.is_empty() {
        error_codes = parse_error_codes(&combined);
    }
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
            diagnostic_fingerprints,
        },
    )))
}

/// Convert test directive options to tsconfig compiler options JSON
fn convert_options_to_tsconfig(options: &HashMap<String, String>) -> serde_json::Value {
    // Delegate to the shared implementation in tsz_wrapper
    // Keys are already lowercase from the parser
    let mut opts = serde_json::Map::new();

    for (key, value) in options {
        let key_lower = key.to_lowercase();
        if HARNESS_ONLY_DIRECTIVES
            .iter()
            .any(|&d| d.to_lowercase() == key_lower)
        {
            continue;
        }

        let canonical_key = canonical_option_name(&key_lower);
        let json_value = if value == "true" {
            serde_json::Value::Bool(true)
        } else if value == "false" {
            serde_json::Value::Bool(false)
        } else if LIST_OPTIONS
            .iter()
            .any(|&opt| opt.to_lowercase() == key_lower)
        {
            let items: Vec<serde_json::Value> = value
                .split(',')
                .map(|s| serde_json::Value::String(s.trim().to_string()))
                .collect();
            serde_json::Value::Array(items)
        } else {
            // For non-list options, take only the first comma-separated value
            let effective_value = value.split(',').next().unwrap_or(value).trim();
            if let Ok(num) = effective_value.parse::<i64>() {
                serde_json::Value::Number(num.into())
            } else {
                serde_json::Value::String(effective_value.to_string())
            }
        };

        opts.insert(canonical_key.to_string(), json_value);
    }

    // Mirror TypeScript strict-family defaulting behavior when `strict` is specified.
    if let Some(serde_json::Value::Bool(strict)) = opts.get("strict") {
        let strict = *strict;
        for key in [
            "noImplicitAny",
            "noImplicitThis",
            "strictNullChecks",
            "strictFunctionTypes",
            "strictBindCallApply",
            "strictPropertyInitialization",
            "useUnknownInCatchVariables",
            "alwaysStrict",
        ] {
            opts.entry(key.to_string())
                .or_insert(serde_json::Value::Bool(strict));
        }
    }

    serde_json::Value::Object(opts)
}

fn canonical_option_name(key_lower: &str) -> &str {
    match key_lower {
        "allowarbitraryextensions" => "allowArbitraryExtensions",
        "allowimportingtsextensions" => "allowImportingTsExtensions",
        "allowjs" => "allowJs",
        "allowsyntheticdefaultimports" => "allowSyntheticDefaultImports",
        "allowunreachablecode" => "allowUnreachableCode",
        "allowunusedlabels" => "allowUnusedLabels",
        "alwaysstrict" => "alwaysStrict",
        "baseurl" => "baseUrl",
        "checkjs" => "checkJs",
        "customconditions" => "customConditions",
        "declaration" => "declaration",
        "declarationdir" => "declarationDir",
        "declarationmap" => "declarationMap",
        "emitdeclarationonly" => "emitDeclarationOnly",
        "emitdecoratormetadata" => "emitDecoratorMetadata",
        "esmoduleinterop" => "esModuleInterop",
        "exactoptionalpropertytypes" => "exactOptionalPropertyTypes",
        "experimentaldecorators" => "experimentalDecorators",
        "importhelpers" => "importHelpers",
        "incremental" => "incremental",
        "isolateddeclarations" => "isolatedDeclarations",
        "isolatedmodules" => "isolatedModules",
        "jsx" => "jsx",
        "lib" => "lib",
        "maxnodemodulejsdepth" => "maxNodeModuleJsDepth",
        "module" => "module",
        "moduleresolution" => "moduleResolution",
        "modulesuffixes" => "moduleSuffixes",
        "noemitonerror" => "noEmitOnError",
        "noemit" => "noEmit",
        "noemithelpers" => "noEmitHelpers",
        "nofallthrough" => "noFallthroughCasesInSwitch",
        "nofallthroughcasesinswitch" => "noFallthroughCasesInSwitch",
        "noimplicitany" => "noImplicitAny",
        "noimplicitreturns" => "noImplicitReturns",
        "noimplicitthis" => "noImplicitThis",
        "nolib" => "noLib",
        "nopropertyaccessfromindexsignature" => "noPropertyAccessFromIndexSignature",
        "noresolve" => "noResolve",
        "nouncheckedsideeffectimports" => "noUncheckedSideEffectImports",
        "nounusedlocals" => "noUnusedLocals",
        "nounusedparameters" => "noUnusedParameters",
        "outdir" => "outDir",
        "outfile" => "outFile",
        "paths" => "paths",
        "preserveconstenums" => "preserveConstEnums",
        "preservesymlinks" => "preserveSymlinks",
        "removecomments" => "removeComments",
        "resolvepackagejsonexports" => "resolvePackageJsonExports",
        "resolvepackagejsonimports" => "resolvePackageJsonImports",
        "resolvejsonmodule" => "resolveJsonModule",
        "rewriterelativeimportextensions" => "rewriteRelativeImportExtensions",
        "rootdir" => "rootDir",
        "rootdirs" => "rootDirs",
        "skiplibcheck" => "skipLibCheck",
        "sourcemap" => "sourceMap",
        "strict" => "strict",
        "strictbindcallapply" => "strictBindCallApply",
        "strictfunctiontypes" => "strictFunctionTypes",
        "strictnullchecks" => "strictNullChecks",
        "strictpropertyinitialization" => "strictPropertyInitialization",
        "target" => "target",
        "tsbuildinfoffile" => "tsBuildInfoFile",
        "typeroots" => "typeRoots",
        "types" => "types",
        "usedefineforclassfields" => "useDefineForClassFields",
        "useunknownincatchvariables" => "useUnknownInCatchVariables",
        _ => key_lower,
    }
}

/// Parse error codes from tsc output
fn parse_error_codes(text: &str) -> Vec<u32> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static DIAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"error TS(\d+):").unwrap());

    let mut codes = Vec::new();
    for line in text.lines() {
        if let Some(caps) = DIAG_RE.captures(line) {
            if let Ok(code) = caps[1].parse::<u32>() {
                codes.push(code);
            }
        }
    }
    codes
}

/// Parse detailed diagnostics and normalize paths relative to a per-test project root.
fn parse_diagnostic_fingerprints(text: &str, project_root: &Path) -> Vec<DiagnosticFingerprint> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static DIAG_WITH_POS_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^(?P<file>.+?)\((?P<line>\d+),(?P<col>\d+)\):\s+error\s+TS(?P<code>\d+):\s*(?P<message>.+)$")
            .expect("valid regex")
    });
    static DIAG_NO_POS_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^error\s+TS(?P<code>\d+):\s*(?P<message>.+)$").unwrap());

    let mut diagnostics = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(caps) = DIAG_WITH_POS_RE.captures(line) {
            if let Some(code) = caps
                .name("code")
                .and_then(|m| m.as_str().parse::<u32>().ok())
            {
                let line_no = caps
                    .name("line")
                    .and_then(|m| m.as_str().parse::<u32>().ok())
                    .unwrap_or(0);
                let col_no = caps
                    .name("col")
                    .and_then(|m| m.as_str().parse::<u32>().ok())
                    .unwrap_or(0);
                let raw_file = caps.name("file").map(|m| m.as_str()).unwrap_or_default();
                let file = normalize_diagnostic_path(raw_file, project_root);
                let message = caps.name("message").map(|m| m.as_str()).unwrap_or_default();
                diagnostics.push(DiagnosticFingerprint::new(
                    code, file, line_no, col_no, message,
                ));
            }
            continue;
        }

        if let Some(caps) = DIAG_NO_POS_RE.captures(line) {
            if let Some(code) = caps
                .name("code")
                .and_then(|m| m.as_str().parse::<u32>().ok())
            {
                let message = caps.name("message").map(|m| m.as_str()).unwrap_or_default();
                diagnostics.push(DiagnosticFingerprint::new(
                    code,
                    String::new(),
                    0,
                    0,
                    message,
                ));
            }
        }
    }

    diagnostics.sort_by(|a, b| {
        (
            a.code,
            a.file.as_str(),
            a.line,
            a.column,
            a.message_key.as_str(),
        )
            .cmp(&(
                b.code,
                b.file.as_str(),
                b.line,
                b.column,
                b.message_key.as_str(),
            ))
    });
    diagnostics.dedup();
    diagnostics
}

fn normalize_diagnostic_path(raw: &str, project_root: &Path) -> String {
    let mut normalized = raw.trim().replace('\\', "/");
    let root = project_root.to_string_lossy().replace('\\', "/");
    if normalized.starts_with(&root) {
        normalized = normalized[root.len()..].trim_start_matches('/').to_string();
    }
    normalized
}

/// Strip @ directive comments from test file content
fn strip_directive_comments(content: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static DIRECTIVE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*//\s*@\w+\s*:").unwrap());

    content
        .lines()
        .filter(|line| !DIRECTIVE_RE.is_match(line))
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

fn resolve_tsc_version() -> Result<String> {
    let script = "const fs = require('fs'); const p = require.resolve('typescript/package.json'); const pkg = JSON.parse(fs.readFileSync(p, 'utf8')); console.log(pkg.version || 'unknown');";
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
