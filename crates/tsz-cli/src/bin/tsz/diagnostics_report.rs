use std::path::PathBuf;
use std::time::Duration;

use tsz::checker::diagnostics::DiagnosticCategory;
use tsz::parallel::residency::{MemoryPressure, ResidencyBudget};

use super::driver;

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
    application_eval_cache_entries: usize,
    application_eval_cache_hits: u64,
    application_eval_cache_misses: u64,
    instantiation_cache_entries: usize,
    instantiation_cache_hits: u64,
    instantiation_cache_misses: u64,
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
    residency_retained_kb: f64,
    residency_pressure: &'static str,
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
        report.application_eval_cache_entries = qc.application_eval_cache_entries;
        report.application_eval_cache_hits = qc.application_eval_cache_hits;
        report.application_eval_cache_misses = qc.application_eval_cache_misses;
        report.instantiation_cache_entries = qc.instantiation_cache_entries;
        report.instantiation_cache_hits = qc.instantiation_cache_hits;
        report.instantiation_cache_misses = qc.instantiation_cache_misses;
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
        report.residency_retained_kb = rs.retained_file_state_bytes_est() as f64 / 1024.0;
        report.residency_pressure = residency_pressure_label(ResidencyBudget::default().assess(rs));
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
    // Measurement-only eval-materialization probe (#13250). Appended to the
    // same field so it renders directly under the perf-counter dump and only
    // when `TSZ_PERF_COUNTERS` is set (both helpers return "" otherwise).
    report
        .perf_counter_dump
        .push_str(&tsz_solver::observability::eval_materialization_probe_report());

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
        let app_eval_total =
            report.application_eval_cache_hits + report.application_eval_cache_misses;
        let app_eval_rate = if app_eval_total == 0 {
            0.0
        } else {
            report.application_eval_cache_hits as f64 * 100.0 / app_eval_total as f64
        };
        let _ = writeln!(
            out,
            "Application eval cache:        {} entries ({} hits, {} misses, {app_eval_rate:.1}%)",
            report.application_eval_cache_entries,
            report.application_eval_cache_hits,
            report.application_eval_cache_misses,
        );
        let inst_total = report.instantiation_cache_hits + report.instantiation_cache_misses;
        let inst_rate = if inst_total == 0 {
            0.0
        } else {
            report.instantiation_cache_hits as f64 * 100.0 / inst_total as f64
        };
        let _ = writeln!(
            out,
            "Instantiation cache:           {} entries ({} hits, {} misses, {inst_rate:.1}%)",
            report.instantiation_cache_entries,
            report.instantiation_cache_hits,
            report.instantiation_cache_misses,
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
        let _ = writeln!(
            out,
            "Retained file state:           {:.1}K",
            report.residency_retained_kb
        );
        let _ = writeln!(
            out,
            "Retained residency pressure:   {}",
            report.residency_pressure
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

const fn residency_pressure_label(pressure: MemoryPressure) -> &'static str {
    match pressure {
        MemoryPressure::Low => "low",
        MemoryPressure::Medium => "medium",
        MemoryPressure::High => "high",
    }
}

/// Thin orchestrator: collect data, render, then emit to stdout.
pub(super) fn print_diagnostics(
    result: &driver::CompilationResult,
    elapsed: Duration,
    extended: bool,
) {
    let file_lines = collect_file_lines(&result.files_read);
    let memory_used_kb = if extended { get_memory_usage_kb() } else { 0 };
    let report = build_diagnostics_report(result, file_lines, elapsed, memory_used_kb, extended);
    print!("{}", render_diagnostics_report(&report, extended));
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
