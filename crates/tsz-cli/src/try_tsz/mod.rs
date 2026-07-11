use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use walkdir::{DirEntry, WalkDir};

use crate::args::CliArgs;
use crate::driver;
use tsz_common::diagnostics::{Diagnostic, DiagnosticCategory};

const SUCCESS: i32 = 0;
const MISMATCH: i32 = 1;
const SETUP_FAILURE: i32 = 2;
const DEFAULT_TSZ_TIMEOUT_SECS: u64 = 120;
const TSZ_PROGRESS_INTERVAL: Duration = Duration::from_secs(15);
const MAX_TSCONFIG_REPORT_FILES: usize = 12;
const MAX_TSCONFIG_REPORT_BYTES: usize = 32 * 1024;
const MAX_TSCONFIG_REPORT_TOTAL_BYTES: usize = 48 * 1024;
const TRY_TSZ_WORKER_CONFIG_DEPRECATION_ENV_KEY: &str = "TSZ_TRY_TSZ_WORKER";
const TSC_HELPER: &str = include_str!("tsc_diagnostics_helper.js");

#[derive(Parser, Debug)]
#[command(
    name = "try-tsz",
    about = "Compare tsz against this project's tsc without emitting files"
)]
pub struct TryTszArgs {
    /// Path to tsconfig.json or a directory containing it.
    #[arg(short = 'p', long = "project")]
    pub project: Option<PathBuf>,

    /// Discover and check every local tsconfig.json outside generated folders.
    #[arg(long)]
    pub all: bool,

    /// Write a machine-readable summary JSON file.
    #[arg(long, value_name = "PATH")]
    pub json: Option<PathBuf>,

    /// Hidden subprocess entrypoint used to isolate tsz crashes from try-tsz.
    #[arg(long = "try-tsz-worker", hide = true)]
    try_tsz_worker: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ComparableDiagnostic {
    pub file: Option<String>,
    pub start: Option<u32>,
    pub length: Option<u32>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub code: u32,
    pub category: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompilerRun {
    command: String,
    version: Option<String>,
    elapsed_ms: u128,
    exit_code: i32,
    diagnostics: Vec<ComparableDiagnostic>,
    raw_output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ResultState {
    MatchedClean,
    MatchedDiagnostics,
    Mismatch,
    TszCrash,
    TszTimeout,
    TszOom,
    SetupFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigReport {
    config: String,
    state: ResultState,
    metadata: ProjectMetadata,
    tsc: Option<CompilerRun>,
    tsz: Option<CompilerRun>,
    extra_tsz_diagnostics: Vec<ComparableDiagnostic>,
    missing_tsc_diagnostics: Vec<ComparableDiagnostic>,
    order_mismatches: usize,
    setup_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectMetadata {
    try_tsz_version: String,
    tsz_version: String,
    typescript_version: Option<String>,
    node_version: Option<String>,
    os: String,
    arch: String,
    package_manager: Option<String>,
    project_references: bool,
    file_count: usize,
    approx_loc: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SummaryReport {
    schema_version: u32,
    configs: Vec<ConfigReport>,
}

#[derive(Debug, Deserialize)]
struct TscHelperOutput {
    typescript_version: String,
    diagnostics: Vec<TscDiagnosticJson>,
}

#[derive(Debug, Deserialize)]
struct TscDiagnosticJson {
    file: Option<String>,
    start: Option<u32>,
    length: Option<u32>,
    line: Option<u32>,
    column: Option<u32>,
    code: u32,
    category: String,
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TszWorkerOutput {
    version: String,
    diagnostics: Vec<ComparableDiagnostic>,
}

enum TszRunOutcome {
    Completed(CompilerRun),
    Crashed { message: String, elapsed_ms: u128 },
    TimedOut { elapsed_ms: u128 },
    OomLike { message: String, elapsed_ms: u128 },
}

pub fn run(args: TryTszArgs, cwd: &Path) -> Result<i32> {
    if let Some(config) = args.try_tsz_worker.as_deref() {
        return run_tsz_worker(cwd, config);
    }

    println!("try-tsz checks your project locally. It will not upload source code.");
    println!();

    let configs = match discover_configs(cwd, args.project.as_deref(), args.all) {
        Ok(configs) => configs,
        Err(error) => {
            println!("{error:#}");
            return Ok(SETUP_FAILURE);
        }
    };

    let mut reports = Vec::new();
    for config in configs {
        reports.push(run_config(cwd, &config));
    }

    let summary = SummaryReport {
        schema_version: 1,
        configs: reports,
    };

    print_summary(&summary);

    if let Some(json_path) = args.json.as_deref() {
        write_json_report(json_path, &summary)?;
        println!("Wrote {}", json_path.display());
    }

    maybe_prepare_interactive_report(cwd, &summary)?;

    if summary
        .configs
        .iter()
        .any(|report| report.state == ResultState::SetupFailure)
    {
        Ok(SETUP_FAILURE)
    } else if summary.configs.iter().all(|report| {
        matches!(
            report.state,
            ResultState::MatchedClean | ResultState::MatchedDiagnostics
        )
    }) {
        Ok(SUCCESS)
    } else {
        Ok(MISMATCH)
    }
}

fn run_config(cwd: &Path, config: &Path) -> ConfigReport {
    let config_label = relative_path(cwd, config);
    println!("Found {config_label}");
    let project_root = config.parent().unwrap_or(cwd);
    let mut metadata = project_metadata(project_root, config, None);

    let tsc = match run_tsc(project_root, config) {
        Ok(run) => run,
        Err(error) => {
            return ConfigReport {
                config: config_label,
                state: ResultState::SetupFailure,
                metadata,
                tsc: None,
                tsz: None,
                extra_tsz_diagnostics: Vec::new(),
                missing_tsc_diagnostics: Vec::new(),
                order_mismatches: 0,
                setup_error: Some(error.to_string()),
            };
        }
    };
    metadata.typescript_version = tsc.version.clone();

    let tsz = match run_tsz(project_root, config) {
        Ok(TszRunOutcome::Completed(run)) => run,
        Ok(TszRunOutcome::Crashed {
            message,
            elapsed_ms,
        }) => {
            return ConfigReport {
                config: config_label,
                state: ResultState::TszCrash,
                metadata,
                tsc: Some(tsc),
                tsz: Some(CompilerRun {
                    command: format!("tsz --pretty false --noEmit -p {}", config.display()),
                    version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    elapsed_ms,
                    exit_code: 1,
                    diagnostics: Vec::new(),
                    raw_output: message.clone(),
                }),
                extra_tsz_diagnostics: Vec::new(),
                missing_tsc_diagnostics: Vec::new(),
                order_mismatches: 0,
                setup_error: Some(message),
            };
        }
        Ok(TszRunOutcome::TimedOut { elapsed_ms }) => {
            let timeout = tsz_timeout();
            let message = format!(
                "tsz exceeded the {}s timeout before producing diagnostics",
                timeout.as_secs()
            );
            return ConfigReport {
                config: config_label,
                state: ResultState::TszTimeout,
                metadata,
                tsc: Some(tsc),
                tsz: Some(CompilerRun {
                    command: format!("tsz --pretty false --noEmit -p {}", config.display()),
                    version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    elapsed_ms,
                    exit_code: 1,
                    diagnostics: Vec::new(),
                    raw_output: message.clone(),
                }),
                extra_tsz_diagnostics: Vec::new(),
                missing_tsc_diagnostics: Vec::new(),
                order_mismatches: 0,
                setup_error: Some(message),
            };
        }
        Ok(TszRunOutcome::OomLike {
            message,
            elapsed_ms,
        }) => {
            return ConfigReport {
                config: config_label,
                state: ResultState::TszOom,
                metadata,
                tsc: Some(tsc),
                tsz: Some(CompilerRun {
                    command: format!("tsz --pretty false --noEmit -p {}", config.display()),
                    version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    elapsed_ms,
                    exit_code: 1,
                    diagnostics: Vec::new(),
                    raw_output: message.clone(),
                }),
                extra_tsz_diagnostics: Vec::new(),
                missing_tsc_diagnostics: Vec::new(),
                order_mismatches: 0,
                setup_error: Some(message),
            };
        }
        Err(error) => {
            return ConfigReport {
                config: config_label,
                state: ResultState::SetupFailure,
                metadata,
                tsc: Some(tsc),
                tsz: None,
                extra_tsz_diagnostics: Vec::new(),
                missing_tsc_diagnostics: Vec::new(),
                order_mismatches: 0,
                setup_error: Some(error.to_string()),
            };
        }
    };

    let diff = diff_diagnostics(&tsc.diagnostics, &tsz.diagnostics);
    let state = if diff.extra_tsz.is_empty()
        && diff.missing_tsc.is_empty()
        && diff.order_mismatches == 0
        && tsc.exit_code == tsz.exit_code
    {
        if tsc.diagnostics.is_empty() {
            ResultState::MatchedClean
        } else {
            ResultState::MatchedDiagnostics
        }
    } else {
        ResultState::Mismatch
    };

    ConfigReport {
        config: config_label,
        state,
        metadata,
        tsc: Some(tsc),
        tsz: Some(tsz),
        extra_tsz_diagnostics: diff.extra_tsz,
        missing_tsc_diagnostics: diff.missing_tsc,
        order_mismatches: diff.order_mismatches,
        setup_error: None,
    }
}

fn run_tsc(cwd: &Path, config: &Path) -> Result<CompilerRun> {
    ensure_typescript_oracle(cwd, config)?;

    let command = format!("node <try-tsz tsc helper> {}", config.display());
    println!("Running tsc --noEmit -p {} ...", relative_path(cwd, config));
    let start = Instant::now();
    let output = Command::new("node")
        .arg("-e")
        .arg(TSC_HELPER)
        .arg(config)
        .current_dir(cwd)
        .output()
        .context("failed to run node for the TypeScript 7 oracle")?;
    let elapsed_ms = start.elapsed().as_millis();

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        bail!("failed to collect tsc diagnostics: {}", stderr.trim());
    }

    let helper: TscHelperOutput =
        serde_json::from_str(&stdout).context("failed to parse tsc diagnostics JSON")?;
    let typescript_version = helper.typescript_version;
    let diagnostics = helper
        .diagnostics
        .into_iter()
        .map(|diagnostic| normalize_tsc_diagnostic(cwd, diagnostic))
        .collect::<Vec<_>>();
    let exit_code = if diagnostics.iter().any(|diag| diag.category == "error") {
        2
    } else {
        0
    };

    Ok(CompilerRun {
        command,
        version: Some(typescript_version),
        elapsed_ms,
        exit_code,
        diagnostics,
        raw_output: stderr,
    })
}

fn run_tsz(cwd: &Path, config: &Path) -> Result<TszRunOutcome> {
    println!("Running tsz --noEmit -p {} ...", relative_path(cwd, config));
    let start = Instant::now();
    let timeout = tsz_timeout();
    let mut next_progress_at = TSZ_PROGRESS_INTERVAL;
    let worker_exe = std::env::current_exe().context("failed to locate try-tsz executable")?;
    let mut child = Command::new(worker_exe)
        .arg("--try-tsz-worker")
        .arg(config)
        .env(TRY_TSZ_WORKER_CONFIG_DEPRECATION_ENV_KEY, "1")
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn isolated tsz worker")?;

    loop {
        if child
            .try_wait()
            .context("failed to poll isolated tsz worker")?
            .is_some()
        {
            break;
        }
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            let _ = child.kill();
            let _output = child
                .wait_with_output()
                .context("failed to collect timed-out tsz worker output")?;
            return Ok(TszRunOutcome::TimedOut {
                elapsed_ms: elapsed.as_millis(),
            });
        }
        if elapsed >= next_progress_at {
            println!(
                "Still running tsz after {:.0}s (timeout at {}s) ...",
                elapsed.as_secs_f64(),
                timeout.as_secs()
            );
            next_progress_at += TSZ_PROGRESS_INTERVAL;
        }
        thread::sleep(Duration::from_millis(50));
    }

    let output = child
        .wait_with_output()
        .context("failed to collect isolated tsz worker output")?;
    let elapsed_ms = start.elapsed().as_millis();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        let message = worker_failure_message(output.status, &stderr);
        if status_looks_oom_like(output.status) {
            return Ok(TszRunOutcome::OomLike {
                message,
                elapsed_ms,
            });
        }
        return Ok(TszRunOutcome::Crashed {
            message,
            elapsed_ms,
        });
    }

    let worker: TszWorkerOutput =
        serde_json::from_str(&stdout).context("failed to parse isolated tsz worker JSON")?;
    let diagnostics = worker.diagnostics;
    let exit_code = if diagnostics.iter().any(|diag| diag.category == "error") {
        2
    } else {
        0
    };

    Ok(TszRunOutcome::Completed(CompilerRun {
        command: format!("tsz --pretty false --noEmit -p {}", config.display()),
        version: Some(worker.version),
        elapsed_ms,
        exit_code,
        diagnostics,
        raw_output: stderr,
    }))
}

fn tsz_timeout() -> Duration {
    tsz_timeout_from_env_value(std::env::var("TRY_TSZ_TIMEOUT_SECS").ok().as_deref())
}

fn tsz_timeout_from_env_value(value: Option<&str>) -> Duration {
    let seconds = value
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_TSZ_TIMEOUT_SECS);
    Duration::from_secs(seconds)
}

fn run_tsz_worker(cwd: &Path, config: &Path) -> Result<i32> {
    let argv = vec![
        "tsz".to_string(),
        "--pretty".to_string(),
        "false".to_string(),
        "--noEmit".to_string(),
        "-p".to_string(),
        config.to_string_lossy().into_owned(),
    ];
    let args = CliArgs::try_parse_from(argv).context("failed to build tsz worker invocation")?;
    let result = catch_unwind(AssertUnwindSafe(|| driver::compile(&args, cwd)));
    let result = match result {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            writeln!(
                io::stderr(),
                "tsz failed before producing diagnostics: {error:#}"
            )?;
            return Ok(101);
        }
        Err(payload) => {
            writeln!(
                io::stderr(),
                "tsz panicked: {}",
                panic_payload_message(payload.as_ref())
            )?;
            return Ok(102);
        }
    };
    let output = TszWorkerOutput {
        version: env!("CARGO_PKG_VERSION").to_string(),
        diagnostics: result
            .diagnostics
            .iter()
            .map(|diagnostic| normalize_tsz_diagnostic(cwd, diagnostic))
            .collect(),
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(0)
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "<non-string panic payload>".to_string()
}

fn worker_failure_message(status: ExitStatus, stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return format!("tsz worker exited with {status}");
    }
    format!("tsz worker exited with {status}: {trimmed}")
}

fn status_looks_oom_like(status: ExitStatus) -> bool {
    if matches!(status.code(), Some(137)) {
        return true;
    }
    status_signal(status).is_some_and(|signal| signal == 9)
}

#[cfg(unix)]
fn status_signal(status: ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn status_signal(_status: ExitStatus) -> Option<i32> {
    None
}

fn discover_configs(cwd: &Path, project: Option<&Path>, all: bool) -> Result<Vec<PathBuf>> {
    if let Some(project) = project {
        return Ok(vec![resolve_project_config(cwd, project)?]);
    }

    if all {
        let mut configs = WalkDir::new(cwd)
            .into_iter()
            .filter_entry(should_visit_entry)
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .filter(|entry| entry.file_name() == OsStr::new("tsconfig.json"))
            .map(|entry| entry.path().to_path_buf())
            .collect::<Vec<_>>();
        configs.sort();
        if configs.is_empty() {
            bail!("no tsconfig.json files found under {}", cwd.display());
        }
        return Ok(configs);
    }

    let mut dir = Some(cwd);
    while let Some(current) = dir {
        let candidate = current.join("tsconfig.json");
        if candidate.is_file() {
            return Ok(vec![candidate]);
        }
        dir = current.parent();
    }

    bail!(
        "no tsconfig.json found from {} or its parents",
        cwd.display()
    )
}

fn resolve_project_config(cwd: &Path, project: &Path) -> Result<PathBuf> {
    let absolute = if project.is_absolute() {
        project.to_path_buf()
    } else {
        cwd.join(project)
    };
    if absolute.is_dir() {
        let config = absolute.join("tsconfig.json");
        if config.is_file() {
            Ok(config)
        } else {
            bail!(
                "directory {} does not contain tsconfig.json",
                absolute.display()
            )
        }
    } else if absolute.is_file() {
        Ok(absolute)
    } else {
        bail!("project path does not exist: {}", absolute.display())
    }
}

fn should_visit_entry(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    !matches!(
        name.as_ref(),
        "node_modules" | "dist" | "build" | "coverage" | ".next" | ".turbo" | ".git" | "vendor"
    )
}

fn ensure_typescript_oracle(cwd: &Path, config: &Path) -> Result<()> {
    if std::env::var_os("TRY_TSZ_TYPESCRIPT_PACKAGE_JSON").is_some()
        || find_local_typescript_package_json(cwd, config).is_some()
    {
        Ok(())
    } else {
        bail!(
            "try-tsz needs TypeScript 7.0.2 or newer for the tsc oracle; the npm package provides it, or install TypeScript locally. Searched from {} and {} for {}",
            cwd.display(),
            config.parent().unwrap_or(cwd).display(),
            local_typescript_package_json_path().display()
        )
    }
}

fn find_local_typescript_package_json(cwd: &Path, config: &Path) -> Option<PathBuf> {
    let config_root = config.parent().unwrap_or(cwd);
    for root in [config_root, cwd] {
        let mut dir = Some(root);
        while let Some(current) = dir {
            let candidate = current.join(local_typescript_package_json_path());
            if candidate.exists() {
                return Some(candidate);
            }
            dir = current.parent();
        }
    }
    None
}

fn local_typescript_package_json_path() -> &'static Path {
    Path::new("node_modules/typescript/package.json")
}

fn project_metadata(
    project_root: &Path,
    config: &Path,
    typescript_version: Option<String>,
) -> ProjectMetadata {
    let (file_count, approx_loc) = project_stats(project_root);
    ProjectMetadata {
        try_tsz_version: env!("CARGO_PKG_VERSION").to_string(),
        tsz_version: env!("CARGO_PKG_VERSION").to_string(),
        typescript_version,
        node_version: node_version(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        package_manager: detect_package_manager(project_root),
        project_references: config_has_project_references(config),
        file_count,
        approx_loc,
    }
}

fn node_version() -> Option<String> {
    let output = Command::new("node").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn detect_package_manager(project_root: &Path) -> Option<String> {
    for (file, label) in [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("package-lock.json", "npm"),
        ("bun.lockb", "bun"),
        ("bun.lock", "bun"),
    ] {
        if project_root.join(file).exists() {
            return Some(label.to_string());
        }
    }
    None
}

fn config_has_project_references(config: &Path) -> bool {
    let Ok(text) = fs::read_to_string(config) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return text.contains("\"references\"");
    };
    value
        .get("references")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|references| !references.is_empty())
}

fn project_stats(project_root: &Path) -> (usize, usize) {
    let mut file_count = 0usize;
    let mut approx_loc = 0usize;
    for entry in WalkDir::new(project_root)
        .into_iter()
        .filter_entry(should_visit_entry)
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| is_typescript_family_file(entry.path()))
    {
        if let Ok(text) = fs::read_to_string(entry.path()) {
            file_count += 1;
            approx_loc += text.lines().count();
        }
    }
    (file_count, approx_loc)
}

fn is_typescript_family_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some("ts" | "tsx" | "mts" | "cts")
    )
}

fn normalize_tsc_diagnostic(cwd: &Path, diagnostic: TscDiagnosticJson) -> ComparableDiagnostic {
    let mut comparable = ComparableDiagnostic {
        file: diagnostic
            .file
            .map(|file| normalize_path_label(cwd, Path::new(&file))),
        start: diagnostic.start,
        length: diagnostic.length,
        line: diagnostic.line,
        column: diagnostic.column,
        code: diagnostic.code,
        category: diagnostic.category,
        message: diagnostic.message,
    };
    normalize_config_deprecation_location(&mut comparable);
    comparable
}

fn normalize_tsz_diagnostic(cwd: &Path, diagnostic: &Diagnostic) -> ComparableDiagnostic {
    let file = if diagnostic.file.is_empty() {
        None
    } else {
        Some(normalize_path_label(cwd, Path::new(&diagnostic.file)))
    };
    let (line, column) = file
        .as_deref()
        .and_then(|label| line_column_for_path_label(cwd, label, diagnostic.start))
        .unwrap_or((None, None));

    let mut comparable = ComparableDiagnostic {
        file,
        start: Some(diagnostic.start),
        length: Some(diagnostic.length),
        line,
        column,
        code: diagnostic.code,
        category: category_label(diagnostic.category).to_string(),
        message: diagnostic.message_text.clone(),
    };
    normalize_config_deprecation_location(&mut comparable);
    comparable
}

fn normalize_config_deprecation_location(diagnostic: &mut ComparableDiagnostic) {
    if matches!(diagnostic.code, 5101 | 5107) {
        diagnostic.file = None;
        diagnostic.start = None;
        diagnostic.length = None;
        diagnostic.line = None;
        diagnostic.column = None;
    }
}

const fn category_label(category: DiagnosticCategory) -> &'static str {
    match category {
        DiagnosticCategory::Warning => "warning",
        DiagnosticCategory::Error => "error",
        DiagnosticCategory::Suggestion => "suggestion",
        DiagnosticCategory::Message => "message",
    }
}

fn line_column_for_path_label(
    cwd: &Path,
    label: &str,
    start: u32,
) -> Option<(Option<u32>, Option<u32>)> {
    let path = cwd.join(label);
    let text = fs::read_to_string(path).ok()?;
    // 1-based line/column with UTF-16 columns, matching how tsc reports
    // diagnostic positions (these values are diffed against tsc output).
    let position = tsz_common::position::LineMap::build(&text).offset_to_position(start, &text);
    Some((
        Some(position.line.saturating_add(1)),
        Some(position.character.saturating_add(1)),
    ))
}

struct DiagnosticDiff {
    extra_tsz: Vec<ComparableDiagnostic>,
    missing_tsc: Vec<ComparableDiagnostic>,
    order_mismatches: usize,
}

fn diff_diagnostics(tsc: &[ComparableDiagnostic], tsz: &[ComparableDiagnostic]) -> DiagnosticDiff {
    let mut tsc_counts = diagnostic_counts(tsc);
    let mut extra_tsz = Vec::new();
    for diagnostic in tsz {
        let count = tsc_counts.entry(diagnostic).or_insert(0);
        if *count == 0 {
            extra_tsz.push(diagnostic.clone());
        } else {
            *count -= 1;
        }
    }

    let mut tsz_counts = diagnostic_counts(tsz);
    let mut missing_tsc = Vec::new();
    for diagnostic in tsc {
        let count = tsz_counts.entry(diagnostic).or_insert(0);
        if *count == 0 {
            missing_tsc.push(diagnostic.clone());
        } else {
            *count -= 1;
        }
    }

    let order_mismatches = if extra_tsz.is_empty() && missing_tsc.is_empty() {
        tsc.iter()
            .zip(tsz)
            .filter(|(left, right)| left != right)
            .count()
    } else {
        0
    };

    DiagnosticDiff {
        extra_tsz,
        missing_tsc,
        order_mismatches,
    }
}

fn diagnostic_counts(
    diagnostics: &[ComparableDiagnostic],
) -> BTreeMap<&ComparableDiagnostic, usize> {
    let mut counts = BTreeMap::new();
    for diagnostic in diagnostics {
        *counts.entry(diagnostic).or_insert(0) += 1;
    }
    counts
}

fn print_summary(summary: &SummaryReport) {
    for report in &summary.configs {
        println!();
        match report.state {
            ResultState::MatchedClean | ResultState::MatchedDiagnostics => {
                let Some(tsc) = report.tsc.as_ref() else {
                    continue;
                };
                let Some(tsz) = report.tsz.as_ref() else {
                    continue;
                };
                println!("Result: tsz matched tsc on this project.");
                println!("tsc: {:.2}s", millis_to_seconds(tsc.elapsed_ms));
                println!("tsz: {:.2}s", millis_to_seconds(tsz.elapsed_ms));
                if tsz.elapsed_ms > 0 {
                    println!(
                        "Speed: {:.1}x",
                        tsc.elapsed_ms as f64 / tsz.elapsed_ms as f64
                    );
                }
                println!();
                println!("tsz works for your project!");
            }
            ResultState::Mismatch => {
                let Some(tsc) = report.tsc.as_ref() else {
                    continue;
                };
                let Some(tsz) = report.tsz.as_ref() else {
                    continue;
                };
                println!("Result: mismatch");
                println!(
                    "tsc: {} diagnostics in {:.2}s",
                    tsc.diagnostics.len(),
                    millis_to_seconds(tsc.elapsed_ms)
                );
                println!(
                    "tsz: {} diagnostics in {:.2}s",
                    tsz.diagnostics.len(),
                    millis_to_seconds(tsz.elapsed_ms)
                );
                println!();
                println!("Differences:");
                println!(
                    "- {} extra tsz diagnostics",
                    report.extra_tsz_diagnostics.len()
                );
                println!(
                    "- {} missing tsc diagnostics",
                    report.missing_tsc_diagnostics.len()
                );
                println!("- {} reordered diagnostics", report.order_mismatches);
            }
            ResultState::TszCrash => {
                let Some(tsc) = report.tsc.as_ref() else {
                    continue;
                };
                println!("Result: tsz crash");
                println!(
                    "tsc: {} diagnostics in {:.2}s",
                    tsc.diagnostics.len(),
                    millis_to_seconds(tsc.elapsed_ms)
                );
                if let Some(tsz) = report.tsz.as_ref() {
                    println!(
                        "tsz: crashed after {:.2}s",
                        millis_to_seconds(tsz.elapsed_ms)
                    );
                }
                if let Some(error) = report.setup_error.as_deref() {
                    println!("{error}");
                }
            }
            ResultState::TszTimeout => {
                let Some(tsc) = report.tsc.as_ref() else {
                    continue;
                };
                println!("Result: tsz timeout");
                println!(
                    "tsc: {} diagnostics in {:.2}s",
                    tsc.diagnostics.len(),
                    millis_to_seconds(tsc.elapsed_ms)
                );
                if let Some(tsz) = report.tsz.as_ref() {
                    println!(
                        "tsz: timed out after {:.2}s",
                        millis_to_seconds(tsz.elapsed_ms)
                    );
                }
                if let Some(error) = report.setup_error.as_deref() {
                    println!("{error}");
                }
            }
            ResultState::TszOom => {
                let Some(tsc) = report.tsc.as_ref() else {
                    continue;
                };
                println!("Result: tsz killed");
                println!(
                    "tsc: {} diagnostics in {:.2}s",
                    tsc.diagnostics.len(),
                    millis_to_seconds(tsc.elapsed_ms)
                );
                if let Some(tsz) = report.tsz.as_ref() {
                    println!(
                        "tsz: killed after {:.2}s",
                        millis_to_seconds(tsz.elapsed_ms)
                    );
                }
                if let Some(error) = report.setup_error.as_deref() {
                    println!("{error}");
                }
            }
            ResultState::SetupFailure => {
                println!("Result: setup failure");
                if let Some(error) = report.setup_error.as_deref() {
                    println!("{error}");
                }
            }
        }
    }
}

fn millis_to_seconds(ms: u128) -> f64 {
    ms as f64 / 1000.0
}

fn maybe_prepare_interactive_report(cwd: &Path, summary: &SummaryReport) -> Result<()> {
    let has_reportable_failure = summary.configs.iter().any(|report| {
        matches!(
            report.state,
            ResultState::Mismatch
                | ResultState::TszCrash
                | ResultState::TszTimeout
                | ResultState::TszOom
                | ResultState::SetupFailure
        )
    });
    if !has_reportable_failure || !io::stdin().is_terminal() {
        return Ok(());
    }

    println!();
    if !confirm(
        "Prepare a local report? This may copy small snippets around failing spans. No files are uploaded. You can review everything before sharing. [y/N] ",
    )? {
        return Ok(());
    }

    let report_dir = cwd.join(".try-tsz");
    fs::create_dir_all(&report_dir)?;
    let summary_path = report_dir.join("summary.json");
    write_json_report(&summary_path, summary)?;
    let markdown = render_markdown_report(cwd, summary);
    let report_path = report_dir.join("report.md");
    fs::write(&report_path, markdown)?;
    write_raw_outputs(summary, &report_dir)?;
    write_tsconfig_context(cwd, summary, &report_dir)?;
    println!("Wrote {}", report_path.display());

    if confirm("Create snippet repro candidates for the first mismatches? [y/N] ")? {
        write_snippet_candidates(cwd, summary, &report_dir)?;
    }

    println!("Report: {}", report_path.display());

    if confirm("Submit this report to GitHub Discussions with gh? [y/N] ")? {
        submit_discussion_or_print_fallback(&report_path)?;
    }

    Ok(())
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "YES" | "Yes"))
}

fn write_json_report(path: &Path, summary: &SummaryReport) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(summary)? + "\n")?;
    Ok(())
}

fn render_markdown_report(cwd: &Path, summary: &SummaryReport) -> String {
    let mut out = String::new();
    out.push_str("# try-tsz report\n\n");
    for report in &summary.configs {
        out.push_str(&format!("## `{}`\n\n", report.config));
        out.push_str(&format!(
            "- State: `{}`\n",
            result_state_label(&report.state)
        ));
        if let Some(tsc) = report.tsc.as_ref() {
            out.push_str(&format!(
                "- tsc: {} diagnostics in {:.2}s\n",
                tsc.diagnostics.len(),
                millis_to_seconds(tsc.elapsed_ms)
            ));
        }
        out.push_str(&format!(
            "- Versions: try-tsz {}, tsz {}, TypeScript {}\n",
            report.metadata.try_tsz_version,
            report.metadata.tsz_version,
            report
                .metadata
                .typescript_version
                .as_deref()
                .unwrap_or("unknown")
        ));
        out.push_str(&format!(
            "- Project size: {} TypeScript-family files, ~{} LOC\n",
            report.metadata.file_count, report.metadata.approx_loc
        ));
        if report.metadata.project_references {
            out.push_str("- Project references: yes\n");
        }
        if let Some(tsz) = report.tsz.as_ref() {
            out.push_str(&format!(
                "- tsz: {} diagnostics in {:.2}s\n",
                tsz.diagnostics.len(),
                millis_to_seconds(tsz.elapsed_ms)
            ));
        }
        if let Some(error) = report.setup_error.as_deref() {
            out.push_str(&format!("- Failure: `{error}`\n"));
        }
        out.push('\n');
        push_diagnostic_section(
            &mut out,
            "Extra tsz diagnostics",
            &report.extra_tsz_diagnostics,
        );
        push_diagnostic_section(
            &mut out,
            "Missing tsc diagnostics",
            &report.missing_tsc_diagnostics,
        );
        push_tsconfig_section(cwd, &mut out, report);
    }
    out
}

const fn result_state_label(state: &ResultState) -> &'static str {
    match state {
        ResultState::MatchedClean => "matched-clean",
        ResultState::MatchedDiagnostics => "matched-diagnostics",
        ResultState::Mismatch => "mismatch",
        ResultState::TszCrash => "tsz-crash",
        ResultState::TszTimeout => "tsz-timeout",
        ResultState::TszOom => "tsz-oom",
        ResultState::SetupFailure => "setup-failure",
    }
}

fn write_raw_outputs(summary: &SummaryReport, report_dir: &Path) -> Result<()> {
    let raw_dir = report_dir.join("raw");
    fs::create_dir_all(&raw_dir)?;
    for (index, report) in summary.configs.iter().enumerate() {
        let stem = format!("config-{:03}", index + 1);
        if let Some(tsc) = report.tsc.as_ref() {
            write_compiler_raw(&raw_dir.join(format!("{stem}-tsc.txt")), tsc)?;
        }
        if let Some(tsz) = report.tsz.as_ref() {
            write_compiler_raw(&raw_dir.join(format!("{stem}-tsz.txt")), tsz)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct TsconfigSnapshot {
    label: String,
    text: String,
    truncated: bool,
}

fn collect_tsconfig_context(cwd: &Path, report: &ConfigReport) -> Vec<TsconfigSnapshot> {
    let mut snapshots = Vec::new();
    let mut seen = Vec::<PathBuf>::new();
    let mut displayed_bytes = 0usize;
    let root = cwd.join(&report.config);
    collect_tsconfig_context_from_path(cwd, &root, &mut snapshots, &mut seen, &mut displayed_bytes);
    snapshots
}

fn collect_tsconfig_context_from_path(
    cwd: &Path,
    path: &Path,
    snapshots: &mut Vec<TsconfigSnapshot>,
    seen: &mut Vec<PathBuf>,
    displayed_bytes: &mut usize,
) {
    if snapshots.len() >= MAX_TSCONFIG_REPORT_FILES
        || *displayed_bytes >= MAX_TSCONFIG_REPORT_TOTAL_BYTES
    {
        return;
    }

    let normalized = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if seen.iter().any(|seen_path| seen_path == &normalized) {
        return;
    }
    seen.push(normalized.clone());

    let Ok(full_text) = fs::read_to_string(&normalized) else {
        return;
    };
    let parsed_config = json5::from_str::<serde_json::Value>(&full_text).ok();
    let remaining_bytes = MAX_TSCONFIG_REPORT_TOTAL_BYTES - *displayed_bytes;
    let display_limit = MAX_TSCONFIG_REPORT_BYTES.min(remaining_bytes);
    let truncated = full_text.len() > display_limit;
    let text = if truncated {
        full_text.chars().take(display_limit).collect()
    } else {
        full_text
    };
    *displayed_bytes += text.len();
    snapshots.push(TsconfigSnapshot {
        label: normalize_path_label(cwd, &normalized),
        text,
        truncated,
    });

    let Some(value) = parsed_config else {
        return;
    };
    let base_dir = normalized.parent().unwrap_or(cwd);

    if let Some(extends) = value.get("extends").and_then(serde_json::Value::as_str)
        && let Some(extended) = resolve_local_extends(base_dir, extends)
    {
        collect_tsconfig_context_from_path(cwd, &extended, snapshots, seen, displayed_bytes);
    }

    let Some(references) = value
        .get("references")
        .and_then(serde_json::Value::as_array)
    else {
        return;
    };
    for reference in references {
        let Some(path_value) = reference
            .get("path")
            .and_then(serde_json::Value::as_str)
            .and_then(|path_value| resolve_project_reference(base_dir, path_value))
        else {
            continue;
        };
        collect_tsconfig_context_from_path(cwd, &path_value, snapshots, seen, displayed_bytes);
    }
}

fn resolve_local_extends(base_dir: &Path, extends: &str) -> Option<PathBuf> {
    if !(extends.starts_with("./") || extends.starts_with("../") || extends.starts_with('/')) {
        return None;
    }
    let candidate = if Path::new(extends).is_absolute() {
        PathBuf::from(extends)
    } else {
        base_dir.join(extends)
    };
    Some(if candidate.extension().is_none() {
        candidate.with_extension("json")
    } else {
        candidate
    })
}

fn resolve_project_reference(base_dir: &Path, path_value: &str) -> Option<PathBuf> {
    if !(path_value.starts_with("./")
        || path_value.starts_with("../")
        || path_value.starts_with('/'))
    {
        return None;
    }
    let candidate = if Path::new(path_value).is_absolute() {
        PathBuf::from(path_value)
    } else {
        base_dir.join(path_value)
    };
    Some(if candidate.is_dir() || candidate.extension().is_none() {
        candidate.join("tsconfig.json")
    } else {
        candidate
    })
}

fn push_tsconfig_section(cwd: &Path, out: &mut String, report: &ConfigReport) {
    let snapshots = collect_tsconfig_context(cwd, report);
    if snapshots.is_empty() {
        return;
    }
    out.push_str("### tsconfig context\n\n");
    for snapshot in snapshots {
        out.push_str(&format!("#### `{}`\n\n", snapshot.label));
        out.push_str("```jsonc\n");
        out.push_str(&snapshot.text);
        if !snapshot.text.ends_with('\n') {
            out.push('\n');
        }
        if snapshot.truncated {
            out.push_str("// ... truncated by try-tsz\n");
        }
        out.push_str("```\n\n");
    }
}

fn write_tsconfig_context(cwd: &Path, summary: &SummaryReport, report_dir: &Path) -> Result<()> {
    let config_dir = report_dir.join("tsconfig");
    fs::create_dir_all(&config_dir)?;
    for (report_index, report) in summary.configs.iter().enumerate() {
        for (config_index, snapshot) in collect_tsconfig_context(cwd, report).iter().enumerate() {
            let stem = format!(
                "config-{:03}-{:03}-{}",
                report_index + 1,
                config_index + 1,
                sanitize_report_file_name(&snapshot.label)
            );
            let mut out = String::new();
            out.push_str(&format!("path: {}\n", snapshot.label));
            out.push_str(&format!("truncated: {}\n\n", snapshot.truncated));
            out.push_str(&snapshot.text);
            if !snapshot.text.ends_with('\n') {
                out.push('\n');
            }
            fs::write(config_dir.join(stem), out)?;
        }
    }
    Ok(())
}

fn sanitize_report_file_name(label: &str) -> String {
    let mut sanitized = label
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.len() > 96 {
        sanitized.truncate(96);
    }
    if sanitized.is_empty() {
        sanitized.push_str("tsconfig.json");
    }
    sanitized
}

fn write_compiler_raw(path: &Path, run: &CompilerRun) -> Result<()> {
    let mut out = String::new();
    out.push_str(&format!("command: {}\n", run.command));
    if let Some(version) = run.version.as_deref() {
        out.push_str(&format!("version: {version}\n"));
    }
    out.push_str(&format!("exit_code: {}\n", run.exit_code));
    out.push_str(&format!("elapsed_ms: {}\n\n", run.elapsed_ms));
    if !run.raw_output.trim().is_empty() {
        out.push_str("raw_output:\n");
        out.push_str(&run.raw_output);
        if !run.raw_output.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str("diagnostics:\n");
    for diagnostic in &run.diagnostics {
        let file = diagnostic.file.as_deref().unwrap_or("<no file>");
        let line = diagnostic
            .line
            .map_or_else(|| "?".to_string(), |line| line.to_string());
        let column = diagnostic
            .column
            .map_or_else(|| "?".to_string(), |column| column.to_string());
        out.push_str(&format!(
            "- {file}:{line}:{column} TS{} {}: {}\n",
            diagnostic.code, diagnostic.category, diagnostic.message
        ));
    }
    fs::write(path, out)?;
    Ok(())
}

fn push_diagnostic_section(out: &mut String, title: &str, diagnostics: &[ComparableDiagnostic]) {
    if diagnostics.is_empty() {
        return;
    }
    out.push_str(&format!("### {title}\n\n"));
    for diagnostic in diagnostics.iter().take(20) {
        let file = diagnostic.file.as_deref().unwrap_or("<no file>");
        let line = diagnostic
            .line
            .map_or_else(|| "?".to_string(), |line| line.to_string());
        let column = diagnostic
            .column
            .map_or_else(|| "?".to_string(), |column| column.to_string());
        out.push_str(&format!(
            "- `{file}:{line}:{column}` TS{}: {}\n",
            diagnostic.code, diagnostic.message
        ));
    }
    out.push('\n');
}

fn write_snippet_candidates(cwd: &Path, summary: &SummaryReport, report_dir: &Path) -> Result<()> {
    let repro_dir = report_dir.join("repros");
    fs::create_dir_all(&repro_dir)?;
    let mut written = 0usize;
    for diagnostic in summary
        .configs
        .iter()
        .flat_map(|report| {
            report
                .extra_tsz_diagnostics
                .iter()
                .chain(&report.missing_tsc_diagnostics)
        })
        .take(5)
    {
        let Some(file) = diagnostic.file.as_deref() else {
            continue;
        };
        let Some(start) = diagnostic.start else {
            continue;
        };
        let source_path = cwd.join(file);
        let Ok(source) = fs::read_to_string(&source_path) else {
            continue;
        };
        let snippet = enclosing_line_window(&source, start);
        if snippet.trim().is_empty() {
            continue;
        }
        written += 1;
        let out_path = repro_dir.join(format!("mismatch-{written:03}.ts"));
        fs::write(
            &out_path,
            format!(
                "// Best-effort try-tsz snippet candidate from {file}\n// TS{}: {}\n\n{snippet}\n",
                diagnostic.code, diagnostic.message
            ),
        )?;
    }
    println!(
        "Wrote {written} snippet candidate(s) to {}",
        repro_dir.display()
    );
    Ok(())
}

fn enclosing_line_window(source: &str, start: u32) -> String {
    let offset = usize::try_from(start).unwrap_or(0).min(source.len());
    let line_starts = source
        .match_indices('\n')
        .map(|(idx, _)| idx + 1)
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let current_line = line_starts
        .iter()
        .enumerate()
        .take_while(|(_, line_start)| **line_start <= offset)
        .map(|(idx, _)| idx)
        .last()
        .unwrap_or(0);
    let first_line = current_line.saturating_sub(4);
    let last_line = (current_line + 5).min(line_starts.len());
    source
        .lines()
        .skip(first_line)
        .take(last_line.saturating_sub(first_line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn submit_discussion_or_print_fallback(report_path: &Path) -> Result<()> {
    let body = fs::read_to_string(report_path)?;
    let title = "[try-tsz] compatibility report";
    let output = Command::new("gh")
        .args([
            "api",
            "graphql",
            "-f",
            "query=mutation($repositoryId:ID!,$categoryId:ID!,$title:String!,$body:String!){createDiscussion(input:{repositoryId:$repositoryId,categoryId:$categoryId,title:$title,body:$body}){discussion{url}}}",
            "-f",
            "repositoryId=R_kgDOQ7o9zQ",
            "-f",
            "categoryId=DIC_kwDOQ7o9zc4C-QRC",
            "-f",
            &format!("title={title}"),
            "-f",
            &format!("body={body}"),
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        _ => {
            println!("Could not submit with gh. Open a Discussion and paste:");
            println!("{}", report_path.display());
            println!(
                "https://github.com/tsz-org/tsz/discussions/new?category=general&title={}&body={}",
                percent_encode_url_component(title),
                percent_encode_url_component(&body)
            );
        }
    }
    Ok(())
}

fn percent_encode_url_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn normalize_path_label(cwd: &Path, path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let normalized_cwd = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let normalized_path = fs::canonicalize(&absolute).unwrap_or(absolute);
    relative_path(&normalized_cwd, &normalized_path)
}

fn relative_path(cwd: &Path, path: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let mut path = std::env::temp_dir();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should work")
                .as_nanos();
            let unique = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            path.push(format!(
                "try_tsz_test_{}_{}_{}",
                std::process::id(),
                nanos,
                unique
            ));
            fs::create_dir_all(&path).expect("temp dir should be created");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_file(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent should be created");
        }
        fs::write(path, text).expect("file should be written");
    }

    fn diag(code: u32, file: &str, message: &str) -> ComparableDiagnostic {
        ComparableDiagnostic {
            file: Some(file.to_string()),
            start: Some(1),
            length: Some(2),
            line: Some(1),
            column: Some(2),
            code,
            category: "error".to_string(),
            message: message.to_string(),
        }
    }

    fn report_for_config(config: &str) -> ConfigReport {
        ConfigReport {
            config: config.to_string(),
            state: ResultState::Mismatch,
            metadata: ProjectMetadata {
                try_tsz_version: "test".to_string(),
                tsz_version: "test".to_string(),
                typescript_version: Some("7.0.2".to_string()),
                node_version: None,
                os: "test".to_string(),
                arch: "test".to_string(),
                package_manager: None,
                project_references: true,
                file_count: 0,
                approx_loc: 0,
            },
            tsc: None,
            tsz: None,
            extra_tsz_diagnostics: Vec::new(),
            missing_tsc_diagnostics: Vec::new(),
            order_mismatches: 0,
            setup_error: None,
        }
    }

    #[test]
    fn discover_nearest_tsconfig() {
        let temp = TempDir::new();
        write_file(&temp.path.join("tsconfig.json"), "{}");
        fs::create_dir_all(temp.path.join("src/nested")).expect("nested dir");

        let configs = discover_configs(&temp.path.join("src/nested"), None, false)
            .expect("config should be discovered");

        assert_eq!(configs, vec![temp.path.join("tsconfig.json")]);
    }

    #[test]
    fn explicit_project_directory_resolves_tsconfig() {
        let temp = TempDir::new();
        write_file(&temp.path.join("pkg/tsconfig.json"), "{}");

        let configs = discover_configs(&temp.path, Some(Path::new("pkg")), false)
            .expect("project dir should resolve");

        assert_eq!(configs, vec![temp.path.join("pkg/tsconfig.json")]);
    }

    #[test]
    fn all_skips_generated_directories() {
        let temp = TempDir::new();
        write_file(&temp.path.join("packages/a/tsconfig.json"), "{}");
        write_file(&temp.path.join("node_modules/pkg/tsconfig.json"), "{}");

        let configs = discover_configs(&temp.path, None, true).expect("all should find configs");

        assert_eq!(configs, vec![temp.path.join("packages/a/tsconfig.json")]);
    }

    #[test]
    fn typescript_oracle_preflight_accepts_hoisted_workspace_package() {
        let temp = TempDir::new();
        let package_dir = temp.path.join("packages/foo");
        let config = package_dir.join("tsconfig.json");
        write_file(&config, "{}");
        write_file(&temp.path.join(local_typescript_package_json_path()), "{}");

        ensure_typescript_oracle(&package_dir, &config)
            .expect("hoisted workspace TypeScript should satisfy preflight");
    }

    #[test]
    fn typescript_oracle_preflight_rejects_missing_tsc() {
        let temp = TempDir::new();
        let package_dir = temp.path.join("packages/foo");
        let config = package_dir.join("tsconfig.json");
        write_file(&config, "{}");

        let error = ensure_typescript_oracle(&package_dir, &config)
            .expect_err("missing local TypeScript should be rejected")
            .to_string();

        assert!(error.contains("TypeScript 7.0.2 or newer"));
        assert!(error.contains("node_modules/typescript/package.json"));
    }

    #[test]
    fn tsz_timeout_env_value_must_be_positive_seconds() {
        assert_eq!(
            tsz_timeout_from_env_value(None),
            Duration::from_secs(DEFAULT_TSZ_TIMEOUT_SECS)
        );
        assert_eq!(
            tsz_timeout_from_env_value(Some("45")),
            Duration::from_secs(45)
        );
        assert_eq!(
            tsz_timeout_from_env_value(Some("0")),
            Duration::from_secs(DEFAULT_TSZ_TIMEOUT_SECS)
        );
        assert_eq!(
            tsz_timeout_from_env_value(Some("nope")),
            Duration::from_secs(DEFAULT_TSZ_TIMEOUT_SECS)
        );
    }

    #[test]
    fn tsconfig_context_collects_local_extends_and_references() {
        let temp = TempDir::new();
        write_file(
            &temp.path.join("tsconfig.base.json"),
            "{ // jsonc is accepted\n  \"compilerOptions\": { \"strict\": true }\n}\n",
        );
        write_file(
            &temp.path.join("packages/shared/tsconfig.json"),
            "{ \"compilerOptions\": { \"composite\": true } }\n",
        );
        write_file(
            &temp.path.join("packages/app/tsconfig.json"),
            "{\n  \"extends\": \"../../tsconfig.base.json\",\n  \"references\": [{ \"path\": \"../shared\" }]\n}\n",
        );

        let snapshots =
            collect_tsconfig_context(&temp.path, &report_for_config("packages/app/tsconfig.json"));
        let labels = snapshots
            .iter()
            .map(|snapshot| snapshot.label.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "packages/app/tsconfig.json",
                "tsconfig.base.json",
                "packages/shared/tsconfig.json"
            ]
        );
        assert!(snapshots.iter().all(|snapshot| !snapshot.truncated));
    }

    #[test]
    fn diagnostic_diff_detects_extra_missing_and_order() {
        let first = diag(2322, "a.ts", "A");
        let second = diag(2339, "b.ts", "B");

        let diff = diff_diagnostics(
            std::slice::from_ref(&first),
            &[first.clone(), second.clone()],
        );
        assert_eq!(diff.extra_tsz, vec![second.clone()]);
        assert!(diff.missing_tsc.is_empty());
        assert_eq!(diff.order_mismatches, 0);

        let diff = diff_diagnostics(&[first.clone(), second.clone()], &[second, first]);
        assert!(diff.extra_tsz.is_empty());
        assert!(diff.missing_tsc.is_empty());
        assert_eq!(diff.order_mismatches, 2);
    }

    #[test]
    fn config_deprecation_diagnostics_ignore_location_for_try_tsz_diff() {
        let message = concat!(
            "Option 'moduleResolution=node10' is deprecated and will stop functioning in TypeScript 7.0.",
            " Specify compilerOption '\"ignoreDeprecations\": \"6.0\"' to silence this error.",
            "\n  Visit https://aka.ms/ts6 for migration information.",
        );
        let mut tsc = ComparableDiagnostic {
            file: None,
            start: None,
            length: None,
            line: None,
            column: None,
            code: 5107,
            category: "error".to_string(),
            message: message.to_string(),
        };
        let mut tsz = diag(5107, "tsconfig.json", message);

        normalize_config_deprecation_location(&mut tsc);
        normalize_config_deprecation_location(&mut tsz);
        let diff = diff_diagnostics(&[tsc], &[tsz]);

        assert!(diff.extra_tsz.is_empty());
        assert!(diff.missing_tsc.is_empty());
        assert_eq!(diff.order_mismatches, 0);
    }

    #[test]
    fn line_window_returns_context_around_offset() {
        let source = "one\ntwo\nthree\nfour\nfive\nsix\nseven\n";
        let snippet = enclosing_line_window(source, 14);

        assert!(snippet.contains("three"));
        assert!(snippet.contains("five"));
    }
}
