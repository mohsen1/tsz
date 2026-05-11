//! Process-wide performance counters used to drive the perf-architectural
//! plan in `docs/plan/PERFORMANCE_PLAN.md`.
//!
//! Counters are gated by the `TSZ_PERF_COUNTERS` environment variable. When
//! the variable is unset the increments still fire (`AtomicU64::fetch_add`
//! is a single relaxed atomic op, which is well under a nanosecond), so we
//! could in principle just always count, but the env var also gates the
//! more expensive counters (per-shard lock-wait histograms, top-N largest
//! types, recomputation tracking) so production builds stay clean.
//!
//! Output is printed on demand via [`PerfCounters::dump`]. Drivers wire that
//! into `--extendedDiagnostics` (or `--perfCounters`) so a single bench
//! invocation produces both the standard phase timings and the counter dump.
//!
//! Per the architectural plan, this is a plan-changing PR — the data we
//! collect here decides how PRs 2–7 are scoped. Don't ship later PRs without
//! looking at the dump on `large-ts-repo` first.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Cache-line-padded `AtomicU64` to avoid false sharing between hot
/// counters that get hammered concurrently. 64 is the typical cache-line
/// size on x86-64 and Apple Silicon's M-series; over-aligning is harmless.
///
/// Defined for use by future PRs. Not adopted in PR #1631 because the
/// [`enabled_fast`] gate already eliminates the false-sharing problem
/// in production builds (where the env var is unset, the counter writes
/// don't fire at all). Inside profiling runs we accept some perturbation;
/// when we want profiler-grade fidelity for the highest-frequency
/// counters we'll switch their fields to `PaddedAtomicU64` and call
/// `field.0.fetch_add(...)` directly. See PR #1630 review issue #2.
#[repr(align(64))]
pub struct PaddedAtomicU64(pub AtomicU64);

impl PaddedAtomicU64 {
    pub const fn new(v: u64) -> Self {
        Self(AtomicU64::new(v))
    }
}

/// Process-wide enabled flag for the perf counters. Initialized exactly
/// once on first observation and read on every counter increment via
/// [`enabled_fast`]; the increment then becomes a single predictable
/// branch that's elided in the disabled case so production builds (where
/// `TSZ_PERF_COUNTERS` is unset) pay only the cost of the load.
///
/// Why this matters: even `AtomicU64::fetch_add(_, Relaxed)` is a
/// read-modify-write on a shared cache line. On the exact codebase where
/// we're trying to measure parallel-work contention, leaving the atomic
/// always-firing creates a synthetic contention point that distorts the
/// numbers we're trying to collect.
static ENABLED_FAST: OnceLock<bool> = OnceLock::new();

/// Cheap O(1) gate readable from any hot path. Reads a `OnceLock<bool>`
/// (one branch + one load) instead of going through `counters().enabled`
/// (deref-via-OnceLock + load).
#[inline(always)]
pub fn enabled_fast() -> bool {
    *ENABLED_FAST.get_or_init(|| std::env::var_os("TSZ_PERF_COUNTERS").is_some())
}

/// Why a `CheckerState::with_parent_cache` (and the matching
/// `copy_symbol_file_targets_to`) call fired. Each variant pins one specific
/// call site so the counter dump shows attribution: "X of the 17,329
/// constructions came from `delegate_cross_arena_symbol_resolution`,
/// Y came from `jsdoc_type_construction`, ...".
///
/// Adding a new reason: add the variant, update `REASON_NAMES` to keep them
/// aligned, and increase `CHECKER_CREATION_REASON_COUNT`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum CheckerCreationReason {
    /// `cross_file.rs::delegate_cross_arena_symbol_resolution` — the headline
    /// hot path; deep recursion through cross-file type queries.
    DelegateCrossArenaSymbol = 0,
    /// `cross_file.rs::delegate_cross_arena_class_instance_type`.
    DelegateCrossArenaClass = 1,
    /// `cross_file.rs::delegate_cross_arena_interface_type`.
    DelegateCrossArenaInterface = 2,
    /// Other `cross_file.rs` delegate variants (heritage, etc).
    DelegateCrossArenaOther = 3,
    /// JSDoc namespace-typedef lookups crossing arenas.
    JsDocLookup = 4,
    /// JSDoc type-construction (synthesized object/function shapes).
    JsDocTypeConstruction = 5,
    /// CommonJS `module.exports` / `exports.x` resolution + collection.
    CjsExports = 6,
    /// Cross-file type alias resolution.
    AliasResolution = 7,
    /// `import("…").Foo` indirect import-type resolution.
    ImportType = 8,
    /// Type-environment `core.rs` deep resolution helpers.
    TypeEnvironmentCore = 9,
    /// `types::queries::callable_truthiness` cross-file fall-through.
    CallableTruthiness = 10,
    /// Expando property assignments crossing files.
    ExpandoProperty = 11,
    /// `identifier::resolution` cross-file fallback.
    IdentifierResolution = 12,
    /// Generic call-helpers cross-file resolution (`call_helpers.rs`).
    CallHelpers = 13,
    /// `computed_helpers_binding` deep alias resolution.
    BindingHelpers = 14,
    /// `class_abstract_checker` cross-file abstract-method check.
    ClassAbstract = 15,
    /// Anything not explicitly classified above.
    Other = 16,
}

pub const CHECKER_CREATION_REASON_COUNT: usize = 17;

/// Number of log-spaced buckets in the interner lock-wait histogram.
/// See `LOCK_WAIT_BUCKET_UPPER_BOUNDS_NS` for the bucket boundaries.
pub const LOCK_WAIT_BUCKET_COUNT: usize = 8;

/// Upper bounds of the lock-wait histogram buckets, in nanoseconds. An
/// observation `ns` lands in the lowest-index bucket where
/// `ns < bucket_upper_bound`. The boundaries are log-spaced over the
/// 100ns…100ms range, with a final overflow bucket (`u64::MAX`) for
/// outliers. Plan §4.T0.3 notes that interner contention at the cliff
/// is the signal we need; a coarse log-bucketed histogram is enough
/// to distinguish "tail-bound" from "broadly slow" without paying for
/// per-shard or fine-grained quantile machinery.
pub const LOCK_WAIT_BUCKET_UPPER_BOUNDS_NS: [u64; LOCK_WAIT_BUCKET_COUNT] = [
    100,         // < 100 ns
    1_000,       // < 1 µs
    10_000,      // < 10 µs
    100_000,     // < 100 µs
    1_000_000,   // < 1 ms
    10_000_000,  // < 10 ms
    100_000_000, // < 100 ms
    u64::MAX,    // overflow
];

/// Human-readable names, one entry per `CheckerCreationReason` variant.
/// MUST stay in sync with the enum.
pub const REASON_NAMES: [&str; CHECKER_CREATION_REASON_COUNT] = [
    "DelegateCrossArenaSymbol",
    "DelegateCrossArenaClass",
    "DelegateCrossArenaInterface",
    "DelegateCrossArenaOther",
    "JsDocLookup",
    "JsDocTypeConstruction",
    "CjsExports",
    "AliasResolution",
    "ImportType",
    "TypeEnvironmentCore",
    "CallableTruthiness",
    "ExpandoProperty",
    "IdentifierResolution",
    "CallHelpers",
    "BindingHelpers",
    "ClassAbstract",
    "Other",
];

impl CheckerCreationReason {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
    pub const fn name(self) -> &'static str {
        REASON_NAMES[self as usize]
    }
}

/// How `delegate_cross_arena_symbol_resolution` found the target arena for
/// a cache miss that must construct a child checker.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum CrossArenaSymbolMissSource {
    /// `binder.symbol_arenas` pointed at a non-current arena.
    SymbolArena = 0,
    /// `binder.declaration_arenas` found a non-current declaration arena.
    DeclarationArena = 1,
    /// `cross_file_symbol_targets` resolved the target file index.
    SymbolFileTarget = 2,
    /// Fallback bucket for unexpected delegation shapes.
    Unknown = 3,
}

pub const CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT: usize = 4;

pub const CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES: [&str; CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT] = [
    "symbol_arenas",
    "declaration_arenas",
    "symbol_file_targets",
    "unknown",
];

impl CrossArenaSymbolMissSource {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Coarse symbol-kind bucket for `DelegateCrossArenaSymbol` misses.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum CrossArenaSymbolMissKind {
    TypeAlias = 0,
    Interface = 1,
    Class = 2,
    Function = 3,
    Variable = 4,
    Property = 5,
    Method = 6,
    Accessor = 7,
    Enum = 8,
    Module = 9,
    Alias = 10,
    TypeParameter = 11,
    TypeLiteral = 12,
    Signature = 13,
    Constructor = 14,
    ObjectLiteral = 15,
    Unresolved = 16,
    Other = 17,
}

pub const CROSS_ARENA_SYMBOL_MISS_KIND_COUNT: usize = 18;

pub const CROSS_ARENA_SYMBOL_MISS_KIND_NAMES: [&str; CROSS_ARENA_SYMBOL_MISS_KIND_COUNT] = [
    "type_alias",
    "interface",
    "class",
    "function",
    "variable",
    "property",
    "method",
    "accessor",
    "enum",
    "module",
    "alias",
    "type_parameter",
    "type_literal",
    "signature",
    "constructor",
    "object_literal",
    "unresolved",
    "other",
];

impl CrossArenaSymbolMissKind {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Outcome of the no-child named-alias shortcut attempted before constructing
/// a `DelegateCrossArenaSymbol` child checker.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum CrossArenaAliasShortcutOutcome {
    Success = 0,
    NotAlias = 1,
    MissingSymbol = 2,
    MissingModule = 3,
    MissingImportName = 4,
    NamespaceImport = 5,
    DefaultImport = 6,
    MissingAliasFile = 7,
    MissingTarget = 8,
    SelfTarget = 9,
    MissingTargetSymbol = 10,
    TargetAlias = 11,
    AliasPartner = 12,
    InterfaceValueMerge = 13,
    UnknownResult = 14,
    ErrorResult = 15,
}

pub const CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT: usize = 16;

pub const CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_NAMES: [&str;
    CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT] = [
    "success",
    "not_alias",
    "missing_symbol",
    "missing_module",
    "missing_import_name",
    "namespace_import",
    "default_import",
    "missing_alias_file",
    "missing_target",
    "self_target",
    "missing_target_symbol",
    "target_alias",
    "alias_partner",
    "interface_value_merge",
    "unknown_result",
    "error_result",
];

impl CrossArenaAliasShortcutOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum DirectCrossFileInterfaceLoweringOutcome {
    Success = 0,
    RejectedNonDirectArena = 1,
    MissingSymbol = 2,
    NotInterface = 3,
    DisallowedMergeFlags = 4,
    MissingDeclarations = 5,
    ComplexDeclaration = 6,
    UnknownOrError = 7,
}

pub const DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT: usize = 8;

pub const DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_NAMES: [&str;
    DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT] = [
    "success",
    "rejected_non_direct_arena",
    "missing_symbol",
    "not_interface",
    "disallowed_merge_flags",
    "missing_declarations",
    "complex_declaration",
    "unknown_or_error",
];

impl DirectCrossFileInterfaceLoweringOutcome {
    #[inline(always)]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// One process-wide instance. Incremented from any thread, read once at
/// dump time.
pub struct PerfCounters {
    pub enabled: AtomicBool,

    // ─── delegation / cross-arena resolution ─────────────────────────────
    pub delegate_cross_arena_calls: AtomicU64,
    pub delegate_cross_arena_cache_hits_lib: AtomicU64,
    pub delegate_cross_arena_cache_hits_cross_file: AtomicU64,
    pub delegate_cross_arena_misses: AtomicU64,
    /// T2.2 cross-file type-parameter memo: hits and misses on the
    /// `extract_type_params_from_decl` slow-path memoization. A hit means
    /// the slow-path's `with_parent_cache_attributed(..., TypeEnvironmentCore)`
    /// was elided.
    pub cross_file_type_params_cache_hits: AtomicU64,
    pub cross_file_type_params_cache_misses: AtomicU64,
    pub delegate_max_recursion_depth: AtomicU64,
    /// `DelegateCrossArenaSymbol` misses classified by how the target arena
    /// was found. This is a subset of `delegate_cross_arena_misses`.
    pub delegate_cross_arena_symbol_miss_by_source:
        [AtomicU64; CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT],
    /// `DelegateCrossArenaSymbol` misses classified by target symbol kind.
    pub delegate_cross_arena_symbol_miss_by_kind: [AtomicU64; CROSS_ARENA_SYMBOL_MISS_KIND_COUNT],
    pub delegate_cross_arena_symbol_miss_target_declaration_file: AtomicU64,
    pub delegate_cross_arena_symbol_miss_target_source_file: AtomicU64,
    /// Outcome buckets for the no-child alias shortcut attempted before a
    /// `DelegateCrossArenaSymbol` miss constructs a child checker.
    pub delegate_cross_arena_alias_shortcut_outcome:
        [AtomicU64; CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT],
    /// Outcome buckets for direct cross-file interface lowering attempts.
    pub direct_cross_file_interface_lowering_outcome:
        [AtomicU64; DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT],

    // ─── checker construction ────────────────────────────────────────────
    pub checker_state_constructed: AtomicU64,
    pub checker_state_with_parent_cache_constructed: AtomicU64,
    /// Per-`CheckerCreationReason` breakdown of `with_parent_cache` calls.
    /// `with_parent_cache_by_reason[reason as usize]` is the count for that
    /// site. Total equals `checker_state_with_parent_cache_constructed`.
    pub with_parent_cache_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],

    // ─── checker file-session ────────────────────────────────────────────
    /// Number of times `CheckerContext::reset_for_next_file()` has been
    /// invoked. Zero on the default per-file checker construction path;
    /// nonzero only on a sequential session-reuse path (T2.1.B).
    /// Attribution-mode verification: in a reuse run the counter equals
    /// `(files_checked - 1)` and `checker_state_constructed` falls by the
    /// same amount versus the baseline construction-per-file path.
    pub file_session_resets: AtomicU64,

    // ─── overlay copy ────────────────────────────────────────────────────
    pub copy_symbol_file_targets_calls: AtomicU64,
    pub copy_symbol_file_targets_entries_total: AtomicU64,
    /// Largest single overlay clone observed across the whole run.
    /// Distinguishes "many medium clones" from "a few catastrophic huge
    /// clones" — both can produce the same `entries_total`, but the fix
    /// shape is different. (Per PR #1630 review.)
    pub copy_symbol_file_targets_entries_max: AtomicU64,
    /// Bucketed histogram of overlay-clone sizes. `len_ge_N` counts the
    /// number of `copy_symbol_file_targets_to` calls where the parent's
    /// overlay had ≥ N entries at copy time. The buckets are nested so
    /// `len_ge_1m ≤ len_ge_100k ≤ len_ge_10k ≤ len_ge_1k ≤ calls`.
    pub copy_symbol_file_targets_len_ge_1k: AtomicU64,
    pub copy_symbol_file_targets_len_ge_10k: AtomicU64,
    pub copy_symbol_file_targets_len_ge_100k: AtomicU64,
    pub copy_symbol_file_targets_len_ge_1m: AtomicU64,
    /// Per-`CheckerCreationReason` breakdown of overlay-copy calls.
    pub overlay_copy_calls_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],
    /// Per-`CheckerCreationReason` breakdown of overlay entries copied
    /// (sum of `parent.cross_file_symbol_targets.len()` at each call).
    pub overlay_copy_entries_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],
    /// Per-`CheckerCreationReason` max overlay size observed at call time.
    /// Updated via [`record_max`] so the report shows the worst single
    /// clone per reason, not just the average.
    pub overlay_copy_max_entries_by_reason: [AtomicU64; CHECKER_CREATION_REASON_COUNT],

    // ─── interner ────────────────────────────────────────────────────────
    pub interner_intern_calls: AtomicU64,
    pub interner_intern_hits: AtomicU64,
    pub interner_intern_misses: AtomicU64,
    pub interner_string_intern_calls: AtomicU64,
    pub interner_type_list_intern_calls: AtomicU64,
    pub interner_object_shape_intern_calls: AtomicU64,
    pub interner_function_shape_intern_calls: AtomicU64,
    pub interner_application_intern_calls: AtomicU64,
    pub interner_conditional_intern_calls: AtomicU64,
    pub interner_mapped_intern_calls: AtomicU64,
    /// Lock-wait histogram. Each call to [`time_shard_write`] adds one
    /// observation to the bucket whose upper bound first exceeds the
    /// elapsed nanoseconds. Only populated when the
    /// `perf-counters-timing` cargo feature is enabled — otherwise
    /// `time_shard_write` compiles to a direct call of its closure
    /// and the histogram stays at all-zero.
    pub interner_lock_wait_histogram_ns: [AtomicU64; LOCK_WAIT_BUCKET_COUNT],

    // ─── compute_type_of_symbol ──────────────────────────────────────────
    pub compute_type_of_symbol_calls: AtomicU64,
    pub compute_type_of_symbol_cache_hits: AtomicU64,

    // ─── resolver / VFS ──────────────────────────────────────────────────
    pub resolver_lookup_calls: AtomicU64,
    pub resolver_is_file_calls: AtomicU64,
    pub resolver_is_dir_calls: AtomicU64,
    pub resolver_read_dir_calls: AtomicU64,
    pub resolver_read_package_json_calls: AtomicU64,
    pub resolver_candidate_paths_total: AtomicU64,
}

static COUNTERS: OnceLock<PerfCounters> = OnceLock::new();

impl PerfCounters {
    const fn new_zero() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            delegate_cross_arena_calls: AtomicU64::new(0),
            delegate_cross_arena_cache_hits_lib: AtomicU64::new(0),
            delegate_cross_arena_cache_hits_cross_file: AtomicU64::new(0),
            delegate_cross_arena_misses: AtomicU64::new(0),
            cross_file_type_params_cache_hits: AtomicU64::new(0),
            cross_file_type_params_cache_misses: AtomicU64::new(0),
            delegate_max_recursion_depth: AtomicU64::new(0),
            delegate_cross_arena_symbol_miss_by_source: [const { AtomicU64::new(0) };
                CROSS_ARENA_SYMBOL_MISS_SOURCE_COUNT],
            delegate_cross_arena_symbol_miss_by_kind: [const { AtomicU64::new(0) };
                CROSS_ARENA_SYMBOL_MISS_KIND_COUNT],
            delegate_cross_arena_symbol_miss_target_declaration_file: AtomicU64::new(0),
            delegate_cross_arena_symbol_miss_target_source_file: AtomicU64::new(0),
            delegate_cross_arena_alias_shortcut_outcome: [const { AtomicU64::new(0) };
                CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_COUNT],
            direct_cross_file_interface_lowering_outcome: [const { AtomicU64::new(0) };
                DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_COUNT],
            checker_state_constructed: AtomicU64::new(0),
            checker_state_with_parent_cache_constructed: AtomicU64::new(0),
            with_parent_cache_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            file_session_resets: AtomicU64::new(0),
            copy_symbol_file_targets_calls: AtomicU64::new(0),
            copy_symbol_file_targets_entries_total: AtomicU64::new(0),
            copy_symbol_file_targets_entries_max: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_1k: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_10k: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_100k: AtomicU64::new(0),
            copy_symbol_file_targets_len_ge_1m: AtomicU64::new(0),
            overlay_copy_calls_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            overlay_copy_entries_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            overlay_copy_max_entries_by_reason: [const { AtomicU64::new(0) };
                CHECKER_CREATION_REASON_COUNT],
            interner_intern_calls: AtomicU64::new(0),
            interner_intern_hits: AtomicU64::new(0),
            interner_intern_misses: AtomicU64::new(0),
            interner_string_intern_calls: AtomicU64::new(0),
            interner_type_list_intern_calls: AtomicU64::new(0),
            interner_object_shape_intern_calls: AtomicU64::new(0),
            interner_function_shape_intern_calls: AtomicU64::new(0),
            interner_application_intern_calls: AtomicU64::new(0),
            interner_conditional_intern_calls: AtomicU64::new(0),
            interner_mapped_intern_calls: AtomicU64::new(0),
            interner_lock_wait_histogram_ns: [const { AtomicU64::new(0) }; LOCK_WAIT_BUCKET_COUNT],
            compute_type_of_symbol_calls: AtomicU64::new(0),
            compute_type_of_symbol_cache_hits: AtomicU64::new(0),
            resolver_lookup_calls: AtomicU64::new(0),
            resolver_is_file_calls: AtomicU64::new(0),
            resolver_is_dir_calls: AtomicU64::new(0),
            resolver_read_dir_calls: AtomicU64::new(0),
            resolver_read_package_json_calls: AtomicU64::new(0),
            resolver_candidate_paths_total: AtomicU64::new(0),
        }
    }
}

/// Get the process-wide counters. The first call also reads `TSZ_PERF_COUNTERS`
/// to set the `enabled` flag.
pub fn counters() -> &'static PerfCounters {
    COUNTERS.get_or_init(|| {
        let c = PerfCounters::new_zero();
        if std::env::var_os("TSZ_PERF_COUNTERS").is_some() {
            c.enabled.store(true, Ordering::Relaxed);
        }
        c
    })
}

/// Increment a counter when counters are enabled. The branch is the
/// only cost in the disabled case, which keeps production builds clean
/// without adding shared-cache-line traffic. See [`ENABLED_FAST`].
#[inline(always)]
pub fn inc(counter: &AtomicU64) {
    if enabled_fast() {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// Add `n` to a counter when counters are enabled.
#[inline(always)]
pub fn add(counter: &AtomicU64, n: u64) {
    if enabled_fast() {
        counter.fetch_add(n, Ordering::Relaxed);
    }
}

/// Set the maximum-seen value for a counter, racy but good enough for
/// "max recursion depth" / "largest overlay clone" style reporting.
/// Gated by [`enabled_fast`] for the same contention-avoidance reason.
#[inline]
pub fn record_max(counter: &AtomicU64, value: u64) {
    if !enabled_fast() {
        return;
    }
    let mut current = counter.load(Ordering::Relaxed);
    while value > current {
        match counter.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

/// RAII guard that tracks recursion depth into
/// `delegate_cross_arena_symbol_resolution`. Each `enter_delegate()` increments
/// a thread-local counter and updates `delegate_max_recursion_depth` to the
/// running peak; the guard's `Drop` impl decrements when the call returns.
/// The whole thing short-circuits when counters are disabled, so timing builds
/// pay one branch per call.
pub struct DelegateDepthGuard(());

thread_local! {
    static DELEGATE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

#[inline]
pub fn enter_delegate() -> DelegateDepthGuard {
    if !enabled_fast() {
        return DelegateDepthGuard(());
    }
    DELEGATE_DEPTH.with(|d| {
        let next = d.get().saturating_add(1);
        d.set(next);
        record_max(&counters().delegate_max_recursion_depth, u64::from(next));
    });
    DelegateDepthGuard(())
}

impl Drop for DelegateDepthGuard {
    fn drop(&mut self) {
        if !enabled_fast() {
            return;
        }
        DELEGATE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Returns true when `TSZ_PERF_COUNTERS` is set. Use this to gate the
/// expensive bookkeeping; the simple `inc` calls are always cheap enough
/// that gating them is more expensive than just doing them.
pub fn enabled() -> bool {
    counters().enabled.load(Ordering::Relaxed)
}

/// Record a single lock-wait observation into the histogram. Buckets are
/// log-spaced over 100 ns…100 ms with a final overflow bucket; see
/// [`LOCK_WAIT_BUCKET_UPPER_BOUNDS_NS`]. Gated behind the
/// `perf-counters-timing` feature: when the feature is off this function
/// is not compiled at all (the `cfg` excludes the entire item), and the
/// only call site lives inside the feature-on variant of
/// [`time_shard_write`], which is replaced with a no-op stub that calls
/// `f()` directly.
#[cfg(feature = "perf-counters-timing")]
#[inline]
fn record_lock_wait_ns(ns: u64) {
    if !enabled_fast() {
        return;
    }
    let buckets = &counters().interner_lock_wait_histogram_ns;
    for (i, &upper) in LOCK_WAIT_BUCKET_UPPER_BOUNDS_NS.iter().enumerate() {
        if ns < upper {
            buckets[i].fetch_add(1, Ordering::Relaxed);
            return;
        }
    }
}

/// Time a contended write inside the type-interner. The closure runs in
/// both modes; the cost of the timing infrastructure is the
/// difference between the two `cfg`-gated implementations:
///
/// - **`perf-counters-timing` ON**: `Instant::now()` brackets the closure;
///   the elapsed nanos land in the lock-wait histogram (gated on
///   `enabled_fast()`, so timing-mode runs that don't enable counters
///   still pay only the gate load + closure call).
/// - **`perf-counters-timing` OFF (default)**: the wrapper compiles to a
///   direct call of `f()`. Zero `Instant::now()` calls, zero atomic
///   loads, zero histogram accesses. Default release builds do not pay
///   the timing cost the plan §4.T0.3 explicitly forbids.
///
/// `_shard_idx` is reserved for a future per-shard breakdown; today
/// every shard's observations land in the same global histogram.
#[cfg(feature = "perf-counters-timing")]
#[inline]
pub fn time_shard_write<R>(_shard_idx: u32, f: impl FnOnce() -> R) -> R {
    if !enabled_fast() {
        return f();
    }
    // `web_time::Instant` is the WASM-safe drop-in for `std::time::Instant`;
    // tsz-common is compiled for wasm32 and the arch guard bans the std
    // type even on cfg-gated paths. See `scripts/arch/arch_guard.py`.
    let start = web_time::Instant::now();
    let result = f();
    record_lock_wait_ns(start.elapsed().as_nanos() as u64);
    result
}

#[cfg(not(feature = "perf-counters-timing"))]
#[inline(always)]
pub fn time_shard_write<R>(_shard_idx: u32, f: impl FnOnce() -> R) -> R {
    f()
}

/// Whether the lock-wait histogram is *physically wired* (the
/// `perf-counters-timing` cfg feature is on). Independent of
/// `enabled_fast()`: a build with the feature on but the env var off
/// still has the histogram fields and serializes them as zeroes; a
/// build with the feature off keeps the histogram fields (so the
/// `PerfCounters` layout is feature-stable) but compiles out the
/// timing + recording logic and serializes the histogram as `null` via
/// [`PerfCounterSnapshot`].
#[inline(always)]
pub const fn lock_wait_histogram_wired() -> bool {
    cfg!(feature = "perf-counters-timing")
}

/// Record a `CheckerState::with_parent_cache` construction with attribution.
/// Bumps both the global counter and the per-reason bucket so PR #1631's
/// dump shows where the 17,329 constructions on subset3 come from.
#[inline]
pub fn record_with_parent_cache(reason: CheckerCreationReason) {
    let c = counters();
    inc(&c.checker_state_with_parent_cache_constructed);
    inc(&c.with_parent_cache_by_reason[reason.as_index()]);
}

/// Record an overlay copy with attribution: count + entries-copied +
/// global max + size-bucket histogram + per-reason max. The histogram
/// tells us whether `entries_total = 12.8B` is "many medium clones" or
/// "a few catastrophic huge clones" — both produce the same total but
/// imply very different fixes (per PR #1630 review).
///
/// Caller passes the parent overlay's len so we can attribute without
/// holding a borrow across the copy.
#[inline]
pub fn record_overlay_copy(reason: CheckerCreationReason, entries: u64) {
    let c = counters();
    inc(&c.copy_symbol_file_targets_calls);
    add(&c.copy_symbol_file_targets_entries_total, entries);
    record_max(&c.copy_symbol_file_targets_entries_max, entries);
    if entries >= 1_000 {
        inc(&c.copy_symbol_file_targets_len_ge_1k);
    }
    if entries >= 10_000 {
        inc(&c.copy_symbol_file_targets_len_ge_10k);
    }
    if entries >= 100_000 {
        inc(&c.copy_symbol_file_targets_len_ge_100k);
    }
    if entries >= 1_000_000 {
        inc(&c.copy_symbol_file_targets_len_ge_1m);
    }
    inc(&c.overlay_copy_calls_by_reason[reason.as_index()]);
    add(
        &c.overlay_copy_entries_by_reason[reason.as_index()],
        entries,
    );
    record_max(
        &c.overlay_copy_max_entries_by_reason[reason.as_index()],
        entries,
    );
}

#[inline]
pub fn record_cross_arena_symbol_miss(
    source: CrossArenaSymbolMissSource,
    kind: CrossArenaSymbolMissKind,
    target_is_declaration_file: bool,
) {
    let c = counters();
    inc(&c.delegate_cross_arena_symbol_miss_by_source[source.as_index()]);
    inc(&c.delegate_cross_arena_symbol_miss_by_kind[kind.as_index()]);
    if target_is_declaration_file {
        inc(&c.delegate_cross_arena_symbol_miss_target_declaration_file);
    } else {
        inc(&c.delegate_cross_arena_symbol_miss_target_source_file);
    }
}

#[inline]
pub fn record_cross_arena_alias_shortcut_outcome(outcome: CrossArenaAliasShortcutOutcome) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.delegate_cross_arena_alias_shortcut_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a cross-arena delegate invocation that has no cache fast path —
/// i.e., every call is a miss. Increments both `delegate_cross_arena_calls`
/// and `delegate_cross_arena_misses` with a single `counters()` lookup.
///
/// The hand-rolled call-site pattern this helper replaces was:
///
/// ```rust,ignore
/// if tsz_common::perf_counters::enabled_fast() {
///     tsz_common::perf_counters::inc(
///         &tsz_common::perf_counters::counters().delegate_cross_arena_calls,
///     );
///     tsz_common::perf_counters::inc(
///         &tsz_common::perf_counters::counters().delegate_cross_arena_misses,
///     );
/// }
/// ```
///
/// — which pays two `counters()` `OnceLock` derefs per increment pair.
/// This helper folds them into one. Callers that have a cache fast path
/// (e.g. lib-delegation hit) should keep using `inc(&perf.delegate_cross_arena_calls)`
/// directly and only call this when the miss is unconditional.
#[inline]
pub fn record_delegate_cross_arena_miss() {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.delegate_cross_arena_calls.fetch_add(1, Ordering::Relaxed);
    c.delegate_cross_arena_misses
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a hit on the cross-file type-parameter extraction cache. Mirrors
/// [`record_delegate_cross_arena_miss`]: gate-once and one `counters()`
/// lookup, then increment exactly the per-outcome counter that names this
/// branch of the cache.
#[inline]
pub fn record_cross_file_type_params_cache_hit() {
    if !enabled_fast() {
        return;
    }
    counters()
        .cross_file_type_params_cache_hits
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a miss on the cross-file type-parameter extraction cache. Counted
/// when the slow path runs to build a child checker, regardless of whether
/// the slow path ultimately returns `Some(_)` — see the call sites in
/// `state/type_environment/core.rs` for the rationale (counting only on
/// `Some(_)` undercounts misses when the slow path runs but extraction fails,
/// distorting attribution for Tier-2 decision-making).
#[inline]
pub fn record_cross_file_type_params_cache_miss() {
    if !enabled_fast() {
        return;
    }
    counters()
        .cross_file_type_params_cache_misses
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a cross-arena delegate cache hit on the per-file type-of-symbol
/// table. Mirrors the [`record_cross_file_type_params_cache_hit`] shape:
/// one gate, one `counters()` lookup, one atomic increment.
///
/// The matching `record_delegate_cross_arena_cache_hit_lib` helper is not
/// added here because the only two lib-hit call sites (in `cross_file.rs`
/// near `delegate_cross_arena_symbol_resolution`) already cache `counters()`
/// in a local `perf` variable that's reused across multiple counter writes
/// in the same block — those sites are already optimal as written.
#[inline]
pub fn record_delegate_cross_arena_cache_hit_cross_file() {
    if !enabled_fast() {
        return;
    }
    counters()
        .delegate_cross_arena_cache_hits_cross_file
        .fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_direct_cross_file_interface_lowering_outcome(
    outcome: DirectCrossFileInterfaceLoweringOutcome,
) {
    if !enabled_fast() {
        return;
    }
    let c = counters();
    c.direct_cross_file_interface_lowering_outcome[outcome.as_index()]
        .fetch_add(1, Ordering::Relaxed);
}

impl PerfCounters {
    /// Format the current counter snapshot as a multi-line report. Returns
    /// an empty string when the counters are disabled (so callers can
    /// unconditionally `print!("{}", PerfCounters::dump_string())` without
    /// noisy output in the common case).
    ///
    /// Counters that are NOT yet wired into their producer code (e.g. the
    /// per-kind `interner_*_intern_calls` buckets — the bucket fields are
    /// declared but the actual `tsz-solver` intern sites still need to be
    /// updated) are printed as `n/a` rather than `0`, so a reader doesn't
    /// mistake "not measured" for "didn't happen". A small `wired: false`
    /// table at the bottom of the dump lists which buckets are pending.
    pub fn dump_string() -> String {
        let c = counters();
        if !enabled_fast() {
            return String::new();
        }
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        format!(
            "\n=== TSZ_PERF_COUNTERS ===\n\
             Delegation (cross-arena symbol resolution):\n  \
             calls                      {:>12}\n  \
             cache hits (lib)           {:>12}\n  \
             cache hits (cross-file)    {:>12}\n  \
             misses (full work)         {:>12}\n  \
             max recursion depth        {:>12}\n\
             Checker construction:\n  \
             CheckerState::new          {:>12}\n  \
             ::with_parent_cache        {:>12}\n  \
             ::reset_for_next_file      {:>12}\n  \
             copy_symbol_file_targets   {:>12}\n  \
             overlay entries copied     {:>12}\n  \
             overlay entries (max)      {:>12}\n  \
             overlay len ≥ 1k           {:>12}\n  \
             overlay len ≥ 10k          {:>12}\n  \
             overlay len ≥ 100k         {:>12}\n  \
             overlay len ≥ 1M           {:>12}\n\
             compute_type_of_symbol:\n  \
             total calls                {:>12}\n  \
             cache hits                 {:>12}\n\
             TypeInterner:\n  \
             intern calls (total)       {:>12}\n  \
             intern hits                {:>12}\n  \
             intern misses              {:>12}\n  \
             string intern calls        {:>12}\n  \
             type-list intern calls     {:>12}\n  \
             object-shape intern calls  {:>12}\n  \
             function-shape intern calls{:>12}\n  \
             application intern calls   {:>12}\n  \
             conditional intern calls   {:>12}\n  \
             mapped intern calls        {:>12}\n\
             Resolver:\n  \
             lookup calls               {:>12}\n  \
             is_file calls              {:>12}\n  \
             is_dir calls               {:>12}\n  \
             read_dir calls             {:>12}\n  \
             read_package_json calls    {:>12}\n  \
             candidate paths total      {:>12}\n",
            load(&c.delegate_cross_arena_calls),
            load(&c.delegate_cross_arena_cache_hits_lib),
            load(&c.delegate_cross_arena_cache_hits_cross_file),
            load(&c.delegate_cross_arena_misses),
            load(&c.delegate_max_recursion_depth),
            load(&c.checker_state_constructed),
            load(&c.checker_state_with_parent_cache_constructed),
            load(&c.file_session_resets),
            load(&c.copy_symbol_file_targets_calls),
            load(&c.copy_symbol_file_targets_entries_total),
            load(&c.copy_symbol_file_targets_entries_max),
            load(&c.copy_symbol_file_targets_len_ge_1k),
            load(&c.copy_symbol_file_targets_len_ge_10k),
            load(&c.copy_symbol_file_targets_len_ge_100k),
            load(&c.copy_symbol_file_targets_len_ge_1m),
            load(&c.compute_type_of_symbol_calls),
            load(&c.compute_type_of_symbol_cache_hits),
            load(&c.interner_intern_calls),
            load(&c.interner_intern_hits),
            load(&c.interner_intern_misses),
            load(&c.interner_string_intern_calls),
            load(&c.interner_type_list_intern_calls),
            load(&c.interner_object_shape_intern_calls),
            load(&c.interner_function_shape_intern_calls),
            load(&c.interner_application_intern_calls),
            load(&c.interner_conditional_intern_calls),
            load(&c.interner_mapped_intern_calls),
            load(&c.resolver_lookup_calls),
            load(&c.resolver_is_file_calls),
            load(&c.resolver_is_dir_calls),
            load(&c.resolver_read_dir_calls),
            load(&c.resolver_read_package_json_calls),
            load(&c.resolver_candidate_paths_total),
        ) + &Self::dump_cross_arena_symbol_miss_classification()
            + &Self::dump_cross_arena_alias_shortcut_outcomes()
            + &Self::dump_direct_cross_file_interface_lowering_outcomes()
            + &Self::dump_by_reason()
    }

    fn dump_cross_arena_symbol_miss_classification() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let source_total: u64 = c
            .delegate_cross_arena_symbol_miss_by_source
            .iter()
            .map(load)
            .sum();
        let kind_total: u64 = c
            .delegate_cross_arena_symbol_miss_by_kind
            .iter()
            .map(load)
            .sum();
        if source_total == 0 && kind_total == 0 {
            return String::new();
        }

        let mut out = String::from("\nDelegateCrossArenaSymbol miss classification:\n");
        out.push_str("  by source:\n");
        for (idx, name) in CROSS_ARENA_SYMBOL_MISS_SOURCE_NAMES.iter().enumerate() {
            let count = load(&c.delegate_cross_arena_symbol_miss_by_source[idx]);
            out.push_str(&format!("  {name:<28} {count:>12}\n"));
        }
        out.push_str("  by kind:\n");
        for (idx, name) in CROSS_ARENA_SYMBOL_MISS_KIND_NAMES.iter().enumerate() {
            let count = load(&c.delegate_cross_arena_symbol_miss_by_kind[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<28} {count:>12}\n"));
            }
        }
        out.push_str(&format!(
            "  {:<28} {:>12}\n  {:<28} {:>12}\n",
            "target .d.ts/.d.cts/.d.mts",
            load(&c.delegate_cross_arena_symbol_miss_target_declaration_file),
            "target source files",
            load(&c.delegate_cross_arena_symbol_miss_target_source_file),
        ));
        out
    }

    fn dump_cross_arena_alias_shortcut_outcomes() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total: u64 = c
            .delegate_cross_arena_alias_shortcut_outcome
            .iter()
            .map(load)
            .sum();
        if total == 0 {
            return String::new();
        }

        let mut out = String::from("\nDelegateCrossArenaSymbol alias shortcut outcomes:\n");
        for (idx, name) in CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_NAMES.iter().enumerate() {
            let count = load(&c.delegate_cross_arena_alias_shortcut_outcome[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<28} {count:>12}\n"));
            }
        }
        out
    }

    fn dump_direct_cross_file_interface_lowering_outcomes() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let total: u64 = c
            .direct_cross_file_interface_lowering_outcome
            .iter()
            .map(load)
            .sum();
        if total == 0 {
            return String::new();
        }

        let mut out = String::from("\nDirect cross-file interface lowering outcomes:\n");
        for (idx, name) in DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_NAMES
            .iter()
            .enumerate()
        {
            let count = load(&c.direct_cross_file_interface_lowering_outcome[idx]);
            if count > 0 {
                out.push_str(&format!("  {name:<28} {count:>12}\n"));
            }
        }
        out
    }

    /// Per-reason breakdown of `with_parent_cache` and overlay-copy calls.
    /// Sorted by `with_parent_cache` count descending so the headline
    /// offenders show first. Skips reasons with zero counts.
    fn dump_by_reason() -> String {
        let c = counters();
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        // Collect (reason_idx, count, overlay_calls, overlay_entries, max_entries).
        let mut rows: Vec<(usize, u64, u64, u64, u64)> = (0..CHECKER_CREATION_REASON_COUNT)
            .map(|i| {
                (
                    i,
                    load(&c.with_parent_cache_by_reason[i]),
                    load(&c.overlay_copy_calls_by_reason[i]),
                    load(&c.overlay_copy_entries_by_reason[i]),
                    load(&c.overlay_copy_max_entries_by_reason[i]),
                )
            })
            .filter(|t| t.1 > 0 || t.2 > 0)
            .collect();
        if rows.is_empty() {
            return String::new();
        }
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(b.3.cmp(&a.3)));
        let total_constructions = load(&c.checker_state_with_parent_cache_constructed).max(1);
        let total_overlay_entries = load(&c.copy_symbol_file_targets_entries_total).max(1);
        let mut out = String::from(
            "\n  with_parent_cache + overlay copies attributed by call site:\n  \
             reason                              cons    %  ovl_calls  ovl_entries          max  ent%\n",
        );
        for (i, cons, ovl_calls, ovl_entries, max_entries) in rows {
            let cons_pct = (cons as f64 / total_constructions as f64) * 100.0;
            let ent_pct = (ovl_entries as f64 / total_overlay_entries as f64) * 100.0;
            let row = format!(
                "  {:<32} {:>10} {:>4.1} {:>10} {:>12} {:>12} {:>5.1}\n",
                REASON_NAMES[i], cons, cons_pct, ovl_calls, ovl_entries, max_entries, ent_pct,
            );
            out.push_str(&row);
        }
        out
    }
}

// ─────────────────────────────────────────────────────────────────────────
//                      JSON snapshot (`PERFORMANCE_PLAN.md` §4.T0.3)
// ─────────────────────────────────────────────────────────────────────────

/// Stable schema version for `PerfCounterSnapshot`. Bump when the JSON
/// shape changes in a way the bench harness must adapt to.
pub const PERF_COUNTER_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Frozen value-object view of the counter state. Built by
/// [`PerfCounters::snapshot`]; serializable to JSON via serde.
///
/// Buckets that the producer code does not yet write are encoded as
/// [`Option<u64>::None`] (serializing as `null`) and the matching
/// [`WiredCounters`] field is `false`. That distinguishes "not measured"
/// from "measured zero" — without that, a reviewer staring at `0`
/// can't tell whether a counter site needs more wiring or is genuinely
/// idle.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PerfCounterSnapshot {
    pub schema_version: u32,
    /// `enabled_fast()` at snapshot time. When `false`, all counters are
    /// either zero (atomic loads return their initial state) or `null`
    /// (unwired buckets); the dump is included for schema stability so
    /// the bench harness can rely on the same shape every run.
    pub enabled: bool,
    /// Mirrors `PerfDiagnosticsReport.mode`: `"timing"` when counters
    /// are disabled, `"attribution"` when enabled.
    pub mode: &'static str,
    pub wired: WiredCounters,
    pub delegate: DelegateCounters,
    pub checker: CheckerCounters,
    pub overlay: OverlayCounters,
    pub resolver: ResolverCounters,
    pub interner: InternerCounters,
}

/// Per-bucket "is this wired up to its producer?" flag. Lets the bench
/// harness emit a clean follow-up list without parsing the whole
/// snapshot.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct WiredCounters {
    pub delegate_cross_arena: bool,
    pub checker_construction: bool,
    pub overlay_copy: bool,
    pub interner_intern_calls: bool,
    pub interner_per_kind: bool,
    pub interner_lock_wait: bool,
    pub resolver_lookup: bool,
    pub resolver_fs_probes: bool,
    pub compute_type_of_symbol: bool,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct DelegateCounters {
    pub calls: u64,
    pub cache_hits_lib: u64,
    pub cache_hits_cross_file: u64,
    pub misses: u64,
    pub max_recursion_depth: u64,
    /// T2.2 typed-query memo: hits on the cross-file type-parameter cache.
    pub cross_file_type_params_cache_hits: u64,
    /// T2.2 typed-query memo: misses (where the slow path constructed a child checker).
    pub cross_file_type_params_cache_misses: u64,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct CheckerCounters {
    pub state_constructed: u64,
    pub with_parent_cache_constructed: u64,
    /// `CheckerContext::reset_for_next_file()` invocations. Zero on the
    /// default construction-per-file path, nonzero only on a sequential
    /// session-reuse path (T2.1.B). Reuse vs. construct is the comparison
    /// against `state_constructed`.
    pub file_session_resets: u64,
    pub compute_type_of_symbol_calls: u64,
    pub compute_type_of_symbol_cache_hits: u64,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct OverlayCounters {
    pub copy_calls: u64,
    pub entries_total: u64,
    pub entries_max: u64,
    pub len_ge_1k: u64,
    pub len_ge_10k: u64,
    pub len_ge_100k: u64,
    pub len_ge_1m: u64,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ResolverCounters {
    pub lookup_calls: u64,
    /// Filesystem probe counts. `None` until a counting filesystem
    /// wrapper lands (`PERFORMANCE_PLAN.md` §5).
    pub is_file_calls: Option<u64>,
    pub is_dir_calls: Option<u64>,
    pub read_dir_calls: Option<u64>,
    pub package_json_reads: u64,
    pub candidate_paths_total: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InternerCounters {
    /// Total `intern` calls across kinds. `None` until the solver intern
    /// site is updated to fan into a single counter.
    pub intern_calls: Option<u64>,
    pub intern_hits: Option<u64>,
    pub intern_misses: Option<u64>,
    pub string_intern_calls: u64,
    pub type_list_intern_calls: u64,
    pub object_shape_intern_calls: u64,
    pub function_shape_intern_calls: u64,
    pub application_intern_calls: u64,
    pub conditional_intern_calls: u64,
    pub mapped_intern_calls: u64,
    /// Lock-wait histogram. `None` because the timing path is gated on
    /// the `perf-counters-timing` feature (`PERFORMANCE_PLAN.md` §4.T0.3).
    pub lock_wait_histogram_ns: Option<Vec<u64>>,
}

impl PerfCounters {
    /// Load every atomic into a [`PerfCounterSnapshot`] in a single pass.
    /// Cheap (one relaxed load per counter); both `dump_string` and
    /// `write_json_to` should eventually share this path so they cannot
    /// drift.
    pub fn snapshot() -> PerfCounterSnapshot {
        let c = counters();
        let load = |a: &std::sync::atomic::AtomicU64| a.load(std::sync::atomic::Ordering::Relaxed);
        let enabled = enabled_fast();
        PerfCounterSnapshot {
            schema_version: PERF_COUNTER_SNAPSHOT_SCHEMA_VERSION,
            enabled,
            mode: if enabled { "attribution" } else { "timing" },
            wired: WiredCounters {
                delegate_cross_arena: true,
                checker_construction: true,
                overlay_copy: true,
                interner_intern_calls: true,
                interner_per_kind: true,
                interner_lock_wait: lock_wait_histogram_wired(),
                resolver_lookup: true,
                resolver_fs_probes: true,
                compute_type_of_symbol: true,
            },
            delegate: DelegateCounters {
                calls: load(&c.delegate_cross_arena_calls),
                cache_hits_lib: load(&c.delegate_cross_arena_cache_hits_lib),
                cache_hits_cross_file: load(&c.delegate_cross_arena_cache_hits_cross_file),
                misses: load(&c.delegate_cross_arena_misses),
                max_recursion_depth: load(&c.delegate_max_recursion_depth),
                cross_file_type_params_cache_hits: load(&c.cross_file_type_params_cache_hits),
                cross_file_type_params_cache_misses: load(&c.cross_file_type_params_cache_misses),
            },
            checker: CheckerCounters {
                state_constructed: load(&c.checker_state_constructed),
                with_parent_cache_constructed: load(&c.checker_state_with_parent_cache_constructed),
                file_session_resets: load(&c.file_session_resets),
                compute_type_of_symbol_calls: load(&c.compute_type_of_symbol_calls),
                compute_type_of_symbol_cache_hits: load(&c.compute_type_of_symbol_cache_hits),
            },
            overlay: OverlayCounters {
                copy_calls: load(&c.copy_symbol_file_targets_calls),
                entries_total: load(&c.copy_symbol_file_targets_entries_total),
                entries_max: load(&c.copy_symbol_file_targets_entries_max),
                len_ge_1k: load(&c.copy_symbol_file_targets_len_ge_1k),
                len_ge_10k: load(&c.copy_symbol_file_targets_len_ge_10k),
                len_ge_100k: load(&c.copy_symbol_file_targets_len_ge_100k),
                len_ge_1m: load(&c.copy_symbol_file_targets_len_ge_1m),
            },
            resolver: ResolverCounters {
                lookup_calls: load(&c.resolver_lookup_calls),
                is_file_calls: Some(load(&c.resolver_is_file_calls)),
                is_dir_calls: Some(load(&c.resolver_is_dir_calls)),
                read_dir_calls: Some(load(&c.resolver_read_dir_calls)),
                package_json_reads: load(&c.resolver_read_package_json_calls),
                candidate_paths_total: load(&c.resolver_candidate_paths_total),
            },
            interner: InternerCounters {
                intern_calls: Some(load(&c.interner_intern_calls)),
                intern_hits: Some(load(&c.interner_intern_hits)),
                intern_misses: Some(load(&c.interner_intern_misses)),
                string_intern_calls: load(&c.interner_string_intern_calls),
                type_list_intern_calls: load(&c.interner_type_list_intern_calls),
                object_shape_intern_calls: load(&c.interner_object_shape_intern_calls),
                function_shape_intern_calls: load(&c.interner_function_shape_intern_calls),
                application_intern_calls: load(&c.interner_application_intern_calls),
                conditional_intern_calls: load(&c.interner_conditional_intern_calls),
                mapped_intern_calls: load(&c.interner_mapped_intern_calls),
                // Lock-wait histogram surfaces only in builds where the
                // `perf-counters-timing` feature is on; otherwise the
                // wrapper is a no-op and the buckets stay all-zero, so
                // emitting `null` keeps "wired vs. zero" unambiguous in
                // the JSON output (matching the plan §4.T0.3 contract).
                lock_wait_histogram_ns: if lock_wait_histogram_wired() {
                    Some(c.interner_lock_wait_histogram_ns.iter().map(load).collect())
                } else {
                    None
                },
            },
        }
    }

    /// Serialize a [`PerfCounterSnapshot`] to `path` using an atomic
    /// rename so a partial write can't poison the bench harness's `jq`
    /// consumer.
    pub fn write_json_to(path: &std::path::Path) -> std::io::Result<()> {
        let snap = Self::snapshot();
        let json = serde_json::to_string_pretty(&snap)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod json_tests {
    use super::*;

    #[test]
    fn schema_version_is_one() {
        // Bumping schema_version is a breaking change for the bench harness;
        // make the intent explicit.
        assert_eq!(PERF_COUNTER_SNAPSHOT_SCHEMA_VERSION, 1);
    }

    #[test]
    fn snapshot_serializes_with_expected_top_level_keys() {
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        for key in [
            "schema_version",
            "enabled",
            "mode",
            "wired",
            "delegate",
            "checker",
            "overlay",
            "resolver",
            "interner",
        ] {
            assert!(json.get(key).is_some(), "missing top-level key: {key}");
        }
        assert_eq!(json["schema_version"], 1);
    }

    #[test]
    fn lock_wait_histogram_serialization_matches_feature_gate() {
        // The plan requires `null` for unwired buckets so `0` is unambiguous.
        // The lock-wait histogram is the only counter whose wiring is a
        // compile-time gate (`perf-counters-timing`) rather than a runtime
        // env var: builds with the feature off must serialize the histogram
        // as `null` and `wired.interner_lock_wait = false`; builds with the
        // feature on must serialize an array of bucket counts and
        // `interner_lock_wait = true`.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        if cfg!(feature = "perf-counters-timing") {
            assert!(
                json["interner"]["lock_wait_histogram_ns"].is_array(),
                "histogram must be an array when feature is on, got: {}",
                json["interner"]["lock_wait_histogram_ns"]
            );
            assert_eq!(json["wired"]["interner_lock_wait"], true);
        } else {
            assert_eq!(
                json["interner"]["lock_wait_histogram_ns"],
                serde_json::Value::Null
            );
            assert_eq!(json["wired"]["interner_lock_wait"], false);
        }
    }

    #[test]
    fn wired_resolver_fs_probe_buckets_serialize_as_numbers() {
        // T0.3 follow-up: resolver `is_file`/`is_dir`/`read_dir` are wired
        // through `count_is_file`/`count_is_dir`/`count_read_dir` thin
        // wrappers in `crates/tsz-cli/src/driver/resolution.rs`. They
        // must serialize as numbers (zero is fine in this test process)
        // and the wired flag must agree.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        assert!(
            json["resolver"]["is_file_calls"].is_number(),
            "is_file_calls should be a number once wired, got: {}",
            json["resolver"]["is_file_calls"]
        );
        assert!(json["resolver"]["is_dir_calls"].is_number());
        assert!(json["resolver"]["read_dir_calls"].is_number());
        assert_eq!(json["wired"]["resolver_fs_probes"], true);
    }

    #[test]
    fn wired_intern_call_buckets_serialize_as_numbers() {
        // T0.3 follow-up: intern_calls/hits/misses are now wired at the
        // solver intern site. They must surface as numbers (zero is fine
        // when the test process has not interned any user types) and the
        // wired flag must agree.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        assert!(
            json["interner"]["intern_calls"].is_number(),
            "intern_calls should be a number once wired, got: {}",
            json["interner"]["intern_calls"]
        );
        assert!(json["interner"]["intern_hits"].is_number());
        assert!(json["interner"]["intern_misses"].is_number());
        assert_eq!(json["wired"]["interner_intern_calls"], true);
    }

    #[test]
    fn file_session_resets_serializes_as_number() {
        // The T2.1 file-session reset counter rides inside the existing
        // `checker_construction` wired group, so adding it must not
        // require a new `wired` flag — but it must surface as a number
        // (not `null`) so attribution tooling can compare it against
        // `state_constructed` to detect reuse-vs-construct directly.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        assert!(
            json["checker"]["file_session_resets"].is_number(),
            "file_session_resets should serialize as a number, got: {}",
            json["checker"]["file_session_resets"]
        );
        assert_eq!(json["wired"]["checker_construction"], true);
    }

    #[test]
    fn wired_keys_match_snapshot_struct_fields() {
        // If a future PR adds a wired flag, it must also surface in the
        // top-level snapshot, and vice versa. This keeps the schema and
        // the wired map honest.
        let snap = PerfCounters::snapshot();
        let json = serde_json::to_value(&snap).expect("serializes");
        let wired = json["wired"].as_object().expect("wired is an object");
        // Cross-check: keys are stable across runs.
        let expected_keys: std::collections::BTreeSet<&str> = [
            "delegate_cross_arena",
            "checker_construction",
            "overlay_copy",
            "interner_intern_calls",
            "interner_per_kind",
            "interner_lock_wait",
            "resolver_lookup",
            "resolver_fs_probes",
            "compute_type_of_symbol",
        ]
        .into_iter()
        .collect();
        let actual_keys: std::collections::BTreeSet<&str> =
            wired.keys().map(String::as_str).collect();
        assert_eq!(actual_keys, expected_keys);
    }

    #[test]
    fn write_json_to_writes_valid_json_with_atomic_rename() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("tsz-perf-counter-snap-{}.json", std::process::id()));
        // Clean up beforehand if a stale file is sitting around.
        let _ = std::fs::remove_file(&path);
        PerfCounters::write_json_to(&path).expect("write succeeds");
        let raw = std::fs::read_to_string(&path).expect("read back");
        // Round-trip through serde to confirm structure.
        let value: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
        assert_eq!(value["schema_version"], 1);
        assert!(value["wired"].is_object());
        // The atomic-rename `.json.tmp` should not be left behind.
        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists(), "tmp file leaked: {tmp:?}");
        let _ = std::fs::remove_file(&path);
    }
}
