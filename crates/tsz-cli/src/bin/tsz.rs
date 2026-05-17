#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use rustc_hash::FxHashMap;
use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::{Component, Path, PathBuf};
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

    if use_large_stack_thread {
        // Initialize Rayon before any driver path can accidentally create the
        // default pool with platform-default worker stacks.
        tsz::parallel::ensure_rayon_global_pool();
    }

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
    // the file-list-only path can print default libs.
    if args.list_files_only
        && args.files.is_empty()
        && args.project.is_none()
        && !cwd.join("tsconfig.json").exists()
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
        tsz_solver::clear_thread_local_cache();
        tsz_solver::reset_subtype_thread_local_state();
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

        // PERF: dump perf counters when TSZ_PERF_COUNTERS is set. Empty
        // string when the env var isn't present, so the noisy counter dump
        // only appears in profiling runs. See
        // `docs/plan/PERFORMANCE_PLAN.md`.
        let counter_dump = tsz_common::perf_counters::PerfCounters::dump_string();
        if !counter_dump.is_empty() {
            print!("{counter_dump}");
        }
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
    use tsz_cli::config::{load_tsconfig_with_diagnostics, resolve_compiler_options};
    use tsz_cli::fs::{FileDiscoveryOptions, discover_ts_files};

    // Resolve a tsconfig path for `--showConfig`. We track whether the path
    // was found by walking up the filesystem so the TS5112 "implicit
    // tsconfig + explicit files" check below knows to fire only on
    // walk-up discoveries (an explicit `--project` path is the user opting
    // into a specific config, and tsc keeps loading it in that case).
    let (tsconfig_path, discovered_via_walkup) = if let Some(p) = args.project.as_ref() {
        // Canonicalize relative paths by joining with cwd first,
        // so that p.parent() later returns a valid directory.
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

    // tsc parity: when files are passed on the command line and a
    // `tsconfig.json` was discovered by walking up the filesystem (i.e. the
    // user did not opt in via `--project`), tsc refuses to silently load
    // that config and emits TS5112. The user can suppress with
    // `--ignoreConfig`. Without this check, tsz would inherit options from
    // an unrelated parent project for what the user expected to be a
    // single-file synthesis.
    if discovered_via_walkup && tsconfig_path.is_some() && !args.files.is_empty() {
        println!(
            "error TS5112: tsconfig.json is present but will not be loaded if files are specified on commandline. Use '--ignoreConfig' to skip this error."
        );
        std::process::exit(1);
    }

    // tsc parity: with no files passed and no tsconfig resolved (either
    // because `--ignoreConfig` was set or because none was found by walking
    // up), tsc emits TS5081 even if other CLI options were provided. The
    // synthesized output requires either a `tsconfig.json` to anchor the
    // project or an explicit root-file list.
    if tsconfig_path.is_none() && args.files.is_empty() {
        println!(
            "error TS5081: Cannot find a tsconfig.json file at the current directory: {}.",
            cwd.display()
        );
        std::process::exit(1);
    }

    // When the resolved tsconfig path does not exist on disk, emit the
    // appropriate tsc error code. This now only fires for explicit
    // `--project` paths because `find_tsconfig` returns `Some` only for an
    // existing file; the `args.project.is_none()` branch is kept as a
    // defensive fallback in case of a TOCTOU race between discovery and
    // the existence check.
    //   TS5057 – `--project` dir exists but has no tsconfig.json
    //   TS5058 – `--project` path does not exist at all
    //   TS5081 – defensive fallback (walk-up race)
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

    // Issue 2: load_tsconfig_with_diagnostics already resolves extends chains
    // and validates compiler-option types (TS5024) on every file in the chain,
    // so the returned config is the fully merged, validated result. Surfacing
    // the config diagnostics keeps `--showConfig` parity with tsc, which exits
    // 1 when an invalid option is present in the root config or any base.
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

    show_config_relativize_resolved_path_options(&mut compiler_options_map, base_dir);

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
    let allow_js = resolved.as_ref().is_some_and(|r| r.allow_js) || args.allow_js || args.check_js;
    let out_dir = args
        .out_dir
        .clone()
        .or_else(|| resolved.as_ref().and_then(|r| r.out_dir.clone()));

    // Discover resolved file list
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

    // `--showConfig` should print the resolved config without validating root
    // files (matching `tsc --showConfig`). When `files` is explicitly set,
    // preserve every entry verbatim — even unsupported extensions or missing
    // paths — and normalize with a `./` prefix. The TS6053/TS6054/TS18003
    // diagnostics belong to normal compilation, not to the config display.
    let file_paths: Vec<String> = if files_explicitly_set {
        explicit_files
            .iter()
            .map(|p| show_config_normalize_relative(base_dir, p))
            .collect()
    } else {
        // No explicit `files` — fall back to glob-based discovery so the
        // displayed list reflects what include/exclude would resolve. Treat
        // discovery errors and empty results as "nothing to print" instead of
        // TS18003; tsc's --showConfig also exits 0 in that case.
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
        discover_ts_files(&discovery)
            .unwrap_or_default()
            .iter()
            .map(|f| {
                if let Ok(rel) = f.strip_prefix(base_dir) {
                    format!("./{}", rel.display())
                } else {
                    f.display().to_string()
                }
            })
            .collect()
    };

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
                let display = show_config_display_selector(base_dir, v);
                output.push_str(&format!("        \"{display}\""));
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
                let display = show_config_display_selector(base_dir, v);
                output.push_str(&format!("        \"{display}\""));
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

fn show_config_display_selector(base_dir: &Path, selector: &str) -> String {
    let path = Path::new(selector);
    if path.is_absolute() {
        show_config_display_path(base_dir, path)
    } else {
        selector.to_string()
    }
}

fn show_config_display_path(base_dir: &Path, path: &Path) -> String {
    let relative = if path.is_absolute() {
        diff_paths(path, base_dir).unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    path_to_show_config_string(&relative)
}

/// Normalize a path for the --showConfig `files` array: strip the tsconfig
/// base directory if the input is absolute, convert backslashes to forward
/// slashes, and prepend `./` for plain relative paths so the output mirrors
/// `tsc --showConfig` (`["./style.css"]` rather than `["style.css"]`).
fn show_config_normalize_relative(base_dir: &std::path::Path, path: &std::path::Path) -> String {
    let rel = if path.is_absolute() {
        path.strip_prefix(base_dir)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    let s = rel.display().to_string().replace('\\', "/");
    if s.starts_with("./") || s.starts_with("../") || s.starts_with('/') {
        s
    } else {
        format!("./{s}")
    }
}

fn show_config_relativize_resolved_path_options(
    options: &mut serde_json::Map<String, serde_json::Value>,
    base_dir: &Path,
) {
    if let Some(serde_json::Value::String(base_url)) = options.get_mut("baseUrl") {
        *base_url = show_config_format_path_option(base_url, base_dir);
    }

    if let Some(serde_json::Value::Array(root_dirs)) = options.get_mut("rootDirs") {
        for root_dir in root_dirs {
            if let serde_json::Value::String(path) = root_dir {
                *path = show_config_format_path_option(path, base_dir);
            }
        }
    }
}

fn show_config_format_path_option(path: &str, base_dir: &Path) -> String {
    let path_obj = Path::new(path);
    if !path_obj.is_absolute() {
        return path.to_string();
    }

    let canonical_base = base_dir
        .canonicalize()
        .unwrap_or_else(|_| base_dir.to_path_buf());
    let canonical_path = path_obj
        .canonicalize()
        .unwrap_or_else(|_| path_obj.to_path_buf());
    let relative = diff_paths(&canonical_path, &canonical_base)
        .unwrap_or_else(|| canonical_path.to_path_buf());
    path_to_show_config_string(&relative)
}

fn path_to_show_config_string(path: &Path) -> String {
    let display = path.to_string_lossy().replace('\\', "/");
    if display.is_empty() || display == "." {
        "./".to_string()
    } else if display.starts_with("../") || display.starts_with('/') {
        display
    } else {
        format!("./{display}")
    }
}

fn diff_paths(path: &Path, base: &Path) -> Option<PathBuf> {
    let path_components: Vec<Component<'_>> = path.components().collect();
    let base_components: Vec<Component<'_>> = base.components().collect();
    let common_len = path_components
        .iter()
        .zip(base_components.iter())
        .take_while(|(a, b)| a == b)
        .count();
    if common_len == 0 && path.is_absolute() && base.is_absolute() {
        return None;
    }

    let mut result = PathBuf::new();
    for _ in common_len..base_components.len() {
        result.push("..");
    }
    for component in &path_components[common_len..] {
        result.push(component);
    }
    Some(result)
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
    if let Some(ref v) = opts.root_dirs {
        map.insert(
            "rootDirs".into(),
            Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()),
        );
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
    if let Some(ref v) = opts.ignore_deprecations {
        map.insert("ignoreDeprecations".into(), Value::String(v.clone()));
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
    set_bool!(no_check, "noCheck");
    set_bool!(no_emit_on_error, "noEmitOnError");
    set_bool!(declaration, "declaration");
    set_bool!(emit_declaration_only, "emitDeclarationOnly");
    set_bool!(source_map, "sourceMap");
    set_bool!(inline_source_map, "inlineSourceMap");
    set_bool!(declaration_map, "declarationMap");
    set_bool!(composite, "composite");
    set_bool!(incremental, "incremental");
    set_bool!(isolated_modules, "isolatedModules");
    set_bool!(isolated_declarations, "isolatedDeclarations");
    set_bool!(verbatim_module_syntax, "verbatimModuleSyntax");
    set_bool!(es_module_interop, "esModuleInterop");
    set_bool!(
        allow_synthetic_default_imports,
        "allowSyntheticDefaultImports"
    );
    set_bool!(allow_js, "allowJs");
    set_bool!(check_js, "checkJs");
    set_bool!(skip_lib_check, "skipLibCheck");
    set_bool!(skip_default_lib_check, "skipDefaultLibCheck");
    set_bool!(strip_internal, "stripInternal");
    set_bool!(no_lib, "noLib");
    set_bool!(lib_replacement, "libReplacement");
    set_bool!(no_types_and_symbols, "noTypesAndSymbols");
    set_bool!(import_helpers, "importHelpers");
    set_bool!(no_emit_helpers, "noEmitHelpers");
    set_bool!(remove_comments, "removeComments");
    set_bool!(emit_bom, "emitBOM");
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
    set_bool!(exact_optional_property_types, "exactOptionalPropertyTypes");
    set_bool!(strict_bind_call_apply, "strictBindCallApply");
    set_bool!(
        strict_builtin_iterator_return,
        "strictBuiltinIteratorReturn"
    );
    set_bool!(no_unchecked_indexed_access, "noUncheckedIndexedAccess");
    set_bool!(
        no_property_access_from_index_signature,
        "noPropertyAccessFromIndexSignature"
    );
    set_bool!(no_unused_locals, "noUnusedLocals");
    set_bool!(no_unused_parameters, "noUnusedParameters");
    set_bool!(allow_unreachable_code, "allowUnreachableCode");
    set_bool!(allow_unused_labels, "allowUnusedLabels");
    set_bool!(no_fallthrough_cases_in_switch, "noFallthroughCasesInSwitch");
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
    set_bool!(allow_umd_global_access, "allowUmdGlobalAccess");
    set_bool!(resolve_package_json_exports, "resolvePackageJsonExports");
    set_bool!(resolve_package_json_imports, "resolvePackageJsonImports");
    set_bool!(resolve_json_module, "resolveJsonModule");
    set_bool!(allow_arbitrary_extensions, "allowArbitraryExtensions");
    set_bool!(allow_importing_ts_extensions, "allowImportingTsExtensions");
    set_bool!(
        rewrite_relative_import_extensions,
        "rewriteRelativeImportExtensions"
    );
    set_bool!(preserve_const_enums, "preserveConstEnums");
    set_bool!(erasable_syntax_only, "erasableSyntaxOnly");
    set_bool!(sound, "sound");

    if let Some(ref v) = opts.new_line {
        map.insert("newLine".into(), Value::String(v.to_lowercase()));
    }
    if let Some(v) = opts.max_node_module_js_depth {
        map.insert(
            "maxNodeModuleJsDepth".into(),
            Value::Number(serde_json::Number::from(v)),
        );
    }

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
            tsz_cli::args::Target::Es3 => "es3",
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
    if let Some(ref v) = args.ignore_deprecations {
        map.insert("ignoreDeprecations".into(), Value::String(v.clone()));
    }
    if args.ignore_config {
        map.insert("ignoreConfig".into(), Value::Bool(true));
    }

    // `--flag false` for plain `bool` flags is forwarded through this hidden
    // side-channel by `preprocess_args`. Explicit `false` must round-trip into
    // `--showConfig` output so a CLI override of a `tsconfig.json` `true` value
    // is visible to the caller, matching `tsc --showConfig --flag false`.
    let disabled_bool_flags: rustc_hash::FxHashSet<&str> = args
        .explicitly_disabled_bool_flags
        .iter()
        .map(String::as_str)
        .collect();

    macro_rules! set_if_true {
        ($f:ident, $k:expr) => {
            if args.$f {
                map.insert($k.into(), Value::Bool(true));
            } else if disabled_bool_flags.contains($k) {
                map.insert($k.into(), Value::Bool(false));
            }
        };
    }
    set_if_true!(strict, "strict");
    set_if_true!(no_emit, "noEmit");
    set_if_true!(no_check, "noCheck");
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

    // --- Helper: parse target string ---
    fn parse_target(s: &str) -> tsz::common::ScriptTarget {
        tsz::common::ScriptTarget::from_ts_str(s).unwrap_or(tsz::common::ScriptTarget::ES2025)
    }

    // --- Helper: compute module from target ---
    const fn compute_module(target: tsz::common::ScriptTarget) -> &'static str {
        match tsz_cli::config::default_module_kind_for_target(target, true) {
            // tsc still prints this computed default as "es6" in --showConfig.
            tsz::common::ModuleKind::ES2015 => "es6",
            module => module.as_ts_str(),
        }
    }

    // --- Helper: compute moduleResolution from module string ---
    fn compute_module_resolution(module_str: &str) -> &'static str {
        tsz::common::ModuleKind::from_ts_str(module_str)
            .map(tsz_cli::config::default_module_resolution_for_module)
            .unwrap_or(tsz_cli::config::ModuleResolutionKind::Bundler)
            .as_ts_str()
    }

    // --- Helper: compute moduleDetection from module string ---
    fn compute_module_detection(module_str: &str) -> &'static str {
        tsz::common::ModuleKind::from_ts_str(module_str)
            .map(tsz_cli::config::default_module_detection_for_module)
            .unwrap_or("auto")
    }

    // v6 defaults (empty config):
    // target=es2025(12), module=es2022, moduleResolution=bundler
    // esModuleInterop=true, allowSyntheticDefaultImports=true
    // useDefineForClassFields=true (es2025 >= ES2022)
    // strict sub-flags: false, declaration=false, incremental=false
    const DEFAULT_TARGET: tsz::common::ScriptTarget = tsz::common::ScriptTarget::ES2025;
    const DEFAULT_MODULE_RESOLUTION: &str = "bundler";
    const DEFAULT_MODULE_DETECTION: &str = "auto";

    // --- Compute effective values using user's config ---
    let user_target_str = map
        .get("target")
        .and_then(|v| v.as_str())
        .unwrap_or("es2025");
    let user_target = parse_target(user_target_str);

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
        let computed = user_target.supports_es2022();
        let default_val = DEFAULT_TARGET.supports_es2022(); // true
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
    // tsc 6.0 keeps `strict: true` compact in --showConfig output. Explicit
    // strict sub-options are already present in `map`, but the aggregate
    // `strict` flag no longer expands every sub-option here.

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
        // tsc 6.0 shows the empty-config default as true (bundler), but node16
        // and legacy resolution compute false while nodenext/bundler compute true.
        let eff_mr = map
            .get("moduleResolution")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_MODULE_RESOLUTION);
        let mr_implies_json = matches!(eff_mr.to_lowercase().as_str(), "nodenext" | "bundler");
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

#[cfg(test)]
#[path = "tsz/tests.rs"]
mod tests;
