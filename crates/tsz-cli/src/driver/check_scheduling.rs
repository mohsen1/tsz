//! Scheduling policy helpers for CLI semantic checking.

#[cfg(test)]
thread_local! {
    pub(super) static FILE_SESSION_REUSE_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn file_session_reuse_test_override() -> Option<bool> {
    FILE_SESSION_REUSE_TEST_OVERRIDE.with(std::cell::Cell::get)
}

// File-session reuse policy.
//
// Previously this defaulted to ON for all batch CLI projects (PRs #6870
// sequential and #6893 parallel), optimising the counter `state_constructed`
// on 40-400 file projects. At 1k+ files the reuse path regresses wall time by
// 4-14x; see PR #7521 and
// `docs/architecture/LSP_PERF_EXPERIMENTS_2026-05-16.md`. Measurements across
// the full scale-cliff matrix (monorepo-001..006) show reuse OFF is faster at
// every large fixture size we tested:
//
//   101 files:    1.5x faster off
//   1,010 files:  3.9x faster off
//   5,099 files:  4.6x faster off
//   5,251 files:  5.4x faster off (cross-pkg mapped types)
//   10,299 files: only finishes with reuse off (E8 1.47 M LOC synthetic)
//
// Tiny no-emit TypeScript projects are a different regime where fresh-checker
// construction and boxed-lib priming dominate the wall-clock floor. Route those
// projects through the deterministic sequential reuse path by default; JS/JSX
// and larger batches remain reuse-off unless explicitly opted in until the
// scale-cliff and byte-identity gaps close.
// Two env knobs remain:
//   * `TSZ_FILE_SESSION_REUSE=1` opts larger projects back in (legacy explicit-opt-in knob
//     from the pre-#6870 era).
//   * `TSZ_DISABLE_FILE_SESSION_REUSE=1` continues to force off, preserving
//     scripts that already pin the off behaviour. Takes precedence over
//     the enable knob.
//
// The LSP server binaries (`tsz_lsp`, `tsz_server`) do not consume this
// driver and are unaffected - they reuse state through the `tsz-lsp`
// `Project` API by construction.

pub(super) const FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES: usize = 32;

/// Pure policy function so tests can assert the env-var rules without
/// touching process-global state. `disable_set` is true when
/// `TSZ_DISABLE_FILE_SESSION_REUSE` is present in the environment;
/// `enable_set` is true when `TSZ_FILE_SESSION_REUSE` is present.
pub(super) const fn file_session_reuse_from_env(disable_set: bool, enable_set: bool) -> bool {
    if disable_set {
        return false;
    }
    enable_set
}

pub(super) const fn file_session_reuse_from_workload(
    disable_set: bool,
    enable_set: bool,
    work_item_count: usize,
    has_js_or_jsx_workload: bool,
) -> bool {
    if disable_set {
        return false;
    }
    if enable_set {
        return true;
    }
    if has_js_or_jsx_workload {
        return false;
    }
    work_item_count <= FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES
}

pub(super) fn file_session_reuse_requested(
    work_item_count: usize,
    has_js_or_jsx_workload: bool,
) -> bool {
    #[cfg(test)]
    if let Some(enabled) = file_session_reuse_test_override() {
        return enabled;
    }

    file_session_reuse_from_workload(
        std::env::var_os("TSZ_DISABLE_FILE_SESSION_REUSE").is_some(),
        std::env::var_os("TSZ_FILE_SESSION_REUSE").is_some(),
        work_item_count,
        has_js_or_jsx_workload,
    )
}

pub(super) fn parallel_file_session_reuse_requested() -> bool {
    #[cfg(test)]
    if let Some(enabled) = file_session_reuse_test_override() {
        return enabled;
    }

    file_session_reuse_from_env(
        std::env::var_os("TSZ_DISABLE_FILE_SESSION_REUSE").is_some(),
        std::env::var_os("TSZ_FILE_SESSION_REUSE").is_some(),
    )
}

/// The user's explicit `TSZ_CHECKER_POOL` request, parsed once.
///
/// The bounded checker pool checks files on a fixed pool of `N` long-lived
/// `CheckerState`s (cost-balanced file assignment, each reused via
/// `switch_to_file`) instead of the per-file fresh checker. This amortises the
/// O(program) per-file setup over `files / N` files - the lever that unblocks
/// large multi-file projects.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum CheckerPoolEnv {
    /// `TSZ_CHECKER_POOL` unset or invalid: defer to the lane default.
    Unset,
    /// `TSZ_CHECKER_POOL=0` or empty: force the pool off on every lane,
    /// overriding the large-lane default.
    ForceOff,
    /// `TSZ_CHECKER_POOL=N` explicit width (`auto`/`1` -> available
    /// parallelism): use this width on any lane.
    Width(usize),
}

/// Parse the explicit `TSZ_CHECKER_POOL` request. `=auto` (or `=1`) sizes the
/// pool to `available_parallelism`; `=N` sets an explicit width; `0` / empty
/// forces it off; unset / invalid defers to [`resolve_checker_pool_size`]
/// (which defaults the pool ON for the large non-DOM parallel lane).
pub(super) fn checker_pool_env() -> CheckerPoolEnv {
    let Ok(raw) = std::env::var("TSZ_CHECKER_POOL") else {
        return CheckerPoolEnv::Unset;
    };
    match raw.trim() {
        "" | "0" => CheckerPoolEnv::ForceOff,
        "auto" | "1" => CheckerPoolEnv::Width(
            std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
        ),
        n => n
            .parse::<usize>()
            .ok()
            .filter(|&w| w >= 1)
            .map_or(CheckerPoolEnv::Unset, CheckerPoolEnv::Width),
    }
}

/// Whether the bounded checker pool is force-disabled via the
/// `TSZ_DISABLE_CHECKER_POOL` kill switch. An explicit `TSZ_CHECKER_POOL=<n>`
/// width still wins over this switch; the switch only suppresses the default-on
/// behavior on the large non-DOM parallel lane.
pub(super) fn checker_pool_disabled() -> bool {
    std::env::var_os("TSZ_DISABLE_CHECKER_POOL").is_some()
}

/// Resolve the effective bounded-checker-pool width, folding the explicit env
/// request, the kill switch, and the default-on lane policy into one decision.
///
/// Precedence, highest first:
///   1. an explicit `TSZ_CHECKER_POOL=<n|auto>` width always wins, on any lane;
///   2. an explicit `TSZ_CHECKER_POOL=0` / empty, or the
///      `TSZ_DISABLE_CHECKER_POOL` kill switch, forces the pool off;
///   3. on the large non-DOM parallel lane (`default_eligible`) the pool
///      defaults ON, sized to `available_parallelism`;
///   4. otherwise the pool stays off and checking falls through to the
///      fresh-checker / sequential-reuse arms.
///
/// `default_eligible` is the large non-DOM parallel lane: more files than the
/// small-project boundary, no order-sensitive global lib (DOM/webworker), and
/// no explicit `TSZ_FILE_SESSION_REUSE` opt-in (which selects its own reuse
/// arm). The per-partition body is the proven sequential-reuse path, so
/// diagnostics stay byte-identical to the fresh-checker arm regardless of
/// partitioning.
pub(super) const fn resolve_checker_pool_size(
    env: CheckerPoolEnv,
    kill_switch: bool,
    default_eligible: bool,
    available_parallelism: usize,
) -> Option<usize> {
    match env {
        // Explicit positive width wins on any lane, even over the kill switch.
        CheckerPoolEnv::Width(width) => Some(width),
        // Explicit `0`/empty forces off, overriding the large-lane default.
        CheckerPoolEnv::ForceOff => None,
        CheckerPoolEnv::Unset => {
            if kill_switch {
                None
            } else if default_eligible {
                Some(available_parallelism)
            } else {
                None
            }
        }
    }
}

/// Decide whether fresh per-file checkers run sequentially instead of on the
/// rayon pool.
///
/// Tiny batches stay sequential to avoid pool overhead. DOM/webworker-lib
/// projects (PR #7312) stay
/// sequential because correctness and termination on type-heavy projects
/// currently depend on checking files in deterministic order with a
/// progressively warmed `SharedQueryCache`.
///
/// `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK` is a diagnosis-only escape hatch for
/// the DOM/webworker gate-lift campaign: it bypasses only the order-sensitive
/// global-lib gate so forced-parallel rows can be byte-compared against the
/// sequential baseline without also changing tiny-batch policy.
///
/// `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK_TINY` is a second diagnosis-only escape
/// hatch (`force_tiny_batch_parallel`) that *additionally* bypasses the
/// tiny-batch floor, forcing the genuine rayon `par_iter` fresh-checker path
/// even for a handful of files. The schedule-determinism regression guards in
/// `parallel_sequential_agreement_tests` rely on it: their distilled witnesses
/// are far below the [`FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES`] floor, so
/// `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK` alone left them on the sequential arm
/// (sequential-vs-sequential, a silent no-op). This flag never changes the
/// production default - tiny batches still run sequentially unless it is set.
///
/// Large wildcard barrels are still detected and tested separately, but they
/// no longer force the entire project onto one core: the mutation-isolation
/// and cold-start cache work that landed before #13244 made the whole-project
/// fallback too blunt for large-ts-repo-sized projects.
///
/// Investigated 2026-06 while attempting to lift the DOM gate for the
/// ts-toolbelt row: the blocker is not (only) racing shared state. Checking
/// `ts-toolbelt/sources/Function/AutoPath.ts` *alone* (cold caches, fully
/// sequential) is a runaway evaluation, and `sources/Number/Greater.ts`
/// alone emits a false `TS2344` (`'undefined'` vs the `Iteration` tuple
/// constraint). The sequential project run masks both because earlier files
/// warm the shared eval/relation caches before the heavy files are reached.
/// Under parallel fresh checking, workers start those files cold, so runs
/// nondeterministically hang or surface the false diagnostics. Lifting this
/// gate requires fixing the cold-start conditional/`infer` evaluation
/// family first (then re-running the 10x byte-diff determinism loop on
/// ts-toolbelt at full width).
///
/// Deep-dive 2026-06-11 (gate-lift campaign, `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK`
/// repro on ts-toolbelt at 4 workers; 0/5 runs correct on the pre-fix binary -
/// 3/5 livelocked >150s, 2/5 emitted false `TS2344`s): the root family is
/// **in-flight shared-`DefinitionStore` state**. Every fresh checker
/// re-derives def bodies into the shared store (last-writer-wins; ~6.4k
/// re-publications with different `TypeId` forms per sequential ts-toolbelt
/// run - benign there because each checker reads its own writes through its
/// `TypeEnvironment` before falling back to the store, and foreign bodies are
/// only read after their writer completed). Under parallelism, sibling
/// workers consume bodies/params mid-rewrite, so deferred-type evaluation
/// (`keyof`/indexed-access/conditional checks) observes half-constructed
/// foreign forms: generic conditionals were resolved to definitive false
/// branches (e.g. `` `${N}` extends keyof IterationMap `` while `N` is still
/// generic -> `IterationMap['__']`), which both emits false `TS2344`s and
/// feeds self-sustaining recursive expansions (`RangeForth` over the `'__'`
/// sentinel tuple) whose accumulator grows fresh `TypeId`s every step -
/// defeating every TypeId-keyed cycle guard and burning the full 2M-op
/// per-query budget per call site (the observed livelock; 67k
/// generic-check false-branch decisions per run vs 106 sequentially).
/// Two structural fixes landed from this investigation (tsc-parity generic
/// check-type deferral in conditional evaluation; atomic body+params
/// publication), which remove the dominant false-branch storm, but
/// write-once/session-scoped visibility experiments on the def store show
/// several more in-flight channels (delegation buckets, lib cache,
/// interner side-state) still leak; the gate stays until the
/// mutation-isolation campaign makes shared def state schedule-independent.
pub(super) const fn should_use_sequential_fresh_checking(
    work_item_count: usize,
    has_parallel_order_sensitive_global_lib: bool,
    force_parallel_order_sensitive_global_lib: bool,
    force_tiny_batch_parallel: bool,
) -> bool {
    (work_item_count <= FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES && !force_tiny_batch_parallel)
        || (has_parallel_order_sensitive_global_lib && !force_parallel_order_sensitive_global_lib)
}

/// Whether the bounded checker pool (`TSZ_CHECKER_POOL`) must be refused
/// because the program includes a parallel-order-sensitive global lib
/// (DOM/webworker).
///
/// The pool runs `pool_size` long-lived `CheckerState`s in parallel over one
/// shared `DefinitionStore`, so it is subject to the same schedule-dependent
/// lib-interface materialization hazard as the fresh-parallel lane gated by
/// [`should_use_sequential_fresh_checking`]: DOM/SVG element interfaces with
/// deep heritage are first-demand materialized concurrently, and sibling
/// workers observe pre-finalize body forms, producing non-deterministic
/// diagnostics. The pool dispatch is evaluated before that gate, so the
/// refusal is enforced here independently - DOM programs take the sequential
/// path whether the pool was reached via an explicit `TSZ_CHECKER_POOL=N` or
/// a default-on policy. The `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK` diagnosis
/// override lifts the refusal so forced-parallel byte-diffs can be driven from
/// the env; non-determinism is never the default.
pub(super) const fn pool_refused_for_order_sensitive_global_lib(
    has_parallel_order_sensitive_global_lib: bool,
    force_parallel_order_sensitive_global_lib: bool,
) -> bool {
    has_parallel_order_sensitive_global_lib && !force_parallel_order_sensitive_global_lib
}

pub(super) const fn needs_separate_boxed_prime_checker(
    no_emit: bool,
    emit_declarations: bool,
    reuse_requested: bool,
    file_count: usize,
    has_libs: bool,
) -> bool {
    if file_count == 0 || !has_libs {
        return false;
    }

    let reused_checker_covers_prime = no_emit
        && !emit_declarations
        && reuse_requested
        && file_count <= FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES;
    !reused_checker_covers_prime
}

pub(super) const FILE_SESSION_REUSE_PARALLEL_CHUNK_SIZE: usize = 8;
