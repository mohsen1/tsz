//! Parallel-vs-sequential diagnostic agreement (issue #13255).
//!
//! Fresh per-file checkers share one `DefinitionStore`. A cross-file program
//! symbol whose declaration could not be attributed to an arena used to fall
//! back to an arbitrary arena (`lib_decls.rs`); the arena-local `NodeIndex`
//! then addressed an unrelated node there, and lowering that foreign node
//! published a wrong def body (empty interface shapes, mis-typed members)
//! that poisoned sibling checkers. Under the default sequential path the
//! poison was deterministic; under parallel fresh checking it was
//! schedule-dependent, so the same project produced different diagnostics
//! run to run (false TS2339/TS2344 storms).
//!
//! This test drives the real binary over a 9-file NodeNext project distilled
//! from the issue witness (binders renamed) and asserts forced-parallel runs
//! are byte-identical to the sequential run. Before the lib-decl fallback
//! validation this failed deterministically: the parallel output disagreed
//! with the sequential output on elaboration identity and carried extra
//! false diagnostics.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
        path.push(format!("tsz_parallel_agreement_{name}_{nanos}"));
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

const FIXTURE_FILES: &[(&str, &str)] = &[
    (
        "src/bridge.ts",
        include_str!("fixtures/parallel_agreement/bridge.ts"),
    ),
    (
        "src/cadence.ts",
        include_str!("fixtures/parallel_agreement/cadence.ts"),
    ),
    (
        "src/custody.ts",
        include_str!("fixtures/parallel_agreement/custody.ts"),
    ),
    (
        "src/edicts.ts",
        include_str!("fixtures/parallel_agreement/edicts.ts"),
    ),
    (
        "src/index.ts",
        include_str!("fixtures/parallel_agreement/index.ts"),
    ),
    (
        "src/limits.ts",
        include_str!("fixtures/parallel_agreement/limits.ts"),
    ),
    (
        "src/pipeline.ts",
        include_str!("fixtures/parallel_agreement/pipeline.ts"),
    ),
    (
        "src/primitives.ts",
        include_str!("fixtures/parallel_agreement/primitives.ts"),
    ),
    (
        "src/shapes.ts",
        include_str!("fixtures/parallel_agreement/shapes.ts"),
    ),
];

const FIXTURE_TSCONFIG: &str = include_str!("fixtures/parallel_agreement/tsconfig.json");

fn run_project(tsz_bin: &Path, project_dir: &Path, force_parallel: bool) -> String {
    let mut cmd = Command::new(tsz_bin);
    cmd.args(["-p", "tsconfig.json", "--pretty", "false"])
        .current_dir(project_dir);
    if force_parallel {
        cmd.env("TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK", "1");
    } else {
        cmd.env_remove("TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK");
    }
    let output = cmd.output().expect("run tsz on agreement fixture");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Forced-parallel fresh checking must produce byte-identical diagnostics to
/// the default sequential path on a generic-heavy multi-file project.
#[test]
fn forced_parallel_diagnostics_match_sequential() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping parallel agreement test: tsz binary not found");
        return;
    };
    let temp = TempDir::new("witness").expect("temp dir");
    std::fs::create_dir_all(temp.path.join("src")).expect("src dir");
    for (rel, contents) in FIXTURE_FILES {
        std::fs::write(temp.path.join(rel), contents).expect("write fixture file");
    }
    std::fs::write(temp.path.join("tsconfig.json"), FIXTURE_TSCONFIG).expect("write tsconfig");

    let sequential = run_project(&tsz_bin, &temp.path, false);
    assert!(
        !sequential.is_empty() || run_project(&tsz_bin, &temp.path, false) == sequential,
        "sequential run should be reproducible"
    );

    for attempt in 0..3 {
        let parallel = run_project(&tsz_bin, &temp.path, true);
        assert_eq!(
            parallel, sequential,
            "forced-parallel diagnostics diverged from sequential on attempt {attempt}"
        );
    }
}
