use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use rustc_hash::FxHashMap;
use std::ffi::OsString;
use std::io::IsTerminal;
use std::time::Duration;

use tsz::checker::diagnostics::DiagnosticCategory;
use tsz_cli::args::CliArgs;
use tsz_cli::help::{self, TSC_VERSION};
use tsz_cli::{driver, locale, reporter::Reporter, watch};

/// tsc exit status codes (matching TypeScript's `ExitStatus` enum)
const EXIT_SUCCESS: i32 = 0;
const EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED: i32 = 1;
const EXIT_DIAGNOSTICS_OUTPUTS_GENERATED: i32 = 2;

fn main() -> Result<()> {
    // Initialize tracing if TSZ_LOG or RUST_LOG is set (zero cost otherwise).
    // Supports TSZ_LOG_FORMAT=tree|json|text (see src/tracing_config.rs).
    tsz_cli::tracing_config::init_tracing();

    let preprocessed = preprocess_args(std::env::args_os().collect());

    // Check for TS6369: --build must be the first argument
    if let Some(msg) = check_build_position(&preprocessed) {
        println!("{msg}");
        std::process::exit(1);
    }

    let args = match CliArgs::try_parse_from(&preprocessed) {
        Ok(args) => args,
        Err(e) => {
            return handle_clap_error(e, &preprocessed);
        }
    };
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;

    // Run on a larger stack for project-sized and multi-file workflows.
    // Single-file CLI probes avoid this extra thread hop for lower startup overhead.
    if should_use_large_stack_thread(&args) {
        const MAIN_STACK_SIZE: usize = 64 * 1024 * 1024;
        std::thread::Builder::new()
            .stack_size(MAIN_STACK_SIZE)
            .spawn(move || actual_main(args, cwd))
            .expect("failed to spawn main thread")
            .join()
            .expect("main thread panicked")
    } else {
        actual_main(args, cwd)
    }
}

fn actual_main(args: CliArgs, cwd: std::path::PathBuf) -> Result<()> {
    // Initialize locale for i18n message translation
    locale::init_locale(args.locale.as_deref());

    // Handle --batch: enter batch compilation mode
    if args.batch {
        return run_batch_mode();
    }

    // Handle --init: create tsconfig.json
    if args.init {
        return handle_init(&args, &cwd);
    }

    // Handle --showConfig: print resolved configuration
    if args.show_config {
        return handle_show_config(&args, &cwd);
    }

    // Handle --listFilesOnly: print file list and exit
    if args.list_files_only {
        return handle_list_files_only(&args, &cwd);
    }

    // Handle --build mode
    if args.build {
        return handle_build(&args, &cwd);
    }

    if args.watch {
        return watch::run(&args, &cwd);
    }

    // No-input behavior: if no files given, no --project, and no tsconfig.json in cwd,
    // print version + help and exit 1 (matching tsc v6 behavior).
    if args.files.is_empty() && args.project.is_none() && !cwd.join("tsconfig.json").exists() {
        println!("Version {TSC_VERSION}");
        print!("{}", help::render_help(TSC_VERSION));
        std::process::exit(1);
    }

    // Initialize tracer if --generateTrace is specified
    let tracer = args.generate_trace.is_some().then(|| {
        let mut t = tsz_cli::trace::Tracer::new();
        // Add process metadata
        let mut meta_args = FxHashMap::default();
        meta_args.insert("name".to_string(), serde_json::json!("tsz"));
        t.metadata("process_name", meta_args);
        t
    });

    // Handle --generateCpuProfile: this is a V8-specific feature not applicable to a native
    // Rust compiler. The flag is accepted for CLI compatibility with tsc but has no effect.
    if let Some(ref _profile_path) = args.generate_cpu_profile {
        eprintln!(
            "The --generateCpuProfile flag is a V8/Node.js feature and is not applicable to tsz (a native Rust compiler). The flag is accepted for compatibility but has no effect."
        );
    }

    let start_time = std::time::Instant::now();
    let result = driver::compile(&args, &cwd)?;
    let elapsed = start_time.elapsed();

    // Write trace file if requested
    if let (Some(trace_path), Some(mut tracer)) = (args.generate_trace.as_ref(), tracer) {
        use tsz_cli::trace::categories;

        // Record compilation summary events
        tracer.complete_with_args("Compile", categories::PROGRAM, start_time, elapsed, {
            let mut args = FxHashMap::default();
            args.insert(
                "fileCount".to_string(),
                serde_json::json!(result.files_read.len()),
            );
            args.insert(
                "errorCount".to_string(),
                serde_json::json!(result.diagnostics.len()),
            );
            args.insert(
                "emittedCount".to_string(),
                serde_json::json!(result.emitted_files.len()),
            );
            args
        });

        // Add per-file events for files read
        for file in &result.files_read {
            let mut args = FxHashMap::default();
            args.insert(
                "path".to_string(),
                serde_json::json!(file.display().to_string()),
            );
            tracer.instant_with_args("FileProcessed", categories::IO, args);
        }

        // Write the trace file
        let trace_file = if trace_path.is_dir() {
            trace_path.join("trace.json")
        } else {
            trace_path.to_path_buf()
        };

        if let Err(e) = tracer.write_to_file(&trace_file) {
            println!("Warning: Failed to write trace file: {e}");
        } else {
            println!("Trace written to: {}", trace_file.display());
        }
    }

    // Handle --listFiles: print all files read during compilation
    if args.list_files {
        for file in &result.files_read {
            println!("{}", file.display());
        }
    }

    // Handle --listEmittedFiles: print emitted file list
    if args.list_emitted_files && !result.emitted_files.is_empty() {
        for file in &result.emitted_files {
            println!("TSFILE: {}", file.display());
        }
    }

    // Handle --explainFiles: print files with inclusion reasons
    if args.explain_files {
        for info in &result.file_infos {
            println!("{}", info.path.display());
            for reason in &info.reasons {
                println!("  {reason}");
            }
        }
    }

    // Handle --traceDependencies: print dependency graph
    if args.trace_dependencies {
        // Note: Full dependency tracing would require access to the dependency map
        // For now, just list all files that were read (which includes dependencies)
        for file in &result.files_read {
            println!("{}", file.display());
        }
    }

    // Handle --diagnostics: print compilation performance info
    if args.diagnostics || args.extended_diagnostics {
        print_diagnostics(&result, elapsed, args.extended_diagnostics);
    }

    if !result.diagnostics.is_empty() {
        let pretty = args
            .pretty
            .unwrap_or_else(|| std::io::stdout().is_terminal());
        let mut reporter = Reporter::new(pretty);
        let output = reporter.render(&result.diagnostics);
        if !output.is_empty() {
            // tsc writes all diagnostics to stdout
            print!("{output}");
        }
    }

    let has_errors = result
        .diagnostics
        .iter()
        .any(|diag| diag.category == DiagnosticCategory::Error);

    if has_errors {
        // Match tsc exit codes:
        // tsc uses exit code 2 when there are errors (DiagnosticsPresent_OutputsGenerated)
        // regardless of whether --noEmit is set. Exit code 1 is only for when emit
        // is explicitly skipped due to errors (noEmitOnError).
        if args.no_emit || !result.emitted_files.is_empty() {
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_GENERATED);
        } else {
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
        }
    }

    std::process::exit(EXIT_SUCCESS);
}

const fn should_use_large_stack_thread(args: &CliArgs) -> bool {
    args.project.is_some() || args.build || args.watch || args.batch || args.files.len() != 1
}

/// Batch compilation mode: read project directory paths from stdin (one per line),
/// compile each with `--project <path> --noEmit --pretty false`, print diagnostics,
/// then print a sentinel line so the caller can demarcate output boundaries.
///
/// Each iteration creates fresh `CliArgs` — no state is shared between compilations.
/// If tsz panics during any compilation, the process exits naturally (no `catch_unwind`).
/// The pool manager detects EOF on stdout and respawns a fresh worker.
fn run_batch_mode() -> Result<()> {
    use std::io::{BufRead, Write};

    let stdin = std::io::stdin();
    let reader = stdin.lock();
    let mut stdout = std::io::stdout().lock();

    for line in reader.lines() {
        let line = line.context("failed to read from stdin")?;
        let project_dir = line.trim();
        if project_dir.is_empty() {
            // Skip empty lines, print sentinel to keep protocol in sync
            writeln!(stdout, "---TSZ-BATCH-DONE---")?;
            stdout.flush()?;
            continue;
        }

        let project_path = std::path::Path::new(project_dir);

        // Build args matching what the conformance runner passes per test
        let batch_args = CliArgs::parse_from([
            "tsz",
            "--project",
            project_dir,
            "--noEmit",
            "--pretty",
            "false",
        ]);

        match driver::compile(&batch_args, project_path) {
            Ok(result) => {
                if !result.diagnostics.is_empty() {
                    let mut reporter = Reporter::new(false);
                    let output = reporter.render(&result.diagnostics);
                    if !output.is_empty() {
                        write!(stdout, "{output}")?;
                    }
                }
            }
            Err(e) => {
                // Print the error so the runner can see it, but don't exit
                writeln!(stdout, "error: {e}")?;
            }
        }

        writeln!(stdout, "---TSZ-BATCH-DONE---")?;
        stdout.flush()?;
    }

    Ok(())
}

/// Preprocess command-line arguments for tsc compatibility.
///
/// Handles (BEFORE clap parsing):
/// - `--version` / `-v` / `-V` → print version, exit 0
/// - `--help` / `-h` / `-?` → print help, exit 0
/// - `--all` (with or without `--help`) → print all options, exit 0
/// - `@file` response file expansion (tsc reads args from response files)
/// - Build mode flag remapping: when `--build`/`-b` is the first argument,
///   `-v` maps to `--build-verbose`, `-d` maps to `--dry`, `-f` maps to `--force`
fn preprocess_args(args: Vec<OsString>) -> Vec<OsString> {
    // First pass: expand response files and collect normalized arg strings
    let mut expanded = Vec::with_capacity(args.len());

    for (i, arg) in args.iter().enumerate() {
        if i == 0 {
            expanded.push(arg.clone());
            continue;
        }

        let arg_str = arg.to_string_lossy();

        if arg_str.starts_with('@') && arg_str.len() > 1 {
            // Response file: @path reads arguments from file
            let path = &arg_str[1..];
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() && !trimmed.starts_with('#') {
                            for part in split_response_line(trimmed) {
                                expanded.push(OsString::from(part));
                            }
                        }
                    }
                }
                Err(_) => {
                    expanded.push(arg.clone());
                }
            }
        } else {
            expanded.push(arg.clone());
        }
    }

    // Second pass: detect --help/-h/-?/--all/--version/-v/-V before clap
    // In build mode, -v means --build-verbose, not --version
    let is_build_mode = expanded
        .get(1)
        .map(|a| {
            let s = a.to_string_lossy();
            s == "--build" || s == "-b"
        })
        .unwrap_or(false);

    let mut has_help = false;
    let mut has_all = false;
    let mut has_version = false;

    for (i, arg) in expanded.iter().enumerate() {
        if i == 0 {
            continue;
        }
        let s = arg.to_string_lossy();
        match s.as_ref() {
            "--help" | "-h" | "-?" => has_help = true,
            "--all" => has_all = true,
            "--version" | "-V" => has_version = true,
            // -v means version only outside build mode; in build mode it means --build-verbose
            "-v" if !is_build_mode => has_version = true,
            _ => {}
        }
    }

    // --all takes precedence (with or without --help)
    if has_all {
        print!("{}", help::render_help_all(TSC_VERSION));
        std::process::exit(0);
    }

    // --help / -h / -?
    if has_help {
        print!("{}", help::render_help(TSC_VERSION));
        std::process::exit(0);
    }

    // --version / -v / -V
    if has_version {
        println!("Version {TSC_VERSION}");
        std::process::exit(0);
    }

    // Third pass: detect build mode and remap flags
    let is_build_mode = expanded
        .get(1)
        .map(|a| {
            let s = a.to_string_lossy();
            s == "--build" || s == "-b"
        })
        .unwrap_or(false);

    let mut result = Vec::with_capacity(expanded.len());
    for (i, arg) in expanded.iter().enumerate() {
        if i == 0 {
            result.push(arg.clone());
            continue;
        }

        let arg_str = arg.to_string_lossy();

        if is_build_mode && i > 1 {
            // In build mode, remap short flags:
            //   -v → --build-verbose (not --version)
            //   -d → --dry           (not --declaration)
            //   -f → --force
            match arg_str.as_ref() {
                "-v" => {
                    result.push(OsString::from("--build-verbose"));
                    continue;
                }
                "-d" => {
                    result.push(OsString::from("--dry"));
                    continue;
                }
                "-f" => {
                    result.push(OsString::from("--force"));
                    continue;
                }
                _ => {}
            }
        }

        result.push(arg.clone());
    }

    result
}

/// Split a response file line into arguments, respecting quoted strings.
///
/// Handles both double (`"`) and single (`'`) quotes. Quotes are stripped
/// from the resulting tokens. Unquoted regions are split on whitespace.
fn split_response_line(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;

    for ch in line.chars() {
        match in_quote {
            Some(q) if ch == q => {
                // Closing quote — end quoted region but don't push yet,
                // there may be more content adjacent (e.g. foo"bar"baz)
                in_quote = None;
            }
            Some(_) => {
                // Inside quotes — take character literally
                current.push(ch);
            }
            None if ch == '"' || ch == '\'' => {
                // Opening quote
                in_quote = Some(ch);
            }
            None if ch.is_ascii_whitespace() => {
                // Unquoted whitespace — flush current token
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            None => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

/// Check that --build/-b is the first argument (TS6369).
/// Returns an error message if --build/-b appears but is not first.
fn check_build_position(args: &[OsString]) -> Option<String> {
    // Skip program name (index 0)
    let mut found_build_pos: Option<usize> = None;
    let mut first_non_program = true;

    for (i, arg) in args.iter().enumerate().skip(1) {
        let s = arg.to_string_lossy();
        if s == "--build" || s == "-b" {
            if !first_non_program {
                found_build_pos = Some(i);
            }
            break;
        }
        first_non_program = false;
    }

    found_build_pos.map(|_| {
        "error TS6369: Option '--build' must be the first command line argument.\n".to_string()
    })
}

/// Handle a clap parsing error by reformatting it as a tsc-style diagnostic.
fn handle_clap_error(err: clap::Error, args: &[OsString]) -> Result<()> {
    use clap::error::ErrorKind;

    match err.kind() {
        ErrorKind::UnknownArgument => {
            // Extract the unknown flag from the error info
            if let Some(flag) = extract_unknown_flag_from_args(args) {
                // Try to find a close match for TS5025
                if let Some(suggestion) = find_closest_option(&flag) {
                    println!(
                        "error TS5025: Unknown compiler option '{flag}'. Did you mean '{suggestion}'?\n"
                    );
                } else {
                    println!("error TS5023: Unknown compiler option '{flag}'.\n");
                }
            } else {
                // Fallback: just print TS5023 with whatever info we have
                println!("error TS5023: Unknown compiler option.\n");
            }
            std::process::exit(1);
        }
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            // Help and version are handled in preprocess_args before clap,
            // but keep this arm for safety
            err.exit();
        }
        _ => {
            // For other clap errors (missing value, etc.), still use exit code 1
            // and tsc-style formatting where possible
            let msg = err.to_string();
            // Strip clap's formatting prefix
            let msg = msg
                .lines()
                .next()
                .unwrap_or(&msg)
                .trim_start_matches("error: ");
            println!("error TS5023: {msg}\n");
            std::process::exit(1);
        }
    }
}

/// Extract the first unknown flag from the preprocessed args by trying clap parsing.
/// Walks args looking for anything starting with `-` that is not a known flag.
fn extract_unknown_flag_from_args(args: &[OsString]) -> Option<String> {
    // Collect all known options from clap's command definition
    let cmd = CliArgs::command();
    let mut known: Vec<String> = Vec::new();
    for a in cmd.get_arguments() {
        if let Some(long) = a.get_long() {
            known.push(format!("--{long}"));
        }
        // get_visible_aliases returns individual aliases
        for alias in a.get_visible_aliases().unwrap_or_default() {
            known.push(format!("--{alias}"));
        }
        if let Some(short) = a.get_short() {
            known.push(format!("-{short}"));
        }
    }
    // Also add -V (clap's version flag after our -v remapping)
    known.push("-V".to_string());
    known.push("--version".to_string());
    known.push("--help".to_string());
    known.push("-h".to_string());

    // Walk preprocessed args (skip program name) to find the first unknown flag
    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with('-') && !s.starts_with("--") && s.len() == 2 {
            // Short flag like -x
            if !known.iter().any(|k| k == s.as_ref()) {
                return Some(s.into_owned());
            }
        } else if s.starts_with("--") {
            // Long flag like --badFlag (may have =value)
            let flag_part = s.split('=').next().unwrap_or(&s);
            if !known.iter().any(|k| k == flag_part) {
                return Some(flag_part.to_string());
            }
        }
    }
    None
}

/// All known tsc compiler option long names (for edit-distance matching).
/// These are the canonical --camelCase forms that tsc recognizes.
const KNOWN_TSC_OPTIONS: &[&str] = &[
    "--all",
    "--allowArbitraryExtensions",
    "--allowImportingTsExtensions",
    "--allowJs",
    "--allowSyntheticDefaultImports",
    "--allowUmdGlobalAccess",
    "--allowUnreachableCode",
    "--allowUnusedLabels",
    "--alwaysStrict",
    "--assumeChangesOnlyAffectDirectDependencies",
    "--baseUrl",
    "--build",
    "--charset",
    "--checkJs",
    "--clean",
    "--composite",
    "--customConditions",
    "--declaration",
    "--declarationDir",
    "--declarationMap",
    "--diagnostics",
    "--disableReferencedProjectLoad",
    "--disableSizeLimit",
    "--disableSolutionSearching",
    "--disableSourceOfProjectReferenceRedirect",
    "--downlevelIteration",
    "--dry",
    "--emitBOM",
    "--emitDeclarationOnly",
    "--emitDecoratorMetadata",
    "--erasableSyntaxOnly",
    "--esModuleInterop",
    "--exactOptionalPropertyTypes",
    "--excludeDirectories",
    "--excludeFiles",
    "--experimentalDecorators",
    "--explainFiles",
    "--extendedDiagnostics",
    "--fallbackPolling",
    "--force",
    "--forceConsistentCasingInFileNames",
    "--generateCpuProfile",
    "--generateTrace",
    "--help",
    "--ignoreConfig",
    "--importHelpers",
    "--importsNotUsedAsValues",
    "--incremental",
    "--init",
    "--inlineSourceMap",
    "--inlineSources",
    "--isolatedDeclarations",
    "--isolatedModules",
    "--jsx",
    "--jsxFactory",
    "--jsxFragmentFactory",
    "--jsxImportSource",
    "--keyofStringsOnly",
    "--lib",
    "--libReplacement",
    "--listEmittedFiles",
    "--listFiles",
    "--listFilesOnly",
    "--locale",
    "--mapRoot",
    "--maxNodeModuleJsDepth",
    "--module",
    "--moduleDetection",
    "--moduleResolution",
    "--moduleSuffixes",
    "--newLine",
    "--noCheck",
    "--noEmit",
    "--noEmitHelpers",
    "--noEmitOnError",
    "--noErrorTruncation",
    "--noFallthroughCasesInSwitch",
    "--noImplicitAny",
    "--noImplicitOverride",
    "--noImplicitReturns",
    "--noImplicitThis",
    "--noImplicitUseStrict",
    "--noLib",
    "--noPropertyAccessFromIndexSignature",
    "--noResolve",
    "--noStrictGenericChecks",
    "--noUncheckedIndexedAccess",
    "--noUncheckedSideEffectImports",
    "--noUnusedLocals",
    "--noUnusedParameters",
    "--out",
    "--outDir",
    "--outFile",
    "--paths",
    "--plugins",
    "--preserveConstEnums",
    "--preserveSymlinks",
    "--preserveValueImports",
    "--preserveWatchOutput",
    "--pretty",
    "--project",
    "--reactNamespace",
    "--removeComments",
    "--resolveJsonModule",
    "--resolvePackageJsonExports",
    "--resolvePackageJsonImports",
    "--rewriteRelativeImportExtensions",
    "--rootDir",
    "--rootDirs",
    "--showConfig",
    "--skipDefaultLibCheck",
    "--skipLibCheck",
    "--sourceMap",
    "--sourceRoot",
    "--stopBuildOnErrors",
    "--strict",
    "--strictBindCallApply",
    "--strictBuiltinIteratorReturn",
    "--strictFunctionTypes",
    "--strictNullChecks",
    "--strictPropertyInitialization",
    "--strict",
    "--stripInternal",
    "--suppressExcessPropertyErrors",
    "--suppressImplicitAnyIndexErrors",
    "--synchronousWatchDirectory",
    "--target",
    "--traceResolution",
    "--tsBuildInfoFile",
    "--typeRoots",
    "--types",
    "--useDefineForClassFields",
    "--useUnknownInCatchVariables",
    "--verbatimModuleSyntax",
    "--version",
    "--watch",
    "--watchDirectory",
    "--watchFile",
];

/// Compute Levenshtein edit distance between two strings (case-insensitive).
fn edit_distance(a: &str, b: &str) -> usize {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    let a_chars: Vec<char> = a_lower.chars().collect();
    let b_chars: Vec<char> = b_lower.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[m][n]
}

/// Find the closest known tsc option to the given unknown flag.
/// Returns `Some(suggestion)` if a reasonably close match exists (edit distance <= 3).
fn find_closest_option(unknown: &str) -> Option<&'static str> {
    let mut best: Option<(&str, usize)> = None;
    for &known in KNOWN_TSC_OPTIONS {
        let dist = edit_distance(unknown, known);
        if let Some((_, best_dist)) = best {
            if dist < best_dist {
                best = Some((known, dist));
            }
        } else {
            best = Some((known, dist));
        }
    }

    // Only suggest if the distance is small enough to be a plausible typo.
    // tsc uses a threshold roughly proportional to the option name length;
    // we use <=3 as a pragmatic cutoff.
    best.and_then(|(name, dist)| if dist <= 3 { Some(name) } else { None })
}

fn print_diagnostics(result: &driver::CompilationResult, elapsed: Duration, extended: bool) {
    let files_count = result.files_read.len();

    // Count lines by file category, matching tsc's --diagnostics output
    let mut lines_of_library: u64 = 0;
    let mut lines_of_definitions: u64 = 0;
    let mut lines_of_typescript: u64 = 0;
    let mut lines_of_javascript: u64 = 0;
    let mut lines_of_json: u64 = 0;
    let mut lines_of_other: u64 = 0;

    for path in &result.files_read {
        let count = std::fs::read_to_string(path)
            .ok()
            .map_or(0, |text| text.lines().count() as u64);
        let name = path.to_string_lossy();
        if name.contains("lib.") && name.ends_with(".d.ts") {
            lines_of_library += count;
        } else if name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts") {
            lines_of_definitions += count;
        } else if name.ends_with(".ts")
            || name.ends_with(".tsx")
            || name.ends_with(".mts")
            || name.ends_with(".cts")
        {
            lines_of_typescript += count;
        } else if name.ends_with(".js")
            || name.ends_with(".jsx")
            || name.ends_with(".mjs")
            || name.ends_with(".cjs")
        {
            lines_of_javascript += count;
        } else if name.ends_with(".json") {
            lines_of_json += count;
        } else {
            lines_of_other += count;
        }
    }

    let errors = result
        .diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::Error)
        .count();

    println!();
    println!("Files:                         {files_count}");
    println!("Lines of Library:              {lines_of_library}");
    println!("Lines of Definitions:          {lines_of_definitions}");
    println!("Lines of TypeScript:           {lines_of_typescript}");
    println!("Lines of JavaScript:           {lines_of_javascript}");
    println!("Lines of JSON:                 {lines_of_json}");
    println!("Lines of Other:                {lines_of_other}");
    println!("Errors:                        {errors}");
    println!(
        "Total time:                    {:.2}s",
        elapsed.as_secs_f64()
    );

    if extended {
        // Use process memory info if available
        let memory_used = get_memory_usage_kb();
        println!(
            "Emitted files:                 {}",
            result.emitted_files.len()
        );
        println!(
            "Total diagnostics:             {}",
            result.diagnostics.len()
        );
        if memory_used > 0 {
            println!("Memory used:                   {memory_used}K");
        }
    }
}

/// Get current process memory usage in KB (Linux only, returns 0 on other platforms).
fn get_memory_usage_kb() -> u64 {
    // Read from /proc/self/status for RSS on Linux
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        return parts[1].parse::<u64>().ok();
                    }
                }
            }
            None
        })
        .unwrap_or(0)
}

fn handle_init(_args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    let tsconfig_path = cwd.join("tsconfig.json");
    if tsconfig_path.exists() {
        println!(
            "error TS5054: A 'tsconfig.json' file is already defined at: '{}'.",
            tsconfig_path.display()
        );
        std::process::exit(0);
    }

    // Build the tsconfig.json content matching tsc 5.x --init output format
    // Uses JSONC (JSON with comments) which TypeScript supports
    let config = r#"{
  // Visit https://aka.ms/tsconfig to read more about this file
  "compilerOptions": {
    // File Layout
    // "rootDir": "./src",
    // "outDir": "./dist",

    // Environment Settings
    // See also https://aka.ms/tsconfig/module
    "module": "nodenext",
    "target": "esnext",
    "types": [],
    // For nodejs:
    // "lib": ["esnext"],
    // "types": ["node"],
    // and npm install -D @types/node

    // Other Outputs
    "sourceMap": true,
    "declaration": true,
    "declarationMap": true,

    // Stricter Typechecking Options
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,

    // Style Options
    // "noImplicitReturns": true,
    // "noImplicitOverride": true,
    // "noUnusedLocals": true,
    // "noUnusedParameters": true,
    // "noFallthroughCasesInSwitch": true,
    // "noPropertyAccessFromIndexSignature": true,

    // Recommended Options
    "strict": true,
    "jsx": "react-jsx",
    "verbatimModuleSyntax": true,
    "isolatedModules": true,
    "noUncheckedSideEffectImports": true,
    "moduleDetection": "force",
    "skipLibCheck": true,
  }
}
"#;

    std::fs::write(&tsconfig_path, config).with_context(|| {
        format!(
            "failed to write tsconfig.json to {}",
            tsconfig_path.display()
        )
    })?;

    println!("\nCreated a new tsconfig.json\n\nYou can learn more at https://aka.ms/tsconfig");

    Ok(())
}

fn handle_show_config(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    use tsz_cli::config::{load_tsconfig, resolve_compiler_options, strip_jsonc};
    use tsz_cli::fs::{FileDiscoveryOptions, discover_ts_files};

    // --ignoreConfig: skip tsconfig loading
    let tsconfig_path = if args.ignore_config {
        None
    } else {
        args.project
            .as_ref()
            .map(|p| {
                if p.is_dir() {
                    p.join("tsconfig.json")
                } else {
                    p.clone()
                }
            })
            .or_else(|| {
                let default_path = cwd.join("tsconfig.json");
                default_path.exists().then_some(default_path)
            })
    };

    let config = if let Some(path) = tsconfig_path.as_ref() {
        Some(load_tsconfig(path)?)
    } else {
        None
    };

    // Parse raw JSON from the tsconfig file to extract explicitly set compilerOptions
    // This avoids relying on struct field coverage -- we reflect back exactly what was set.
    let raw_compiler_options: serde_json::Map<String, serde_json::Value> =
        if let Some(ref path) = tsconfig_path {
            let source = std::fs::read_to_string(path).unwrap_or_default();
            let stripped = strip_jsonc(&source);
            // Remove trailing commas for valid JSON parsing
            let normalized = remove_trailing_commas_for_showconfig(&stripped);
            if let Ok(raw_val) = serde_json::from_str::<serde_json::Value>(&normalized) {
                raw_val
                    .get("compilerOptions")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default()
            } else {
                serde_json::Map::new()
            }
        } else {
            serde_json::Map::new()
        };

    let raw_opts = config
        .as_ref()
        .and_then(|cfg| cfg.compiler_options.as_ref());

    // We need resolved options for file discovery (allow_js, out_dir).
    // For --showConfig, lib file resolution failure is non-fatal.
    let resolved = resolve_compiler_options(raw_opts).ok();
    let allow_js = raw_opts.and_then(|o| o.allow_js).unwrap_or(false);
    let out_dir = resolved.as_ref().and_then(|r| r.out_dir.clone());

    // Discover resolved file list
    let base_dir = tsconfig_path
        .as_ref()
        .and_then(|p| p.parent())
        .unwrap_or(cwd);

    let explicit_files: Vec<std::path::PathBuf> = if !args.files.is_empty() {
        args.files.clone()
    } else if let Some(ref cfg) = config {
        cfg.files
            .as_ref()
            .map(|f| f.iter().map(std::path::PathBuf::from).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let files_explicitly_set =
        !args.files.is_empty() || config.as_ref().and_then(|c| c.files.as_ref()).is_some();
    let discovery = FileDiscoveryOptions {
        base_dir: base_dir.to_path_buf(),
        files: explicit_files,
        files_explicitly_set,
        include: config.as_ref().and_then(|c| c.include.clone()),
        exclude: config.as_ref().and_then(|c| c.exclude.clone()),
        out_dir,
        follow_links: false,
        allow_js,
    };

    let discovered_files = discover_ts_files(&discovery).unwrap_or_default();

    // If no input files found, emit TS18003 error and exit 1
    if discovered_files.is_empty() && config.is_some() {
        println!(
            "error TS18003: No inputs were found in config file '{}'. Specified 'include' paths were '{}' and 'exclude' paths were '{}'.",
            tsconfig_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            config
                .as_ref()
                .and_then(|c| c.include.as_ref())
                .map(|i| i.join("', '"))
                .unwrap_or_default(),
            config
                .as_ref()
                .and_then(|c| c.exclude.as_ref())
                .map(|e| e.join("', '"))
                .unwrap_or_default(),
        );
        std::process::exit(1);
    }

    // Build file paths relative to tsconfig dir with "./" prefix (matching tsc)
    let file_paths: Vec<String> = discovered_files
        .iter()
        .map(|f| {
            if let Ok(rel) = f.strip_prefix(base_dir) {
                format!("./{}", rel.display())
            } else {
                f.display().to_string()
            }
        })
        .collect();

    // Build the output manually with 4-space indentation (matching tsc --showConfig)
    let mut output = String::from("{\n");

    // compilerOptions - reflect raw values from tsconfig (only explicitly set options)
    output.push_str("    \"compilerOptions\": {");
    if raw_compiler_options.is_empty() {
        output.push('}');
    } else {
        output.push('\n');
        let entries: Vec<_> = raw_compiler_options.iter().collect();
        for (i, (key, value)) in entries.iter().enumerate() {
            output.push_str("        ");
            output.push_str(&format_json_value_with_indent(key, value, 8));
            if i + 1 < entries.len() {
                output.push(',');
            }
            output.push('\n');
        }
        output.push_str("    }");
    }

    // files
    if !file_paths.is_empty() {
        output.push_str(",\n    \"files\": [\n");
        for (i, f) in file_paths.iter().enumerate() {
            output.push_str(&format!("        \"{}\"", f));
            if i + 1 < file_paths.len() {
                output.push(',');
            }
            output.push('\n');
        }
        output.push_str("    ]");
    }

    // include
    if let Some(ref cfg) = config {
        if let Some(ref include) = cfg.include {
            output.push_str(",\n    \"include\": [\n");
            for (i, v) in include.iter().enumerate() {
                output.push_str(&format!("        \"{}\"", v));
                if i + 1 < include.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            output.push_str("    ]");
        }
        // exclude
        if let Some(ref exclude) = cfg.exclude {
            output.push_str(",\n    \"exclude\": [\n");
            for (i, v) in exclude.iter().enumerate() {
                output.push_str(&format!("        \"{}\"", v));
                if i + 1 < exclude.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            output.push_str("    ]");
        }
        // references
        if let Some(ref refs) = cfg.references {
            output.push_str(",\n    \"references\": [\n");
            for (i, r) in refs.iter().enumerate() {
                if r.prepend {
                    output.push_str(&format!(
                        "        {{\n            \"path\": \"{}\",\n            \"prepend\": true\n        }}",
                        r.path
                    ));
                } else {
                    output.push_str(&format!(
                        "        {{\n            \"path\": \"{}\"\n        }}",
                        r.path
                    ));
                }
                if i + 1 < refs.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            output.push_str("    ]");
        }
    }

    output.push_str("\n}\n");
    print!("{output}");

    Ok(())
}

/// Format a JSON key-value pair for --showConfig output with proper indentation.
fn format_json_value_with_indent(key: &str, value: &serde_json::Value, _indent: usize) -> String {
    let formatted_value = format_json_value(value);
    format!("\"{key}\": {formatted_value}")
}

/// Format a serde_json::Value for --showConfig output.
fn format_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", s),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                "[]".to_string()
            } else {
                let items: Vec<String> = arr.iter().map(format_json_value).collect();
                format!("[{}]", items.join(", "))
            }
        }
        serde_json::Value::Object(map) => {
            let entries: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", k, format_json_value(v)))
                .collect();
            format!("{{{}}}", entries.join(", "))
        }
        serde_json::Value::Null => "null".to_string(),
    }
}

/// Remove trailing commas from JSON strings (for showConfig raw JSON parsing).
fn remove_trailing_commas_for_showconfig(s: &str) -> String {
    // Simple approach: remove commas before } or ]
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    for i in 0..len {
        let ch = chars[i];
        if ch == ',' {
            // Look ahead to see if the next non-whitespace character is } or ]
            let mut j = i + 1;
            while j < len && chars[j].is_whitespace() {
                j += 1;
            }
            if j < len && (chars[j] == '}' || chars[j] == ']') {
                // Skip this comma
                continue;
            }
        }
        result.push(ch);
    }
    result
}

fn handle_list_files_only(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    use tsz_cli::config::{load_tsconfig, resolve_compiler_options};
    use tsz_cli::driver::apply_cli_overrides;
    use tsz_cli::fs::{FileDiscoveryOptions, discover_ts_files};

    // --ignoreConfig: skip tsconfig loading
    let tsconfig_path = if args.ignore_config {
        None
    } else {
        args.project
            .as_ref()
            .map(|p| {
                if p.is_dir() {
                    p.join("tsconfig.json")
                } else {
                    p.clone()
                }
            })
            .or_else(|| {
                let default_path = cwd.join("tsconfig.json");
                default_path.exists().then_some(default_path)
            })
    };

    let config = if let Some(path) = tsconfig_path.as_ref() {
        Some(load_tsconfig(path)?)
    } else {
        None
    };

    let mut resolved = resolve_compiler_options(
        config
            .as_ref()
            .and_then(|cfg| cfg.compiler_options.as_ref()),
    )?;
    apply_cli_overrides(&mut resolved, args)?;

    let base_dir = tsconfig_path
        .as_ref()
        .and_then(|p| p.parent())
        .unwrap_or(cwd);

    // Build file list from CLI args or config
    let files: Vec<std::path::PathBuf> = if !args.files.is_empty() {
        args.files.clone()
    } else if let Some(ref cfg) = config {
        cfg.files
            .as_ref()
            .map(|f| f.iter().map(std::path::PathBuf::from).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let files_explicitly_set =
        !args.files.is_empty() || config.as_ref().and_then(|c| c.files.as_ref()).is_some();
    let discovery = FileDiscoveryOptions {
        base_dir: base_dir.to_path_buf(),
        files,
        files_explicitly_set,
        include: config.as_ref().and_then(|c| c.include.clone()),
        exclude: config.as_ref().and_then(|c| c.exclude.clone()),
        out_dir: resolved.out_dir.clone(),
        follow_links: false,
        allow_js: resolved.allow_js,
    };

    let files = discover_ts_files(&discovery)?;
    for file in files {
        println!("{}", file.display());
    }

    Ok(())
}

fn handle_build(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    use tsz::checker::diagnostics::DiagnosticCategory;
    use tsz_cli::build;
    use tsz_cli::project_refs::ProjectReferenceGraph;

    let tsconfig_path = args
        .project
        .as_ref()
        .map(|p| {
            if p.is_dir() {
                p.join("tsconfig.json")
            } else {
                p.clone()
            }
        })
        .or_else(|| {
            let default_path = cwd.join("tsconfig.json");
            default_path.exists().then_some(default_path)
        });

    let Some(ref root_config_path) = tsconfig_path else {
        anyhow::bail!("No tsconfig.json found. Use --project to specify one.");
    };

    // Load project reference graph
    let graph = match ProjectReferenceGraph::load(root_config_path) {
        Ok(g) => g,
        Err(e) => {
            println!("Warning: Could not load project references: {e}");
            // Fall back to single project build
            return handle_build_single_project(args, cwd, root_config_path);
        }
    };

    // Validate project reference constraints (TS6306, TS6310, TS6202)
    let ref_diagnostics = graph.validate();
    if !ref_diagnostics.is_empty() {
        let _pretty = args
            .pretty
            .unwrap_or_else(|| std::io::stdout().is_terminal());
        for diag in &ref_diagnostics {
            println!("error TS{}: {}", diag.code, diag.message);
        }
        std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
    }

    // Handle --clean: delete build artifacts for all projects
    if args.clean {
        return handle_build_clean(&graph, args.build_verbose);
    }

    // Get build order (topologically sorted)
    let build_order: Vec<tsz_cli::project_refs::ProjectId> = match graph.build_order() {
        Ok(order) => order,
        Err(e) => {
            println!("Error: {e}");
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
        }
    };

    // Handle --dry: show what would be built without building
    if args.dry {
        println!(
            "Dry run - would build {} project(s) in order:",
            build_order.len()
        );
        for (i, project_id) in build_order.iter().enumerate() {
            if let Some(project) = graph.get_project(*project_id) {
                println!("  {}. {}", i + 1, project.config_path.display());
            }
        }
        return Ok(());
    }

    // Build each project in dependency order
    let mut total_errors = 0;
    let mut built_count = 0;
    let mut skipped_count = 0;
    let pretty = args
        .pretty
        .unwrap_or_else(|| std::io::stdout().is_terminal());
    let mut reporter = Reporter::new(pretty);

    if args.build_verbose {
        println!("Checking {} project(s)...", build_order.len());
    }

    for project_id in &build_order {
        let Some(project) = graph.get_project(*project_id) else {
            continue;
        };

        // Check if project is up-to-date (unless --force is set)
        if !args.force && build::is_project_up_to_date(project, args) {
            if args.build_verbose {
                println!("✓ Up to date: {}", project.config_path.display());
            }
            skipped_count += 1;
            continue;
        }

        if args.build_verbose {
            println!("\nBuilding: {}", project.config_path.display());
        }

        // Compile the project using the project-specific tsconfig
        let project_cwd = project.root_dir.clone();

        // Use driver::compile_project which accepts the tsconfig path directly
        let result = driver::compile_project(args, &project_cwd, &project.config_path)?;

        // Count errors
        let error_count = result
            .diagnostics
            .iter()
            .filter(|d| d.category == DiagnosticCategory::Error)
            .count();

        if error_count > 0 {
            total_errors += error_count;
            if !result.diagnostics.is_empty() {
                let output = reporter.render(&result.diagnostics);
                if !output.is_empty() {
                    print!("{output}");
                }
            }

            // Stop on first error if --stopBuildOnErrors is set
            if args.stop_build_on_errors {
                println!(
                    "\nBuild stopped due to errors in {}",
                    project.config_path.display()
                );
                std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
            }
        }

        built_count += 1;
    }

    if args.build_verbose {
        println!(
            "\nBuilt {built_count} project(s), skipped {skipped_count} up-to-date project(s), {total_errors} error(s)"
        );
    }

    if total_errors > 0 {
        std::process::exit(if built_count > 0 {
            EXIT_DIAGNOSTICS_OUTPUTS_GENERATED
        } else {
            EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED
        });
    }

    Ok(())
}

/// Handle --build --clean for all projects in the graph
fn handle_build_clean(
    graph: &tsz_cli::project_refs::ProjectReferenceGraph,
    verbose: bool,
) -> Result<()> {
    use std::fs;
    use tsz_cli::config::resolve_compiler_options;

    let mut deleted_count = 0;

    for project in graph.projects() {
        let base_dir = &project.root_dir;

        // Delete .tsbuildinfo file
        let buildinfo_path = project.config_path.with_extension("tsbuildinfo");
        if buildinfo_path.exists() {
            fs::remove_file(&buildinfo_path)?;
            if verbose {
                println!("Deleted: {}", buildinfo_path.display());
            }
            deleted_count += 1;
        }

        // Get resolved options to find output directories
        let resolved = resolve_compiler_options(project.config.base.compiler_options.as_ref())?;

        // Delete outDir
        if let Some(ref out_dir) = resolved.out_dir {
            let full_out_dir = base_dir.join(out_dir);
            if full_out_dir.exists() {
                fs::remove_dir_all(&full_out_dir)?;
                if verbose {
                    println!("Deleted: {}", full_out_dir.display());
                }
                deleted_count += 1;
            }
        }

        // Delete declarationDir
        if let Some(ref declaration_dir) = resolved.declaration_dir {
            let full_decl_dir = base_dir.join(declaration_dir);
            if full_decl_dir.exists() {
                fs::remove_dir_all(&full_decl_dir)?;
                if verbose {
                    println!("Deleted: {}", full_decl_dir.display());
                }
                deleted_count += 1;
            }
        }
    }

    println!(
        "Build cleaned successfully ({} project(s), {} item(s) deleted).",
        graph.project_count(),
        deleted_count
    );
    Ok(())
}

/// Fallback to single project build when no references are found
fn handle_build_single_project(
    args: &CliArgs,
    cwd: &std::path::Path,
    config_path: &std::path::Path,
) -> Result<()> {
    use tsz::checker::diagnostics::DiagnosticCategory;

    let result = driver::compile(args, cwd)?;

    if args.build_verbose {
        println!("Projects in this build: ");
        println!("  * {}", config_path.display());
    }

    if !result.diagnostics.is_empty() {
        let pretty = args
            .pretty
            .unwrap_or_else(|| std::io::stdout().is_terminal());
        let mut reporter = Reporter::new(pretty);
        let output = reporter.render(&result.diagnostics);
        if !output.is_empty() {
            print!("{output}");
        }
    }

    let has_errors = result
        .diagnostics
        .iter()
        .any(|d| d.category == DiagnosticCategory::Error);

    if has_errors {
        std::process::exit(if result.emitted_files.is_empty() {
            EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED
        } else {
            EXIT_DIAGNOSTICS_OUTPUTS_GENERATED
        });
    }

    Ok(())
}

#[cfg(test)]
#[path = "tsz/tests.rs"]
mod tests;
