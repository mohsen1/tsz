//! Default compile command: run the driver, write optional trace/diagnostics
//! output, render diagnostics, and exit with tsc's status code.
//!
//! This is the execution half of the entrypoint's parse → execute split. The
//! arguments handed here are already validated and normalized by
//! `select_command`, so this module only orchestrates the compile and its
//! side effects.

use anyhow::Result;
use rustc_hash::FxHashMap;
use std::io::IsTerminal;

use tsz::checker::diagnostics::DiagnosticCategory;
use tsz_cli::args::CliArgs;
use tsz_cli::{driver, reporter::Reporter};

use super::diagnostics_report::print_diagnostics;
use super::{EXIT_DIAGNOSTICS_OUTPUTS_GENERATED, EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED, EXIT_SUCCESS};

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

/// Compile `args` rooted at `cwd` and emit all of tsc's post-compile output
/// (trace file, file lists, diagnostics) before exiting with the matching
/// status code.
pub(crate) fn run_compile(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
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
    let result = match driver::compile(args, cwd) {
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
            let mut event_args = FxHashMap::default();
            event_args.insert(
                "fileCount".to_string(),
                serde_json::json!(result.files_read.len()),
            );
            event_args.insert(
                "errorCount".to_string(),
                serde_json::json!(result.diagnostics.len()),
            );
            event_args.insert(
                "emittedCount".to_string(),
                serde_json::json!(result.emitted_files.len()),
            );
            event_args
        });

        // Add per-file events for files read
        for file in &result.files_read {
            let mut event_args = FxHashMap::default();
            event_args.insert(
                "path".to_string(),
                serde_json::json!(file.display().to_string()),
            );
            tracer.instant_with_args("FileProcessed", categories::IO, event_args);
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

    // #14345 Stage-2 PROBE dump (temporary; gated by `TSZ_STAGE2_PROBE`).
    // Placed before every exit path in this function so the bins always flush.
    tsz_solver::stage2_probe_dump();

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
