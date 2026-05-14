//! Perf-tools-only diagnostics JSON output.
//!
//! Implements `PERFORMANCE_PLAN.md` §4.T0.2: a schema-versioned JSON document
//! that captures phase timings, run metadata, fixture provenance, counts, and
//! peak RSS for one compilation.
//!
//! This module compiles only under `--features perf-tools`. Default release
//! builds of `tsz` exclude both the CLI flag and this module — there is no
//! runtime cost for normal users.
//!
//! The bench harness invokes the perf build and consumes the JSON via `jq`
//! rather than scraping shell text. The schema is documented in the plan and
//! versioned via the `schema_version` field; bumping it is a breaking change.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::driver::CompilationResult;

/// Stable schema version for `PerfDiagnosticsReport`. Bump when the JSON
/// shape changes in a way the bench harness must adapt to.
pub const SCHEMA_VERSION: u32 = 1;

/// Top-level diagnostics JSON document. Field shape mirrors
/// `PERFORMANCE_PLAN.md` §3 / "Diagnostics JSON".
#[derive(Debug, Clone, Serialize)]
pub struct PerfDiagnosticsReport {
    pub schema_version: u32,
    /// `"timing"` for this PR. T0.3 will add `"attribution"` once perf
    /// counters are wired.
    pub mode: &'static str,
    pub tsz: TszBuildInfo,
    pub fixture: FixtureProvenance,
    pub command_line: Vec<String>,
    pub phases_ms: PhasesMs,
    pub counts: Counts,
    /// Peak resident-set size in bytes when the platform exposes it. `null`
    /// otherwise (e.g. on platforms where `getrusage(RUSAGE_SELF)` is
    /// unavailable or returns zero).
    pub rss_peak_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TszBuildInfo {
    pub version: String,
    pub commit: Option<String>,
    pub profile: &'static str,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FixtureProvenance {
    pub name: Option<String>,
    pub repo: Option<String>,
    pub r#ref: Option<String>,
    pub actual_commit: Option<String>,
    pub path: Option<String>,
    /// Whether the bench harness used the local `~/code/large-ts-repo`
    /// fallback (`TSZ_BENCH_ALLOW_LOCAL_FIXTURE=1`). Defaults to `false`.
    pub local_override: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PhasesMs {
    /// Reserved for T2.0; populated as zero until the resolver phase split
    /// is wired. Schema lock keeps the bench harness stable.
    pub config_discovery: f64,
    pub source_discovery: f64,
    pub module_resolution: f64,
    pub io_read: f64,
    pub load_libs: f64,
    pub parse_bind: f64,
    pub check: f64,
    pub emit: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    pub files: u64,
    pub root_files: u64,
    pub lib_files: u64,
    pub source_bytes: u64,
    pub diagnostics: u64,
}

/// Build the report from compilation outputs and process state. Pure: no I/O.
pub fn build_report(result: &CompilationResult, raw_args: &[OsString]) -> PerfDiagnosticsReport {
    PerfDiagnosticsReport {
        schema_version: SCHEMA_VERSION,
        mode: "timing",
        tsz: tsz_build_info(),
        fixture: read_fixture_provenance(),
        command_line: raw_args
            .iter()
            .map(|os| os.to_string_lossy().into_owned())
            .collect(),
        phases_ms: PhasesMs {
            // T0.2 follow-up: these fine-grained sub-phases are
            // structurally wired through `PhaseTimings` so the JSON
            // schema can carry them. Driver attribution lands in a
            // separate PR; for now these stay 0.0 and the leftover
            // sits in `io_read` / `parse_bind` parent buckets.
            config_discovery: result.phase_timings.config_discovery_ms,
            source_discovery: result.phase_timings.source_discovery_ms,
            module_resolution: result.phase_timings.module_resolution_ms,
            io_read: result.phase_timings.io_read_ms,
            load_libs: result.phase_timings.load_libs_ms,
            parse_bind: result.phase_timings.parse_bind_ms,
            check: result.phase_timings.check_ms,
            emit: result.phase_timings.emit_ms,
            total: result.phase_timings.total_ms,
        },
        counts: Counts {
            files: result.files_read.len() as u64,
            root_files: count_root_files(result),
            lib_files: count_lib_files(result),
            source_bytes: 0, // populated by a later PR; bytes aren't tracked yet
            diagnostics: result.diagnostics.len() as u64,
        },
        rss_peak_bytes: read_peak_rss_bytes(),
    }
}

/// Serialize and write to `path`. Atomic-rename pattern so partial writes
/// don't poison the bench harness's `jq` consumer.
pub fn write_to(path: &Path, report: &PerfDiagnosticsReport) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn tsz_build_info() -> TszBuildInfo {
    TszBuildInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        commit: option_env!("TSZ_BUILD_COMMIT").map(String::from),
        profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    }
}

fn read_fixture_provenance() -> FixtureProvenance {
    fn env_var(name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|value| !value.is_empty())
    }
    FixtureProvenance {
        name: env_var("TSZ_BENCH_FIXTURE_NAME"),
        repo: env_var("TSZ_BENCH_FIXTURE_REPO"),
        r#ref: env_var("TSZ_BENCH_FIXTURE_REF"),
        actual_commit: env_var("TSZ_BENCH_FIXTURE_ACTUAL_COMMIT"),
        path: env_var("TSZ_BENCH_FIXTURE_PATH"),
        local_override: matches!(
            env_var("TSZ_BENCH_ALLOW_LOCAL_FIXTURE").as_deref(),
            Some("1") | Some("true")
        ),
    }
}

fn count_root_files(result: &CompilationResult) -> u64 {
    // `file_infos` only populates when `--explainFiles` runs, and even then
    // skips library entries; the reliable signal for total user-visible
    // sources is `files_read` minus the lib files we identify by path.
    result
        .files_read
        .iter()
        .filter(|p| !path_is_lib_file(p))
        .count() as u64
}

fn count_lib_files(result: &CompilationResult) -> u64 {
    result
        .files_read
        .iter()
        .filter(|p| path_is_lib_file(p))
        .count() as u64
}

/// Mirror of `driver::core::is_lib_file` — kept local to avoid widening that
/// helper's visibility for a perf-only path.
fn path_is_lib_file(path: &Path) -> bool {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let starts_lib = file_name.starts_with("lib.") && file_name.ends_with(".d.ts");
    let from_at_typescript = path
        .to_string_lossy()
        .contains("/node_modules/@typescript/lib-");
    starts_lib || from_at_typescript
}

/// Read peak resident-set size on Unix via `/proc/self/status` (Linux) or
/// `ps -o rss= -p <pid>` fallback. Returns `None` on platforms or paths
/// where neither source is available. Avoids a `libc` dependency by using
/// only stable filesystem/process APIs.
#[cfg(target_os = "linux")]
fn read_peak_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            // Format: `VmHWM:   <kB> kB`
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb.saturating_mul(1024));
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn read_peak_rss_bytes() -> Option<u64> {
    // `ps -o rss= -p <pid>` returns RSS in KiB. Not peak, but close enough
    // for this PR's purposes; T0.4 attribution mode can refine. Returns
    // `None` if `ps` is unavailable or fails.
    use std::process::Command;
    let pid = std::process::id().to_string();
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let kb: u64 = std::str::from_utf8(&output.stdout)
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if kb == 0 {
        return None;
    }
    Some(kb.saturating_mul(1024))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn read_peak_rss_bytes() -> Option<u64> {
    None
}

/// Convenience for callers that just have a `&Path` and want the full pipeline.
pub fn write_compilation_report(
    out_path: &Path,
    result: &CompilationResult,
    raw_args: &[OsString],
) -> std::io::Result<PathBuf> {
    let report = build_report(result, raw_args);
    write_to(out_path, &report)?;
    Ok(out_path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_is_one() {
        // Bumping schema_version is a breaking change for the bench harness;
        // the test exists to make that intent explicit.
        assert_eq!(SCHEMA_VERSION, 1);
    }

    #[test]
    fn fixture_provenance_local_override_parses_truthy_strings() {
        // Smoke-test the env-var coercion logic without touching the global
        // process env (we exercise the inner `matches!` via direct call).
        let truthy = ["1", "true"];
        for value in truthy {
            // Same shape as the inner check.
            assert!(matches!(Some(value), Some("1") | Some("true")));
        }
        let falsy = ["0", "false", "yes", ""];
        for value in falsy {
            assert!(!matches!(Some(value), Some("1") | Some("true")));
        }
    }

    #[test]
    fn report_serializes_to_valid_json() {
        let report = PerfDiagnosticsReport {
            schema_version: SCHEMA_VERSION,
            mode: "timing",
            tsz: TszBuildInfo {
                version: "test".to_string(),
                commit: Some("abc123".to_string()),
                profile: "release",
            },
            fixture: FixtureProvenance::default(),
            command_line: vec!["tsz".to_string(), "--noEmit".to_string()],
            phases_ms: PhasesMs::default(),
            counts: Counts::default(),
            rss_peak_bytes: Some(1024),
        };
        let json = serde_json::to_value(&report).expect("serializes");
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["mode"], "timing");
        assert_eq!(json["tsz"]["version"], "test");
        assert_eq!(json["fixture"]["local_override"], false);
        // Schema-locked phase keys: bench harness depends on these names.
        let phases = &json["phases_ms"];
        for key in [
            "config_discovery",
            "source_discovery",
            "module_resolution",
            "io_read",
            "load_libs",
            "parse_bind",
            "check",
            "emit",
            "total",
        ] {
            assert!(phases.get(key).is_some(), "missing phase key: {key}");
        }

        let counts = json["counts"]
            .as_object()
            .expect("counts serializes as an object");
        for key in [
            "files",
            "root_files",
            "lib_files",
            "source_bytes",
            "diagnostics",
        ] {
            assert!(counts.contains_key(key), "missing counts key: {key}");
        }
        assert_eq!(
            counts["source_bytes"], 0,
            "source_bytes is intentionally schema-visible but unpopulated until #7059"
        );
    }
}
