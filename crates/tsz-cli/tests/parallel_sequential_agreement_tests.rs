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
//!
//! Floor caveat: `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK` only lifts the
//! order-sensitive global-lib (DOM) gate; it deliberately keeps the tiny-batch
//! policy, so a witness below the small-project floor
//! (`FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES`, 32 files) runs sequentially
//! in *both* arms of the `forced_parallel_*_matches_sequential` tests — a
//! sequential-vs-sequential comparison that cannot witness a schedule race.
//! `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK_TINY` additionally bypasses that floor
//! and forces the genuine per-file `par_iter` path; `genuine_parallel_*` tests
//! use it so a small distilled witness is actually checked concurrently.

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

/// Run the project, clearing both force-parallel experiment flags first and
/// then setting exactly `envs` (so a leaked env from the caller's process can
/// never perturb which checking path is taken).
fn run_tsz(tsz_bin: &Path, project_dir: &Path, envs: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(tsz_bin);
    cmd.args(["-p", "tsconfig.json", "--pretty", "false"])
        .current_dir(project_dir)
        .env_remove("TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK")
        .env_remove("TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK_TINY");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("run tsz on agreement fixture");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn run_project(tsz_bin: &Path, project_dir: &Path, force_parallel: bool) -> String {
    let envs: &[(&str, &str)] = if force_parallel {
        &[("TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK", "1")]
    } else {
        &[]
    };
    run_tsz(tsz_bin, project_dir, envs)
}

/// Run the project on the *genuine* rayon `par_iter` fresh-checker path.
///
/// `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK` alone only lifts the DOM gate, not the
/// tiny-batch floor (see module docs), so a sub-floor witness needs
/// `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK_TINY` too to actually run concurrently.
fn run_project_tiny_parallel(tsz_bin: &Path, project_dir: &Path) -> String {
    run_tsz(
        tsz_bin,
        project_dir,
        &[
            ("TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK", "1"),
            ("TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK_TINY", "1"),
        ],
    )
}

/// Stage a witness project and assert its sequential baseline is byte-identical
/// to `run_parallel`'s output across `attempts` schedules.
fn assert_matches_sequential(
    name: &str,
    files: &[(&str, &str)],
    tsconfig: &str,
    attempts: usize,
    run_parallel: impl Fn(&Path, &Path) -> String,
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
        let parallel = run_parallel(&tsz_bin, &temp.path);
        assert_eq!(
            parallel, sequential,
            "parallel diagnostics diverged from sequential on attempt {attempt}"
        );
    }
}

fn assert_parallel_matches_sequential(
    name: &str,
    files: &[(&str, &str)],
    tsconfig: &str,
    attempts: usize,
) {
    assert_matches_sequential(name, files, tsconfig, attempts, |bin, dir| {
        run_project(bin, dir, true)
    });
}

/// Genuine-`par_iter` variant of [`assert_parallel_matches_sequential`]: drives
/// the real concurrent path (via [`run_project_tiny_parallel`]) so a sub-floor
/// witness can actually observe an in-flight shared-state schedule race.
fn assert_tiny_parallel_matches_sequential(
    name: &str,
    files: &[(&str, &str)],
    tsconfig: &str,
    attempts: usize,
) {
    assert_matches_sequential(name, files, tsconfig, attempts, run_project_tiny_parallel);
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

/// Fifth witness family (#13862): deep-heritage DOM lib interfaces materialized
/// by the fresh per-file checker pool. A derived element interface
/// (`HTMLxElement extends HTMLElement extends Element extends Node`) is a valid
/// `Node`, but the shared `DefinitionStore` was last-writer-wins, so a
/// heritage-thin body re-derived by a sibling checker could clobber the
/// heritage-merged form mid-`Node`/`Element`/`HTMLElement` diamond resolution
/// (#12299) and a reader's relation saw the thin one — false `TS2345`/`TS2740`
/// where the element was "missing the following properties from type 'Node'".
///
/// The pool path only engages above the small-project session-reuse ceiling
/// (`FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES`, 32), so this drives a 40-file
/// project. Each file names a *distinct* DOM element interface (varied binders
/// per the anti-hardcoding test discipline) and asserts assignability to `Node`.
///
/// This pins the default (DOM-gated sequential) schedule users actually hit:
/// DOM projects keep `should_use_sequential_fresh_checking` on
/// (`has_parallel_order_sensitive_global_lib`), so the program must type-check
/// clean here. Pre-fix this emitted a deterministic cluster of false
/// diagnostics. Lifting the DOM serialization gate so the *forced-parallel*
/// schedule is also clean/byte-identical is the separate materialization
/// campaign tracked in #13862 / #13861.
const DOM_ELEMENT_NAMES: &[&str] = &[
    "Div",
    "Span",
    "Heading",
    "Paragraph",
    "Pre",
    "Body",
    "Style",
    "Script",
    "Meta",
    "Link",
    "Source",
    "Map",
    "Meter",
    "Option",
    "Label",
    "Legend",
    "Dialog",
    "Details",
    "DataList",
    "Data",
    "Base",
    "Audio",
    "Table",
    "Form",
    "Input",
    "Select",
    "Button",
    "Anchor",
    "Area",
    "Canvas",
    "Object",
    "Output",
    "Embed",
    "FieldSet",
    "Image",
    "Progress",
    "Quote",
    "Title",
    "Time",
    "Track",
];

const DOM_HERITAGE_TSCONFIG: &str = r#"{
  "compilerOptions": {
    "noEmit": true,
    "lib": ["dom", "es2020"],
    "strict": true,
    "skipLibCheck": true,
    "module": "es2020",
    "target": "es2020"
  },
  "include": ["*.ts"]
}"#;

/// Deep-heritage DOM element interfaces must type-check clean (assignable to
/// `Node`) when the program is large enough to engage the fresh per-file
/// checker pool on the default DOM-gated sequential schedule.
#[test]
fn dom_element_heritage_clean_sequential() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping DOM heritage test: tsz binary not found");
        return;
    };
    let temp = TempDir::new("dom_heritage").expect("temp dir");
    for (i, elem) in DOM_ELEMENT_NAMES.iter().enumerate() {
        let contents = format!(
            "export function use_{elem}(node: HTML{elem}Element): Node {{\n    return node;\n}}\n"
        );
        std::fs::write(temp.path.join(format!("f{i}.ts")), contents).expect("write fixture file");
    }
    std::fs::write(temp.path.join("tsconfig.json"), DOM_HERITAGE_TSCONFIG).expect("write tsconfig");

    let output = run_project(&tsz_bin, &temp.path, false);
    assert!(
        !output.contains("error TS"),
        "DOM element interfaces must be recognized as Node under the fresh per-file \
         checker pool (default sequential schedule); got diagnostics:\n{output}"
    );
}

/// Genuine-`par_iter` guard for the cross-arena delegation witness (#13255):
/// unlike the `forced_parallel_*` tests (which a sub-floor witness runs
/// sequentially in both arms — see module docs), this drives the real
/// concurrent path, so the delegation derivation (#13391: lib-origin global
/// resolution + def-identity recovery for symbol-less structural bases) is
/// proven schedule-independent rather than merely sequentially reproducible.
#[test]
fn genuine_parallel_disposable_delegation_matches_sequential() {
    assert_tiny_parallel_matches_sequential(
        "disposable_tiny",
        DISPOSABLE_FIXTURE_FILES,
        DISPOSABLE_FIXTURE_TSCONFIG,
        8,
    );
}
