#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use anyhow::{Context, Result};
use clap::Parser;
use std::io::IsTerminal;

use tsz::parallel::residency::{MemoryPressure, ResidencyBudget};
use tsz_cli::args::CliArgs;
use tsz_cli::help::{self, TSC_VERSION};
use tsz_cli::{driver, locale, reporter::Reporter, watch};
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

use arg_preprocess::{EarlyExit, PreprocessOutcome, preprocess_args};
use clap_errors::handle_clap_error;

/// tsc exit status codes (matching TypeScript's `ExitStatus` enum). Shared with
/// the `run` execution submodule, so they are crate-visible rather than private.
pub(crate) const EXIT_SUCCESS: i32 = 0;
pub(crate) const EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED: i32 = 1;
pub(crate) const EXIT_DIAGNOSTICS_OUTPUTS_GENERATED: i32 = 2;
const TS5112_COMMAND_LINE_FILES_MESSAGE: &str = "tsconfig.json is present but will not be loaded if files are specified on commandline. Use '--ignoreConfig' to skip this error.";

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

    let (preprocessed, batch_residency_budget) = extract_batch_residency_budget_arg(preprocessed);
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
            .spawn(move || actual_main(args, cwd, batch_residency_budget))
            .expect("failed to spawn main thread")
            .join()
            .expect("main thread panicked")
    } else {
        actual_main(args, cwd, batch_residency_budget)
    }
}

fn actual_main(
    mut args: CliArgs,
    cwd: std::path::PathBuf,
    batch_residency_budget: bool,
) -> Result<()> {
    validate_locale_or_exit(&args);

    // Initialize locale for i18n message translation
    locale::init_locale(args.locale.as_deref());

    match select_command(&mut args, &cwd, batch_residency_budget) {
        Command::Batch { residency_budget } => run_batch_mode(residency_budget),
        Command::Init => init::handle_init(&args, &cwd),
        Command::ShowConfig => handle_show_config(&args, &cwd),
        Command::ListFilesOnly => handle_list_files_only(&args, &cwd),
        Command::Build => handle_build(&args, &cwd),
        Command::Watch => watch::run(&args, &cwd),
        Command::Compile => run::run_compile(&args, &cwd),
    }
}

/// Print the version banner and full help text, then exit 1 — tsc's behavior
/// when a command line resolves to no compilation input.
fn print_no_input_help_and_exit() -> ! {
    println!("Version {TSC_VERSION}");
    println!("{}", help::colorize_help(&help::render_help(TSC_VERSION)));
    std::process::exit(1);
}

/// Reject a malformed `--locale` value the way tsc does, before any locale
/// initialization or command dispatch happens.
fn validate_locale_or_exit(args: &CliArgs) {
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
}

/// The top-level action chosen from parsed CLI arguments. Modeling the
/// dispatch decision as a value keeps `actual_main` readable as orchestration
/// and separates the (process-exiting) argument validation that runs while the
/// command is selected from the code that executes it.
enum Command {
    Batch { residency_budget: bool },
    Init,
    ShowConfig,
    ListFilesOnly,
    Build,
    Watch,
    Compile,
}

/// Resolve which command to run from the parsed arguments, performing the same
/// ordered validation and normalization tsc does. Invalid argument
/// combinations terminate the process here with tsc's exit codes; otherwise the
/// returned `Command` names the action to execute. `args` may be normalized in
/// place (promoting a lone directory positional to `--project`, merging
/// output-only tsconfig options) before the compile command is returned.
fn select_command(
    args: &mut CliArgs,
    cwd: &std::path::Path,
    batch_residency_budget: bool,
) -> Command {
    // Handle --batch: enter batch compilation mode
    if args.batch {
        return Command::Batch {
            residency_budget: batch_residency_budget,
        };
    }

    // Handle --init: create tsconfig.json
    if args.init {
        return Command::Init;
    }

    reject_tsconfig_only_cli_options(args);
    reject_build_only_cli_options(args);

    // Handle --showConfig: print resolved configuration
    if args.show_config {
        return Command::ShowConfig;
    }

    if should_report_ts5112_for_command_line_files(args, cwd) {
        println!("error TS5112: {TS5112_COMMAND_LINE_FILES_MESSAGE}");
        std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
    }

    // `--listFilesOnly` still uses the normal no-input command-line behavior before
    // the file-list-only path can print default libs. Use walk-up discovery to
    // match tsc: a tsconfig.json in any ancestor directory counts as "has input".
    if args.list_files_only
        && args.files.is_empty()
        && args.project.is_none()
        && driver::find_tsconfig(cwd).is_none()
    {
        print_no_input_help_and_exit();
    }

    // Handle --listFilesOnly: print file list and exit
    if args.list_files_only {
        return Command::ListFilesOnly;
    }

    // Handle --build mode
    if args.build {
        return Command::Build;
    }

    if args.watch {
        return Command::Watch;
    }

    // No-input behavior: if no files given, no --project, and no tsconfig.json
    // can be discovered from cwd or an ancestor, print version + help and exit
    // 1 (matching tsc v6 behavior).
    if args.files.is_empty() && args.project.is_none() && driver::find_tsconfig(cwd).is_none() {
        print_no_input_help_and_exit();
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
    merge_output_only_options_from_tsconfig(args, cwd);

    Command::Compile
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

/// Pull the internal batch residency probe out before clap sees it.
///
/// The flag is part of the batch worker protocol, not tsconfig/compiler-option
/// compatibility. Only strip it when `--batch` is present; otherwise the normal
/// parser keeps reporting it as an unknown option like `tsc` would for any
/// unsupported compiler option.
fn extract_batch_residency_budget_arg(
    args: Vec<std::ffi::OsString>,
) -> (Vec<std::ffi::OsString>, bool) {
    let is_batch = args
        .iter()
        .skip(1)
        .any(|arg| arg.to_string_lossy() == "--batch");
    if !is_batch {
        return (args, false);
    }

    let mut normalized = Vec::with_capacity(args.len());
    let mut residency_budget = false;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].to_string_lossy();
        let flag = arg.as_ref();
        let mut matched = false;
        let mut value = None;
        for spelling in ["--batchResidencyBudget", "--batch-residency-budget"] {
            if flag == spelling {
                matched = true;
                break;
            }
            if let Some(raw_value) = flag.strip_prefix(&format!("{spelling}=")) {
                matched = true;
                value = Some(raw_value.to_ascii_lowercase());
                break;
            }
        }

        if matched {
            let next_value = args
                .get(i + 1)
                .map(|arg| arg.to_string_lossy().to_ascii_lowercase());
            match value.as_deref().or(next_value.as_deref()) {
                Some("false") => residency_budget = false,
                _ => residency_budget = true,
            }
            if value.is_none() && matches!(next_value.as_deref(), Some("true" | "false")) {
                i += 1;
            }
        } else {
            normalized.push(args[i].clone());
        }
        i += 1;
    }

    (normalized, residency_budget)
}

fn clear_batch_iteration_state() {
    tsz_solver::construction::clear_thread_local_cache();
    tsz_solver::relations::subtype::reset_subtype_thread_local_state();
    tsz::checker::clear_all_thread_local_state();
    // Drop the resolver's thread-local filesystem-existence caches too. They
    // live in a `thread_local!` (not on the resolver instance), so a fresh
    // resolver per compilation never clears them; without this a later
    // compilation on the reused worker could read a stale `is_file`/`is_dir`
    // answer for a path whose on-disk state changed between compilations
    // (emit-then-recheck, watch rebuild, reused temp path). Same worker-reuse
    // isolation contract as the three resets above (#13368 / #13255 family).
    tsz::module_resolver::reset_path_existence_caches();
}

fn write_batch_residency_report(
    stdout: &mut impl std::io::Write,
    result: &driver::CompilationResult,
) -> Result<Option<MemoryPressure>> {
    let Some(stats) = result.residency_stats.as_ref() else {
        return Ok(None);
    };
    let retained_kb = stats.retained_file_state_bytes_est() as f64 / 1024.0;
    let pressure = ResidencyBudget::default().assess(stats);
    let pressure_label = match pressure {
        MemoryPressure::Low => "low",
        MemoryPressure::Medium => "medium",
        MemoryPressure::High => "high",
    };
    let eviction_savings_kb = ResidencyBudget::eviction_savings(stats) as f64 / 1024.0;
    writeln!(stdout, "Batch retained file state:     {retained_kb:.1}K")?;
    writeln!(
        stdout,
        "Batch retained residency pressure: {pressure_label}"
    )?;
    writeln!(
        stdout,
        "Batch estimated eviction savings: {eviction_savings_kb:.1}K"
    )?;
    let cleanup_action = if batch_residency_should_clear(pressure) {
        "clear finished-project thread-local state"
    } else {
        "observe only"
    };
    writeln!(stdout, "Batch residency cleanup action: {cleanup_action}")?;
    Ok(Some(pressure))
}

const fn batch_residency_should_clear(pressure: MemoryPressure) -> bool {
    matches!(pressure, MemoryPressure::Medium | MemoryPressure::High)
}

/// Batch compilation mode: read project directory paths from stdin (one per line),
/// compile each with `--project <path> --noEmit --pretty false`, print diagnostics,
/// then print a sentinel line so the caller can demarcate output boundaries.
///
/// Each iteration creates fresh `CliArgs` — no state is shared between compilations.
/// If tsz panics during any compilation, the process exits naturally (no `catch_unwind`).
/// The pool manager detects EOF on stdout and respawns a fresh worker.
fn run_batch_mode(residency_budget: bool) -> Result<()> {
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
        clear_batch_iteration_state();

        let project_path = std::path::Path::new(project_dir);

        // Build args matching what the conformance runner passes per test
        let mut batch_args_raw = vec![
            "tsz",
            "--project",
            project_dir,
            "--noEmit",
            "--pretty",
            "false",
        ];
        if residency_budget {
            batch_args_raw.push("--extendedDiagnostics");
        }
        let batch_args = CliArgs::parse_from(batch_args_raw);

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
        let compile_result = driver::compile(&batch_args, project_path);
        match compile_result {
            Ok(result) => {
                if !result.diagnostics.is_empty() {
                    let mut reporter = Reporter::new(false);
                    reporter.set_cwd(project_path);
                    let output = reporter.render(&result.diagnostics);
                    if !output.is_empty() {
                        write!(stdout, "{output}")?;
                    }
                }
                if residency_budget {
                    let pressure = write_batch_residency_report(&mut stdout, &result)?;
                    drop(result);
                    if pressure.is_some_and(batch_residency_should_clear) {
                        clear_batch_iteration_state();
                    }
                }
            }
            Err(e) => {
                // Print the error so the runner can see it, but don't exit
                writeln!(stdout, "error: {e}")?;
                if residency_budget {
                    clear_batch_iteration_state();
                }
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
        print_no_input_help_and_exit();
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
    use tsz::checker::diagnostics::{Diagnostic, diagnostic_codes};
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
                .push(Diagnostic::related_message(
                    diagnostic_codes::THE_FILE_IS_IN_THE_PROGRAM_BECAUSE,
                    String::new(),
                    0,
                    0,
                    "The file is in the program because:",
                ));
            let (code, message): (u32, &str) = if files_from_config {
                (
                    diagnostic_codes::PART_OF_FILES_LIST_IN_TSCONFIG_JSON,
                    "Part of 'files' list in tsconfig.json",
                )
            } else {
                (
                    diagnostic_codes::ROOT_FILE_SPECIFIED_FOR_COMPILATION,
                    "Root file specified for compilation",
                )
            };
            diagnostic
                .related_information
                .push(Diagnostic::related_message(code, String::new(), 0, 0, message));
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

#[path = "tsz/arg_preprocess.rs"]
mod arg_preprocess;

#[path = "tsz/clap_errors.rs"]
mod clap_errors;

#[path = "tsz/diagnostics_report.rs"]
mod diagnostics_report;

#[path = "tsz/init.rs"]
mod init;

#[path = "tsz/run.rs"]
mod run;

#[cfg(test)]
use arg_preprocess::split_response_line;

#[cfg(test)]
use init::{collect_init_overrides, render_init_template};

#[cfg(test)]
#[path = "tsz/tests.rs"]
mod tests;
