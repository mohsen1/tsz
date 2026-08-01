//! Evaluator cache kill-switch agreement guards (#14351).
//!
//! The evaluator cache switches are process-latched with `OnceLock`, so an
//! in-process `set_var` test can silently reuse the first configuration it
//! observed. These tests drive the real CLI in fresh child processes and compare
//! the default cache-on result with one disabled cache family at a time.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const CLOSED_EVAL_SWITCH: &str = "TSZ_DISABLE_CLOSED_EVAL_CACHE";
const CONDITIONAL_BRANCH_SWITCH: &str = "TSZ_DISABLE_CONDITIONAL_BRANCH_CACHE";
const LIMIT_RESULT_SWITCH: &str = "TSZ_DISABLE_LIMIT_RESULT_CACHE";
const ALL_CACHE_SWITCHES: &[&str] = &[
    CLOSED_EVAL_SWITCH,
    CONDITIONAL_BRANCH_SWITCH,
    LIMIT_RESULT_SWITCH,
];

const VALIDATOR_KEYS_FILES: &[(&str, &str)] = &[(
    "src/index.ts",
    include_str!("fixtures/evaluator_cache_agreement/validator_keys/src/index.ts"),
)];
const VALIDATOR_KEYS_TSCONFIG: &str =
    include_str!("fixtures/evaluator_cache_agreement/validator_keys/tsconfig.json");

const BRANCH_FILTERS_FILES: &[(&str, &str)] = &[(
    "src/index.ts",
    include_str!("fixtures/evaluator_cache_agreement/branch_filters/src/index.ts"),
)];
const BRANCH_FILTERS_TSCONFIG: &str =
    include_str!("fixtures/evaluator_cache_agreement/branch_filters/tsconfig.json");

const RECURSIVE_ITERATION_FILES: &[(&str, &str)] = &[(
    "src/index.ts",
    include_str!("fixtures/evaluator_cache_agreement/recursive_iteration/src/index.ts"),
)];
const RECURSIVE_ITERATION_TSCONFIG: &str =
    include_str!("fixtures/evaluator_cache_agreement/recursive_iteration/tsconfig.json");

#[derive(Debug, PartialEq, Eq)]
struct TszRun {
    status_code: Option<i32>,
    stdout: String,
    stderr: String,
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_eval_cache_agreement_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn find_tsz_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let current_exe = std::env::current_exe().ok()?;
    let debug_dir = current_exe.parent()?.parent()?;
    let candidate = debug_dir.join("tsz");
    candidate.exists().then_some(candidate)
}

fn stage_project(name: &str, files: &[(&str, &str)], tsconfig: &str) -> TempDir {
    let temp = TempDir::new(name).expect("temp dir");
    for (rel, contents) in files {
        let path = temp.path.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("fixture parent dir");
        }
        std::fs::write(path, contents).expect("write fixture file");
    }
    std::fs::write(temp.path.join("tsconfig.json"), tsconfig).expect("write tsconfig");
    temp
}

fn run_tsz(tsz_bin: &Path, project_dir: &Path, disabled_switch: Option<&str>) -> TszRun {
    let mut cmd = Command::new(tsz_bin);
    cmd.args(["-p", "tsconfig.json", "--pretty", "false"])
        .current_dir(project_dir)
        .env_remove("TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK")
        .env_remove("TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK_TINY");
    for switch in ALL_CACHE_SWITCHES {
        cmd.env_remove(switch);
    }
    if let Some(switch) = disabled_switch {
        cmd.env(switch, "1");
    }

    let output = cmd.output().expect("run tsz on cache agreement fixture");
    TszRun {
        status_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn assert_clean_success(name: &str, run: &TszRun) {
    assert_eq!(
        run.status_code,
        Some(0),
        "{name} should compile cleanly; stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr,
    );
    assert!(
        !run.stdout.contains("error TS") && !run.stderr.contains("error TS"),
        "{name} should not report TypeScript diagnostics; stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr,
    );
}

fn assert_cache_modes_agree(
    name: &str,
    files: &[(&str, &str)],
    tsconfig: &str,
    disabled_switches: &[&str],
) {
    assert_cache_modes_agree_impl(name, files, tsconfig, disabled_switches, true);
}

/// Like `assert_cache_modes_agree`, but does not require the baseline run to
/// compile cleanly first. Agreement between cache modes is the only thing
/// under test here; a fixture that legitimately reports diagnostics under
/// every cache configuration still proves the caches agree.
fn assert_cache_modes_agree_regardless_of_diagnostics(
    name: &str,
    files: &[(&str, &str)],
    tsconfig: &str,
    disabled_switches: &[&str],
) {
    assert_cache_modes_agree_impl(name, files, tsconfig, disabled_switches, false);
}

fn assert_cache_modes_agree_impl(
    name: &str,
    files: &[(&str, &str)],
    tsconfig: &str,
    disabled_switches: &[&str],
    require_clean: bool,
) {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping evaluator cache agreement test: tsz binary not found");
        return;
    };
    let temp = stage_project(name, files, tsconfig);
    let baseline = run_tsz(&tsz_bin, &temp.path, None);
    if require_clean {
        assert_clean_success(name, &baseline);
    }

    for switch in disabled_switches {
        let disabled = run_tsz(&tsz_bin, &temp.path, Some(switch));
        if require_clean {
            assert_clean_success(name, &disabled);
        }
        assert_eq!(disabled, baseline, "{name} output changed when {switch}=1");
    }
}

#[test]
fn validator_key_inference_agrees_with_closed_eval_and_branch_caches_disabled() {
    // Structural rule: cache-enabled and cache-disabled runs must agree when
    // `RequiredKeys`/`OptionalKeys` probe `Validator<infer T>` through mapped
    // keys and indexed access; the caches may reduce repeated work, not change
    // the inferred key set.
    assert_cache_modes_agree(
        "validator_keys",
        VALIDATOR_KEYS_FILES,
        VALIDATOR_KEYS_TSCONFIG,
        &[CLOSED_EVAL_SWITCH, CONDITIONAL_BRANCH_SWITCH],
    );
}

#[test]
fn branch_filter_agrees_with_conditional_branch_cache_disabled() {
    // Structural rule: a repeated conditional branch verdict for the same
    // structural `(check, extends)` pair may be reused, but disabling that reuse
    // must still select the same true/false branches.
    //
    // The fixture's two `Assert<Equal<...>>` checks genuinely evaluate to
    // `false` under a live `tsc@7.0.2` oracle (confirmed #15983), so the
    // baseline run was never clean; requiring exit 0 here tested a
    // precondition that could never hold, not cache agreement.
    assert_cache_modes_agree_regardless_of_diagnostics(
        "branch_filters",
        BRANCH_FILTERS_FILES,
        BRANCH_FILTERS_TSCONFIG,
        &[CONDITIONAL_BRANCH_SWITCH],
    );
}

#[test]
fn recursive_iteration_agrees_with_limit_result_cache_disabled() {
    // Structural rule: limit-aware cache publication can keep clean
    // intermediates from a recursive utility run, but disabling that cache
    // family must not change the collapsed project result.
    assert_cache_modes_agree(
        "recursive_iteration",
        RECURSIVE_ITERATION_FILES,
        RECURSIVE_ITERATION_TSCONFIG,
        &[LIMIT_RESULT_SWITCH],
    );
}
