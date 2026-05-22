#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use rustc_hash::FxHashMap;
use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tsz::checker::diagnostics::DiagnosticCategory;
use tsz_cli::args::CliArgs;
use tsz_cli::help::{self, TSC_VERSION};
use tsz_cli::{driver, locale, reporter::Reporter, watch};
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

/// tsc exit status codes (matching TypeScript's `ExitStatus` enum)
const EXIT_SUCCESS: i32 = 0;
const EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED: i32 = 1;
const EXIT_DIAGNOSTICS_OUTPUTS_GENERATED: i32 = 2;
const TS5112_COMMAND_LINE_FILES_MESSAGE: &str = "tsconfig.json is present but will not be loaded if files are specified on commandline. Use '--ignoreConfig' to skip this error.";

/// Extensions tsc lists in TS6231 "could not resolve path" messages, in tsc's display order.
const TS6231_EXTENSIONS: &str = "'.ts', '.tsx', '.d.ts', '.cts', '.d.cts', '.mts', '.d.mts'";

/// Prints a root-file resolution failure in tsc's format and exits with the diagnostics
/// status code. Keeps the "file is in the program because" context consistent across all
/// root-file error codes.
fn report_root_file_diagnostic(code: u32, message: &str) -> ! {
    println!("error TS{code}: {message}");
    println!("  The file is in the program because:");
    println!("    Root file specified for compilation\n");
    std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_GENERATED)
}

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
    let use_large_stack_thread = should_use_large_stack_thread(&args);

    // Run on a larger stack for project-sized and multi-file workflows.
    // Single-file CLI probes avoid this extra thread hop for lower startup overhead.
    if use_large_stack_thread {
        std::thread::Builder::new()
            .stack_size(tsz_common::limits::THREAD_STACK_SIZE_BYTES)
            .spawn(move || actual_main(args, cwd))
            .expect("failed to spawn main thread")
            .join()
            .expect("main thread panicked")
    } else {
        actual_main(args, cwd)
    }
}

fn actual_main(mut args: CliArgs, cwd: std::path::PathBuf) -> Result<()> {
    if let Some(locale_id) = args.locale.as_deref()
        && !locale::is_valid_locale_shape(locale_id)
    {
        let message =
            diagnostic_messages::LOCALE_MUST_BE_OF_THE_FORM_LANGUAGE_OR_LANGUAGE_TERRITORY_FOR_EXAMPLE_OR
                .replace("{0}", "en")
                .replace("{1}", "ja-jp");
        println!(
            "error TS{}: {message}",
            diagnostic_codes::LOCALE_MUST_BE_OF_THE_FORM_LANGUAGE_OR_LANGUAGE_TERRITORY_FOR_EXAMPLE_OR
        );
        std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
    }

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

    reject_tsconfig_only_cli_options(&args);
    reject_build_only_cli_options(&args);

    // Handle --showConfig: print resolved configuration
    if args.show_config {
        return handle_show_config(&args, &cwd);
    }

    if should_report_ts5112_for_command_line_files(&args, &cwd) {
        println!("error TS5112: {TS5112_COMMAND_LINE_FILES_MESSAGE}");
        std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
    }

    // `--listFilesOnly` still uses the normal no-input command-line behavior before
    // the file-list-only path can print default libs. Use walk-up discovery to
    // match tsc: a tsconfig.json in any ancestor directory counts as "has input".
    if args.list_files_only
        && args.files.is_empty()
        && args.project.is_none()
        && driver::find_tsconfig(&cwd).is_none()
    {
        println!("Version {TSC_VERSION}");
        println!("{}", help::colorize_help(&help::render_help(TSC_VERSION)));
        std::process::exit(1);
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

    // No-input behavior: if no files given, no --project, and no tsconfig.json
    // can be discovered from cwd or an ancestor, print version + help and exit
    // 1 (matching tsc v6 behavior).
    if args.files.is_empty() && args.project.is_none() && driver::find_tsconfig(&cwd).is_none() {
        println!("Version {TSC_VERSION}");
        println!("{}", help::colorize_help(&help::render_help(TSC_VERSION)));
        std::process::exit(1);
    }

    // `tsz <dir>` should behave like `tsz --project <dir>` when no
    // `--project` was supplied and the only positional arg is a directory.
    // tsc treats this as a project root and loads the directory's
    // tsconfig.json. Without this promotion we emit TS5112 ("tsconfig.json
    // is present but will not be loaded …") because `<dir>` is classified
    // as an explicit file input (#6002).
    if args.project.is_none() && args.files.len() == 1 {
        let candidate = cwd.join(&args.files[0]);
        if candidate.is_dir() {
            args.project = Some(args.files.remove(0));
        }
    }

    // TS5042: Option 'project' cannot be mixed with source files on a command line.
    if args.project.is_some() && !args.files.is_empty() {
        println!(
            "error TS5042: Option 'project' cannot be mixed with source files on a command line."
        );
        std::process::exit(1);
    }

    // Issue #3500: TS5069 for `--emitDeclarationOnly` is enforced by the
    // driver/config validation (see `crates/tsz-cli/src/driver/core.rs`'s
    // group-1 prerequisite merge and `crates/tsz-core/src/config/mod.rs`'s
    // TS5069 emission). The previous early CLI-only short-circuit fired
    // before tsconfig was loaded, so projects with `declaration: true`
    // in their config were incorrectly rejected.

    // Issue #3860: tsc honors output-only `compilerOptions` flags
    // (`listFiles`, `listEmittedFiles`, `explainFiles`, `diagnostics`,
    // `extendedDiagnostics`, `traceResolution`) from tsconfig. tsz only
    // checked the CLI-flag side. OR the tsconfig-side values into `args`
    // before the CLI gates further down inspect them.
    let mut args = args;
    merge_output_only_options_from_tsconfig(&mut args, &cwd);

    if let Some(profile_path) = args.generate_cpu_profile.as_ref() {
        println!(
            "error: --generateCpuProfile is not supported by tsz; requested profile '{}' was not created. Use --generateTrace for native trace output.",
            profile_path.display()
        );
        std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
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

    let start_time = std::time::Instant::now();
    let result = match driver::compile(&args, &cwd) {
        Ok(r) => r,
        Err(e) => {
            let msg = e.to_string();
            if let Some(rest) = msg.strip_prefix("TS6053: ") {
                report_root_file_diagnostic(6053, rest);
            }
            if let Some(path_str) = msg.strip_prefix("TS6231: ") {
                report_root_file_diagnostic(
                    6231,
                    &format!(
                        "Could not resolve the path '{path_str}' with the extensions: {TS6231_EXTENSIONS}."
                    ),
                );
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

    // Perf-tools-only: write the machine-readable diagnostics JSON report.
    // The flag and call site both compile out of default release builds.
    #[cfg(feature = "perf-tools")]
    if let Some(path) = args.diagnostics_json.as_deref() {
        let raw_args: Vec<std::ffi::OsString> = std::env::args_os().collect();
        if let Err(err) = tsz_cli::perf_json::write_compilation_report(path, &result, &raw_args) {
            tracing::warn!(
                "failed to write diagnostics JSON to {}: {err}",
                path.display()
            );
        }
    }

    // Perf-tools-only: write the perf-counter JSON snapshot. The flag and
    // the call both compile out of default release builds.
    #[cfg(feature = "perf-tools")]
    if let Some(path) = args.perf_counters_json.as_deref()
        && let Err(err) = tsz_common::perf_counters::PerfCounters::write_json_to(path)
    {
        tracing::warn!(
            "failed to write perf-counter JSON to {}: {err}",
            path.display()
        );
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

    if args.sound_report_only {
        std::process::exit(EXIT_SUCCESS);
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
        // `result.no_emit` reflects the resolved option (CLI + tsconfig.json),
        // so a tsconfig-only `noEmit` selects exit 2 just like the CLI flag.
        if args.no_emit_on_error {
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
        } else if result.no_emit || !result.emitted_files.is_empty() {
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_GENERATED);
        } else {
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
        }
    }

    std::process::exit(EXIT_SUCCESS);
}

fn should_report_ts5112_for_command_line_files(args: &CliArgs, cwd: &std::path::Path) -> bool {
    !args.ignore_config
        && !args.build
        && args.project.is_none()
        && !args.files.is_empty()
        && cwd.join("tsconfig.json").exists()
}

const fn should_use_large_stack_thread(args: &CliArgs) -> bool {
    args.project.is_some() || args.build || args.watch || args.batch || !args.files.is_empty()
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
    // Worker process cwd is captured here in case future batch protocol
    // additions need it as a fallback. Per-test compilations use the
    // explicit `project_path` from stdin so diagnostics render relative
    // to the test's project root, not the long-lived worker's cwd.
    let _worker_cwd = std::env::current_dir().context("failed to resolve batch worker cwd")?;

    for line in reader.lines() {
        let line = line.context("failed to read from stdin")?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            // Skip empty lines, print sentinel to keep protocol in sync
            writeln!(stdout, "---TSZ-BATCH-DONE---")?;
            stdout.flush()?;
            continue;
        }

        let project_dir = trimmed;

        // Clear all thread-local state between compilations.
        // The type interner cache holds TypeId→TypeData mappings from the previous
        // compilation's TypeInterner. Without clearing, a new interner reusing the
        // same TypeId values would get stale TypeData from the old interner.
        // The checker thread-locals hold NodeIndex-keyed caches that similarly get
        // stale when a new AST arena reuses the same indices.
        tsz_solver::construction::clear_thread_local_cache();
        tsz_solver::relations::subtype::reset_subtype_thread_local_state();
        tsz::checker::clear_all_thread_local_state();

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

        // Match subprocess mode for code paths that still consult process cwd
        // during JS module/JSDoc symbol resolution. Keep passing project_path
        // through compile/reporter so diagnostics remain project-relative for
        // tests that opt into a non-root currentDirectory.
        let previous_cwd = std::env::current_dir().context("failed to resolve batch cwd")?;
        std::env::set_current_dir(project_path).with_context(|| {
            format!(
                "failed to enter batch project directory {}",
                project_path.display()
            )
        })?;
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
        std::env::set_current_dir(previous_cwd).context("failed to restore batch cwd")?;

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
/// - Optional boolean flags: `--strictNullChecks file.ts` → `--strictNullChecks=true file.ts`
/// - Duplicate flags: `--strict --strict` → deduplicated (tsc v6 compat)
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
                        if !trimmed.is_empty() {
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
                if let Some(canonical) = canonicalize_long_flag(flag_part) {
                    *arg = OsString::from(format!("{canonical}{value_part}"));
                }
            } else {
                let flag_part = &s[2..];
                if let Some(canonical) = canonicalize_long_flag(flag_part) {
                    *arg = OsString::from(canonical);
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
    for arg in expanded.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "--" || s == "-" {
            println!("error TS5023: Unknown compiler option '{s}'.");
            std::process::exit(1);
        }
        // tsc treats --boolFlag=value as an unknown option (the whole --flag=value string)
        if let Some(eq_pos) = s.find('=') {
            let flag_part = &s[..eq_pos];
            if is_boolean_flag(flag_part) {
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

            let is_boolean = is_boolean_flag(flag_name.as_str());
            let takes_value = is_valued_flag(flag_name.as_str());

            // Check if next arg is "true" or "false" for boolean flags
            if is_boolean
                && !arg_str.contains('=')
                && let Some(next) = result.get(i + 1)
            {
                let next_str = next.to_string_lossy();
                let next_lower = next_str.to_lowercase();
                if next_lower == "false" {
                    if is_option_bool_flag(flag_name.as_str()) {
                        push_option_bool_arg(
                            &mut final_result,
                            &mut skip_positions,
                            &mut flag_positions,
                            &flag_name,
                            false,
                        );
                        i += 2;
                        continue;
                    }
                    // Plain bool flag: clap can't represent an explicit `false`,
                    // so strip the `--flag false` pair and forward the intent
                    // through a hidden side-channel arg. The override pipeline
                    // reads `args.explicitly_disabled_bool_flags` and uses it to
                    // flip a `true` value loaded from `tsconfig.json` to `false`.
                    if let Some(&prev_idx) = flag_positions.get(&flag_name) {
                        skip_positions[prev_idx] = true;
                    }
                    flag_positions.remove(&flag_name);
                    let bare = flag_name.trim_start_matches("--");
                    final_result.push(OsString::from(format!(
                        "--__explicitly-disabled-bool-flag={bare}"
                    )));
                    skip_positions.push(false);
                    i += 2;
                    continue;
                } else if next_lower == "true" {
                    if is_option_bool_flag(flag_name.as_str()) {
                        push_option_bool_arg(
                            &mut final_result,
                            &mut skip_positions,
                            &mut flag_positions,
                            &flag_name,
                            true,
                        );
                        i += 2;
                        continue;
                    }
                    // Plain bool flag: keep the flag, skip the "true" token
                    i += 1;
                }
            }

            if is_boolean && !arg_str.contains('=') && is_option_bool_flag(flag_name.as_str()) {
                push_option_bool_arg(
                    &mut final_result,
                    &mut skip_positions,
                    &mut flag_positions,
                    &flag_name,
                    true,
                );
                i += 1;
                continue;
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

fn push_option_bool_arg(
    final_result: &mut Vec<OsString>,
    skip_positions: &mut Vec<bool>,
    flag_positions: &mut FxHashMap<String, usize>,
    flag_name: &str,
    value: bool,
) {
    if let Some(&prev_idx) = flag_positions.get(flag_name) {
        skip_positions[prev_idx] = true;
    }

    let current_idx = final_result.len();
    flag_positions.insert(flag_name.to_string(), current_idx);
    final_result.push(OsString::from(format!("{flag_name}={value}")));
    skip_positions.push(false);
}

/// Return the canonical long flag spelling for tsc-compatible case-insensitive
/// input, accepting both camelCase and kebab-case spellings.
fn canonicalize_long_flag(flag: &str) -> Option<&'static str> {
    for &known in KNOWN_TSC_OPTIONS {
        if flag_key_matches(&known[2..], flag) {
            return Some(known);
        }
    }

    match normalized_flag_key(flag).as_str() {
        "buildverbose" | "verbose" => Some("--build-verbose"),
        "batch" => Some("--batch"),
        "diagnosticsjson" => Some("--diagnostics-json"),
        "perfcountersjson" => Some("--perf-counters-json"),
        "tracedependencies" => Some("--traceDependencies"),
        "__explicitlydisabledboolflag" => Some("--__explicitly-disabled-bool-flag"),
        _ => None,
    }
}

fn flag_key_matches(canonical: &str, input: &str) -> bool {
    canonical
        .bytes()
        .filter(|&b| b != b'-')
        .map(|b| b.to_ascii_lowercase())
        .eq(input
            .bytes()
            .filter(|&b| b != b'-')
            .map(|b| b.to_ascii_lowercase()))
}

fn normalized_flag_key(flag: &str) -> String {
    flag.bytes()
        .filter(|&b| b != b'-')
        .map(|b| b.to_ascii_lowercase() as char)
        .collect()
}

/// Known boolean flags (flags that accept no value or optional true/false).
const BOOLEAN_FLAGS: &[&str] = &[
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
    "--soundReportOnly",
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
];

fn is_boolean_flag(flag: &str) -> bool {
    BOOLEAN_FLAGS.contains(&flag)
}

/// Flags that take a mandatory value argument (not boolean flags).
const VALUED_FLAGS: &[&str] = &[
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
    "--ignoreDeprecations",
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
];

fn is_valued_flag(flag: &str) -> bool {
    VALUED_FLAGS.contains(&flag)
}

/// Flags that are Option<bool> (tri-state: None, Some(true), Some(false)).
/// These need --flag=true or --flag=false rather than flag removal.
const OPTION_BOOL_FLAGS: &[&str] = &[
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
];

fn is_option_bool_flag(flag: &str) -> bool {
    OPTION_BOOL_FLAGS.contains(&flag)
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
    // Value ordering and inclusion matches tsc baselines exactly.
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
    "--ignoreDeprecations",
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
    "--sound",
    "--soundReportOnly",
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
    "--typesVersions",
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

/// Read the discovered tsconfig and OR its output-only `compilerOptions`
/// flags into `args`. tsc honors `listFiles`, `listEmittedFiles`,
/// `explainFiles`, `diagnostics`, `extendedDiagnostics`, and
/// `traceResolution` from tsconfig; tsz used to ignore them. See #3860.
///
/// This is a best-effort merge: the full config resolver runs later
/// (with extends-resolution, JSONC, etc.). For these output-only flags,
/// reading the literal top-level `compilerOptions` is sufficient because
/// their values aren't redefined in extends chains in practice.
fn merge_output_only_options_from_tsconfig(args: &mut CliArgs, cwd: &std::path::Path) {
    if args.ignore_config {
        return;
    }
    // Resolve tsconfig path the same way handle_show_config does, falling
    // back to upward search from cwd.
    let tsconfig_path = args
        .project
        .as_ref()
        .map(|p| {
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
        .or_else(|| driver::find_tsconfig(cwd));
    let Some(path) = tsconfig_path else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    // Tolerate JSONC. We don't need the full extends chain here — only the
    // top-level compilerOptions block.
    let normalized = tsz_cli::config::normalize_jsonc(&text);
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&normalized) else {
        return;
    };
    let Some(opts) = json.get("compilerOptions").and_then(|v| v.as_object()) else {
        return;
    };

    let take_bool = |key: &str, current: &mut bool| {
        if !*current
            && opts
                .get(key)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        {
            *current = true;
        }
    };
    take_bool("listFiles", &mut args.list_files);
    take_bool("listEmittedFiles", &mut args.list_emitted_files);
    take_bool("explainFiles", &mut args.explain_files);
    take_bool("diagnostics", &mut args.diagnostics);
    take_bool("extendedDiagnostics", &mut args.extended_diagnostics);
    take_bool("traceResolution", &mut args.trace_resolution);
}

/// Line counts categorized by source-file type, matching tsc's `--diagnostics` output.
#[derive(Debug, Default, PartialEq, Eq)]
struct FileLinesStats {
    library: u64,
    definitions: u64,
    typescript: u64,
    javascript: u64,
    json: u64,
    other: u64,
}

/// Read each file and count its lines, grouped by source-file category.
///
/// This is the only I/O-performing step in the diagnostics pipeline; all
/// downstream rendering operates on the returned counts.
fn collect_file_lines(files: &[PathBuf]) -> FileLinesStats {
    let mut stats = FileLinesStats::default();
    for path in files {
        let count = std::fs::read_to_string(path)
            .ok()
            .map_or(0, |text| text.lines().count() as u64);
        let name = path.to_string_lossy();
        if name.contains("lib.") && name.ends_with(".d.ts") {
            stats.library += count;
        } else if name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts") {
            stats.definitions += count;
        } else if name.ends_with(".ts")
            || name.ends_with(".tsx")
            || name.ends_with(".mts")
            || name.ends_with(".cts")
        {
            stats.typescript += count;
        } else if name.ends_with(".js")
            || name.ends_with(".jsx")
            || name.ends_with(".mjs")
            || name.ends_with(".cjs")
        {
            stats.javascript += count;
        } else if name.ends_with(".json") {
            stats.json += count;
        } else {
            stats.other += count;
        }
    }
    stats
}

/// Flat snapshot of all values needed to render the `--diagnostics` /
/// `--extendedDiagnostics` report.  Contains only primitive types so it is
/// easy to construct in unit tests without pulling in solver/checker types.
#[derive(Debug, Default)]
struct DiagnosticsReport {
    // Basic (always rendered)
    files_count: usize,
    lines: FileLinesStats,
    error_count: usize,
    has_phase_timings: bool,
    io_read_secs: f64,
    parse_bind_secs: f64,
    check_secs: f64,
    emit_secs: f64,
    total_secs: f64,
    // Extended section
    memory_used_kb: u64,
    emitted_files_count: usize,
    total_diagnostics: usize,
    request_cache_hits: u64,
    request_cache_misses: u64,
    contextual_cache_bypasses: u64,
    clear_type_cache_recursive_calls: u64,
    property_access_cache_hits: u64,
    property_access_cache_lookups: u64,
    interned_types_count: usize,
    interner_kb: f64,
    // Solver relation caches (present when query_cache_stats is Some)
    has_query_cache: bool,
    subtype_entries: usize,
    subtype_hits: u64,
    subtype_misses: u64,
    assignability_entries: usize,
    assignability_hits: u64,
    assignability_misses: u64,
    eval_cache_entries: usize,
    property_cache_entries: usize,
    variance_cache_entries: usize,
    query_cache_kb: f64,
    // Definition store (present when def_store_stats is Some)
    has_def_store: bool,
    def_total: usize,
    def_type_aliases: usize,
    def_interfaces: usize,
    def_classes: usize,
    def_enums: usize,
    def_type_to_def_entries: usize,
    def_symbol_def_index_entries: usize,
    def_body_to_alias_entries: usize,
    def_shape_to_def_entries: usize,
    def_store_kb: f64,
    // AST residency (present when residency_stats is Some)
    has_residency: bool,
    residency_unique_arena_count: usize,
    residency_arena_kb: f64,
    residency_file_count: usize,
    residency_bound_kb: f64,
    residency_has_pre_merge: bool,
    residency_pre_merge_kb: f64,
    residency_has_skeleton: bool,
    residency_skeleton_symbol_count: usize,
    residency_skeleton_merge_candidate_count: usize,
    residency_skeleton_kb: f64,
    // Module dependency graph (present when module_dep_stats is Some)
    has_module_deps: bool,
    module_file_count: usize,
    module_dependency_edges: usize,
    module_import_cycles: usize,
    module_largest_cycle: usize,
    // Perf-counter dump (non-empty only when TSZ_PERF_COUNTERS is set)
    perf_counter_dump: String,
}

/// Collect all data needed for the diagnostics report from a `CompilationResult`.
///
/// Performs no I/O; the caller is responsible for supplying `file_lines`
/// (from `collect_file_lines`) and `memory_used_kb` (from `get_memory_usage_kb`).
fn build_diagnostics_report(
    result: &driver::CompilationResult,
    file_lines: FileLinesStats,
    elapsed: Duration,
    memory_used_kb: u64,
    extended: bool,
) -> DiagnosticsReport {
    let pt = &result.phase_timings;
    let error_count = result
        .diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::Error)
        .count();

    let mut report = DiagnosticsReport {
        files_count: result.files_read.len(),
        lines: file_lines,
        error_count,
        has_phase_timings: pt.total_ms > 0.0,
        io_read_secs: pt.io_read_ms / 1000.0,
        parse_bind_secs: (pt.load_libs_ms + pt.parse_bind_ms) / 1000.0,
        check_secs: pt.check_ms / 1000.0,
        emit_secs: pt.emit_ms / 1000.0,
        total_secs: elapsed.as_secs_f64(),
        ..DiagnosticsReport::default()
    };

    if !extended {
        return report;
    }

    let counters = result.request_cache_counters;
    report.memory_used_kb = memory_used_kb;
    report.emitted_files_count = result.emitted_files.len();
    report.total_diagnostics = result.diagnostics.len();
    report.request_cache_hits = counters.request_cache_hits;
    report.request_cache_misses = counters.request_cache_misses;
    report.contextual_cache_bypasses = counters.contextual_cache_bypasses;
    report.clear_type_cache_recursive_calls = counters.clear_type_cache_recursive_calls;
    report.property_access_cache_hits = counters.property_access_request_cache_hits;
    report.property_access_cache_lookups = counters.property_access_request_cache_lookups;
    report.interned_types_count = result.interned_types_count;
    report.interner_kb = result.interner_estimated_bytes as f64 / 1024.0;

    if let Some(ref qc) = result.query_cache_stats {
        report.has_query_cache = true;
        report.subtype_entries = qc.relation.subtype_entries;
        report.subtype_hits = qc.relation.subtype_hits;
        report.subtype_misses = qc.relation.subtype_misses;
        report.assignability_entries = qc.relation.assignability_entries;
        report.assignability_hits = qc.relation.assignability_hits;
        report.assignability_misses = qc.relation.assignability_misses;
        report.eval_cache_entries = qc.eval_cache_entries;
        report.property_cache_entries = qc.property_cache_entries;
        report.variance_cache_entries = qc.variance_cache_entries;
        report.query_cache_kb = qc.estimated_size_bytes() as f64 / 1024.0;
    }

    if let Some(ref ds) = result.def_store_stats {
        report.has_def_store = true;
        report.def_total = ds.total_definitions;
        report.def_type_aliases = ds.type_aliases;
        report.def_interfaces = ds.interfaces;
        report.def_classes = ds.classes;
        report.def_enums = ds.enums;
        report.def_type_to_def_entries = ds.type_to_def_entries;
        report.def_symbol_def_index_entries = ds.symbol_def_index_entries;
        report.def_body_to_alias_entries = ds.body_to_alias_entries;
        report.def_shape_to_def_entries = ds.shape_to_def_entries;
        report.def_store_kb = ds.estimated_size_bytes as f64 / 1024.0;
    }

    if let Some(ref rs) = result.residency_stats {
        report.has_residency = true;
        report.residency_unique_arena_count = rs.unique_arena_count;
        report.residency_arena_kb = rs.unique_arena_estimated_bytes as f64 / 1024.0;
        report.residency_file_count = rs.file_count;
        report.residency_bound_kb = rs.total_bound_file_bytes as f64 / 1024.0;
        report.residency_has_pre_merge = rs.pre_merge_bind_total_bytes > 0;
        report.residency_pre_merge_kb = rs.pre_merge_bind_total_bytes as f64 / 1024.0;
        report.residency_has_skeleton = rs.has_skeleton_index;
        report.residency_skeleton_symbol_count = rs.skeleton_total_symbol_count;
        report.residency_skeleton_merge_candidate_count = rs.skeleton_merge_candidate_count;
        report.residency_skeleton_kb = rs.skeleton_estimated_size_bytes as f64 / 1024.0;
    }

    if let Some(ref md) = result.module_dep_stats {
        report.has_module_deps = true;
        report.module_file_count = md.file_count;
        report.module_dependency_edges = md.dependency_edges;
        report.module_import_cycles = md.import_cycles;
        report.module_largest_cycle = md.largest_cycle_size;
    }

    report.perf_counter_dump = tsz_common::perf_counters::PerfCounters::dump_string();

    report
}

/// Render a `--diagnostics` / `--extendedDiagnostics` report as a `String`.
///
/// This function is pure: it reads only from `report` and writes only to the
/// returned `String`.  All I/O (file reads, memory probing, env-var access)
/// must have been completed by the caller before calling this function.
fn render_diagnostics_report(report: &DiagnosticsReport, extended: bool) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();

    let _ = writeln!(out);
    let _ = writeln!(out, "Files:                         {}", report.files_count);
    let _ = writeln!(
        out,
        "Lines of Library:              {}",
        report.lines.library
    );
    let _ = writeln!(
        out,
        "Lines of Definitions:          {}",
        report.lines.definitions
    );
    let _ = writeln!(
        out,
        "Lines of TypeScript:           {}",
        report.lines.typescript
    );
    let _ = writeln!(
        out,
        "Lines of JavaScript:           {}",
        report.lines.javascript
    );
    let _ = writeln!(out, "Lines of JSON:                 {}", report.lines.json);
    let _ = writeln!(out, "Lines of Other:                {}", report.lines.other);
    let _ = writeln!(out, "Errors:                        {}", report.error_count);

    if report.has_phase_timings {
        let _ = writeln!(
            out,
            "I/O Read:                      {:.2}s",
            report.io_read_secs
        );
        let _ = writeln!(
            out,
            "Parse & Bind:                  {:.2}s",
            report.parse_bind_secs
        );
        let _ = writeln!(
            out,
            "Check:                         {:.2}s",
            report.check_secs
        );
        let _ = writeln!(
            out,
            "Emit:                          {:.2}s",
            report.emit_secs
        );
    }
    let _ = writeln!(
        out,
        "Total time:                    {:.2}s",
        report.total_secs
    );

    if !extended {
        return out;
    }

    let _ = writeln!(
        out,
        "Emitted files:                 {}",
        report.emitted_files_count
    );
    let _ = writeln!(
        out,
        "Total diagnostics:             {}",
        report.total_diagnostics
    );
    let _ = writeln!(
        out,
        "Request cache hits:            {}",
        report.request_cache_hits
    );
    let _ = writeln!(
        out,
        "Request cache misses:          {}",
        report.request_cache_misses
    );

    let request_lookups = report.request_cache_hits + report.request_cache_misses;
    let request_hit_rate = if request_lookups == 0 {
        0.0
    } else {
        report.request_cache_hits as f64 * 100.0 / request_lookups as f64
    };
    let _ = writeln!(out, "Request cache hit rate:        {request_hit_rate:.1}%");
    let _ = writeln!(
        out,
        "Contextual cache bypasses:     {}",
        report.contextual_cache_bypasses
    );
    let _ = writeln!(
        out,
        "clear_type_cache_recursive:    {}",
        report.clear_type_cache_recursive_calls
    );

    let access_hit_rate = if report.property_access_cache_lookups == 0 {
        0.0
    } else {
        report.property_access_cache_hits as f64 * 100.0
            / report.property_access_cache_lookups as f64
    };
    let _ = writeln!(
        out,
        "Access request-cache hit rate: {:.1}% ({}/{})",
        access_hit_rate, report.property_access_cache_hits, report.property_access_cache_lookups
    );

    if report.interned_types_count > 0 {
        let _ = writeln!(
            out,
            "Interned types:                {}",
            report.interned_types_count
        );
    }

    if report.has_query_cache {
        let sub_total = report.subtype_hits + report.subtype_misses;
        let sub_rate = if sub_total == 0 {
            0.0
        } else {
            report.subtype_hits as f64 * 100.0 / sub_total as f64
        };
        let assign_total = report.assignability_hits + report.assignability_misses;
        let assign_rate = if assign_total == 0 {
            0.0
        } else {
            report.assignability_hits as f64 * 100.0 / assign_total as f64
        };
        let _ = writeln!(
            out,
            "Subtype cache:                 {} entries ({} hits, {} misses, {sub_rate:.1}%)",
            report.subtype_entries, report.subtype_hits, report.subtype_misses,
        );
        let _ = writeln!(
            out,
            "Assignability cache:           {} entries ({} hits, {} misses, {assign_rate:.1}%)",
            report.assignability_entries, report.assignability_hits, report.assignability_misses,
        );
        let _ = writeln!(
            out,
            "Eval cache:                    {}",
            report.eval_cache_entries
        );
        let _ = writeln!(
            out,
            "Property cache:                {}",
            report.property_cache_entries
        );
        let _ = writeln!(
            out,
            "Variance cache:                {}",
            report.variance_cache_entries
        );
    }

    if report.has_def_store {
        let _ = writeln!(
            out,
            "Definitions:                   {} total ({} aliases, {} interfaces, {} classes, {} enums)",
            report.def_total,
            report.def_type_aliases,
            report.def_interfaces,
            report.def_classes,
            report.def_enums,
        );
        let _ = writeln!(
            out,
            "Def indices:                   type_to_def={}, symbol_def={}, body_to_alias={}, shape_to_def={}",
            report.def_type_to_def_entries,
            report.def_symbol_def_index_entries,
            report.def_body_to_alias_entries,
            report.def_shape_to_def_entries,
        );
    }

    let solver_total_kb = report.interner_kb + report.query_cache_kb + report.def_store_kb;
    if solver_total_kb > 0.0 {
        let _ = writeln!(
            out,
            "Type interner memory:          {:.1}K ({} types)",
            report.interner_kb, report.interned_types_count,
        );
        let _ = writeln!(
            out,
            "Query cache memory:            {:.1}K",
            report.query_cache_kb
        );
        let _ = writeln!(
            out,
            "Definition store memory:       {:.1}K",
            report.def_store_kb
        );
        let _ = writeln!(out, "Solver total memory:           {solver_total_kb:.1}K");
    }

    if report.has_residency {
        let _ = writeln!(
            out,
            "AST arenas:                    {} unique ({:.1}K)",
            report.residency_unique_arena_count, report.residency_arena_kb,
        );
        let _ = writeln!(
            out,
            "Bound files:                   {} ({:.1}K)",
            report.residency_file_count, report.residency_bound_kb,
        );
        if report.residency_has_pre_merge {
            let _ = writeln!(
                out,
                "Pre-merge bind data:           {:.1}K",
                report.residency_pre_merge_kb
            );
        }
        if report.residency_has_skeleton {
            let _ = writeln!(
                out,
                "Skeleton index:                {} symbols, {} merge candidates ({:.1}K)",
                report.residency_skeleton_symbol_count,
                report.residency_skeleton_merge_candidate_count,
                report.residency_skeleton_kb,
            );
        }
    }

    if report.has_module_deps {
        let _ = writeln!(
            out,
            "Module files:                  {}",
            report.module_file_count
        );
        let _ = writeln!(
            out,
            "Dependency edges:              {}",
            report.module_dependency_edges
        );
        let _ = writeln!(
            out,
            "Import cycles:                 {}",
            report.module_import_cycles
        );
        if report.module_largest_cycle > 0 {
            let _ = writeln!(
                out,
                "Largest cycle:                 {} files",
                report.module_largest_cycle
            );
        }
    }

    if report.memory_used_kb > 0 {
        let _ = writeln!(
            out,
            "Memory used:                   {}K",
            report.memory_used_kb
        );
    }

    if !report.perf_counter_dump.is_empty() {
        out.push_str(&report.perf_counter_dump);
    }

    out
}

/// Thin orchestrator: collect data, render, then emit to stdout.
fn print_diagnostics(result: &driver::CompilationResult, elapsed: Duration, extended: bool) {
    let file_lines = collect_file_lines(&result.files_read);
    let memory_used_kb = if extended { get_memory_usage_kb() } else { 0 };
    let report = build_diagnostics_report(result, file_lines, elapsed, memory_used_kb, extended);
    print!("{}", render_diagnostics_report(&report, extended));
}

#[cfg(test)]
mod diagnostics_report_tests {
    use super::*;

    fn basic_report() -> DiagnosticsReport {
        DiagnosticsReport {
            files_count: 3,
            lines: FileLinesStats {
                library: 100,
                definitions: 50,
                typescript: 200,
                ..FileLinesStats::default()
            },
            error_count: 2,
            total_secs: 1.23,
            ..DiagnosticsReport::default()
        }
    }

    #[test]
    fn basic_output_contains_required_fields() {
        let report = basic_report();
        let out = render_diagnostics_report(&report, false);
        assert!(
            out.contains("Files:                         3"),
            "files count"
        );
        assert!(
            out.contains("Lines of Library:              100"),
            "library lines"
        );
        assert!(
            out.contains("Lines of TypeScript:           200"),
            "typescript lines"
        );
        assert!(out.contains("Errors:                        2"), "errors");
        assert!(out.contains("Total time:                    1.23s"), "time");
    }

    #[test]
    fn basic_output_excludes_extended_fields() {
        let report = basic_report();
        let out = render_diagnostics_report(&report, false);
        assert!(
            !out.contains("Request cache"),
            "no cache stats in basic mode"
        );
        assert!(!out.contains("Memory used"), "no memory in basic mode");
    }

    #[test]
    fn extended_output_includes_cache_stats() {
        let report = DiagnosticsReport {
            files_count: 1,
            total_secs: 0.5,
            request_cache_hits: 80,
            request_cache_misses: 20,
            has_query_cache: true,
            subtype_entries: 10,
            subtype_hits: 8,
            subtype_misses: 2,
            assignability_entries: 5,
            assignability_hits: 3,
            assignability_misses: 2,
            memory_used_kb: 4096,
            ..DiagnosticsReport::default()
        };
        let out = render_diagnostics_report(&report, true);
        assert!(
            out.contains("Request cache hit rate:        80.0%"),
            "hit rate: {out}"
        );
        assert!(out.contains("Subtype cache:"), "subtype cache");
        assert!(
            out.contains("Memory used:                   4096K"),
            "memory"
        );
    }

    #[test]
    fn phase_timings_only_shown_when_present() {
        let without = DiagnosticsReport {
            has_phase_timings: false,
            total_secs: 0.1,
            ..DiagnosticsReport::default()
        };
        let with_timings = DiagnosticsReport {
            has_phase_timings: true,
            io_read_secs: 0.05,
            parse_bind_secs: 0.03,
            check_secs: 0.02,
            emit_secs: 0.01,
            total_secs: 0.1,
            ..DiagnosticsReport::default()
        };
        let out_without = render_diagnostics_report(&without, false);
        let out_with = render_diagnostics_report(&with_timings, false);
        assert!(
            !out_without.contains("I/O Read:"),
            "no timings without flag"
        );
        assert!(
            out_with.contains("I/O Read:                      0.05s"),
            "has timings"
        );
    }

    #[test]
    fn collect_file_lines_categorizes_correctly() {
        // Build a temp dir with files of different types to verify categorization.
        let dir = std::env::temp_dir().join("tsz_test_file_lines");
        let _ = std::fs::create_dir_all(&dir);

        let lib_d_ts = dir.join("lib.es5.d.ts");
        let user_ts = dir.join("user.ts");
        let js_file = dir.join("helper.js");

        std::fs::write(&lib_d_ts, "line1\nline2\nline3\n").unwrap();
        std::fs::write(&user_ts, "line1\nline2\n").unwrap();
        std::fs::write(&js_file, "line1\n").unwrap();

        let stats = collect_file_lines(&[lib_d_ts, user_ts, js_file]);

        assert_eq!(stats.library, 3, "lib.d.ts lines");
        assert_eq!(stats.typescript, 2, "ts lines");
        assert_eq!(stats.javascript, 1, "js lines");
        assert_eq!(stats.definitions, 0);
        assert_eq!(stats.json, 0);
        assert_eq!(stats.other, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn basic_golden_full_output() {
        let report = DiagnosticsReport {
            files_count: 5,
            lines: FileLinesStats {
                library: 1000,
                definitions: 200,
                typescript: 300,
                javascript: 50,
                json: 10,
                other: 0,
            },
            error_count: 3,
            has_phase_timings: true,
            io_read_secs: 0.10,
            parse_bind_secs: 0.20,
            check_secs: 0.30,
            emit_secs: 0.05,
            total_secs: 0.65,
            ..DiagnosticsReport::default()
        };
        let out = render_diagnostics_report(&report, false);
        let expected = "\n\
Files:                         5\n\
Lines of Library:              1000\n\
Lines of Definitions:          200\n\
Lines of TypeScript:           300\n\
Lines of JavaScript:           50\n\
Lines of JSON:                 10\n\
Lines of Other:                0\n\
Errors:                        3\n\
I/O Read:                      0.10s\n\
Parse & Bind:                  0.20s\n\
Check:                         0.30s\n\
Emit:                          0.05s\n\
Total time:                    0.65s\n";
        assert_eq!(out, expected, "basic golden mismatch:\n{out}");
    }

    #[test]
    fn extended_golden_full_output() {
        let report = DiagnosticsReport {
            files_count: 2,
            lines: FileLinesStats {
                library: 500,
                definitions: 0,
                typescript: 100,
                javascript: 0,
                json: 0,
                other: 0,
            },
            error_count: 0,
            has_phase_timings: false,
            total_secs: 1.00,
            // Extended fields
            memory_used_kb: 8192,
            emitted_files_count: 1,
            total_diagnostics: 0,
            request_cache_hits: 90,
            request_cache_misses: 10,
            contextual_cache_bypasses: 2,
            clear_type_cache_recursive_calls: 1,
            property_access_cache_hits: 45,
            property_access_cache_lookups: 50,
            interned_types_count: 0,
            interner_kb: 0.0,
            has_query_cache: false,
            has_def_store: false,
            has_residency: false,
            has_module_deps: false,
            perf_counter_dump: String::new(),
            ..DiagnosticsReport::default()
        };
        let out = render_diagnostics_report(&report, true);
        let expected = "\n\
Files:                         2\n\
Lines of Library:              500\n\
Lines of Definitions:          0\n\
Lines of TypeScript:           100\n\
Lines of JavaScript:           0\n\
Lines of JSON:                 0\n\
Lines of Other:                0\n\
Errors:                        0\n\
Total time:                    1.00s\n\
Emitted files:                 1\n\
Total diagnostics:             0\n\
Request cache hits:            90\n\
Request cache misses:          10\n\
Request cache hit rate:        90.0%\n\
Contextual cache bypasses:     2\n\
clear_type_cache_recursive:    1\n\
Access request-cache hit rate: 90.0% (45/50)\n\
Memory used:                   8192K\n";
        assert_eq!(out, expected, "extended golden mismatch:\n{out}");
    }
}

fn reject_tsconfig_only_cli_options(args: &CliArgs) {
    for (name, values) in [
        ("paths", args.paths.as_ref()),
        ("plugins", args.plugins.as_ref()),
    ] {
        let provided_non_null = values
            .is_some_and(|values| !(values.len() == 1 && values[0].eq_ignore_ascii_case("null")));
        if provided_non_null {
            println!(
                "error TS6064: Option '{name}' can only be specified in 'tsconfig.json' file or set to 'null' on command line."
            );
            std::process::exit(1);
        }
    }
}

fn reject_build_only_cli_options(args: &CliArgs) {
    if args.build {
        return;
    }

    let explicitly_disabled = |name: &str| {
        args.explicitly_disabled_bool_flags
            .iter()
            .any(|flag| flag == name)
    };

    for (name, provided) in [
        (
            "verbose",
            args.build_verbose
                || explicitly_disabled("build-verbose")
                || explicitly_disabled("verbose"),
        ),
        ("dry", args.dry || explicitly_disabled("dry")),
        ("force", args.force || explicitly_disabled("force")),
        ("clean", args.clean || explicitly_disabled("clean")),
        (
            "stopBuildOnErrors",
            args.stop_build_on_errors || explicitly_disabled("stopBuildOnErrors"),
        ),
    ] {
        if provided {
            println!("error TS5093: Compiler option '--{name}' may only be used with '--build'.");
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
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

fn handle_init(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    let tsconfig_path = cwd.join("tsconfig.json");
    if tsconfig_path.exists() {
        println!(
            "error TS5054: A 'tsconfig.json' file is already defined at: '{}'.",
            tsconfig_path.display()
        );
        std::process::exit(0);
    }

    let raw_args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let overrides = collect_init_overrides(&raw_args, args);
    let config = render_init_template(&overrides);

    std::fs::write(&tsconfig_path, config).with_context(|| {
        format!(
            "failed to write tsconfig.json to {}",
            tsconfig_path.display()
        )
    })?;

    println!("\nCreated a new tsconfig.json\n\nYou can learn more at https://aka.ms/tsconfig");

    Ok(())
}

/// Walk the original CLI args in order and collect (canonical option name,
/// JSON-formatted value) pairs for every recognized compiler option the user
/// passed. Later occurrences supersede earlier ones (matching tsc's
/// last-write-wins behavior). Order is preserved so that options not in the
/// fixed `--init` template are appended in the order they appear.
fn collect_init_overrides(raw_args: &[OsString], args: &CliArgs) -> Vec<(&'static str, String)> {
    let mut overrides: Vec<(&'static str, String)> = Vec::new();
    let mut i = 0;
    while i < raw_args.len() {
        let arg = raw_args[i].to_string_lossy().to_string();
        if !arg.starts_with("--") || arg == "--" {
            i += 1;
            continue;
        }
        let (flag, has_inline_value) = match arg.find('=') {
            Some(eq) => (arg[..eq].to_string(), true),
            None => (arg.clone(), false),
        };
        let canonical = canonicalize_init_option(&flag);
        let takes_value = canonical.is_some_and(init_option_takes_value);
        if let Some(name) = canonical
            && let Some(value) = init_option_value(name, args)
        {
            if let Some(pos) = overrides.iter().position(|(k, _)| *k == name) {
                overrides[pos] = (name, value);
            } else {
                overrides.push((name, value));
            }
        }
        if takes_value && !has_inline_value {
            i += 2;
        } else {
            i += 1;
        }
    }
    overrides
}

/// Returns the canonical (camelCase) compiler-option name for a CLI flag.
/// Matching is case-insensitive and ignores `-` characters so that
/// `--rootDir`, `--rootdir`, and `--root-dir` all map to `"rootDir"`.
fn canonicalize_init_option(flag: &str) -> Option<&'static str> {
    let key: String = flag
        .trim_start_matches('-')
        .chars()
        .filter(|c| *c != '-')
        .flat_map(char::to_lowercase)
        .collect();
    INIT_OPTION_TABLE.iter().find_map(|(canonical, _)| {
        let canonical_key: String = canonical
            .chars()
            .filter(|c| *c != '-')
            .flat_map(char::to_lowercase)
            .collect();
        (canonical_key == key).then_some(*canonical)
    })
}

#[derive(Clone, Copy)]
enum InitOptionKind {
    /// Boolean flag (`--strict`, `--strict false`, `--strict true`).
    Bool,
    /// Flag that requires a value (`--target esnext`, `--lib es2015,dom`).
    Value,
}

fn init_option_takes_value(name: &str) -> bool {
    INIT_OPTION_TABLE
        .iter()
        .find(|(n, _)| *n == name)
        .is_some_and(|(_, k)| matches!(k, InitOptionKind::Value))
}

/// Recognized compiler options for the `--init` flow.
///
/// The set is intentionally small relative to the full CLI surface: it covers
/// every option that has a slot in the default template plus the most common
/// command-line options that tsc users pass alongside `--init`. Unrecognized
/// options are silently ignored, matching `tsc`.
const INIT_OPTION_TABLE: &[(&str, InitOptionKind)] = &[
    // Language and environment
    ("target", InitOptionKind::Value),
    ("module", InitOptionKind::Value),
    ("moduleResolution", InitOptionKind::Value),
    ("moduleDetection", InitOptionKind::Value),
    ("jsx", InitOptionKind::Value),
    ("jsxFactory", InitOptionKind::Value),
    ("jsxFragmentFactory", InitOptionKind::Value),
    ("jsxImportSource", InitOptionKind::Value),
    ("lib", InitOptionKind::Value),
    ("types", InitOptionKind::Value),
    ("typeRoots", InitOptionKind::Value),
    ("rootDir", InitOptionKind::Value),
    ("outDir", InitOptionKind::Value),
    ("outFile", InitOptionKind::Value),
    ("baseUrl", InitOptionKind::Value),
    ("declarationDir", InitOptionKind::Value),
    ("newLine", InitOptionKind::Value),
    ("noLib", InitOptionKind::Bool),
    // Emit / output
    ("declaration", InitOptionKind::Bool),
    ("declarationMap", InitOptionKind::Bool),
    ("sourceMap", InitOptionKind::Bool),
    ("inlineSourceMap", InitOptionKind::Bool),
    ("inlineSources", InitOptionKind::Bool),
    ("emitDeclarationOnly", InitOptionKind::Bool),
    ("noEmit", InitOptionKind::Bool),
    ("noEmitOnError", InitOptionKind::Bool),
    ("noEmitHelpers", InitOptionKind::Bool),
    ("importHelpers", InitOptionKind::Bool),
    ("downlevelIteration", InitOptionKind::Bool),
    ("removeComments", InitOptionKind::Bool),
    ("preserveConstEnums", InitOptionKind::Bool),
    ("emitBOM", InitOptionKind::Bool),
    // Interop / modules
    ("esModuleInterop", InitOptionKind::Bool),
    ("allowSyntheticDefaultImports", InitOptionKind::Bool),
    ("isolatedModules", InitOptionKind::Bool),
    ("isolatedDeclarations", InitOptionKind::Bool),
    ("verbatimModuleSyntax", InitOptionKind::Bool),
    ("forceConsistentCasingInFileNames", InitOptionKind::Bool),
    ("preserveSymlinks", InitOptionKind::Bool),
    ("erasableSyntaxOnly", InitOptionKind::Bool),
    ("resolveJsonModule", InitOptionKind::Bool),
    ("noResolve", InitOptionKind::Bool),
    ("allowUmdGlobalAccess", InitOptionKind::Bool),
    ("noUncheckedSideEffectImports", InitOptionKind::Bool),
    ("allowImportingTsExtensions", InitOptionKind::Bool),
    ("rewriteRelativeImportExtensions", InitOptionKind::Bool),
    ("allowArbitraryExtensions", InitOptionKind::Bool),
    // JavaScript support
    ("allowJs", InitOptionKind::Bool),
    ("checkJs", InitOptionKind::Bool),
    // Decorators
    ("experimentalDecorators", InitOptionKind::Bool),
    ("emitDecoratorMetadata", InitOptionKind::Bool),
    // Type checking
    ("strict", InitOptionKind::Bool),
    ("noImplicitAny", InitOptionKind::Bool),
    ("strictNullChecks", InitOptionKind::Bool),
    ("strictFunctionTypes", InitOptionKind::Bool),
    ("strictBindCallApply", InitOptionKind::Bool),
    ("strictPropertyInitialization", InitOptionKind::Bool),
    ("strictBuiltinIteratorReturn", InitOptionKind::Bool),
    ("noImplicitThis", InitOptionKind::Bool),
    ("useUnknownInCatchVariables", InitOptionKind::Bool),
    ("alwaysStrict", InitOptionKind::Bool),
    ("noUnusedLocals", InitOptionKind::Bool),
    ("noUnusedParameters", InitOptionKind::Bool),
    ("exactOptionalPropertyTypes", InitOptionKind::Bool),
    ("noImplicitReturns", InitOptionKind::Bool),
    ("noFallthroughCasesInSwitch", InitOptionKind::Bool),
    ("noUncheckedIndexedAccess", InitOptionKind::Bool),
    ("noImplicitOverride", InitOptionKind::Bool),
    ("noPropertyAccessFromIndexSignature", InitOptionKind::Bool),
    ("allowUnreachableCode", InitOptionKind::Bool),
    ("allowUnusedLabels", InitOptionKind::Bool),
    ("useDefineForClassFields", InitOptionKind::Bool),
    // Completeness
    ("skipDefaultLibCheck", InitOptionKind::Bool),
    ("skipLibCheck", InitOptionKind::Bool),
    // Projects
    ("composite", InitOptionKind::Bool),
    ("incremental", InitOptionKind::Bool),
    // Diagnostics / output formatting
    ("diagnostics", InitOptionKind::Bool),
    ("extendedDiagnostics", InitOptionKind::Bool),
    ("explainFiles", InitOptionKind::Bool),
    ("listFiles", InitOptionKind::Bool),
    ("listEmittedFiles", InitOptionKind::Bool),
    ("traceResolution", InitOptionKind::Bool),
    ("noCheck", InitOptionKind::Bool),
    ("noErrorTruncation", InitOptionKind::Bool),
    ("preserveWatchOutput", InitOptionKind::Bool),
    ("pretty", InitOptionKind::Bool),
];

/// Format the user-supplied value for a recognized option as a JSON literal.
/// Returns `None` if the option is recognized but the parsed `args` struct
/// does not carry a meaningful value (e.g., a `bool` field is `false` because
/// the option was never on the command line — but in that case the caller
/// would not invoke this function).
fn init_option_value(name: &'static str, args: &CliArgs) -> Option<String> {
    match name {
        "target" => args.target.map(|t| json_str(target_init_str(t))),
        "module" => args.module.map(|m| json_str(module_init_str(m))),
        "moduleResolution" => args
            .module_resolution
            .map(|m| json_str(module_resolution_init_str(m))),
        "moduleDetection" => args
            .module_detection
            .map(|m| json_str(module_detection_init_str(m))),
        "jsx" => args.jsx.map(|j| json_str(jsx_init_str(j))),
        "jsxFactory" => args.jsx_factory.as_deref().map(json_str),
        "jsxFragmentFactory" => args.jsx_fragment_factory.as_deref().map(json_str),
        "jsxImportSource" => args.jsx_import_source.as_deref().map(json_str),
        "newLine" => args.new_line.map(|n| json_str(new_line_init_str(n))),
        "lib" => args.lib.as_ref().map(|v| json_str_array(v)),
        "types" => args.types.as_ref().map(|v| json_str_array(v)),
        "typeRoots" => args
            .type_roots
            .as_ref()
            .map(|v| json_path_array(v.iter().map(PathBuf::as_path))),
        "rootDir" => args.root_dir.as_deref().map(json_path),
        "outDir" => args.out_dir.as_deref().map(json_path),
        "outFile" => args.out_file.as_deref().map(json_path),
        "baseUrl" => args.base_url.as_deref().map(json_path),
        "declarationDir" => args.declaration_dir.as_deref().map(json_path),
        // Plain bool flags. The preprocessor in `preprocess_args` strips
        // `--flag false` pairs and either flips the field to `false` directly
        // or records the flag name in `explicitly_disabled_bool_flags`. By
        // the time we get here, `args.<field>` already reflects the user's
        // intent.
        "noLib" => Some(bool_str(args.no_lib)),
        "declaration" => Some(bool_str(args.declaration)),
        "declarationMap" => Some(bool_str(args.declaration_map)),
        "sourceMap" => Some(bool_str(args.source_map)),
        "inlineSourceMap" => Some(bool_str(args.inline_source_map)),
        "inlineSources" => Some(bool_str(args.inline_sources)),
        "emitDeclarationOnly" => Some(bool_str(args.emit_declaration_only)),
        "noEmit" => Some(bool_str(args.no_emit)),
        "noEmitOnError" => Some(bool_str(args.no_emit_on_error)),
        "noEmitHelpers" => Some(bool_str(args.no_emit_helpers)),
        "importHelpers" => Some(bool_str(args.import_helpers)),
        "downlevelIteration" => Some(bool_str(args.downlevel_iteration)),
        "removeComments" => Some(bool_str(args.remove_comments)),
        "preserveConstEnums" => Some(bool_str(args.preserve_const_enums)),
        "emitBOM" => Some(bool_str(args.emit_bom)),
        "esModuleInterop" => Some(bool_str(args.es_module_interop)),
        "isolatedModules" => Some(bool_str(args.isolated_modules)),
        "isolatedDeclarations" => Some(bool_str(args.isolated_declarations)),
        "verbatimModuleSyntax" => Some(bool_str(args.verbatim_module_syntax)),
        "preserveSymlinks" => Some(bool_str(args.preserve_symlinks)),
        "erasableSyntaxOnly" => Some(bool_str(args.erasable_syntax_only)),
        "resolveJsonModule" => Some(bool_str(args.resolve_json_module)),
        "noResolve" => Some(bool_str(args.no_resolve)),
        "allowUmdGlobalAccess" => Some(bool_str(args.allow_umd_global_access)),
        "noUncheckedSideEffectImports" => Some(bool_str(args.no_unchecked_side_effect_imports)),
        "allowImportingTsExtensions" => Some(bool_str(args.allow_importing_ts_extensions)),
        "rewriteRelativeImportExtensions" => {
            Some(bool_str(args.rewrite_relative_import_extensions))
        }
        "allowArbitraryExtensions" => Some(bool_str(args.allow_arbitrary_extensions)),
        "allowJs" => Some(bool_str(args.allow_js)),
        "checkJs" => Some(bool_str(args.check_js)),
        "experimentalDecorators" => Some(bool_str(args.experimental_decorators)),
        "emitDecoratorMetadata" => Some(bool_str(args.emit_decorator_metadata)),
        "strict" => Some(bool_str(args.strict)),
        "noUnusedLocals" => Some(bool_str(args.no_unused_locals)),
        "noUnusedParameters" => Some(bool_str(args.no_unused_parameters)),
        "exactOptionalPropertyTypes" => Some(bool_str(args.exact_optional_property_types)),
        "noImplicitReturns" => Some(bool_str(args.no_implicit_returns)),
        "noFallthroughCasesInSwitch" => Some(bool_str(args.no_fallthrough_cases_in_switch)),
        "noUncheckedIndexedAccess" => Some(bool_str(args.no_unchecked_indexed_access)),
        "noImplicitOverride" => Some(bool_str(args.no_implicit_override)),
        "noPropertyAccessFromIndexSignature" => {
            Some(bool_str(args.no_property_access_from_index_signature))
        }
        "skipDefaultLibCheck" => Some(bool_str(args.skip_default_lib_check)),
        "skipLibCheck" => Some(bool_str(args.skip_lib_check)),
        "composite" => Some(bool_str(args.composite)),
        "incremental" => Some(bool_str(args.incremental)),
        "diagnostics" => Some(bool_str(args.diagnostics)),
        "extendedDiagnostics" => Some(bool_str(args.extended_diagnostics)),
        "explainFiles" => Some(bool_str(args.explain_files)),
        "listFiles" => Some(bool_str(args.list_files)),
        "listEmittedFiles" => Some(bool_str(args.list_emitted_files)),
        "traceResolution" => Some(bool_str(args.trace_resolution)),
        "noCheck" => Some(bool_str(args.no_check)),
        "noErrorTruncation" => Some(bool_str(args.no_error_truncation)),
        "preserveWatchOutput" => Some(bool_str(args.preserve_watch_output)),
        // Tri-state Option<bool> flags.
        "pretty" => args.pretty.map(bool_str),
        "noImplicitAny" => args.no_implicit_any.map(bool_str),
        "strictNullChecks" => args.strict_null_checks.map(bool_str),
        "strictFunctionTypes" => args.strict_function_types.map(bool_str),
        "strictBindCallApply" => args.strict_bind_call_apply.map(bool_str),
        "strictPropertyInitialization" => args.strict_property_initialization.map(bool_str),
        "strictBuiltinIteratorReturn" => args.strict_builtin_iterator_return.map(bool_str),
        "noImplicitThis" => args.no_implicit_this.map(bool_str),
        "useUnknownInCatchVariables" => args.use_unknown_in_catch_variables.map(bool_str),
        "alwaysStrict" => args.always_strict.map(bool_str),
        "allowSyntheticDefaultImports" => args.allow_synthetic_default_imports.map(bool_str),
        "forceConsistentCasingInFileNames" => {
            args.force_consistent_casing_in_file_names.map(bool_str)
        }
        "allowUnreachableCode" => args.allow_unreachable_code.map(bool_str),
        "allowUnusedLabels" => args.allow_unused_labels.map(bool_str),
        "useDefineForClassFields" => args.use_define_for_class_fields.map(bool_str),
        _ => None,
    }
}

fn bool_str(b: bool) -> String {
    if b { "true".into() } else { "false".into() }
}

fn json_str(s: &str) -> String {
    // Escape backslashes and double quotes; tsconfig is JSONC and our values
    // are user-supplied strings or paths.
    let mut escaped = String::with_capacity(s.len() + 2);
    escaped.push('"');
    for c in s.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            _ => escaped.push(c),
        }
    }
    escaped.push('"');
    escaped
}

fn json_path(p: &Path) -> String {
    // tsc emits paths with forward slashes; do the same so that snapshots are
    // stable on Windows-style inputs and so that the path round-trips through
    // tsconfig parsing.
    json_str(&p.to_string_lossy().replace('\\', "/"))
}

fn json_str_array(values: &[String]) -> String {
    if values.is_empty() {
        return "[]".into();
    }
    let items: Vec<String> = values.iter().map(|v| json_str(v)).collect();
    format!("[{}]", items.join(", "))
}

fn json_path_array<'a, I: Iterator<Item = &'a Path>>(paths: I) -> String {
    let items: Vec<String> = paths.map(json_path).collect();
    if items.is_empty() {
        "[]".into()
    } else {
        format!("[{}]", items.join(", "))
    }
}

const fn target_init_str(t: tsz_cli::args::Target) -> &'static str {
    use tsz_cli::args::Target;
    match t {
        Target::Es3 => "es3",
        Target::Es5 => "es5",
        // tsc canonicalizes ES2015 to "es6".
        Target::Es2015 => "es6",
        Target::Es2016 => "es2016",
        Target::Es2017 => "es2017",
        Target::Es2018 => "es2018",
        Target::Es2019 => "es2019",
        Target::Es2020 => "es2020",
        Target::Es2021 => "es2021",
        Target::Es2022 => "es2022",
        Target::Es2023 => "es2023",
        Target::Es2024 => "es2024",
        Target::Es2025 => "es2025",
        Target::EsNext => "esnext",
    }
}

const fn module_init_str(m: tsz_cli::args::Module) -> &'static str {
    use tsz_cli::args::Module;
    match m {
        Module::None => "none",
        Module::CommonJs => "commonjs",
        Module::Amd => "amd",
        Module::Umd => "umd",
        Module::System => "system",
        // tsc canonicalizes ES2015 to "es6" for module too.
        Module::Es2015 => "es6",
        Module::Es2020 => "es2020",
        Module::Es2022 => "es2022",
        Module::EsNext => "esnext",
        Module::Node16 => "node16",
        Module::Node18 => "node18",
        Module::Node20 => "node20",
        Module::NodeNext => "nodenext",
        Module::Preserve => "preserve",
    }
}

const fn module_resolution_init_str(m: tsz_cli::args::ModuleResolution) -> &'static str {
    use tsz_cli::args::ModuleResolution;
    match m {
        ModuleResolution::Classic => "classic",
        // tsc emits "node10" as the canonical name for Node10/node.
        ModuleResolution::Node10 => "node10",
        ModuleResolution::Node16 => "node16",
        ModuleResolution::NodeNext => "nodenext",
        ModuleResolution::Bundler => "bundler",
    }
}

const fn module_detection_init_str(m: tsz_cli::args::ModuleDetection) -> &'static str {
    use tsz_cli::args::ModuleDetection;
    match m {
        ModuleDetection::Auto => "auto",
        ModuleDetection::Force => "force",
        ModuleDetection::Legacy => "legacy",
    }
}

const fn jsx_init_str(j: tsz_cli::args::JsxEmit) -> &'static str {
    use tsz_cli::args::JsxEmit;
    match j {
        JsxEmit::Preserve => "preserve",
        JsxEmit::React => "react",
        JsxEmit::ReactJsx => "react-jsx",
        JsxEmit::ReactJsxDev => "react-jsxdev",
        JsxEmit::ReactNative => "react-native",
    }
}

const fn new_line_init_str(n: tsz_cli::args::NewLine) -> &'static str {
    use tsz_cli::args::NewLine;
    match n {
        NewLine::Crlf => "crlf",
        NewLine::Lf => "lf",
    }
}

fn emit_init_line(
    out: &mut String,
    map: &std::collections::HashMap<&'static str, &str>,
    key: &'static str,
    default_value: &str,
    comment_default: bool,
) {
    if let Some(value) = map.get(key) {
        out.push_str("    \"");
        out.push_str(key);
        out.push_str("\": ");
        out.push_str(value);
        out.push_str(",\n");
    } else if comment_default {
        out.push_str("    // \"");
        out.push_str(key);
        out.push_str("\": ");
        out.push_str(default_value);
        out.push_str(",\n");
    } else {
        out.push_str("    \"");
        out.push_str(key);
        out.push_str("\": ");
        out.push_str(default_value);
        out.push_str(",\n");
    }
}

/// Render the `tsconfig.json` body using the JSONC template that tsc 6.x
/// emits, with each templated option replaced by the user-provided value (if
/// any). Options that the user passed but that don't have a slot in the
/// template are appended after the template body in CLI order, matching tsc.
fn render_init_template(overrides: &[(&'static str, String)]) -> String {
    use std::collections::HashMap;
    let map: HashMap<&'static str, &str> =
        overrides.iter().map(|(k, v)| (*k, v.as_str())).collect();

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  // Visit https://aka.ms/tsconfig to read more about this file\n");
    out.push_str("  \"compilerOptions\": {\n");

    out.push_str("    // File Layout\n");
    emit_init_line(&mut out, &map, "rootDir", "\"./src\"", true);
    emit_init_line(&mut out, &map, "outDir", "\"./dist\"", true);
    out.push('\n');
    out.push_str("    // Environment Settings\n");
    out.push_str("    // See also https://aka.ms/tsconfig/module\n");
    emit_init_line(&mut out, &map, "module", "\"nodenext\"", false);
    emit_init_line(&mut out, &map, "target", "\"esnext\"", false);
    emit_init_line(&mut out, &map, "types", "[]", false);
    out.push_str("    // For nodejs:\n");
    out.push_str("    // \"lib\": [\"esnext\"],\n");
    out.push_str("    // \"types\": [\"node\"],\n");
    out.push_str("    // and npm install -D @types/node\n");
    out.push('\n');
    out.push_str("    // Other Outputs\n");
    emit_init_line(&mut out, &map, "sourceMap", "true", false);
    emit_init_line(&mut out, &map, "declaration", "true", false);
    emit_init_line(&mut out, &map, "declarationMap", "true", false);
    out.push('\n');
    out.push_str("    // Stricter Typechecking Options\n");
    emit_init_line(&mut out, &map, "noUncheckedIndexedAccess", "true", false);
    emit_init_line(&mut out, &map, "exactOptionalPropertyTypes", "true", false);
    out.push('\n');
    out.push_str("    // Style Options\n");
    emit_init_line(&mut out, &map, "noImplicitReturns", "true", true);
    emit_init_line(&mut out, &map, "noImplicitOverride", "true", true);
    emit_init_line(&mut out, &map, "noUnusedLocals", "true", true);
    emit_init_line(&mut out, &map, "noUnusedParameters", "true", true);
    emit_init_line(&mut out, &map, "noFallthroughCasesInSwitch", "true", true);
    emit_init_line(
        &mut out,
        &map,
        "noPropertyAccessFromIndexSignature",
        "true",
        true,
    );
    out.push('\n');
    out.push_str("    // Recommended Options\n");
    emit_init_line(&mut out, &map, "strict", "true", false);
    emit_init_line(&mut out, &map, "jsx", "\"react-jsx\"", false);
    emit_init_line(&mut out, &map, "verbatimModuleSyntax", "true", false);
    emit_init_line(&mut out, &map, "isolatedModules", "true", false);
    emit_init_line(
        &mut out,
        &map,
        "noUncheckedSideEffectImports",
        "true",
        false,
    );
    emit_init_line(&mut out, &map, "moduleDetection", "\"force\"", false);
    emit_init_line(&mut out, &map, "skipLibCheck", "true", false);

    // Append any overrides that don't have a slot in the template, preserving
    // the order they appeared on the command line. tsc emits these after a
    // single blank line separating them from the recommended-options block.
    let template_keys: &[&str] = &[
        "rootDir",
        "outDir",
        "module",
        "target",
        "types",
        "sourceMap",
        "declaration",
        "declarationMap",
        "noUncheckedIndexedAccess",
        "exactOptionalPropertyTypes",
        "noImplicitReturns",
        "noImplicitOverride",
        "noUnusedLocals",
        "noUnusedParameters",
        "noFallthroughCasesInSwitch",
        "noPropertyAccessFromIndexSignature",
        "strict",
        "jsx",
        "verbatimModuleSyntax",
        "isolatedModules",
        "noUncheckedSideEffectImports",
        "moduleDetection",
        "skipLibCheck",
    ];
    let mut appended_any = false;
    for (key, value) in overrides.iter() {
        if template_keys.contains(key) {
            continue;
        }
        if !appended_any {
            out.push('\n');
            appended_any = true;
        }
        out.push_str("    \"");
        out.push_str(key);
        out.push_str("\": ");
        out.push_str(value);
        out.push_str(",\n");
    }

    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

fn handle_show_config(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    use tsz::checker::diagnostics::diagnostic_codes;
    use tsz_cli::config::load_tsconfig_with_diagnostics;

    // Track whether the path was discovered via filesystem walk-up so the
    // TS5112 "implicit tsconfig + explicit files" check fires only for
    // walk-up discoveries (an explicit --project is the user opting in).
    let (tsconfig_path, discovered_via_walkup) = if let Some(p) = args.project.as_ref() {
        let resolved = if p.is_relative() {
            cwd.join(p)
        } else {
            p.clone()
        };
        let resolved = if resolved.is_dir() {
            resolved.join("tsconfig.json")
        } else {
            resolved
        };
        (Some(resolved), false)
    } else if args.ignore_config && !args.files.is_empty() {
        (None, false)
    } else {
        (driver::find_tsconfig(cwd), true)
    };

    if discovered_via_walkup && tsconfig_path.is_some() && !args.files.is_empty() {
        println!("error TS5112: {TS5112_COMMAND_LINE_FILES_MESSAGE}");
        std::process::exit(1);
    }

    if tsconfig_path.is_none() && args.files.is_empty() {
        println!(
            "error TS5081: Cannot find a tsconfig.json file at the current directory: {}.",
            cwd.display()
        );
        std::process::exit(1);
    }

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

    let (config, config_diagnostics) = if let Some(path) = tsconfig_path.as_ref() {
        let parsed = load_tsconfig_with_diagnostics(path)?;
        (Some(parsed.config), parsed.diagnostics)
    } else {
        (None, Vec::new())
    };
    if config_diagnostics.iter().any(|d| {
        d.code == diagnostic_codes::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE
            || d.code
                == diagnostic_codes::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION
    }) {
        let pretty = args
            .pretty
            .unwrap_or_else(|| std::io::stdout().is_terminal());
        if args.pretty == Some(true) {
            Reporter::force_colors(true);
        }
        let mut reporter = Reporter::new(pretty);
        let output = reporter.render(&config_diagnostics);
        if !output.is_empty() {
            print!("{output}");
        }
        std::process::exit(1);
    }

    let base_dir = tsconfig_path
        .as_ref()
        .and_then(|p| p.parent())
        .unwrap_or(cwd);

    let compiler_options_map =
        show_config::build_compiler_options_map(config.as_ref(), args, base_dir);
    let (file_paths, effective_exclude) = show_config::collect_files_and_excludes(
        args,
        config.as_ref(),
        base_dir,
        &compiler_options_map,
    );
    let output = show_config::render_output(
        &compiler_options_map,
        &file_paths,
        &effective_exclude,
        config.as_ref(),
        base_dir,
    );
    print!("{output}");
    Ok(())
}

fn handle_list_files_only(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    use tsz::checker::diagnostics::diagnostic_codes;
    use tsz_cli::config::{load_tsconfig_with_diagnostics, resolve_compiler_options};
    use tsz_cli::driver::apply_cli_overrides;
    use tsz_cli::fs::{FileDiscoveryOptions, discover_ts_files};

    if args.ignore_config && args.files.is_empty() {
        println!("Version {TSC_VERSION}");
        println!("{}", help::colorize_help(&help::render_help(TSC_VERSION)));
        std::process::exit(1);
    }

    let tsconfig_path = if args.ignore_config {
        None
    } else {
        args.project
            .as_ref()
            .map(|p| {
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
            .or_else(|| driver::find_tsconfig(cwd))
    };

    // Route through the diagnostic loader so TS5024 / TS5102 in the root
    // config or any base reached via `extends` surface as errors instead of
    // being silently coerced (matching tsc's `--listFilesOnly` exit-1
    // behavior).
    let (config, config_diagnostics) = if let Some(path) = tsconfig_path.as_ref() {
        let parsed = load_tsconfig_with_diagnostics(path)?;
        (Some(parsed.config), parsed.diagnostics)
    } else {
        (None, Vec::new())
    };
    if config_diagnostics.iter().any(|d| {
        d.code == diagnostic_codes::COMPILER_OPTION_REQUIRES_A_VALUE_OF_TYPE
            || d.code
                == diagnostic_codes::OPTION_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION
    }) {
        let pretty = args
            .pretty
            .unwrap_or_else(|| std::io::stdout().is_terminal());
        if args.pretty == Some(true) {
            Reporter::force_colors(true);
        }
        let mut reporter = Reporter::new(pretty);
        let output = reporter.render(&config_diagnostics);
        if !output.is_empty() {
            print!("{output}");
        }
        std::process::exit(1);
    }

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

    let files = discover_ts_files(&discovery)?;
    let files_from_config = args.files.is_empty()
        && config
            .as_ref()
            .and_then(|config| config.files.as_ref())
            .is_some();
    let unsupported_js_root_diagnostics =
        list_files_only_unsupported_js_root_diagnostics(&discovery, &files, files_from_config);
    if !unsupported_js_root_diagnostics.is_empty() {
        let pretty = args
            .pretty
            .unwrap_or_else(|| std::io::stdout().is_terminal());
        if args.pretty == Some(true) {
            Reporter::force_colors(true);
        }
        let mut reporter = Reporter::new(pretty);
        let output = reporter.render(&unsupported_js_root_diagnostics);
        if !output.is_empty() {
            print!("{output}");
        }
        std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
    }

    // Print lib files first (matching tsc --listFilesOnly order)
    if !resolved.checker.no_lib {
        for lib_file in &resolved.lib_files {
            println!("{}", lib_file.display());
        }
    }

    for file in files {
        println!("{}", file.display());
    }

    Ok(())
}

fn list_files_only_unsupported_js_root_diagnostics(
    discovery: &tsz_cli::fs::FileDiscoveryOptions,
    files: &[std::path::PathBuf],
    files_from_config: bool,
) -> Vec<tsz::checker::diagnostics::Diagnostic> {
    use tsz::checker::diagnostics::{
        Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    };
    use tsz_common::file_extensions::is_js_file;

    if discovery.allow_js || !discovery.files_explicitly_set {
        return Vec::new();
    }

    files
        .iter()
        .filter(|file| is_js_file(file))
        .map(|file| {
            let file_name = file.display().to_string();
            let mut diagnostic = Diagnostic::from_code(
                diagnostic_codes::FILE_IS_A_JAVASCRIPT_FILE_DID_YOU_MEAN_TO_ENABLE_THE_ALLOWJS_OPTION,
                "",
                0,
                0,
                &[&file_name],
            );
            diagnostic
                .related_information
                .push(DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Message,
                    code: diagnostic_codes::THE_FILE_IS_IN_THE_PROGRAM_BECAUSE,
                    file: String::new(),
                    start: 0,
                    length: 0,
                    message_text: "The file is in the program because:".to_string(),
                });
            diagnostic
                .related_information
                .push(DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Message,
                    code: if files_from_config {
                        diagnostic_codes::PART_OF_FILES_LIST_IN_TSCONFIG_JSON
                    } else {
                        diagnostic_codes::ROOT_FILE_SPECIFIED_FOR_COMPILATION
                    },
                    file: String::new(),
                    start: 0,
                    length: 0,
                    message_text: if files_from_config {
                        "Part of 'files' list in tsconfig.json".to_string()
                    } else {
                        "Root file specified for compilation".to_string()
                    },
                });
            diagnostic
        })
        .collect()
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
    use tsz_cli::build::get_build_info_path;

    let mut deleted_count = 0;

    for project in graph.projects() {
        // Use the same build-info path logic as the build/driver paths so that
        // `--clean` removes the file the build actually wrote. Previously this
        // always wrote next to the tsconfig, which missed the case where
        // `outDir` relocates the .tsbuildinfo file.
        let Some(buildinfo_path) = get_build_info_path(project) else {
            continue;
        };
        if buildinfo_path.exists() {
            fs::remove_file(&buildinfo_path)?;
            if verbose {
                println!("Deleted: {}", buildinfo_path.display());
            }
            deleted_count += 1;
        }

        // `ResolvedProject` already stores absolute out/declaration dirs
        // resolved against `root_dir`, so re-running `resolve_compiler_options`
        // only duplicates work and risks drifting from the build path.
        if let Some(ref out_dir) = project.out_dir
            && out_dir.exists()
        {
            fs::remove_dir_all(out_dir)?;
            if verbose {
                println!("Deleted: {}", out_dir.display());
            }
            deleted_count += 1;
        }

        if let Some(ref declaration_dir) = project.declaration_dir
            && declaration_dir.exists()
        {
            fs::remove_dir_all(declaration_dir)?;
            if verbose {
                println!("Deleted: {}", declaration_dir.display());
            }
            deleted_count += 1;
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

#[path = "tsz/show_config.rs"]
mod show_config;

#[cfg(test)]
#[path = "tsz/tests.rs"]
mod tests;
