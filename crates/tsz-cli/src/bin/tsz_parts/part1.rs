#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use anyhow::{Context, Result};

use clap::Parser;

use rustc_hash::FxHashMap;

use std::ffi::OsString;

use std::io::IsTerminal;

use std::path::{Path, PathBuf};

use tsz::checker::diagnostics::DiagnosticCategory;

use tsz_cli::args::CliArgs;

use tsz_cli::help::{self, TSC_VERSION};

use tsz_cli::{driver, locale, reporter::Reporter, watch};

use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

use arg_preprocess::{EarlyExit, PreprocessOutcome, preprocess_args};

use clap_errors::handle_clap_error;

use diagnostics_report::print_diagnostics;

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

    // Preprocessing is side-effect free: it either hands back normalized args
    // or an early-exit directive (help/version/--all/TS5023/TS6369) whose I/O
    // we own here. The entrypoint adds the trailing newline.
    let preprocessed = match preprocess_args(std::env::args_os().collect()) {
        PreprocessOutcome::Continue(args) => args,
        PreprocessOutcome::EarlyExit(EarlyExit { message, code }) => {
            println!("{message}");
            std::process::exit(code);
        }
    };

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
