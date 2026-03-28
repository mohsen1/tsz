#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
        print!("{msg}");
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
        println!("{}", help::colorize_help(&help::render_help(TSC_VERSION)));
        std::process::exit(1);
    }

    // TS5042: Option 'project' cannot be mixed with source files on a command line.
    if args.project.is_some() && !args.files.is_empty() {
        println!(
            "error TS5042: Option 'project' cannot be mixed with source files on a command line."
        );
        std::process::exit(1);
    }

    // TS5069: Option 'emitDeclarationOnly' cannot be specified without specifying option
    // 'declaration' or option 'composite'.
    if args.emit_declaration_only && !args.declaration && !args.composite {
        println!(
            "error TS5069: Option 'emitDeclarationOnly' cannot be specified without specifying option 'declaration' or option 'composite'."
        );
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
        tracing::warn!(
            "The --generateCpuProfile flag is a V8/Node.js feature and is not applicable to tsz (a native Rust compiler). The flag is accepted for compatibility but has no effect."
        );
    }

    let start_time = std::time::Instant::now();
    let result = match driver::compile(&args, &cwd) {
        Ok(r) => r,
        Err(e) => {
            let msg = e.to_string();
            // Intercept TS6053 file-not-found errors from discover_ts_files and
            // format them matching tsc v6 output.
            if let Some(rest) = msg.strip_prefix("TS6053: ") {
                println!("error TS6053: {rest}");
                println!("  The file is in the program because:");
                println!("    Root file specified for compilation\n");
                std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_GENERATED);
            }
            return Err(e);
        }
    };
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
        // When --pretty true is explicitly passed, force ANSI colors even
        // when piped (not a TTY), matching tsc v6 behavior.
        if args.pretty == Some(true) {
            Reporter::force_colors(true);
        }
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
        // Exit code 1 (DiagnosticsPresent_OutputsSkipped): emit was suppressed due to errors
        //   (--noEmitOnError with errors means no outputs were generated).
        // Exit code 2 (DiagnosticsPresent_OutputsGenerated): errors exist but outputs were
        //   still generated (or --noEmit where there's nothing to emit regardless).
        if args.no_emit_on_error {
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
        } else if args.no_emit || !result.emitted_files.is_empty() {
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

        // Clear the thread-local type interner cache between compilations.
        // The cache holds TypeId→TypeData and TypeData→TypeId mappings from the
        // previous compilation's TypeInterner. Without clearing, a new interner
        // reusing the same TypeId values would get stale TypeData from the old
        // interner, causing incorrect type resolution and panics.
        tsz_solver::clear_thread_local_cache();

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
                    reporter.set_cwd(project_path);
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
/// - Case-insensitive flag names: `--NoEmit` → `--noEmit` (tsc v6 compat)
/// - Boolean flag values: `--strict false` → strip the flag (tsc v6 compat)
/// - Duplicate flags: `--strict --strict` → deduplicated (tsc v6 compat)
fn preprocess_args(args: Vec<OsString>) -> Vec<OsString> {
    let flag_lookup = build_flag_lookup();

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

    // Pass 1.5: normalize --Flag names to canonical casing (case-insensitive, tsc compat).
    // Only the flag name portion is lowercased; values and file paths are preserved as-is.
    for arg in expanded.iter_mut().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with("--") && s.len() > 2 {
            if let Some(eq_pos) = s.find('=') {
                let flag_part = &s[2..eq_pos];
                let value_part = &s[eq_pos..];
                let lower = flag_part.to_lowercase();
                if let Some(canonical) = flag_lookup.get(lower.as_str()) {
                    *arg = OsString::from(format!("{canonical}{value_part}"));
                }
            } else {
                let flag_part = &s[2..];
                let lower = flag_part.to_lowercase();
                if let Some(canonical) = flag_lookup.get(lower.as_str()) {
                    *arg = OsString::from(canonical.to_string());
                }
            }
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
        println!(
            "{}",
            help::colorize_help(&help::render_help_all(TSC_VERSION))
        );
        std::process::exit(0);
    }

    // --help / -h / -?
    if has_help {
        println!("{}", help::colorize_help(&help::render_help(TSC_VERSION)));
        std::process::exit(0);
    }

    // --version / -v / -V
    if has_version {
        println!("Version {TSC_VERSION}");
        std::process::exit(0);
    }

    // Check for bare `--` and `-` which tsc treats as unknown options (TS5023).
    // These must be detected before clap sees them (clap treats `--` as
    // end-of-options and `-` as a positional arg).
    // Also check for --boolFlag=value which tsc treats as an unknown option.
    let boolean_flags = build_boolean_flag_set();
    for arg in expanded.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "--" || s == "-" {
            println!("error TS5023: Unknown compiler option '{s}'.");
            std::process::exit(1);
        }
        // tsc treats --boolFlag=value as an unknown option (the whole --flag=value string)
        if let Some(eq_pos) = s.find('=') {
            let flag_part = &s[..eq_pos];
            if boolean_flags.contains(flag_part) {
                println!("error TS5023: Unknown compiler option '{s}'.");
                std::process::exit(1);
            }
        }
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

    // Fourth pass: handle boolean flag values ("true"/"false") and deduplicate flags.
    // tsc treats `--strict false` as setting strict=false, and accepts duplicate flags silently.
    let boolean_flags = build_boolean_flag_set();
    let valued_flags = build_valued_flag_set();
    let option_bool_flags = build_option_bool_flag_set();
    let mut final_result = Vec::with_capacity(result.len());
    let mut flag_positions: FxHashMap<String, usize> = FxHashMap::default();
    let mut skip_positions: Vec<bool> = Vec::new();

    let mut i = 0;
    if !result.is_empty() {
        final_result.push(result[0].clone());
        skip_positions.push(false);
        i = 1;
    }

    while i < result.len() {
        let arg_str = result[i].to_string_lossy().to_string();

        if arg_str.starts_with("--") {
            let flag_name = if let Some(eq_pos) = arg_str.find('=') {
                arg_str[..eq_pos].to_string()
            } else {
                arg_str.clone()
            };

            let is_boolean = boolean_flags.contains(flag_name.as_str());
            let takes_value = valued_flags.contains(flag_name.as_str());

            // Check if next arg is "true" or "false" for boolean flags
            if is_boolean
                && !arg_str.contains('=')
                && let Some(next) = result.get(i + 1)
            {
                let next_str = next.to_string_lossy();
                let next_lower = next_str.to_lowercase();
                if next_lower == "false" {
                    if option_bool_flags.contains(flag_name.as_str()) {
                        // Option<bool> flag: emit --flag=false so clap gets the value
                        let combined = format!("{flag_name}=false");
                        if let Some(&prev_idx) = flag_positions.get(&flag_name) {
                            skip_positions[prev_idx] = true;
                        }
                        let current_idx = final_result.len();
                        flag_positions.insert(flag_name, current_idx);
                        final_result.push(OsString::from(combined));
                        skip_positions.push(false);
                        i += 2;
                        continue;
                    }
                    // Plain bool flag: skip both (flag is not set)
                    if let Some(&prev_idx) = flag_positions.get(&flag_name) {
                        skip_positions[prev_idx] = true;
                    }
                    flag_positions.remove(&flag_name);
                    i += 2;
                    continue;
                } else if next_lower == "true" {
                    if option_bool_flags.contains(flag_name.as_str()) {
                        // Option<bool> flag: emit --flag=true so clap gets the value
                        let combined = format!("{flag_name}=true");
                        if let Some(&prev_idx) = flag_positions.get(&flag_name) {
                            skip_positions[prev_idx] = true;
                        }
                        let current_idx = final_result.len();
                        flag_positions.insert(flag_name, current_idx);
                        final_result.push(OsString::from(combined));
                        skip_positions.push(false);
                        i += 2;
                        continue;
                    }
                    // Plain bool flag: keep the flag, skip the "true" token
                    i += 1;
                }
            }

            // Deduplicate: if we've seen this flag before, mark old position for skip
            if let Some(&prev_idx) = flag_positions.get(&flag_name) {
                skip_positions[prev_idx] = true;
                if takes_value
                    && !final_result[prev_idx].to_string_lossy().contains('=')
                    && prev_idx + 1 < skip_positions.len()
                {
                    skip_positions[prev_idx + 1] = true;
                }
            }

            let current_idx = final_result.len();
            flag_positions.insert(flag_name, current_idx);
            final_result.push(OsString::from(&arg_str));
            skip_positions.push(false);

            if takes_value && !arg_str.contains('=') {
                i += 1;
                if i < result.len() {
                    final_result.push(result[i].clone());
                    skip_positions.push(false);
                }
            }
        } else {
            final_result.push(result[i].clone());
            skip_positions.push(false);
        }

        i += 1;
    }

    final_result
        .into_iter()
        .zip(skip_positions)
        .filter_map(|(arg, skip)| if skip { None } else { Some(arg) })
        .collect()
}

/// Build a lookup table from lowercase flag names (without `--`) to their canonical
/// `--flagName` forms. Used for case-insensitive flag normalization (tsc v6 compat).
fn build_flag_lookup() -> FxHashMap<String, String> {
    let cmd = CliArgs::command();
    let mut map = FxHashMap::default();
    for a in cmd.get_arguments() {
        if let Some(long) = a.get_long() {
            let canonical = format!("--{long}");
            map.insert(long.to_lowercase(), canonical.clone());
            if let Some(aliases) = a.get_all_aliases() {
                for alias in aliases {
                    map.insert(alias.to_lowercase(), canonical.clone());
                }
            }
        }
    }
    for opt in KNOWN_TSC_OPTIONS {
        let name = &opt[2..];
        map.entry(name.to_lowercase())
            .or_insert_with(|| opt.to_string());
    }
    map
}

/// Set of known boolean flags (flags that accept no value or optional true/false).
fn build_boolean_flag_set() -> rustc_hash::FxHashSet<&'static str> {
    [
        "--all",
        "--build",
        "--init",
        "--listFilesOnly",
        "--showConfig",
        "--ignoreConfig",
        "--libReplacement",
        "--watch",
        "--noLib",
        "--useDefineForClassFields",
        "--experimentalDecorators",
        "--emitDecoratorMetadata",
        "--resolveJsonModule",
        "--resolvePackageJsonExports",
        "--resolvePackageJsonImports",
        "--allowArbitraryExtensions",
        "--allowImportingTsExtensions",
        "--rewriteRelativeImportExtensions",
        "--noResolve",
        "--allowUmdGlobalAccess",
        "--noUncheckedSideEffectImports",
        "--allowJs",
        "--checkJs",
        "--declaration",
        "--declarationMap",
        "--emitDeclarationOnly",
        "--sourceMap",
        "--inlineSourceMap",
        "--inlineSources",
        "--noEmit",
        "--noEmitOnError",
        "--noEmitHelpers",
        "--importHelpers",
        "--downlevelIteration",
        "--removeComments",
        "--preserveConstEnums",
        "--stripInternal",
        "--emitBOM",
        "--esModuleInterop",
        "--allowSyntheticDefaultImports",
        "--isolatedModules",
        "--isolatedDeclarations",
        "--verbatimModuleSyntax",
        "--forceConsistentCasingInFileNames",
        "--preserveSymlinks",
        "--erasableSyntaxOnly",
        "--strict",
        "--noImplicitAny",
        "--strictNullChecks",
        "--strictFunctionTypes",
        "--strictBindCallApply",
        "--strictPropertyInitialization",
        "--strictBuiltinIteratorReturn",
        "--noImplicitThis",
        "--useUnknownInCatchVariables",
        "--alwaysStrict",
        "--noUnusedLocals",
        "--noUnusedParameters",
        "--exactOptionalPropertyTypes",
        "--noImplicitReturns",
        "--noFallthroughCasesInSwitch",
        "--sound",
        "--noUncheckedIndexedAccess",
        "--noImplicitOverride",
        "--noPropertyAccessFromIndexSignature",
        "--allowUnreachableCode",
        "--allowUnusedLabels",
        "--skipDefaultLibCheck",
        "--skipLibCheck",
        "--composite",
        "--incremental",
        "--disableReferencedProjectLoad",
        "--disableSolutionSearching",
        "--disableSourceOfProjectReferenceRedirect",
        "--diagnostics",
        "--extendedDiagnostics",
        "--explainFiles",
        "--listFiles",
        "--listEmittedFiles",
        "--traceResolution",
        "--traceDependencies",
        "--noCheck",
        "--pretty",
        "--noErrorTruncation",
        "--preserveWatchOutput",
        "--synchronousWatchDirectory",
        "--build-verbose",
        "--dry",
        "--force",
        "--clean",
        "--stopBuildOnErrors",
        "--assumeChangesOnlyAffectDirectDependencies",
        "--keyofStringsOnly",
        "--noImplicitUseStrict",
        "--noStrictGenericChecks",
        "--preserveValueImports",
        "--suppressExcessPropertyErrors",
        "--suppressImplicitAnyIndexErrors",
        "--disableSizeLimit",
        "--batch",
    ]
    .into_iter()
    .collect()
}

/// Set of flags that take a mandatory value argument (not boolean flags).
fn build_valued_flag_set() -> rustc_hash::FxHashSet<&'static str> {
    [
        "--locale",
        "--project",
        "--target",
        "--module",
        "--lib",
        "--jsx",
        "--jsxFactory",
        "--jsxFragmentFactory",
        "--jsxImportSource",
        "--moduleDetection",
        "--moduleResolution",
        "--baseUrl",
        "--typeRoots",
        "--types",
        "--rootDirs",
        "--paths",
        "--plugins",
        "--moduleSuffixes",
        "--customConditions",
        "--maxNodeModuleJsDepth",
        "--declarationDir",
        "--outDir",
        "--rootDir",
        "--outFile",
        "--mapRoot",
        "--sourceRoot",
        "--newLine",
        "--tsBuildInfoFile",
        "--generateTrace",
        "--generateCpuProfile",
        "--watchFile",
        "--watchDirectory",
        "--fallbackPolling",
        "--excludeDirectories",
        "--excludeFiles",
        "--reactNamespace",
        "--charset",
        "--importsNotUsedAsValues",
        "--out",
        "--typesVersions",
    ]
    .into_iter()
    .collect()
}

/// Set of flags that are Option<bool> (tri-state: None, Some(true), Some(false)).
/// These need --flag=true or --flag=false rather than flag removal.
fn build_option_bool_flag_set() -> rustc_hash::FxHashSet<&'static str> {
    [
        "--useDefineForClassFields",
        "--resolvePackageJsonExports",
        "--resolvePackageJsonImports",
        "--allowSyntheticDefaultImports",
        "--forceConsistentCasingInFileNames",
        "--noImplicitAny",
        "--strictNullChecks",
        "--strictFunctionTypes",
        "--strictBindCallApply",
        "--strictPropertyInitialization",
        "--strictBuiltinIteratorReturn",
        "--noImplicitThis",
        "--useUnknownInCatchVariables",
        "--alwaysStrict",
        "--allowUnreachableCode",
        "--allowUnusedLabels",
        "--pretty",
    ]
    .into_iter()
    .collect()
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

/// Check that --build/-b is the first argument.
/// tsc v6 behavior:
///   - `--build` (long form) not first → TS6369 ("must be first")
///   - `-b` (short form) not first → TS5023 ("unknown compiler option")
///
/// Returns an error message if either form appears but is not first.
fn check_build_position(args: &[OsString]) -> Option<String> {
    // Skip program name (index 0)
    let mut first_non_program = true;

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "--build" {
            if !first_non_program {
                return Some(
                    "error TS6369: Option '--build' must be the first command line argument.\n"
                        .to_string(),
                );
            }
            return None;
        }
        if s == "-b" {
            if !first_non_program {
                return Some("error TS5023: Unknown compiler option '-b'.\n".to_string());
            }
            return None;
        }
        first_non_program = false;
    }

    None
}

/// Handle a clap parsing error by reformatting it as a tsc-style diagnostic.
fn handle_clap_error(err: clap::Error, args: &[OsString]) -> Result<()> {
    use clap::error::ErrorKind;

    match err.kind() {
        ErrorKind::UnknownArgument => {
            // Extract ALL unknown flags from the args and report each one
            let unknown_flags = extract_all_unknown_flags(args);
            if unknown_flags.is_empty() {
                // Fallback: just print TS5023 with whatever info we have
                println!("error TS5023: Unknown compiler option.");
            } else {
                for flag in &unknown_flags {
                    // Try to find a close match for TS5025
                    if let Some(suggestion) = find_closest_option(flag) {
                        let suggestion_name = suggestion.strip_prefix("--").unwrap_or(suggestion);
                        println!(
                            "error TS5025: Unknown compiler option '{flag}'. Did you mean '{suggestion_name}'?"
                        );
                    } else {
                        println!("error TS5023: Unknown compiler option '{flag}'.");
                    }
                }
            }
            std::process::exit(1);
        }
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            // Help and version are handled in preprocess_args before clap,
            // but keep this arm for safety
            err.exit();
        }
        ErrorKind::MissingRequiredArgument => {
            // TS6044: Compiler option 'X' expects an argument.
            // Extract the option name from clap's error message
            let msg = err.to_string();
            if let Some(option_name) = extract_option_from_missing_value(&msg) {
                println!("error TS6044: Compiler option '{option_name}' expects an argument.");
                // Also emit TS6046 with valid values if this is an enum option
                if let Some(valid_values) = get_valid_values_for_option(&option_name) {
                    println!(
                        "error TS6046: Argument for '--{option_name}' option must be: {valid_values}."
                    );
                }
            } else {
                let msg = msg
                    .lines()
                    .next()
                    .unwrap_or(&msg)
                    .trim_start_matches("error: ");
                println!("error TS5023: {msg}");
            }
            std::process::exit(1);
        }
        ErrorKind::InvalidValue => {
            let msg = err.to_string();
            // Detect the "missing value" case: clap says "a value is required for"
            let is_missing_value = msg.contains("a value is required for");
            if let Some(option_name) = extract_option_from_invalid_value(&msg) {
                // TS6044: emit when the option was given without any value
                if is_missing_value {
                    println!("error TS6044: Compiler option '{option_name}' expects an argument.");
                }
                // TS6046: list valid values for enum options
                if let Some(valid_values) = get_valid_values_for_option(&option_name) {
                    println!(
                        "error TS6046: Argument for '--{option_name}' option must be: {valid_values}."
                    );
                } else if !is_missing_value {
                    let msg = msg
                        .lines()
                        .next()
                        .unwrap_or(&msg)
                        .trim_start_matches("error: ");
                    println!("error TS5023: {msg}");
                }
            } else {
                let msg = msg
                    .lines()
                    .next()
                    .unwrap_or(&msg)
                    .trim_start_matches("error: ");
                println!("error TS5023: {msg}");
            }
            std::process::exit(1);
        }
        _ => {
            // For other clap errors, still use exit code 1
            // and tsc-style formatting where possible
            let msg = err.to_string();
            // Strip clap's formatting prefix
            let msg = msg
                .lines()
                .next()
                .unwrap_or(&msg)
                .trim_start_matches("error: ");
            println!("error TS5023: {msg}");
            std::process::exit(1);
        }
    }
}

/// Extract ALL unknown flags from the preprocessed args.
/// Scans all args against known options and returns every unrecognized flag.
fn extract_all_unknown_flags(args: &[OsString]) -> Vec<String> {
    let known = collect_known_flags();
    let mut unknown = Vec::new();

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "-" || s == "--" {
            // Bare `-` and `--` are treated as unknown options to match tsc
            unknown.push(s.into_owned());
        } else if s.starts_with('-') && !s.starts_with("--") && s.len() == 2 {
            // Short flag like -x
            if !known.iter().any(|k| k == s.as_ref()) {
                unknown.push(s.into_owned());
            }
        } else if s.starts_with("--") {
            // Long flag like --badFlag (may have =value)
            let flag_part = s.split('=').next().unwrap_or(&s);
            if !known.iter().any(|k| k == flag_part) {
                unknown.push(flag_part.to_string());
            }
        }
    }
    unknown
}

/// Collect all known CLI flags from clap's command definition.
fn collect_known_flags() -> Vec<String> {
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
    known
}

/// Extract the option name from a clap "missing required argument" error.
/// Clap formats these as: "a value is required for '--target <TARGET>' but none was supplied"
fn extract_option_from_missing_value(msg: &str) -> Option<String> {
    // Look for pattern: '--optionName' or '--optionName <VALUE>'
    let start = msg.find("'--")?;
    let after = &msg[start + 3..];
    let end = after.find(['\'', ' ', '<'])?;
    Some(after[..end].to_string())
}

/// Extract the option name from a clap "invalid value" error.
/// Clap formats these as: "invalid value 'blah' for '--target <TARGET>'"
fn extract_option_from_invalid_value(msg: &str) -> Option<String> {
    let start = msg.find("'--")?;
    let after = &msg[start + 3..];
    let end = after.find(['\'', ' ', '<'])?;
    Some(after[..end].to_string())
}

/// Get the valid values string for enum-typed CLI options, matching tsc's TS6046 format.
fn get_valid_values_for_option(option_name: &str) -> Option<&'static str> {
    // Value ordering and inclusion matches tsc v6 baselines exactly.
    // Deprecated values (es3, es5, none, amd, system, umd, node10, node, classic) are excluded.
    match option_name {
        "target" => Some(
            "'es6', 'es2015', 'es2016', 'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', 'es2023', 'es2024', 'es2025', 'esnext'",
        ),
        "module" => Some(
            "'commonjs', 'es6', 'es2015', 'es2020', 'es2022', 'esnext', 'node16', 'node18', 'node20', 'nodenext', 'preserve'",
        ),
        "jsx" => Some("'preserve', 'react-native', 'react-jsx', 'react-jsxdev', 'react'"),
        "moduleResolution" | "module-resolution" | "moduleresolution" => {
            Some("'node16', 'nodenext', 'bundler'")
        }
        "moduleDetection" | "module-detection" | "moduledetection" => {
            Some("'auto', 'legacy', 'force'")
        }
        _ => None,
    }
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
    for (i, row) in dp.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    for (j, val) in dp[0].iter_mut().enumerate().take(n + 1) {
        *val = j;
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
    // tsc uses a threshold proportional to the option name length.
    // We use max(unknown_len, candidate_len) * 0.4 as the cutoff, with a minimum of 1.
    best.and_then(|(name, dist)| {
        let max_len = unknown.len().max(name.len());
        let threshold = (max_len * 2 / 5).max(1); // ~40% of the longer name
        if dist <= threshold { Some(name) } else { None }
    })
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

    // Phase timing breakdown (shown for both --diagnostics and --extendedDiagnostics)
    let pt = &result.phase_timings;
    if pt.total_ms > 0.0 {
        println!(
            "I/O Read:                      {:.2}s",
            pt.io_read_ms / 1000.0
        );
        println!(
            "Parse & Bind:                  {:.2}s",
            (pt.load_libs_ms + pt.parse_bind_ms) / 1000.0
        );
        println!(
            "Check:                         {:.2}s",
            pt.check_ms / 1000.0
        );
        println!("Emit:                          {:.2}s", pt.emit_ms / 1000.0);
    }
    println!(
        "Total time:                    {:.2}s",
        elapsed.as_secs_f64()
    );

    if extended {
        // Use process memory info if available
        let memory_used = get_memory_usage_kb();
        let counters = result.request_cache_counters;
        let request_lookups = counters.request_cache_hits + counters.request_cache_misses;
        let request_hit_rate = if request_lookups == 0 {
            0.0
        } else {
            counters.request_cache_hits as f64 * 100.0 / request_lookups as f64
        };
        let access_hit_rate = if counters.property_access_request_cache_lookups == 0 {
            0.0
        } else {
            counters.property_access_request_cache_hits as f64 * 100.0
                / counters.property_access_request_cache_lookups as f64
        };
        println!(
            "Emitted files:                 {}",
            result.emitted_files.len()
        );
        println!(
            "Total diagnostics:             {}",
            result.diagnostics.len()
        );
        println!(
            "Request cache hits:            {}",
            counters.request_cache_hits
        );
        println!(
            "Request cache misses:          {}",
            counters.request_cache_misses
        );
        println!("Request cache hit rate:        {request_hit_rate:.1}%");
        println!(
            "Contextual cache bypasses:     {}",
            counters.contextual_cache_bypasses
        );
        println!(
            "clear_type_cache_recursive:    {}",
            counters.clear_type_cache_recursive_calls
        );
        println!(
            "Access request-cache hit rate: {:.1}% ({}/{})",
            access_hit_rate,
            counters.property_access_request_cache_hits,
            counters.property_access_request_cache_lookups
        );
        // Type interner statistics
        if result.interned_types_count > 0 {
            println!(
                "Interned types:                {}",
                result.interned_types_count
            );
        }

        // Solver query-cache statistics
        if let Some(ref qc) = result.query_cache_stats {
            let sub_total = qc.relation.subtype_hits + qc.relation.subtype_misses;
            let sub_rate = if sub_total == 0 {
                0.0
            } else {
                qc.relation.subtype_hits as f64 * 100.0 / sub_total as f64
            };
            let assign_total = qc.relation.assignability_hits + qc.relation.assignability_misses;
            let assign_rate = if assign_total == 0 {
                0.0
            } else {
                qc.relation.assignability_hits as f64 * 100.0 / assign_total as f64
            };
            println!(
                "Subtype cache:                 {} entries ({} hits, {} misses, {sub_rate:.1}%)",
                qc.relation.subtype_entries, qc.relation.subtype_hits, qc.relation.subtype_misses,
            );
            println!(
                "Assignability cache:           {} entries ({} hits, {} misses, {assign_rate:.1}%)",
                qc.relation.assignability_entries,
                qc.relation.assignability_hits,
                qc.relation.assignability_misses,
            );
            println!("Eval cache:                    {}", qc.eval_cache_entries);
            println!(
                "Property cache:                {}",
                qc.property_cache_entries
            );
            println!(
                "Variance cache:                {}",
                qc.variance_cache_entries
            );
        }

        // Definition-store statistics
        if let Some(ref ds) = result.def_store_stats {
            println!(
                "Definitions:                   {} total ({} aliases, {} interfaces, {} classes, {} enums)",
                ds.total_definitions, ds.type_aliases, ds.interfaces, ds.classes, ds.enums,
            );
            println!(
                "Def indices:                   type_to_def={}, symbol_def={}, body_to_alias={}, shape_to_def={}",
                ds.type_to_def_entries,
                ds.symbol_def_index_entries,
                ds.body_to_alias_entries,
                ds.shape_to_def_entries,
            );
        }

        // Solver/interner memory breakdown
        {
            let interner_kb = result.interner_estimated_bytes as f64 / 1024.0;
            let qc_kb = result
                .query_cache_stats
                .as_ref()
                .map_or(0.0, |qc| qc.estimated_size_bytes() as f64 / 1024.0);
            let ds_kb = result
                .def_store_stats
                .as_ref()
                .map_or(0.0, |ds| ds.estimated_size_bytes as f64 / 1024.0);
            let total_kb = interner_kb + qc_kb + ds_kb;
            if total_kb > 0.0 {
                println!(
                    "Type interner memory:          {interner_kb:.1}K ({} types)",
                    result.interned_types_count,
                );
                println!("Query cache memory:            {qc_kb:.1}K");
                println!("Definition store memory:       {ds_kb:.1}K");
                println!("Solver total memory:           {total_kb:.1}K");
            }
        }

        // AST / program residency statistics
        if let Some(ref rs) = result.residency_stats {
            let arena_kb = rs.unique_arena_estimated_bytes as f64 / 1024.0;
            let bound_kb = rs.total_bound_file_bytes as f64 / 1024.0;
            let pre_merge_kb = rs.pre_merge_bind_total_bytes as f64 / 1024.0;
            println!(
                "AST arenas:                    {} unique ({arena_kb:.1}K)",
                rs.unique_arena_count,
            );
            println!(
                "Bound files:                   {} ({bound_kb:.1}K)",
                rs.file_count,
            );
            if rs.pre_merge_bind_total_bytes > 0 {
                println!("Pre-merge bind data:           {pre_merge_kb:.1}K");
            }
            if rs.has_skeleton_index {
                let skel_kb = rs.skeleton_estimated_size_bytes as f64 / 1024.0;
                println!(
                    "Skeleton index:                {} symbols, {} merge candidates ({skel_kb:.1}K)",
                    rs.skeleton_total_symbol_count, rs.skeleton_merge_candidate_count,
                );
            }
        }

        // Module dependency graph statistics
        if let Some(ref md) = result.module_dep_stats {
            println!("Module files:                  {}", md.file_count,);
            println!("Dependency edges:              {}", md.dependency_edges,);
            println!("Import cycles:                 {}", md.import_cycles,);
            if md.largest_cycle_size > 0 {
                println!(
                    "Largest cycle:                 {} files",
                    md.largest_cycle_size,
                );
            }
        }

        if memory_used > 0 {
            println!("Memory used:                   {memory_used}K");
        }
    }
}

/// Get peak memory usage (max RSS) in KB.
///
/// Supported platforms:
/// - **Unix (Linux + macOS)**: calls `getrusage(RUSAGE_SELF)` to read `ru_maxrss`.
///   On macOS `ru_maxrss` is in bytes, on Linux it is in KB.
/// - **Other**: returns 0.
///
/// This reports *peak* RSS (matching tsc's `--extendedDiagnostics` behavior).
#[cfg(unix)]
#[allow(unsafe_code)]
fn get_memory_usage_kb() -> u64 {
    // Minimal repr(C) struct matching the POSIX rusage layout.
    // We only need fields through ru_maxrss; remaining fields are padding.
    #[repr(C)]
    struct Rusage {
        ru_utime: [i64; 2], // struct timeval (tv_sec + tv_usec), 16 bytes on 64-bit
        ru_stime: [i64; 2], // struct timeval
        ru_maxrss: i64,     // max resident set size
        _pad: [i64; 13],    // remaining fields (ixrss through nivcsw)
    }

    const RUSAGE_SELF: i32 = 0;

    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }

    unsafe {
        let mut usage: Rusage = std::mem::zeroed();
        if getrusage(RUSAGE_SELF, &mut usage) == 0 {
            let maxrss = usage.ru_maxrss as u64;
            // macOS reports bytes; Linux reports KB.
            #[cfg(target_os = "macos")]
            {
                maxrss / 1024
            }
            #[cfg(not(target_os = "macos"))]
            {
                maxrss
            }
        } else {
            0
        }
    }
}

#[cfg(not(unix))]
fn get_memory_usage_kb() -> u64 {
    0
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
    use tsz_cli::config::{load_tsconfig, resolve_compiler_options};
    use tsz_cli::fs::{FileDiscoveryOptions, discover_ts_files};

    // --ignoreConfig: skip tsconfig loading
    let tsconfig_path = if args.ignore_config {
        None
    } else {
        args.project
            .as_ref()
            .map(|p| {
                // Canonicalize relative paths by joining with cwd first,
                // so that p.parent() later returns a valid directory.
                let resolved = if p.is_relative() {
                    cwd.join(p)
                } else {
                    p.clone()
                };
                if resolved.is_dir() {
                    resolved.join("tsconfig.json")
                } else {
                    resolved
                }
            })
            .or_else(|| Some(cwd.join("tsconfig.json")))
    };

    // When no tsconfig.json is found (and --ignoreConfig is not set), emit the
    // appropriate tsc error code depending on how the path was resolved:
    //   TS5081 – no --project flag, default cwd/tsconfig.json missing
    //   TS5057 – --project dir exists but has no tsconfig.json
    //   TS5058 – --project path does not exist at all
    if let Some(ref path) = tsconfig_path
        && !path.exists()
    {
        if let Some(project_val) = args.project.as_ref() {
            if project_val.is_dir() {
                println!(
                    "error TS5057: Cannot find a tsconfig.json file at the specified directory: '{}'.",
                    project_val.display()
                );
            } else {
                println!(
                    "error TS5058: The specified path does not exist: '{}'.",
                    project_val.display()
                );
            }
        } else {
            println!(
                "error TS5081: Cannot find a tsconfig.json file at the current directory: {}.",
                cwd.display()
            );
        }
        std::process::exit(1);
    }

    // Issue 2: load_tsconfig already resolves extends chains, so the returned
    // config is the fully merged result.
    let config = if let Some(path) = tsconfig_path.as_ref() {
        Some(load_tsconfig(path)?)
    } else {
        None
    };

    // Build the compiler options map from the MERGED config (extends-resolved).
    let mut compiler_options_map: serde_json::Map<String, serde_json::Value> =
        if let Some(ref cfg) = config {
            show_config_compiler_options_to_json(cfg.compiler_options.as_ref())
        } else {
            serde_json::Map::new()
        };

    // Issue 1: Merge CLI-provided flags into the compiler options map.
    show_config_apply_cli_overrides(&mut compiler_options_map, args);

    // Issue 3: Compute and add implied options.
    show_config_add_implied_options(&mut compiler_options_map);

    // Issue 5: Normalize outDir/outFile/rootDir paths with "./" prefix
    for path_key in &["outDir", "outFile", "rootDir", "declarationDir", "baseUrl"] {
        if let Some(serde_json::Value::String(s)) = compiler_options_map.get(*path_key) {
            let normalized = if s == "." {
                // "." → "./" (not "./.")
                "./".to_string()
            } else if !s.starts_with("./") && !s.starts_with("../") && !s.starts_with('/') {
                format!("./{s}")
            } else {
                continue;
            };
            compiler_options_map.insert(
                (*path_key).to_string(),
                serde_json::Value::String(normalized),
            );
        }
    }

    let raw_opts = config
        .as_ref()
        .and_then(|cfg| cfg.compiler_options.as_ref());

    // We need resolved options for file discovery (allow_js, out_dir).
    let resolved = resolve_compiler_options(raw_opts).ok();
    let allow_js = raw_opts.and_then(|o| o.allow_js).unwrap_or(false) || args.allow_js;
    let out_dir = args
        .out_dir
        .clone()
        .or_else(|| resolved.as_ref().and_then(|r| r.out_dir.clone()));

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
        out_dir: out_dir.clone(),
        follow_links: false,
        allow_js,
        resolve_json_module: resolved.as_ref().is_some_and(|r| r.resolve_json_module),
    };

    let discovered_files = discover_ts_files(&discovery).unwrap_or_default();

    // If no input files found, emit TS18003 error and exit 1
    if discovered_files.is_empty() && config.is_some() {
        // tsc formats include/exclude as JSON arrays. When `include` is omitted,
        // it reports the implicit discovery set rather than `[]`.
        let include_json = config
            .as_ref()
            .and_then(|c| c.include.as_ref())
            .map(|items| {
                let inner: Vec<String> = items.iter().map(|s| format!("\"{s}\"")).collect();
                format!("[{}]", inner.join(","))
            })
            .unwrap_or_else(|| {
                let inner: Vec<String> = tsz_cli::fs::default_include_display()
                    .into_iter()
                    .map(|s| format!("\"{s}\""))
                    .collect();
                format!("[{}]", inner.join(","))
            });
        let exclude_json = config
            .as_ref()
            .and_then(|c| c.exclude.as_ref())
            .map(|items| {
                let inner: Vec<String> = items.iter().map(|s| format!("\"{s}\"")).collect();
                format!("[{}]", inner.join(","))
            })
            .unwrap_or_else(|| "[]".to_string());
        println!(
            "error TS18003: No inputs were found in config file '{}'. Specified 'include' paths were '{}' and 'exclude' paths were '{}'.",
            tsconfig_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            include_json,
            exclude_json,
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

    // Issue 6: Auto-add outDir to exclude array (tsc behavior)
    let effective_exclude = {
        let mut excl = config
            .as_ref()
            .and_then(|c| c.exclude.clone())
            .unwrap_or_default();
        if let Some(ref od) = out_dir {
            let od_str = od.display().to_string();
            let normalized_od = if !od_str.starts_with("./")
                && !od_str.starts_with("../")
                && !od_str.starts_with('/')
            {
                format!("./{od_str}")
            } else {
                od_str
            };
            if !excl.iter().any(|e| e == &normalized_od) {
                excl.push(normalized_od);
            }
        }
        excl
    };

    // Build the output manually with 4-space indentation (matching tsc --showConfig)
    let mut output = String::from("{\n");

    // compilerOptions
    output.push_str("    \"compilerOptions\": {");
    if compiler_options_map.is_empty() {
        output.push('}');
    } else {
        output.push('\n');
        let entries: Vec<_> = compiler_options_map.iter().collect();
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

    // tsc v6 output order: compilerOptions, references, files, include, exclude

    // references (before files, matching tsc v6 ordering)
    if let Some(ref cfg) = config
        && let Some(ref refs) = cfg.references
    {
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

    // files
    if !file_paths.is_empty() {
        output.push_str(",\n    \"files\": [\n");
        for (i, f) in file_paths.iter().enumerate() {
            output.push_str(&format!("        \"{f}\""));
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
                output.push_str(&format!("        \"{v}\""));
                if i + 1 < include.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            output.push_str("    ]");
        }
        // exclude (with auto-added outDir)
        if !effective_exclude.is_empty() {
            output.push_str(",\n    \"exclude\": [\n");
            for (i, v) in effective_exclude.iter().enumerate() {
                output.push_str(&format!("        \"{v}\""));
                if i + 1 < effective_exclude.len() {
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

/// Convert merged `CompilerOptions` (from extends-resolved `TsConfig`) into a JSON map
/// for --showConfig output. Only includes options that are explicitly set (non-None).
fn show_config_compiler_options_to_json(
    opts: Option<&tsz_cli::config::CompilerOptions>,
) -> serde_json::Map<String, serde_json::Value> {
    use serde_json::Value;
    let mut map = serde_json::Map::new();
    let Some(opts) = opts else { return map };

    if let Some(ref v) = opts.target {
        // tsc uses the first key in the type map: ES2015 → "es6"
        let lowered = v.to_lowercase();
        let normalized = if lowered == "es2015" {
            "es6".to_string()
        } else {
            lowered
        };
        map.insert("target".into(), Value::String(normalized));
    }
    if let Some(ref v) = opts.module {
        // tsc uses the first key in the type map: ES2015 → "es6"
        let lowered = v.to_lowercase();
        let normalized = if lowered == "es2015" {
            "es6".to_string()
        } else {
            lowered
        };
        map.insert("module".into(), Value::String(normalized));
    }
    if let Some(ref v) = opts.module_resolution {
        map.insert("moduleResolution".into(), Value::String(v.to_lowercase()));
    }
    if let Some(ref v) = opts.jsx {
        map.insert("jsx".into(), Value::String(v.to_lowercase()));
    }
    if let Some(ref v) = opts.jsx_factory {
        map.insert("jsxFactory".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = opts.jsx_fragment_factory {
        map.insert("jsxFragmentFactory".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = opts.jsx_import_source {
        map.insert("jsxImportSource".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = opts.react_namespace {
        map.insert("reactNamespace".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = opts.base_url {
        map.insert("baseUrl".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = opts.root_dir {
        map.insert("rootDir".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = opts.out_dir {
        map.insert("outDir".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = opts.out_file {
        map.insert("outFile".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = opts.declaration_dir {
        map.insert("declarationDir".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = opts.ts_build_info_file {
        map.insert("tsBuildInfoFile".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = opts.module_detection {
        map.insert("moduleDetection".into(), Value::String(v.to_lowercase()));
    }

    macro_rules! set_bool {
        ($f:ident, $k:expr) => {
            if let Some(v) = opts.$f {
                map.insert($k.into(), Value::Bool(v));
            }
        };
    }
    set_bool!(strict, "strict");
    set_bool!(no_emit, "noEmit");
    set_bool!(no_emit_on_error, "noEmitOnError");
    set_bool!(declaration, "declaration");
    set_bool!(source_map, "sourceMap");
    set_bool!(declaration_map, "declarationMap");
    set_bool!(composite, "composite");
    set_bool!(incremental, "incremental");
    set_bool!(isolated_modules, "isolatedModules");
    set_bool!(verbatim_module_syntax, "verbatimModuleSyntax");
    set_bool!(es_module_interop, "esModuleInterop");
    set_bool!(
        allow_synthetic_default_imports,
        "allowSyntheticDefaultImports"
    );
    set_bool!(allow_js, "allowJs");
    set_bool!(check_js, "checkJs");
    set_bool!(skip_lib_check, "skipLibCheck");
    set_bool!(strip_internal, "stripInternal");
    set_bool!(no_lib, "noLib");
    set_bool!(import_helpers, "importHelpers");
    set_bool!(no_implicit_any, "noImplicitAny");
    set_bool!(no_implicit_returns, "noImplicitReturns");
    set_bool!(strict_null_checks, "strictNullChecks");
    set_bool!(strict_function_types, "strictFunctionTypes");
    set_bool!(
        strict_property_initialization,
        "strictPropertyInitialization"
    );
    set_bool!(no_implicit_this, "noImplicitThis");
    set_bool!(use_unknown_in_catch_variables, "useUnknownInCatchVariables");
    set_bool!(strict_bind_call_apply, "strictBindCallApply");
    set_bool!(no_unchecked_indexed_access, "noUncheckedIndexedAccess");
    set_bool!(no_unused_locals, "noUnusedLocals");
    set_bool!(no_unused_parameters, "noUnusedParameters");
    set_bool!(allow_unreachable_code, "allowUnreachableCode");
    set_bool!(no_resolve, "noResolve");
    set_bool!(
        no_unchecked_side_effect_imports,
        "noUncheckedSideEffectImports"
    );
    set_bool!(no_implicit_override, "noImplicitOverride");
    set_bool!(always_strict, "alwaysStrict");
    set_bool!(preserve_symlinks, "preserveSymlinks");
    set_bool!(use_define_for_class_fields, "useDefineForClassFields");
    set_bool!(experimental_decorators, "experimentalDecorators");
    set_bool!(emit_decorator_metadata, "emitDecoratorMetadata");
    set_bool!(resolve_package_json_exports, "resolvePackageJsonExports");
    set_bool!(resolve_package_json_imports, "resolvePackageJsonImports");
    set_bool!(resolve_json_module, "resolveJsonModule");
    set_bool!(allow_arbitrary_extensions, "allowArbitraryExtensions");
    set_bool!(allow_importing_ts_extensions, "allowImportingTsExtensions");
    set_bool!(
        rewrite_relative_import_extensions,
        "rewriteRelativeImportExtensions"
    );

    if let Some(ref v) = opts.lib {
        map.insert(
            "lib".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(ref v) = opts.types {
        map.insert(
            "types".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(ref v) = opts.type_roots {
        map.insert(
            "typeRoots".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(ref v) = opts.module_suffixes {
        map.insert(
            "moduleSuffixes".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(ref v) = opts.custom_conditions {
        map.insert(
            "customConditions".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(ref paths) = opts.paths {
        let mut paths_obj = serde_json::Map::new();
        for (pattern, targets) in paths {
            paths_obj.insert(
                pattern.clone(),
                Value::Array(targets.iter().map(|s| Value::String(s.clone())).collect()),
            );
        }
        map.insert("paths".into(), Value::Object(paths_obj));
    }
    map
}

/// Merge CLI-provided flags into the compiler options JSON map for --showConfig output.
fn show_config_apply_cli_overrides(
    map: &mut serde_json::Map<String, serde_json::Value>,
    args: &CliArgs,
) {
    use serde_json::Value;
    if let Some(target) = args.target {
        let s = match target {
            tsz_cli::args::Target::Es5 => "es5",
            tsz_cli::args::Target::Es2015 => "es6",
            tsz_cli::args::Target::Es2016 => "es2016",
            tsz_cli::args::Target::Es2017 => "es2017",
            tsz_cli::args::Target::Es2018 => "es2018",
            tsz_cli::args::Target::Es2019 => "es2019",
            tsz_cli::args::Target::Es2020 => "es2020",
            tsz_cli::args::Target::Es2021 => "es2021",
            tsz_cli::args::Target::Es2022 => "es2022",
            tsz_cli::args::Target::Es2023 => "es2023",
            tsz_cli::args::Target::Es2024 => "es2024",
            tsz_cli::args::Target::Es2025 => "es2025",
            tsz_cli::args::Target::EsNext => "esnext",
        };
        map.insert("target".into(), Value::String(s.into()));
    }
    if let Some(module) = args.module {
        let s = match module {
            tsz_cli::args::Module::None => "none",
            tsz_cli::args::Module::CommonJs => "commonjs",
            tsz_cli::args::Module::Amd => "amd",
            tsz_cli::args::Module::Umd => "umd",
            tsz_cli::args::Module::System => "system",
            tsz_cli::args::Module::Es2015 => "es6",
            tsz_cli::args::Module::Es2020 => "es2020",
            tsz_cli::args::Module::Es2022 => "es2022",
            tsz_cli::args::Module::EsNext => "esnext",
            tsz_cli::args::Module::Node16 => "node16",
            tsz_cli::args::Module::Node18 => "node18",
            tsz_cli::args::Module::Node20 => "node20",
            tsz_cli::args::Module::NodeNext => "nodenext",
            tsz_cli::args::Module::Preserve => "preserve",
        };
        map.insert("module".into(), Value::String(s.into()));
    }
    if let Some(mr) = args.module_resolution {
        let s = match mr {
            tsz_cli::args::ModuleResolution::Classic => "classic",
            tsz_cli::args::ModuleResolution::Node10 => "node10",
            tsz_cli::args::ModuleResolution::Node16 => "node16",
            tsz_cli::args::ModuleResolution::NodeNext => "nodenext",
            tsz_cli::args::ModuleResolution::Bundler => "bundler",
        };
        map.insert("moduleResolution".into(), Value::String(s.into()));
    }
    if let Some(jsx) = args.jsx {
        let s = match jsx {
            tsz_cli::args::JsxEmit::Preserve => "preserve",
            tsz_cli::args::JsxEmit::React => "react",
            tsz_cli::args::JsxEmit::ReactJsx => "react-jsx",
            tsz_cli::args::JsxEmit::ReactJsxDev => "react-jsxdev",
            tsz_cli::args::JsxEmit::ReactNative => "react-native",
        };
        map.insert("jsx".into(), Value::String(s.into()));
    }
    if let Some(ref v) = args.jsx_factory {
        map.insert("jsxFactory".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = args.jsx_fragment_factory {
        map.insert("jsxFragmentFactory".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = args.jsx_import_source {
        map.insert("jsxImportSource".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = args.out_dir {
        map.insert("outDir".into(), Value::String(v.display().to_string()));
    }
    if let Some(ref v) = args.out_file {
        map.insert("outFile".into(), Value::String(v.display().to_string()));
    }
    if let Some(ref v) = args.root_dir {
        map.insert("rootDir".into(), Value::String(v.display().to_string()));
    }
    if let Some(ref v) = args.declaration_dir {
        map.insert(
            "declarationDir".into(),
            Value::String(v.display().to_string()),
        );
    }
    if let Some(ref v) = args.base_url {
        map.insert("baseUrl".into(), Value::String(v.display().to_string()));
    }

    macro_rules! set_if_true {
        ($f:ident, $k:expr) => {
            if args.$f {
                map.insert($k.into(), Value::Bool(true));
            }
        };
    }
    set_if_true!(strict, "strict");
    set_if_true!(no_emit, "noEmit");
    set_if_true!(no_emit_on_error, "noEmitOnError");
    set_if_true!(declaration, "declaration");
    set_if_true!(source_map, "sourceMap");
    set_if_true!(declaration_map, "declarationMap");
    set_if_true!(composite, "composite");
    set_if_true!(incremental, "incremental");
    set_if_true!(isolated_modules, "isolatedModules");
    set_if_true!(verbatim_module_syntax, "verbatimModuleSyntax");
    set_if_true!(es_module_interop, "esModuleInterop");
    set_if_true!(allow_js, "allowJs");
    set_if_true!(check_js, "checkJs");
    set_if_true!(skip_lib_check, "skipLibCheck");
    set_if_true!(skip_default_lib_check, "skipDefaultLibCheck");
    set_if_true!(strip_internal, "stripInternal");
    set_if_true!(no_lib, "noLib");
    set_if_true!(import_helpers, "importHelpers");
    set_if_true!(no_emit_helpers, "noEmitHelpers");
    set_if_true!(no_unused_locals, "noUnusedLocals");
    set_if_true!(no_unused_parameters, "noUnusedParameters");
    set_if_true!(no_implicit_returns, "noImplicitReturns");
    set_if_true!(no_fallthrough_cases_in_switch, "noFallthroughCasesInSwitch");
    set_if_true!(exact_optional_property_types, "exactOptionalPropertyTypes");
    set_if_true!(no_unchecked_indexed_access, "noUncheckedIndexedAccess");
    set_if_true!(no_implicit_override, "noImplicitOverride");
    set_if_true!(
        no_property_access_from_index_signature,
        "noPropertyAccessFromIndexSignature"
    );
    set_if_true!(no_resolve, "noResolve");
    set_if_true!(
        no_unchecked_side_effect_imports,
        "noUncheckedSideEffectImports"
    );
    set_if_true!(allow_umd_global_access, "allowUmdGlobalAccess");
    set_if_true!(downlevel_iteration, "downlevelIteration");
    set_if_true!(experimental_decorators, "experimentalDecorators");
    set_if_true!(emit_decorator_metadata, "emitDecoratorMetadata");
    set_if_true!(preserve_const_enums, "preserveConstEnums");
    set_if_true!(remove_comments, "removeComments");
    set_if_true!(emit_bom, "emitBOM");
    set_if_true!(inline_source_map, "inlineSourceMap");
    set_if_true!(inline_sources, "inlineSources");
    set_if_true!(resolve_json_module, "resolveJsonModule");
    set_if_true!(allow_arbitrary_extensions, "allowArbitraryExtensions");
    set_if_true!(allow_importing_ts_extensions, "allowImportingTsExtensions");
    set_if_true!(
        rewrite_relative_import_extensions,
        "rewriteRelativeImportExtensions"
    );
    set_if_true!(preserve_symlinks, "preserveSymlinks");
    set_if_true!(isolated_declarations, "isolatedDeclarations");
    set_if_true!(erasable_syntax_only, "erasableSyntaxOnly");

    macro_rules! set_opt_bool {
        ($f:ident, $k:expr) => {
            if let Some(v) = args.$f {
                map.insert($k.into(), Value::Bool(v));
            }
        };
    }
    set_opt_bool!(no_implicit_any, "noImplicitAny");
    set_opt_bool!(strict_null_checks, "strictNullChecks");
    set_opt_bool!(strict_function_types, "strictFunctionTypes");
    set_opt_bool!(strict_bind_call_apply, "strictBindCallApply");
    set_opt_bool!(
        strict_property_initialization,
        "strictPropertyInitialization"
    );
    set_opt_bool!(
        strict_builtin_iterator_return,
        "strictBuiltinIteratorReturn"
    );
    set_opt_bool!(no_implicit_this, "noImplicitThis");
    set_opt_bool!(use_unknown_in_catch_variables, "useUnknownInCatchVariables");
    set_opt_bool!(always_strict, "alwaysStrict");
    set_opt_bool!(use_define_for_class_fields, "useDefineForClassFields");
    set_opt_bool!(allow_unreachable_code, "allowUnreachableCode");
    set_opt_bool!(allow_unused_labels, "allowUnusedLabels");
    set_opt_bool!(
        allow_synthetic_default_imports,
        "allowSyntheticDefaultImports"
    );
    set_opt_bool!(
        force_consistent_casing_in_file_names,
        "forceConsistentCasingInFileNames"
    );
    set_opt_bool!(pretty, "pretty");
    set_opt_bool!(resolve_package_json_exports, "resolvePackageJsonExports");
    set_opt_bool!(resolve_package_json_imports, "resolvePackageJsonImports");

    if let Some(ref v) = args.lib {
        map.insert(
            "lib".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(ref v) = args.types {
        map.insert(
            "types".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(ref v) = args.type_roots {
        map.insert(
            "typeRoots".into(),
            Value::Array(
                v.iter()
                    .map(|s| Value::String(s.display().to_string()))
                    .collect(),
            ),
        );
    }
    if let Some(ref v) = args.root_dirs {
        map.insert(
            "rootDirs".into(),
            Value::Array(
                v.iter()
                    .map(|s| Value::String(s.display().to_string()))
                    .collect(),
            ),
        );
    }
    if let Some(ref v) = args.module_suffixes {
        map.insert(
            "moduleSuffixes".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(ref v) = args.custom_conditions {
        map.insert(
            "customConditions".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if let Some(md) = args.module_detection {
        let s = match md {
            tsz_cli::args::ModuleDetection::Auto => "auto",
            tsz_cli::args::ModuleDetection::Force => "force",
            tsz_cli::args::ModuleDetection::Legacy => "legacy",
        };
        map.insert("moduleDetection".into(), Value::String(s.into()));
    }
    if let Some(nl) = args.new_line {
        let s = match nl {
            tsz_cli::args::NewLine::Crlf => "crlf",
            tsz_cli::args::NewLine::Lf => "lf",
        };
        map.insert("newLine".into(), Value::String(s.into()));
    }
    if let Some(ref v) = args.map_root {
        map.insert("mapRoot".into(), Value::String(v.clone()));
    }
    if let Some(ref v) = args.source_root {
        map.insert("sourceRoot".into(), Value::String(v.clone()));
    }
    if let Some(v) = args.max_node_module_js_depth {
        map.insert(
            "maxNodeModuleJsDepth".into(),
            Value::Number(serde_json::Number::from(v)),
        );
    }
}

/// Add implied options that tsc v6 shows in --showConfig output.
///
/// Algorithm from tsc v6 `convertToTSConfig` (commandLineParser.ts:2686-2697):
/// 1. Get the set of explicitly provided options (providedKeys)
/// 2. For each computed option:
///    a. If NOT in providedKeys AND transitively depends on any provided key
///    b. Compute its value using the user's config
///    c. Compute its value using empty config (defaults)
///    d. Only show it if computed != default
fn show_config_add_implied_options(map: &mut serde_json::Map<String, serde_json::Value>) {
    use serde_json::Value;

    // Collect the set of explicitly provided option keys (owned to avoid borrow conflicts)
    let provided: std::collections::HashSet<String> = map.keys().cloned().collect();

    // --- Helper: parse target string to numeric level ---
    fn parse_target(s: &str) -> u8 {
        match s.to_lowercase().as_str() {
            "es3" => 0,
            "es5" => 1,
            "es6" | "es2015" => 2,
            "es2016" => 3,
            "es2017" => 4,
            "es2018" => 5,
            "es2019" => 6,
            "es2020" => 7,
            "es2021" => 8,
            "es2022" => 9,
            "es2023" => 10,
            "es2024" => 11,
            "esnext" => 99,
            _ => 12, // default ES2025 (also covers "es2025" explicitly)
        }
    }

    // --- Helper: compute module from target ---
    const fn compute_module(target: u8) -> &'static str {
        if target == 99 {
            "esnext"
        } else if target >= 9 {
            // >= ES2022
            "es2022"
        } else if target >= 7 {
            // >= ES2020
            "es2020"
        } else if target >= 2 {
            // >= ES2015
            "es6"
        } else {
            "commonjs"
        }
    }

    // --- Helper: compute moduleResolution from module string ---
    fn compute_module_resolution(module_str: &str) -> &'static str {
        match module_str.to_lowercase().as_str() {
            "none" | "amd" | "umd" | "system" => "classic",
            "nodenext" => "nodenext",
            "node16" | "node18" | "node20" => "node16",
            _ => "bundler",
        }
    }

    // --- Helper: compute moduleDetection from module string ---
    fn compute_module_detection(module_str: &str) -> &'static str {
        match module_str.to_lowercase().as_str() {
            "node16" | "node18" | "node20" | "nodenext" => "force",
            _ => "auto",
        }
    }

    // v6 defaults (empty config):
    // target=es2025(12), module=es2022, moduleResolution=bundler
    // esModuleInterop=true, allowSyntheticDefaultImports=true
    // useDefineForClassFields=true (es2025 >= ES2022)
    // strict sub-flags: false, declaration=false, incremental=false
    const DEFAULT_TARGET: u8 = 12; // ES2025
    const DEFAULT_MODULE_RESOLUTION: &str = "bundler";
    const DEFAULT_MODULE_DETECTION: &str = "auto";

    // --- Compute effective values using user's config ---
    let user_target_str = map
        .get("target")
        .and_then(|v| v.as_str())
        .unwrap_or("es2025");
    let user_target = parse_target(user_target_str);

    let user_strict = map.get("strict").and_then(|v| v.as_bool()).unwrap_or(false);
    let user_composite = map
        .get("composite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let user_verbatim = map
        .get("verbatimModuleSyntax")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let user_check_js = map
        .get("checkJs")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let user_isolated_modules = map
        .get("isolatedModules")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let user_rewrite_relative = map
        .get("rewriteRelativeImportExtensions")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // --- Helper: check if a computed option depends on any provided key ---
    let depends_on_provided =
        |deps: &[&str]| -> bool { deps.iter().any(|d| provided.contains(*d)) };

    // --- target: deps=[], computed = target ?? ES2025 ---
    // target has no deps, so it's only shown if explicitly set (already in map)

    // --- module: deps=["target"] ---
    if !provided.contains("module") && depends_on_provided(&["target"]) {
        let computed = compute_module(user_target);
        let default = compute_module(DEFAULT_TARGET); // es2022
        if computed != default {
            map.insert("module".into(), Value::String(computed.into()));
        }
    }

    // Re-compute effective module after possible insertion (clone to avoid borrow)
    let eff_module: String = map
        .get("module")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| compute_module(user_target).to_string());

    // --- moduleResolution: deps=["module","target"] ---
    if !provided.contains("moduleResolution") && depends_on_provided(&["module", "target"]) {
        let computed = compute_module_resolution(&eff_module);
        if computed != DEFAULT_MODULE_RESOLUTION {
            map.insert("moduleResolution".into(), Value::String(computed.into()));
        }
    }

    // --- moduleDetection: deps=["module","target"] ---
    if !provided.contains("moduleDetection") && depends_on_provided(&["module", "target"]) {
        let computed = compute_module_detection(&eff_module);
        if computed != DEFAULT_MODULE_DETECTION {
            map.insert("moduleDetection".into(), Value::String(computed.into()));
        }
    }

    // --- useDefineForClassFields: deps=["target","module"] ---
    if !provided.contains("useDefineForClassFields") && depends_on_provided(&["target", "module"]) {
        let computed = user_target >= 9; // >= ES2022
        let default_val = DEFAULT_TARGET >= 9; // true
        if computed != default_val {
            map.insert("useDefineForClassFields".into(), Value::Bool(computed));
        }
    }

    // --- esModuleInterop: deps=[] ---
    // No deps, so only shown if explicitly set (already in map)

    // --- allowSyntheticDefaultImports: deps=[] ---
    // No deps, so only shown if explicitly set (already in map)

    // --- declaration: deps=["composite"] ---
    if !provided.contains("declaration") && depends_on_provided(&["composite"]) {
        let user_decl = map
            .get("declaration")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let computed = user_decl || user_composite;
        let default_val = false; // default declaration = false, default composite = false
        if computed != default_val {
            map.insert("declaration".into(), Value::Bool(computed));
        }
    }

    // --- incremental: deps=["composite"] ---
    if !provided.contains("incremental") && depends_on_provided(&["composite"]) {
        let user_incr = map
            .get("incremental")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let computed = user_incr || user_composite;
        let default_val = false;
        if computed != default_val {
            map.insert("incremental".into(), Value::Bool(computed));
        }
    }

    // --- strict sub-flags: deps=["strict"] ---
    let strict_sub_flags = [
        "noImplicitAny",
        "noImplicitThis",
        "strictNullChecks",
        "strictFunctionTypes",
        "strictBindCallApply",
        "strictPropertyInitialization",
        "strictBuiltinIteratorReturn",
        "alwaysStrict",
        "useUnknownInCatchVariables",
    ];
    for flag_name in &strict_sub_flags {
        if !provided.contains(*flag_name) && depends_on_provided(&["strict"]) {
            // computed = strict ?? false; default = false (no strict in empty config)
            let computed = user_strict;
            let default_val = false;
            if computed != default_val {
                map.insert((*flag_name).to_string(), Value::Bool(computed));
            }
        }
    }

    // --- isolatedModules: deps=["verbatimModuleSyntax"] ---
    if !provided.contains("isolatedModules") && depends_on_provided(&["verbatimModuleSyntax"]) {
        let computed = user_isolated_modules || user_verbatim;
        let default_val = false;
        if computed != default_val {
            map.insert("isolatedModules".into(), Value::Bool(computed));
        }
    }

    // --- allowJs: deps=["checkJs"] ---
    if !provided.contains("allowJs") && depends_on_provided(&["checkJs"]) {
        let user_allow_js = map
            .get("allowJs")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // allowJs ?? checkJs
        let computed = if provided.contains("allowJs") {
            user_allow_js
        } else {
            user_check_js
        };
        let default_val = false;
        if computed != default_val {
            map.insert("allowJs".into(), Value::Bool(computed));
        }
    }

    // --- preserveConstEnums: deps=["isolatedModules","verbatimModuleSyntax"] ---
    if !provided.contains("preserveConstEnums")
        && depends_on_provided(&["isolatedModules", "verbatimModuleSyntax"])
    {
        let user_preserve = map
            .get("preserveConstEnums")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let computed = user_preserve || user_isolated_modules || user_verbatim;
        let default_val = false;
        if computed != default_val {
            map.insert("preserveConstEnums".into(), Value::Bool(computed));
        }
    }

    // --- declarationMap: deps=["declaration","composite"] ---
    if !provided.contains("declarationMap") && depends_on_provided(&["declaration", "composite"]) {
        let user_decl_map = map
            .get("declarationMap")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let eff_declaration = map
            .get("declaration")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let computed = user_decl_map && eff_declaration;
        let default_val = false;
        if computed != default_val {
            map.insert("declarationMap".into(), Value::Bool(computed));
        }
    }

    // --- allowImportingTsExtensions: deps=["rewriteRelativeImportExtensions"] ---
    if !provided.contains("allowImportingTsExtensions")
        && depends_on_provided(&["rewriteRelativeImportExtensions"])
    {
        let user_allow = map
            .get("allowImportingTsExtensions")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let computed = user_allow || user_rewrite_relative;
        let default_val = false;
        if computed != default_val {
            map.insert("allowImportingTsExtensions".into(), Value::Bool(computed));
        }
    }

    // --- resolveJsonModule: deps=["moduleResolution","module","target"] ---
    // Complex logic; for now skip unless we need it. tsc computes this based on
    // whether moduleResolution >= Node16. The default (bundler) resolves to true.
    // Only show if computed != default.
    if !provided.contains("resolveJsonModule")
        && depends_on_provided(&["moduleResolution", "module", "target"])
    {
        let user_resolve_json = map.get("resolveJsonModule").and_then(|v| v.as_bool());
        // Compute: resolveJsonModule defaults to true if moduleResolution is node16/nodenext/bundler
        let eff_mr = map
            .get("moduleResolution")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_MODULE_RESOLUTION);
        let mr_implies_json = matches!(
            eff_mr.to_lowercase().as_str(),
            "node16" | "nodenext" | "bundler"
        );
        let computed = user_resolve_json.unwrap_or(mr_implies_json);
        // default: bundler implies true
        let default_mr_implies = matches!(
            DEFAULT_MODULE_RESOLUTION.to_lowercase().as_str(),
            "node16" | "nodenext" | "bundler"
        );
        let default_val = default_mr_implies; // true
        if computed != default_val {
            map.insert("resolveJsonModule".into(), Value::Bool(computed));
        }
    }

    // --- resolvePackageJsonExports: deps=["moduleResolution","module","target"] ---
    if !provided.contains("resolvePackageJsonExports")
        && depends_on_provided(&["moduleResolution", "module", "target"])
    {
        let user_val = map
            .get("resolvePackageJsonExports")
            .and_then(|v| v.as_bool());
        let eff_mr = map
            .get("moduleResolution")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_MODULE_RESOLUTION);
        let mr_implies = matches!(
            eff_mr.to_lowercase().as_str(),
            "node16" | "nodenext" | "bundler"
        );
        let computed = user_val.unwrap_or(mr_implies);
        let default_val = true; // bundler implies true
        if computed != default_val {
            map.insert("resolvePackageJsonExports".into(), Value::Bool(computed));
        }
    }

    // --- resolvePackageJsonImports: deps=["moduleResolution","resolvePackageJsonExports","module","target"] ---
    if !provided.contains("resolvePackageJsonImports")
        && depends_on_provided(&[
            "moduleResolution",
            "resolvePackageJsonExports",
            "module",
            "target",
        ])
    {
        let user_val = map
            .get("resolvePackageJsonImports")
            .and_then(|v| v.as_bool());
        let eff_mr = map
            .get("moduleResolution")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_MODULE_RESOLUTION);
        let mr_implies = matches!(
            eff_mr.to_lowercase().as_str(),
            "node16" | "nodenext" | "bundler"
        );
        let computed = user_val.unwrap_or(mr_implies);
        let default_val = true; // bundler implies true
        if computed != default_val {
            map.insert("resolvePackageJsonImports".into(), Value::Bool(computed));
        }
    }
}

/// Format a JSON key-value pair for --showConfig output with proper indentation.
/// `indent` is the current indentation level (number of spaces for the key line).
fn format_json_value_with_indent(key: &str, value: &serde_json::Value, indent: usize) -> String {
    let formatted_value = format_json_value(value, indent);
    format!("\"{key}\": {formatted_value}")
}

/// Format a `serde_json::Value` for --showConfig output.
/// `indent` is the indentation level of the line containing this value.
/// Arrays and objects are formatted multi-line with items at indent+4 and
/// closing bracket at indent.
fn format_json_value(value: &serde_json::Value, indent: usize) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{s}\""),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                "[]".to_string()
            } else {
                let item_indent = indent + 4;
                let item_pad = " ".repeat(item_indent);
                let close_pad = " ".repeat(indent);
                let mut result = String::from("[\n");
                for (i, item) in arr.iter().enumerate() {
                    result.push_str(&item_pad);
                    result.push_str(&format_json_value(item, item_indent));
                    if i + 1 < arr.len() {
                        result.push(',');
                    }
                    result.push('\n');
                }
                result.push_str(&close_pad);
                result.push(']');
                result
            }
        }
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                "{}".to_string()
            } else {
                let item_indent = indent + 4;
                let item_pad = " ".repeat(item_indent);
                let close_pad = " ".repeat(indent);
                let mut result = String::from("{\n");
                let entries: Vec<_> = map.iter().collect();
                for (i, (k, v)) in entries.iter().enumerate() {
                    result.push_str(&item_pad);
                    result.push_str(&format!("\"{}\": {}", k, format_json_value(v, item_indent)));
                    if i + 1 < entries.len() {
                        result.push(',');
                    }
                    result.push('\n');
                }
                result.push_str(&close_pad);
                result.push('}');
                result
            }
        }
        serde_json::Value::Null => "null".to_string(),
    }
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
        resolve_json_module: resolved.resolve_json_module,
    };

    // Print lib files first (matching tsc --listFilesOnly order)
    if !resolved.checker.no_lib {
        for lib_file in &resolved.lib_files {
            println!("{}", lib_file.display());
        }
    }

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
        .unwrap_or_else(|| cwd.join("tsconfig.json"));

    if !tsconfig_path.exists() {
        // Match tsc behavior: TS5083 to stdout, exit code 1
        let display_path = if tsconfig_path.is_absolute() {
            tsconfig_path
        } else {
            cwd.join(&tsconfig_path)
        };
        println!(
            "error TS5083: Cannot read file '{}'.",
            display_path.display()
        );
        std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
    }

    let root_config_path = &tsconfig_path;

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
    if args.pretty == Some(true) {
        Reporter::force_colors(true);
    }
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
        if args.pretty == Some(true) {
            Reporter::force_colors(true);
        }
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
