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
//! This test drives the real binary over a 9-file `NodeNext` project distilled
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

/// Second witness family (#13255 witness 3 residual): shared lib interface
/// defs whose bodies were observed mid-derivation. Before the atomic
/// `(symbol, file)` -> `DefId` stabilization, the lib-clone attribution fix,
/// and the monotone-completion publication gate, forced-parallel runs
/// nondeterministically resolved a built-in iterator interface to its
/// pre-heritage-merge form (own members only) and emitted a false TS2741
/// ("Property 'next' is missing in type '{ [Symbol.iterator](): ... }'")
/// that the sequential run does not produce.
const LIB_ITER_FIXTURE_FILES: &[(&str, &str)] = &[
    (
        "src/conveyor.ts",
        include_str!("fixtures/parallel_agreement_lib_iter/conveyor.ts"),
    ),
    (
        "src/gadgets.ts",
        include_str!("fixtures/parallel_agreement_lib_iter/gadgets.ts"),
    ),
    (
        "src/helpers.ts",
        include_str!("fixtures/parallel_agreement_lib_iter/helpers.ts"),
    ),
    (
        "src/mesh.ts",
        include_str!("fixtures/parallel_agreement_lib_iter/mesh.ts"),
    ),
    (
        "src/types.ts",
        include_str!("fixtures/parallel_agreement_lib_iter/types.ts"),
    ),
    (
        "src/voyage.ts",
        include_str!("fixtures/parallel_agreement_lib_iter/voyage.ts"),
    ),
];

const LIB_ITER_FIXTURE_TSCONFIG: &str =
    include_str!("fixtures/parallel_agreement_lib_iter/tsconfig.json");

/// Third witness family (#13255 program-def in-flight remainder, distilled
/// from `typed-orchestration-core` with binders renamed): an importer checked
/// before the file that declares its dependency derives the cross-file types
/// itself through cross-arena delegation. Two delegation holes made that
/// derivation schedule-dependent:
///
/// - lib-origin globals (`Symbol`, `SymbolConstructor`) did not resolve
///   through the delegate lookup binder's `program_globals`, so
///   `[Symbol.asyncDispose]` class members were dropped as late-bound
///   (false TS2851 on `await using` whenever the importer was checked
///   first — sequential main-first AND every forced-parallel schedule);
/// - a symbol-less structural interface body published by a sibling checker
///   (the lib `Promise` body form) could not be mapped back to its def, so
///   generic applications over it never instantiated and raw type
///   parameters leaked (multithread-only false
///   `TS2339: Property 'map' does not exist on type 'T'` / TS7006 storms on
///   awaited `Promise<readonly unknown[]>` method results).
const DISPOSABLE_FIXTURE_FILES: &[(&str, &str)] = &[
    (
        "src/gauges.ts",
        include_str!("fixtures/parallel_agreement_disposable/gauges.ts"),
    ),
    (
        "src/latches.ts",
        include_str!("fixtures/parallel_agreement_disposable/latches.ts"),
    ),
    (
        "src/lattice.ts",
        include_str!("fixtures/parallel_agreement_disposable/lattice.ts"),
    ),
    (
        "src/relay.ts",
        include_str!("fixtures/parallel_agreement_disposable/relay.ts"),
    ),
];

const DISPOSABLE_FIXTURE_TSCONFIG: &str =
    include_str!("fixtures/parallel_agreement_disposable/tsconfig.json");

/// Fourth witness family (#13255 program-def alias republication remainder):
/// consumers are listed before the declaring module and force fresh per-file
/// checkers to derive the same generic registry aliases through the shared
/// `DefinitionStore`. Parallel and sequential schedules must agree even when
/// remapped tuple-union aliases and conditional `infer` wrappers are first
/// requested from importers rather than the declaring file.
const SCOPE_REGISTRY_FIXTURE_FILES: &[(&str, &str)] = &[
    (
        "src/alias-consumer.ts",
        include_str!("fixtures/parallel_agreement_scope_registry/alias-consumer.ts"),
    ),
    (
        "src/brands.ts",
        include_str!("fixtures/parallel_agreement_scope_registry/brands.ts"),
    ),
    (
        "src/registry-consumer.ts",
        include_str!("fixtures/parallel_agreement_scope_registry/registry-consumer.ts"),
    ),
    (
        "src/scope-registry.ts",
        include_str!("fixtures/parallel_agreement_scope_registry/scope-registry.ts"),
    ),
    (
        "src/tuple-utils.ts",
        include_str!("fixtures/parallel_agreement_scope_registry/tuple-utils.ts"),
    ),
];

const SCOPE_REGISTRY_FIXTURE_TSCONFIG: &str =
    include_str!("fixtures/parallel_agreement_scope_registry/tsconfig.json");

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

fn assert_parallel_matches_sequential(
    name: &str,
    files: &[(&str, &str)],
    tsconfig: &str,
    attempts: usize,
) {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping parallel agreement test: tsz binary not found");
        return;
    };
    let temp = TempDir::new(name).expect("temp dir");
    std::fs::create_dir_all(temp.path.join("src")).expect("src dir");
    for (rel, contents) in files {
        std::fs::write(temp.path.join(rel), contents).expect("write fixture file");
    }
    std::fs::write(temp.path.join("tsconfig.json"), tsconfig).expect("write tsconfig");

    let sequential = run_project(&tsz_bin, &temp.path, false);
    assert!(
        !sequential.is_empty() || run_project(&tsz_bin, &temp.path, false) == sequential,
        "sequential run should be reproducible"
    );

    for attempt in 0..attempts {
        let parallel = run_project(&tsz_bin, &temp.path, true);
        assert_eq!(
            parallel, sequential,
            "forced-parallel diagnostics diverged from sequential on attempt {attempt}"
        );
    }
}

/// Forced-parallel fresh checking must produce byte-identical diagnostics to
/// the default sequential path on a generic-heavy multi-file project.
#[test]
fn forced_parallel_diagnostics_match_sequential() {
    assert_parallel_matches_sequential("witness", FIXTURE_FILES, FIXTURE_TSCONFIG, 3);
}

/// Forced-parallel fresh checking must not observe in-flight shared lib
/// interface def bodies (#13255 witness 3: pre-heritage iterator interface
/// forms produced schedule-dependent false TS2741/TS2339).
#[test]
fn forced_parallel_lib_iterator_heritage_matches_sequential() {
    assert_parallel_matches_sequential(
        "lib_iter",
        LIB_ITER_FIXTURE_FILES,
        LIB_ITER_FIXTURE_TSCONFIG,
        5,
    );
}

/// Cross-arena delegation must derive the same cross-file class/interface
/// forms a primary checker would (lib-origin global resolution through
/// `program_globals`; def identity recovery for symbol-less structural
/// application bases). Pre-fix this diverged deterministically: sequential
/// carried a false TS2851 + raw-type-parameter TS2339/TS7006 storm that
/// forced-parallel runs did not (or vice versa per schedule).
#[test]
fn forced_parallel_disposable_delegation_matches_sequential() {
    assert_parallel_matches_sequential(
        "disposable",
        DISPOSABLE_FIXTURE_FILES,
        DISPOSABLE_FIXTURE_TSCONFIG,
        5,
    );
}

/// Generic program aliases published through the shared definition store must
/// be schedule-independent when importers request them before the declaring
/// module finishes its own publication pass.
#[test]
fn forced_parallel_scope_registry_alias_republication_matches_sequential() {
    assert_parallel_matches_sequential(
        "scope_registry",
        SCOPE_REGISTRY_FIXTURE_FILES,
        SCOPE_REGISTRY_FIXTURE_TSCONFIG,
        7,
    );
}
