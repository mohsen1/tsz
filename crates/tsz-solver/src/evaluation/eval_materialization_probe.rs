//! Measurement-only probe of recursive type-evaluator materialization
//! (issue #13250, ts-toolbelt / type-fest `AutoPath`/`MetaPath` lever).
//!
//! The recursive `TypeEvaluator` eagerly interns full conditional / mapped /
//! application expansions, materializing many distinct intermediate shapes.
//! Before committing to the large (4-8 week, high-conformance-risk) deferred /
//! lazy-materialization change, this probe quantifies the *headroom* of three
//! candidate sub-strategies so the campaign can be redirected on evidence:
//!
//! 1. **Distinct intermediate shapes** — how many distinct result `TypeId`s the
//!    eval engine materializes for conditional / mapped / application inputs.
//! 2. **Structural-dedup headroom (hash-cons)** — the central `TypeInterner`
//!    *already* hash-conses: structurally identical `TypeData` collapses to one
//!    `TypeId` (see `crates/tsz-solver/src/types.rs` `TypeData` doc and
//!    `intern()` in `intern/core/interner.rs`). So additional hash-consing of
//!    the *result* buys nothing unless two distinct result `TypeId`s carry
//!    structurally equal `TypeData` (a hash-cons *gap*). We count those gaps;
//!    an ~0 count is the finding that interner-level dedup is already complete,
//!    redirecting the headroom question to *recomputation* (below).
//! 3. **Recompute headroom (defer / memo)** — `computes - distinct_inputs`:
//!    how many computes re-derive a result for an input `TypeId` already
//!    computed earlier. This is the work a deferred-materialization or a
//!    longer-lived eval memo would skip (the interner dedups the *output* but
//!    the recursion still *runs* to produce it).
//! 4. **Defer headroom (conditionals)** — of conditional computes, how many
//!    resolve eagerly to a concrete branch vs stay deferred (result is still a
//!    `Conditional`). `tsc` keeps more conditionals deferred; eager computes
//!    whose input repeats are the redundant-eager-eval headroom.
//! 5. **Fan-out** — distinct intermediate result shapes vs distinct inputs, and
//!    total computes vs distinct results: the "wasted" intermediate
//!    materialization the lever would collapse.
//!
//! Everything is gated on `TSZ_PERF_COUNTERS` via
//! [`tsz_common::perf_counters::enabled_fast`]. In a normal run every hook is a
//! single predictable branch and the global maps are never touched. The probe
//! never feeds back into evaluation: it only accumulates measurement state and
//! is read once at end-of-run via [`dump_report`]. Default behavior is
//! unchanged.
//!
//! State is process-wide (the evaluator runs across rayon worker threads), so
//! distinct-shape sets use [`DashSet`] and the scalar totals a small
//! [`Mutex`]-guarded struct, matching the existing perf-counter conventions.

use crate::caches::db::{QueryDatabase, TypeDatabase};
use crate::types::{TypeData, TypeId};
use dashmap::{DashMap, DashSet};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHasher};
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tsz_common::perf_counters;

/// Test-only force flag so the probe's own unit tests can drive the recorder
/// without relying on `tsz_common`'s `debug_assertions`-gated force hook (which
/// is not compiled when the dependency is built `--release`). Compiled only in
/// the probe's own `cfg(test)` builds; production builds keep the single
/// `enabled_fast` gate.
#[cfg(test)]
static FORCE_PROBE_FOR_TESTS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// The probe gate: process-wide `TSZ_PERF_COUNTERS` (via `enabled_fast`), plus
/// a `cfg(test)`-only force flag. One predictable branch on the hot path in a
/// production build (the `cfg(test)` arm does not exist there).
#[inline]
fn gate_enabled() -> bool {
    #[cfg(test)]
    if FORCE_PROBE_FOR_TESTS.load(Ordering::Relaxed) {
        return true;
    }
    perf_counters::enabled_fast()
}

/// SCC re-entry probe (#14101 step-2 headroom). Counts how often a `DefId`'s
/// application is re-entered while already in-flight up the evaluation stack
/// (a recursive-heritage back-edge — `def_depth[def_id] >= 1`), plus the deepest
/// such prior depth. This is the "redundant per-member re-eval" headroom that
/// decides whether the SCC materialize-once fixpoint is worth its soundness
/// cost; pure instrumentation, gated on the probe, no behavior change.
static DEF_REENTRIES: AtomicU64 = AtomicU64::new(0);
static DEF_REENTRY_MAX_DEPTH: AtomicU64 = AtomicU64::new(0);

/// Record a recursive re-entry of a def's application at `prior_depth` (`>= 1`
/// means a back-edge / cycle through that def). No-op unless the probe is gated on.
#[inline]
pub(crate) fn record_def_reentry(prior_depth: u32) {
    if !gate_enabled() {
        return;
    }
    DEF_REENTRIES.fetch_add(1, Ordering::Relaxed);
    DEF_REENTRY_MAX_DEPTH.fetch_max(prior_depth as u64, Ordering::Relaxed);
}

/// #14101 SCC-DISCRIMINATING counters: of all observed re-entries, how many are
/// MULTI-MEMBER (>=1 distinct other `DefId` on the eval stack between the two
/// entries of the same `DefId` = a genuine A->B->A SCC) vs single-def self-
/// recursion, plus the max distinct-member count. Settles whether the SCC
/// materialize-once fixpoint has any valid target (the open xstate/arktype branch).
static DEF_REENTRY_OBSERVED: AtomicU64 = AtomicU64::new(0);
static DEF_REENTRY_MULTIMEMBER: AtomicU64 = AtomicU64::new(0);
static DEF_REENTRY_MAX_DISTINCT: AtomicU64 = AtomicU64::new(0);

/// Record one re-entry, classified by the count of distinct OTHER `DefId`s on
/// `stack` between its top and the previous entry of `def_id`. 0 distinct =
/// single-def self-recursion; >=1 = a multi-member SCC. No-op unless gated on,
/// so the (bounded) scan never runs in a production build.
#[inline]
pub(crate) fn record_def_reentry_distinct(stack: &[crate::def::DefId], def_id: crate::def::DefId) {
    if !gate_enabled() {
        return;
    }
    let mut others: Vec<crate::def::DefId> = Vec::new();
    for &d in stack.iter().rev() {
        if d == def_id {
            break;
        }
        if !others.contains(&d) {
            others.push(d);
        }
    }
    let n = others.len() as u64;
    DEF_REENTRY_OBSERVED.fetch_add(1, Ordering::Relaxed);
    if n > 0 {
        DEF_REENTRY_MULTIMEMBER.fetch_add(1, Ordering::Relaxed);
    }
    DEF_REENTRY_MAX_DISTINCT.fetch_max(n, Ordering::Relaxed);
}

/// Eval-engine kinds the lever targets. Index into [`ProbeState`] arrays.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(usize)]
enum ProbeKind {
    Conditional = 0,
    Mapped = 1,
    Application = 2,
}

const PROBE_KIND_COUNT: usize = 3;
const PROBE_KIND_NAMES: [&str; PROBE_KIND_COUNT] = ["conditional", "mapped", "application"];

/// Per-kind scalar totals. Distinct-shape membership lives in the sibling
/// [`DashSet`]s on [`ProbeState`]; these atomics carry the running counts.
struct KindTotals {
    /// Nodes of this kind computed (passed every memo/cache layer).
    computes: AtomicU64,
    /// Computes whose result stayed the *same* kind as the input — for
    /// conditionals/mapped this is "deferred" (re-interned, not resolved).
    deferred_results: AtomicU64,
    /// Distinct result-`TypeId`s that carried `TypeData` structurally equal to
    /// an *earlier* distinct result-`TypeId` of this kind: a genuine
    /// interner-level hash-cons gap (expected ~0 because `intern()` already
    /// hash-conses). Non-zero here would be true additional dedup headroom.
    hash_cons_gap_results: AtomicU64,
}

impl KindTotals {
    const fn new() -> Self {
        Self {
            computes: AtomicU64::new(0),
            deferred_results: AtomicU64::new(0),
            hash_cons_gap_results: AtomicU64::new(0),
        }
    }
}

struct ProbeState {
    totals: [KindTotals; PROBE_KIND_COUNT],
    /// Distinct input `TypeId`s computed, per kind. Size =
    /// `distinct_inputs`; `computes - distinct_inputs` = recompute headroom.
    distinct_inputs: [DashSet<u32, FxBuildHasher>; PROBE_KIND_COUNT],
    /// Distinct result `TypeId`s materialized, per kind. Size =
    /// `distinct_results` (the interner already collapses structurally equal
    /// results into one id, so this is the realized distinct-shape count).
    distinct_results: [DashSet<u32, FxBuildHasher>; PROBE_KIND_COUNT],
    /// Distinct input `TypeId`s whose conditional resolved *eagerly* to a
    /// concrete branch. Index 0 only meaningful for `Conditional`.
    eager_conditional_inputs: DashSet<u32, FxBuildHasher>,
    /// Total eager conditional computes (result is not a `Conditional`).
    eager_conditional_computes: AtomicU64,
    /// Structural-hash -> first distinct result `TypeId` of that hash, per
    /// kind. Detects the hash-cons gap in #2 above.
    result_structural_hash: [DashMap<u64, u32, FxBuildHasher>; PROBE_KIND_COUNT],
    application_cache: ApplicationCacheTotals,
    canon_headroom: CanonHeadroomTotals,
}

/// State for the instantiation-identity dedup ceiling probe (#14101 / #13242
/// OPEN-2). Measures how many distinct result `TypeId`s the evaluator
/// materializes would COLLAPSE if the nominal `symbol` brand (`ObjectShape.symbol`
/// and per-property `PropertyInfo.parent_id`) were ignored when computing
/// instantiation identity. The `N - Fr` per kind (distinct results sampled
/// minus distinct symbol-stripped fingerprints) is the raw headroom — the count
/// of results that are structurally identical once the symbol brand is dropped.
struct CanonHeadroomTotals {
    /// Distinct result `TypeId.0` already fingerprinted, per kind. First-sight
    /// gate so each result is structurally hashed at most once.
    canon_seen_results: [DashSet<u32, FxBuildHasher>; PROBE_KIND_COUNT],
    /// Distinct symbol-stripped fingerprints of the raw `result`, per kind. Its
    /// size is `Fr`; `N - Fr` is the raw dedup headroom.
    symbol_stripped_forms_raw: [DashSet<u64, FxBuildHasher>; PROBE_KIND_COUNT],
    /// Distinct symbol-stripped fingerprints of `canonical_id(result)`, per
    /// kind. Only populated when a `QueryDatabase` is available; its size is
    /// `Fc`. `C - Fc` isolates the symbol-only collapse beyond what
    /// `canonical_id` already merges.
    symbol_stripped_forms_canon: [DashSet<u64, FxBuildHasher>; PROBE_KIND_COUNT],
    /// Distinct `canonical_id(result).0`, per kind (symbol-PRESERVING baseline).
    /// Only populated when a `QueryDatabase` is available; its size is `C`.
    canonical_ids_seen: [DashSet<u32, FxBuildHasher>; PROBE_KIND_COUNT],
    /// Samples for which a `QueryDatabase` was available (so the canon-seeded
    /// forms cover them).
    canon_samples_with_query_db: AtomicU64,
    /// Total first-sight result samples (covers the raw forms).
    canon_samples_total: AtomicU64,
}

impl CanonHeadroomTotals {
    fn new() -> Self {
        Self {
            canon_seen_results: [new_dashset(), new_dashset(), new_dashset()],
            symbol_stripped_forms_raw: [new_dashset_u64(), new_dashset_u64(), new_dashset_u64()],
            symbol_stripped_forms_canon: [new_dashset_u64(), new_dashset_u64(), new_dashset_u64()],
            canonical_ids_seen: [new_dashset(), new_dashset(), new_dashset()],
            canon_samples_with_query_db: AtomicU64::new(0),
            canon_samples_total: AtomicU64::new(0),
        }
    }
}

fn new_dashset() -> DashSet<u32, FxBuildHasher> {
    DashSet::with_hasher(FxBuildHasher)
}

fn new_dashset_u64() -> DashSet<u64, FxBuildHasher> {
    DashSet::with_hasher(FxBuildHasher)
}

fn new_dashmap() -> DashMap<u64, u32, FxBuildHasher> {
    DashMap::with_hasher(FxBuildHasher)
}

static STATE: OnceLock<ProbeState> = OnceLock::new();

fn state() -> &'static ProbeState {
    STATE.get_or_init(|| ProbeState {
        totals: [KindTotals::new(), KindTotals::new(), KindTotals::new()],
        distinct_inputs: [new_dashset(), new_dashset(), new_dashset()],
        distinct_results: [new_dashset(), new_dashset(), new_dashset()],
        eager_conditional_inputs: new_dashset(),
        eager_conditional_computes: AtomicU64::new(0),
        result_structural_hash: [new_dashmap(), new_dashmap(), new_dashmap()],
        application_cache: ApplicationCacheTotals::new(),
        canon_headroom: CanonHeadroomTotals::new(),
    })
}

struct ApplicationCacheTotals {
    entries_with_def_id: AtomicU64,
    entries_without_def_id: AtomicU64,
    entries_without_query_db: AtomicU64,
    raw_lookup_hits: AtomicU64,
    raw_lookup_misses: AtomicU64,
    expanded_lookup_hits: AtomicU64,
    expanded_lookup_misses: AtomicU64,
    body_known_params: AtomicU64,
    body_extracted_params: AtomicU64,
    body_value_call_return: AtomicU64,
    body_typeof_specialized: AtomicU64,
    body_opaque_unresolved_no_body: AtomicU64,
    body_opaque_resolved_unknown: AtomicU64,
    body_opaque_self_lazy: AtomicU64,
    body_opaque_typequery_callable: AtomicU64,
    body_opaque_extracted_mismatch: AtomicU64,
    body_opaque_no_registered_body: AtomicU64,
    cache_insert_eligible: AtomicU64,
    cache_insert_skipped_limit: AtomicU64,
    cache_insert_skipped_no_query_db: AtomicU64,
}

impl ApplicationCacheTotals {
    const fn new() -> Self {
        Self {
            entries_with_def_id: AtomicU64::new(0),
            entries_without_def_id: AtomicU64::new(0),
            entries_without_query_db: AtomicU64::new(0),
            raw_lookup_hits: AtomicU64::new(0),
            raw_lookup_misses: AtomicU64::new(0),
            expanded_lookup_hits: AtomicU64::new(0),
            expanded_lookup_misses: AtomicU64::new(0),
            body_known_params: AtomicU64::new(0),
            body_extracted_params: AtomicU64::new(0),
            body_value_call_return: AtomicU64::new(0),
            body_typeof_specialized: AtomicU64::new(0),
            body_opaque_unresolved_no_body: AtomicU64::new(0),
            body_opaque_resolved_unknown: AtomicU64::new(0),
            body_opaque_self_lazy: AtomicU64::new(0),
            body_opaque_typequery_callable: AtomicU64::new(0),
            body_opaque_extracted_mismatch: AtomicU64::new(0),
            body_opaque_no_registered_body: AtomicU64::new(0),
            cache_insert_eligible: AtomicU64::new(0),
            cache_insert_skipped_limit: AtomicU64::new(0),
            cache_insert_skipped_no_query_db: AtomicU64::new(0),
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) enum ApplicationLookupSite {
    RawArgs,
    ExpandedArgs,
}

#[derive(Copy, Clone)]
pub(crate) enum ApplicationBodyPath {
    KnownParams,
    ExtractedParams,
    ValueCallReturn,
    TypeofSpecialized,
    OpaqueUnresolvedNoBody,
    OpaqueResolvedUnknown,
    OpaqueSelfLazy,
    OpaqueTypeQueryCallable,
    OpaqueExtractedMismatch,
    OpaqueNoRegisteredBody,
}

/// Record whether an application input has a `DefId` cache key and whether
/// this evaluator may consult the authoritative query cache.
#[inline]
pub(crate) fn record_application_entry(has_def_id: bool, has_query_db: bool) {
    if !gate_enabled() {
        return;
    }
    let totals = &state().application_cache;
    if has_def_id {
        totals.entries_with_def_id.fetch_add(1, Ordering::Relaxed);
        if !has_query_db {
            totals
                .entries_without_query_db
                .fetch_add(1, Ordering::Relaxed);
        }
    } else {
        totals
            .entries_without_def_id
            .fetch_add(1, Ordering::Relaxed);
    }
}

/// Record one `application_eval_cache` lookup and whether it short-circuited.
#[inline]
pub(crate) fn record_application_cache_lookup(site: ApplicationLookupSite, hit: bool) {
    if !gate_enabled() {
        return;
    }
    let totals = &state().application_cache;
    let counter = match (site, hit) {
        (ApplicationLookupSite::RawArgs, true) => &totals.raw_lookup_hits,
        (ApplicationLookupSite::RawArgs, false) => &totals.raw_lookup_misses,
        (ApplicationLookupSite::ExpandedArgs, true) => &totals.expanded_lookup_hits,
        (ApplicationLookupSite::ExpandedArgs, false) => &totals.expanded_lookup_misses,
    };
    counter.fetch_add(1, Ordering::Relaxed);
}

/// Record which application body path the evaluator dispatched to after the
/// raw-args lookup. Body-specific expanded-args lookups may still short-circuit
/// inside the known/extracted path.
#[inline]
pub(crate) fn record_application_body_path(path: ApplicationBodyPath) {
    if !gate_enabled() {
        return;
    }
    let totals = &state().application_cache;
    let counter = match path {
        ApplicationBodyPath::KnownParams => &totals.body_known_params,
        ApplicationBodyPath::ExtractedParams => &totals.body_extracted_params,
        ApplicationBodyPath::ValueCallReturn => &totals.body_value_call_return,
        ApplicationBodyPath::TypeofSpecialized => &totals.body_typeof_specialized,
        ApplicationBodyPath::OpaqueUnresolvedNoBody => &totals.body_opaque_unresolved_no_body,
        ApplicationBodyPath::OpaqueResolvedUnknown => &totals.body_opaque_resolved_unknown,
        ApplicationBodyPath::OpaqueSelfLazy => &totals.body_opaque_self_lazy,
        ApplicationBodyPath::OpaqueTypeQueryCallable => &totals.body_opaque_typequery_callable,
        ApplicationBodyPath::OpaqueExtractedMismatch => &totals.body_opaque_extracted_mismatch,
        ApplicationBodyPath::OpaqueNoRegisteredBody => &totals.body_opaque_no_registered_body,
    };
    counter.fetch_add(1, Ordering::Relaxed);
}

/// Record whether a computed application result was eligible to be written to
/// the `(DefId, expanded_args, options)` cache.
#[inline]
pub(crate) fn record_application_cache_insert(cacheable: bool, has_query_db: bool) {
    if !gate_enabled() {
        return;
    }
    let totals = &state().application_cache;
    if !cacheable {
        totals
            .cache_insert_skipped_limit
            .fetch_add(1, Ordering::Relaxed);
    } else if !has_query_db {
        totals
            .cache_insert_skipped_no_query_db
            .fetch_add(1, Ordering::Relaxed);
    } else {
        totals.cache_insert_eligible.fetch_add(1, Ordering::Relaxed);
    }
}

/// Classify an input `TypeData` into the eval-engine kind the lever targets.
#[inline]
const fn probe_kind(key: &TypeData) -> Option<ProbeKind> {
    match key {
        TypeData::Conditional(_) => Some(ProbeKind::Conditional),
        TypeData::Mapped(_) => Some(ProbeKind::Mapped),
        TypeData::Application(_) => Some(ProbeKind::Application),
        _ => None,
    }
}

/// Record one computed eval node. Called from `evaluate_guarded_inner` after
/// `visit_type_key` produces `result` for input `type_id` whose `TypeData` is
/// `key`. `result_key` is `interner.lookup(result)` (the result's `TypeData`,
/// if interned) so the probe can classify deferred-vs-eager and detect
/// hash-cons gaps without re-borrowing the interner.
///
/// No-op (single branch) unless `TSZ_PERF_COUNTERS` is set.
#[inline]
pub(crate) fn record_compute(
    type_id: TypeId,
    key: &TypeData,
    result: TypeId,
    result_key: Option<&TypeData>,
) {
    if !gate_enabled() {
        return;
    }
    let Some(kind) = probe_kind(key) else {
        return;
    };
    let s = state();
    let idx = kind as usize;
    let t = &s.totals[idx];

    t.computes.fetch_add(1, Ordering::Relaxed);
    s.distinct_inputs[idx].insert(type_id.0);

    // First time we see this distinct result id, fold it into the structural
    // hash-cons-gap detector. A *different* result id whose `TypeData` hashes
    // (and compares) equal to an earlier one is dedup the interner missed.
    if s.distinct_results[idx].insert(result.0)
        && let Some(rk) = result_key
    {
        let mut hasher = rustc_hash::FxHasher::default();
        rk.hash(&mut hasher);
        let h = hasher.finish();
        // Distinct ids colliding on structural hash: the cheap signal. We
        // cannot re-`lookup` the previous id's `TypeData` here without the
        // interner, so we treat a hash collision between two distinct result
        // ids as a (rare) candidate gap. Equal `TypeData` would have collapsed
        // to the same id at `intern()`, so a true gap is essentially
        // impossible — a non-zero count is the headroom signal worth probing.
        if let Some(prev) = s.result_structural_hash[idx].insert(h, result.0)
            && prev != result.0
        {
            t.hash_cons_gap_results.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Deferred-vs-eager: result kind same as input kind => deferred
    // (re-interned), otherwise the recursion resolved it to a concrete shape.
    let result_is_same_kind = result_key.and_then(probe_kind) == Some(kind);
    if result_is_same_kind {
        t.deferred_results.fetch_add(1, Ordering::Relaxed);
    } else if matches!(kind, ProbeKind::Conditional) {
        // Conditional that resolved eagerly to a concrete branch.
        s.eager_conditional_computes.fetch_add(1, Ordering::Relaxed);
        s.eager_conditional_inputs.insert(type_id.0);
    }
}

/// Depth ceiling for [`symbol_stripped_fingerprint`]. Beyond this the
/// fingerprint hashes a sentinel and stops recursing, so a pathologically deep
/// type does not blow the stack. The probe is fully gated, so this only runs
/// under `TSZ_PERF_COUNTERS`.
const MAX_FP_DEPTH: u32 = 96;

/// Record one materialized result for the instantiation-identity dedup ceiling
/// probe (#14101 / #13242 OPEN-2). First-sight per distinct result `TypeId`
/// (so each result is fingerprinted at most once), classified by the input
/// `key`'s eval-engine kind.
///
/// Fingerprints `result` with [`symbol_stripped_fingerprint`] (which drops the
/// nominal `symbol` brand on `ObjectShape.symbol` and `PropertyInfo.parent_id`)
/// and folds it into the per-kind raw form set. When a [`QueryDatabase`] is
/// available it also records `canonical_id(result)` (the symbol-PRESERVING
/// baseline) and the symbol-stripped fingerprint of the canonical id.
///
/// No-op (single branch) unless the probe gate is on.
pub(crate) fn record_canon_headroom(
    key: &TypeData,
    result: TypeId,
    db: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
) {
    if !gate_enabled() {
        return;
    }
    let Some(kind) = probe_kind(key) else {
        return;
    };
    let idx = kind as usize;
    let c = &state().canon_headroom;
    // First-sight gate: only fingerprint each distinct result id once.
    if !c.canon_seen_results[idx].insert(result.0) {
        return;
    }
    c.canon_samples_total.fetch_add(1, Ordering::Relaxed);

    let fp_raw = symbol_stripped_fingerprint(db, result);
    c.symbol_stripped_forms_raw[idx].insert(fp_raw);

    if let Some(qdb) = query_db {
        c.canon_samples_with_query_db
            .fetch_add(1, Ordering::Relaxed);
        let canon = qdb.canonical_id(result);
        c.canonical_ids_seen[idx].insert(canon.0);
        let fp_c = symbol_stripped_fingerprint(db, canon);
        c.symbol_stripped_forms_canon[idx].insert(fp_c);
    }
}

/// Deterministic, recursive, **symbol-stripped** structural fingerprint of a
/// type, used only by the #14101 dedup-ceiling probe.
///
/// The fingerprint hashes the structural shape of `root` while deliberately
/// IGNORING the nominal `symbol` brand: `ObjectShape.symbol`,
/// `PropertyInfo.parent_id`, and the `symbol` field of function/callable
/// shapes. Two types that differ ONLY in that brand therefore hash equal, so
/// the count of distinct fingerprints among the materialized results is the
/// dedup ceiling if instantiation identity were canonicalized to ignore the
/// brand.
///
/// ## Variant coverage (ceiling exactness)
///
/// The ceiling is EXACT (precise structural hash, symbol stripped) for
/// `Object` / `ObjectWithIndex`, `Tuple`, `Function` / `Callable`, `Enum`,
/// `StringIntrinsic`, and the leaves (`Intrinsic`, `Literal`, `ThisType`,
/// `Lazy`, `BoundParameter`, `Recursive`, `TypeQuery`, `UniqueSymbol`,
/// `ModuleNamespace`). For every OTHER composite variant (`Union`,
/// `Intersection`, `Array`, `KeyOf`, `ReadonlyType`, `NoInfer`, `IndexAccess`,
/// `Conditional`, `Mapped`, `Application`, `TemplateLiteral`, `TypeParameter`,
/// `Infer`, ...) the fingerprint hashes only the variant discriminant plus the
/// recursed children via [`for_each_child`](crate::visitors::visitor::for_each_child).
/// Modifier-level scalar metadata (mapped `+?`/`-readonly` modifiers,
/// conditional distributivity, template literal string parts) is NOT captured,
/// so two such types that differ only in that metadata collapse — making the
/// reported ceiling a conservative UPPER bound for those exotic variants and
/// exact for the object/tuple/function symbol-stripping the probe targets.
///
/// ## Cycle / depth handling
///
/// A path-scoped map records the De Bruijn depth of each `TypeId` currently on
/// the recursion path. Re-encountering an id already on the path hashes a
/// back-reference marker (`relative depth`) and returns without recursing; the
/// entry is removed on exit (path-scoped, NOT a global visited set, so shared
/// sub-shapes are still hashed structurally). Depth is capped at
/// [`MAX_FP_DEPTH`].
fn symbol_stripped_fingerprint(db: &dyn TypeDatabase, root: TypeId) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    let mut path: FxHashMap<u32, u32> = FxHashMap::default();
    fp_recurse(db, root, 0, &mut path, &mut hasher);
    hasher.finish()
}

/// Hash one structural marker tag into `hasher`.
#[inline]
fn fp_tag(hasher: &mut rustc_hash::FxHasher, tag: u16) {
    tag.hash(hasher);
}

fn fp_recurse(
    db: &dyn TypeDatabase,
    id: TypeId,
    depth: u32,
    path: &mut FxHashMap<u32, u32>,
    hasher: &mut rustc_hash::FxHasher,
) {
    // Back-reference: this id is already on the current path => emit a relative
    // De Bruijn marker and stop (path-scoped cycle break).
    if let Some(&prior) = path.get(&id.0) {
        fp_tag(hasher, 0xBACE);
        depth.saturating_sub(prior).hash(hasher);
        return;
    }
    if depth >= MAX_FP_DEPTH {
        fp_tag(hasher, 0xDEEF);
        return;
    }

    let Some(key) = db.lookup(id) else {
        fp_tag(hasher, 0x4E4F);
        id.0.hash(hasher);
        return;
    };

    path.insert(id.0, depth);
    let child_depth = depth + 1;

    match &key {
        TypeData::Object(s) | TypeData::ObjectWithIndex(s) => {
            // Distinct discriminant for the two object encodings.
            fp_tag(
                hasher,
                if matches!(key, TypeData::Object(_)) {
                    0x0B10
                } else {
                    0x0B11
                },
            );
            let shape = db.object_shape(*s);
            // Identity-bearing structural flags only: index-signature optionality.
            // Cosmetic flags (FRESH_LITERAL, PRESERVE_DECLARATION_ORDER, display
            // aliasing, INTERSECTION_MERGED) are NOT hashed — they do not change
            // the structural shape and would split otherwise-equal results.
            shape.string_index_is_optional().hash(hasher);
            shape.number_index_is_optional().hash(hasher);
            // Properties IN ORDER. Skip parent_id (the symbol brand),
            // declaration_order, is_class_prototype, single_quoted_name.
            for prop in &shape.properties {
                prop.name.0.hash(hasher);
                prop.optional.hash(hasher);
                prop.readonly.hash(hasher);
                prop.is_method.hash(hasher);
                std::mem::discriminant(&prop.visibility).hash(hasher);
                prop.is_string_named.hash(hasher);
                prop.is_symbol_named.hash(hasher);
                fp_recurse(db, prop.type_id, child_depth, path, hasher);
                fp_recurse(db, prop.write_type, child_depth, path, hasher);
            }
            fp_index_signature(
                db,
                shape.string_index.as_ref(),
                0x1D00,
                child_depth,
                path,
                hasher,
            );
            fp_index_signature(
                db,
                shape.number_index.as_ref(),
                0x1D01,
                child_depth,
                path,
                hasher,
            );
            // shape.symbol deliberately NOT hashed (the nominal brand).
        }
        TypeData::Tuple(l) => {
            fp_tag(hasher, 0x70B1);
            let elems = db.tuple_list(*l);
            (elems.len() as u32).hash(hasher);
            for el in elems.iter() {
                el.name.map(|a| a.0).hash(hasher);
                el.optional.hash(hasher);
                el.rest.hash(hasher);
                fp_recurse(db, el.type_id, child_depth, path, hasher);
            }
        }
        TypeData::Function(f) => {
            fp_tag(hasher, 0xF000);
            let shape = db.function_shape(*f);
            shape.is_constructor.hash(hasher);
            shape.is_method.hash(hasher);
            fp_signature(
                &shape.params,
                shape.this_type,
                Some(shape.return_type),
                &mut FpSignatureContext {
                    db,
                    depth: child_depth,
                    path,
                    hasher,
                },
            );
            // shape.symbol does not exist on FunctionShape; nothing to strip.
        }
        TypeData::Callable(c) => {
            fp_tag(hasher, 0xCA11);
            let shape = db.callable_shape(*c);
            shape.is_abstract.hash(hasher);
            (shape.call_signatures.len() as u32).hash(hasher);
            (shape.construct_signatures.len() as u32).hash(hasher);
            for sig in &shape.call_signatures {
                fp_tag(hasher, 0xC5A1);
                sig.is_method.hash(hasher);
                fp_signature(
                    &sig.params,
                    sig.this_type,
                    Some(sig.return_type),
                    &mut FpSignatureContext {
                        db,
                        depth: child_depth,
                        path,
                        hasher,
                    },
                );
            }
            for sig in &shape.construct_signatures {
                fp_tag(hasher, 0xC5A2);
                sig.is_method.hash(hasher);
                fp_signature(
                    &sig.params,
                    sig.this_type,
                    Some(sig.return_type),
                    &mut FpSignatureContext {
                        db,
                        depth: child_depth,
                        path,
                        hasher,
                    },
                );
            }
            // Callable properties (e.g. statics) in order, symbol stripped.
            for prop in &shape.properties {
                prop.name.0.hash(hasher);
                prop.optional.hash(hasher);
                prop.readonly.hash(hasher);
                fp_recurse(db, prop.type_id, child_depth, path, hasher);
                fp_recurse(db, prop.write_type, child_depth, path, hasher);
            }
            fp_index_signature(
                db,
                shape.string_index.as_ref(),
                0x1D02,
                child_depth,
                path,
                hasher,
            );
            fp_index_signature(
                db,
                shape.number_index.as_ref(),
                0x1D03,
                child_depth,
                path,
                hasher,
            );
            // shape.symbol deliberately NOT hashed (the nominal brand).
        }
        TypeData::Enum(def_id, t) => {
            fp_tag(hasher, 0xE10E);
            def_id.0.hash(hasher);
            fp_recurse(db, *t, child_depth, path, hasher);
        }
        TypeData::StringIntrinsic { kind, type_arg } => {
            fp_tag(hasher, 0x5141);
            std::mem::discriminant(kind).hash(hasher);
            fp_recurse(db, *type_arg, child_depth, path, hasher);
        }
        // Nominal / alias leaves: keep identity, do NOT resolve.
        TypeData::Lazy(def_id) => {
            fp_tag(hasher, 0x1A2E);
            def_id.0.hash(hasher);
        }
        TypeData::BoundParameter(n) => {
            fp_tag(hasher, 0xB0A0);
            n.hash(hasher);
        }
        TypeData::Recursive(n) => {
            fp_tag(hasher, 0x2EC0);
            n.hash(hasher);
        }
        TypeData::TypeQuery(s) => {
            fp_tag(hasher, 0x70F0);
            s.0.hash(hasher);
        }
        TypeData::UniqueSymbol(s) => {
            fp_tag(hasher, 0x0501);
            s.0.hash(hasher);
        }
        TypeData::ModuleNamespace(s) => {
            fp_tag(hasher, 0x0502);
            s.0.hash(hasher);
        }
        TypeData::Intrinsic(k) => {
            fp_tag(hasher, 0x1417);
            std::mem::discriminant(k).hash(hasher);
        }
        TypeData::Literal(v) => {
            fp_tag(hasher, 0x1117);
            v.hash(hasher);
        }
        TypeData::ThisType => {
            fp_tag(hasher, 0x7415);
        }
        // Order-independent composite variants: fingerprint children into a
        // sorted vec so a raw (non-canonicalized) union/intersection result
        // hashes independent of `for_each_child` member order.
        TypeData::Union(_) | TypeData::Intersection(_) => {
            fp_tag(
                hasher,
                if matches!(key, TypeData::Union(_)) {
                    0x0410
                } else {
                    0x0411
                },
            );
            let mut child_fps: Vec<u64> = Vec::new();
            crate::visitors::visitor::for_each_child(db, &key, |child| {
                let mut sub = rustc_hash::FxHasher::default();
                // Children are hashed with a fresh sub-hasher under the SAME
                // path map so cycles through a union member are still broken.
                fp_recurse(db, child, child_depth, path, &mut sub);
                child_fps.push(sub.finish());
            });
            child_fps.sort_unstable();
            for fp in child_fps {
                fp.hash(hasher);
            }
        }
        // All remaining composite variants: discriminant + recursed children.
        // Modifier-level scalar metadata is captured only at this granularity
        // (a conservative upper bound on collapse for these exotic shapes).
        _ => {
            std::mem::discriminant(&key).hash(hasher);
            crate::visitors::visitor::for_each_child(db, &key, |child| {
                fp_recurse(db, child, child_depth, path, hasher);
            });
        }
    }

    path.remove(&id.0);
}

/// Hash an optional index signature, symbol-/cosmetic-stripped.
fn fp_index_signature(
    db: &dyn TypeDatabase,
    sig: Option<&crate::types::IndexSignature>,
    tag: u16,
    depth: u32,
    path: &mut FxHashMap<u32, u32>,
    hasher: &mut rustc_hash::FxHasher,
) {
    fp_tag(hasher, tag);
    match sig {
        None => false.hash(hasher),
        Some(s) => {
            true.hash(hasher);
            s.readonly.hash(hasher);
            // param_name is cosmetic; skip.
            fp_recurse(db, s.key_type, depth, path, hasher);
            fp_recurse(db, s.value_type, depth, path, hasher);
        }
    }
}

struct FpSignatureContext<'a, 'b> {
    db: &'a dyn TypeDatabase,
    depth: u32,
    path: &'b mut FxHashMap<u32, u32>,
    hasher: &'b mut FxHasher,
}

/// Hash a call/function signature: arity + per-parameter optional/rest/name-
/// presence, return-type presence, then recurse parameter and return types.
fn fp_signature(
    params: &[crate::types::ParamInfo],
    this_type: Option<TypeId>,
    return_type: Option<TypeId>,
    ctx: &mut FpSignatureContext<'_, '_>,
) {
    (params.len() as u32).hash(ctx.hasher);
    this_type.is_some().hash(ctx.hasher);
    if let Some(t) = this_type {
        fp_recurse(ctx.db, t, ctx.depth, ctx.path, ctx.hasher);
    }
    for p in params {
        p.optional.hash(ctx.hasher);
        p.rest.hash(ctx.hasher);
        p.name.is_some().hash(ctx.hasher);
        fp_recurse(ctx.db, p.type_id, ctx.depth, ctx.path, ctx.hasher);
    }
    return_type.is_some().hash(ctx.hasher);
    if let Some(r) = return_type {
        fp_recurse(ctx.db, r, ctx.depth, ctx.path, ctx.hasher);
    }
}

/// Per-kind snapshot row for the dump.
struct KindRow {
    name: &'static str,
    computes: u64,
    distinct_inputs: u64,
    distinct_results: u64,
    deferred_results: u64,
    hash_cons_gap_results: u64,
}

fn snapshot_kind(idx: usize) -> KindRow {
    let s = state();
    let t = &s.totals[idx];
    KindRow {
        name: PROBE_KIND_NAMES[idx],
        computes: t.computes.load(Ordering::Relaxed),
        distinct_inputs: s.distinct_inputs[idx].len() as u64,
        distinct_results: s.distinct_results[idx].len() as u64,
        deferred_results: t.deferred_results.load(Ordering::Relaxed),
        hash_cons_gap_results: t.hash_cons_gap_results.load(Ordering::Relaxed),
    }
}

#[derive(Default)]
struct ApplicationCacheRow {
    entries_with_def_id: u64,
    entries_without_def_id: u64,
    entries_without_query_db: u64,
    raw_lookup_hits: u64,
    raw_lookup_misses: u64,
    expanded_lookup_hits: u64,
    expanded_lookup_misses: u64,
    body_known_params: u64,
    body_extracted_params: u64,
    body_value_call_return: u64,
    body_typeof_specialized: u64,
    body_opaque_unresolved_no_body: u64,
    body_opaque_resolved_unknown: u64,
    body_opaque_self_lazy: u64,
    body_opaque_typequery_callable: u64,
    body_opaque_extracted_mismatch: u64,
    body_opaque_no_registered_body: u64,
    cache_insert_eligible: u64,
    cache_insert_skipped_limit: u64,
    cache_insert_skipped_no_query_db: u64,
}

fn snapshot_application_cache() -> ApplicationCacheRow {
    let t = &state().application_cache;
    ApplicationCacheRow {
        entries_with_def_id: t.entries_with_def_id.load(Ordering::Relaxed),
        entries_without_def_id: t.entries_without_def_id.load(Ordering::Relaxed),
        entries_without_query_db: t.entries_without_query_db.load(Ordering::Relaxed),
        raw_lookup_hits: t.raw_lookup_hits.load(Ordering::Relaxed),
        raw_lookup_misses: t.raw_lookup_misses.load(Ordering::Relaxed),
        expanded_lookup_hits: t.expanded_lookup_hits.load(Ordering::Relaxed),
        expanded_lookup_misses: t.expanded_lookup_misses.load(Ordering::Relaxed),
        body_known_params: t.body_known_params.load(Ordering::Relaxed),
        body_extracted_params: t.body_extracted_params.load(Ordering::Relaxed),
        body_value_call_return: t.body_value_call_return.load(Ordering::Relaxed),
        body_typeof_specialized: t.body_typeof_specialized.load(Ordering::Relaxed),
        body_opaque_unresolved_no_body: t.body_opaque_unresolved_no_body.load(Ordering::Relaxed),
        body_opaque_resolved_unknown: t.body_opaque_resolved_unknown.load(Ordering::Relaxed),
        body_opaque_self_lazy: t.body_opaque_self_lazy.load(Ordering::Relaxed),
        body_opaque_typequery_callable: t.body_opaque_typequery_callable.load(Ordering::Relaxed),
        body_opaque_extracted_mismatch: t.body_opaque_extracted_mismatch.load(Ordering::Relaxed),
        body_opaque_no_registered_body: t.body_opaque_no_registered_body.load(Ordering::Relaxed),
        cache_insert_eligible: t.cache_insert_eligible.load(Ordering::Relaxed),
        cache_insert_skipped_limit: t.cache_insert_skipped_limit.load(Ordering::Relaxed),
        cache_insert_skipped_no_query_db: t
            .cache_insert_skipped_no_query_db
            .load(Ordering::Relaxed),
    }
}

/// Format the probe measurements as a multi-line report. Returns an empty
/// string when counters are disabled, so callers can append it
/// unconditionally next to the perf-counter dump.
///
/// The report exposes, per eval-engine kind, the four de-risking numbers:
/// recompute headroom (`computes - distinct_inputs`), defer/eager split,
/// hash-cons gap, and fan-out (`distinct_results` vs `distinct_inputs`).
pub fn dump_report() -> String {
    if !gate_enabled() {
        return String::new();
    }
    // No computes recorded => nothing materialized; keep the dump quiet.
    let any = (0..PROBE_KIND_COUNT).any(|i| state().totals[i].computes.load(Ordering::Relaxed) > 0);
    if !any {
        return String::new();
    }

    let mut out = String::new();
    let _ = writeln!(out, "\n=== TSZ eval-materialization probe (#13250) ===");
    let _ = writeln!(
        out,
        "Per eval-engine kind: computes, distinct inputs, distinct results,\n\
         recompute headroom (computes - distinct inputs), defer/eager, hash-cons gap.\n"
    );

    let mut total_computes = 0u64;
    let mut total_distinct_results = 0u64;
    for idx in 0..PROBE_KIND_COUNT {
        let r = snapshot_kind(idx);
        total_computes += r.computes;
        total_distinct_results += r.distinct_results;
        let name = r.name;
        let computes = r.computes;
        let distinct_inputs = r.distinct_inputs;
        let distinct_results = r.distinct_results;
        let deferred_results = r.deferred_results;
        let hash_cons_gap = r.hash_cons_gap_results;
        let recompute_headroom = computes.saturating_sub(distinct_inputs);
        let recompute_pct = pct(recompute_headroom, computes);
        let fanout_pct = pct(
            distinct_results.saturating_sub(distinct_inputs),
            distinct_results.max(1),
        );
        let _ = writeln!(out, "[{name}]");
        let _ = writeln!(out, "  computes                 {computes:>12}");
        let _ = writeln!(out, "  distinct inputs          {distinct_inputs:>12}");
        let _ = writeln!(out, "  distinct results         {distinct_results:>12}");
        let _ = writeln!(
            out,
            "  recompute headroom       {recompute_headroom:>12}  ({recompute_pct:.1}% of computes)"
        );
        let _ = writeln!(out, "  deferred results         {deferred_results:>12}");
        let _ = writeln!(out, "  hash-cons gap (results)  {hash_cons_gap:>12}");
        let _ = writeln!(out, "  fan-out (results>inputs) {fanout_pct:>12.1}%");
    }

    let s = state();
    let eager_computes = s.eager_conditional_computes.load(Ordering::Relaxed);
    let eager_distinct = s.eager_conditional_inputs.len() as u64;
    let eager_recompute = eager_computes.saturating_sub(eager_distinct);
    let eager_pct = pct(eager_recompute, eager_computes);
    let _ = writeln!(out, "[conditional defer headroom]");
    let _ = writeln!(out, "  eager computes           {eager_computes:>12}");
    let _ = writeln!(out, "  eager distinct inputs    {eager_distinct:>12}");
    let _ = writeln!(
        out,
        "  eager recompute headroom {eager_recompute:>12}  ({eager_pct:.1}% of eager computes)"
    );

    let app_kind = snapshot_kind(ProbeKind::Application as usize);
    let app_cache = snapshot_application_cache();
    let app_recompute = app_kind.computes.saturating_sub(app_kind.distinct_inputs);
    let app_cache_hits = app_cache.raw_lookup_hits + app_cache.expanded_lookup_hits;
    let not_explained_by_def_args = app_recompute.saturating_sub(app_cache_hits);
    let opaque_body_paths = app_cache.body_opaque_unresolved_no_body
        + app_cache.body_opaque_resolved_unknown
        + app_cache.body_opaque_self_lazy
        + app_cache.body_opaque_typequery_callable
        + app_cache.body_opaque_extracted_mismatch
        + app_cache.body_opaque_no_registered_body;
    let _ = writeln!(out, "[application cache eligibility]");
    let _ = writeln!(out, "  application input recomputes {app_recompute:>12}");
    let _ = writeln!(
        out,
        "  `(DefId,args)` cache hits     {app_cache_hits:>12}  (raw {}, expanded {})",
        app_cache.raw_lookup_hits, app_cache.expanded_lookup_hits
    );
    let _ = writeln!(
        out,
        "  recomputes beyond cache hits {not_explained_by_def_args:>12}  (lower bound)"
    );
    let _ = writeln!(
        out,
        "  entries with DefId           {:>12}",
        app_cache.entries_with_def_id
    );
    let _ = writeln!(
        out,
        "  entries without DefId        {:>12}",
        app_cache.entries_without_def_id
    );
    let _ = writeln!(
        out,
        "  DefId entries without DB     {:>12}",
        app_cache.entries_without_query_db
    );
    let _ = writeln!(
        out,
        "  raw lookups hit/miss         {:>12}/{:<12}",
        app_cache.raw_lookup_hits, app_cache.raw_lookup_misses
    );
    let _ = writeln!(
        out,
        "  expanded lookups hit/miss    {:>12}/{:<12}",
        app_cache.expanded_lookup_hits, app_cache.expanded_lookup_misses
    );
    let _ = writeln!(
        out,
        "  dispatch known/extracted     {:>12}/{:<12}",
        app_cache.body_known_params, app_cache.body_extracted_params
    );
    let _ = writeln!(
        out,
        "  uncached special paths       {:>12}  (value-call {}, typeof {})",
        app_cache.body_value_call_return + app_cache.body_typeof_specialized,
        app_cache.body_value_call_return,
        app_cache.body_typeof_specialized
    );
    let _ = writeln!(
        out,
        "  opaque body paths            {opaque_body_paths:>12}"
    );
    let _ = writeln!(
        out,
        "    unresolved/unknown/self    {:>12}/{:<12}/{:<12}",
        app_cache.body_opaque_unresolved_no_body,
        app_cache.body_opaque_resolved_unknown,
        app_cache.body_opaque_self_lazy
    );
    let _ = writeln!(
        out,
        "    typequery/mismatch/nobody  {:>12}/{:<12}/{:<12}",
        app_cache.body_opaque_typequery_callable,
        app_cache.body_opaque_extracted_mismatch,
        app_cache.body_opaque_no_registered_body
    );
    let _ = writeln!(
        out,
        "  cache inserts eligible       {:>12}",
        app_cache.cache_insert_eligible
    );
    let _ = writeln!(
        out,
        "  cache inserts skipped        {:>12}  (limit {}, no-db {})",
        app_cache.cache_insert_skipped_limit + app_cache.cache_insert_skipped_no_query_db,
        app_cache.cache_insert_skipped_limit,
        app_cache.cache_insert_skipped_no_query_db
    );

    let overall_fanout = pct(
        total_computes.saturating_sub(total_distinct_results),
        total_computes.max(1),
    );
    let _ = writeln!(
        out,
        "[totals] computes {total_computes} -> distinct results {total_distinct_results} \
         (overall fan-out {overall_fanout:.1}%)"
    );

    append_canon_headroom_report(&mut out);

    let reentries = DEF_REENTRIES.load(Ordering::Relaxed);
    if reentries > 0 {
        let _ = writeln!(
            out,
            "[scc] def re-entries (recursive-heritage back-edges) {reentries}, \
             max prior depth {}  (#14101 step-2 materialize-once headroom)",
            DEF_REENTRY_MAX_DEPTH.load(Ordering::Relaxed)
        );
        let observed = DEF_REENTRY_OBSERVED.load(Ordering::Relaxed);
        let multimember = DEF_REENTRY_MULTIMEMBER.load(Ordering::Relaxed);
        let _ = writeln!(
            out,
            "[scc] multi-member re-entries {multimember}/{observed} (max distinct members {}) \
             — 0 multi-member means single-def recursion, SCC fixpoint has no target",
            DEF_REENTRY_MAX_DISTINCT.load(Ordering::Relaxed)
        );
    }
    out
}

/// Append the instantiation-identity dedup-ceiling section (#14101 / #13242
/// OPEN-2). Printed only when at least one result sample was recorded. The
/// decisive number is `N - Fr` per kind: distinct materialized results that
/// COLLAPSE once the nominal `symbol` brand is ignored — the headroom for a
/// future canonicalize-instantiation-identity change. `C - Fc` isolates the
/// symbol-only collapse beyond what `canonical_id` already merges.
fn append_canon_headroom_report(out: &mut String) {
    let c = &state().canon_headroom;
    let total = c.canon_samples_total.load(Ordering::Relaxed);
    if total == 0 {
        return;
    }
    let with_db = c.canon_samples_with_query_db.load(Ordering::Relaxed);
    let _ = writeln!(out, "[instantiation-identity dedup ceiling (#14101)]");
    let _ = writeln!(
        out,
        "  query_db coverage           {with_db}/{total}  \
         (canonical_id-seeded forms only cover the {with_db} samples)"
    );
    let _ = writeln!(
        out,
        "  per kind: results sampled (N), canonical_id forms (C, symbol-preserving),"
    );
    let _ = writeln!(
        out,
        "            symbol-stripped raw (Fr), symbol-stripped canon-seeded (Fc)"
    );
    for (idx, &name) in PROBE_KIND_NAMES.iter().enumerate() {
        let n = c.canon_seen_results[idx].len() as u64;
        let canon_c = c.canonical_ids_seen[idx].len() as u64;
        let fr = c.symbol_stripped_forms_raw[idx].len() as u64;
        let fc = c.symbol_stripped_forms_canon[idx].len() as u64;
        let raw_headroom = n.saturating_sub(fr);
        let raw_pct = pct(raw_headroom, n.max(1));
        let canon_headroom = canon_c.saturating_sub(fc);
        let canon_pct = pct(canon_headroom, canon_c.max(1));
        let _ = writeln!(
            out,
            "  [{name}]  N={n} C={canon_c} Fr={fr} Fc={fc}   \
             raw headroom N-Fr={raw_headroom} ({raw_pct:.1}%)   \
             canon headroom C-Fc={canon_headroom} ({canon_pct:.1}%)"
        );
    }
}

#[inline]
fn pct(num: u64, den: u64) -> f64 {
    if den == 0 {
        0.0
    } else {
        (num as f64) * 100.0 / (den as f64)
    }
}

/// Test-only read of the `(computes, distinct_inputs, distinct_results,
/// deferred_results)` totals for one kind index. Used to assert recorder
/// semantics on deltas (the global state is process-wide via `OnceLock`).
#[cfg(test)]
fn kind_snapshot_for_tests(idx: usize) -> (u64, u64, u64, u64) {
    let r = snapshot_kind(idx);
    (
        r.computes,
        r.distinct_inputs,
        r.distinct_results,
        r.deferred_results,
    )
}

#[cfg(test)]
fn application_cache_snapshot_for_tests() -> ApplicationCacheRow {
    snapshot_application_cache()
}

/// Test-only read of `(canon_seen_results.len(), symbol_stripped_forms_raw.len(),
/// canon_samples_total)` for one kind index, for asserting the canon-headroom
/// recorder's first-sight dedup behavior on deltas.
#[cfg(test)]
fn canon_headroom_snapshot_for_tests(idx: usize) -> (u64, u64, u64) {
    let c = &state().canon_headroom;
    (
        c.canon_seen_results[idx].len() as u64,
        c.symbol_stripped_forms_raw[idx].len() as u64,
        c.canon_samples_total.load(Ordering::Relaxed),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ConditionalTypeId, MappedTypeId, TypeApplicationId};

    /// `record_compute` distinguishes distinct inputs from recomputes and
    /// distinct results from collapsed ones, and classifies deferred-vs-eager.
    /// Asserts on deltas because the probe state is process-wide.
    #[test]
    fn record_compute_counts_distinct_and_recompute_deltas() {
        FORCE_PROBE_FOR_TESTS.store(true, Ordering::Relaxed);
        let idx = ProbeKind::Conditional as usize;
        let before = kind_snapshot_for_tests(idx);

        let cond = TypeData::Conditional(ConditionalTypeId(7));
        // Eager: input conditional resolves to a non-conditional concrete
        // result. Same input twice => one distinct input, two computes
        // (the second is a recompute). Two distinct concrete results.
        let concrete_a = TypeData::Intrinsic(crate::types::IntrinsicKind::Number);
        let concrete_b = TypeData::Intrinsic(crate::types::IntrinsicKind::String);
        record_compute(TypeId(100), &cond, TypeId(200), Some(&concrete_a));
        record_compute(TypeId(100), &cond, TypeId(201), Some(&concrete_b));
        // Deferred: result stays a conditional (re-interned, not resolved).
        let deferred_cond = TypeData::Conditional(ConditionalTypeId(9));
        record_compute(TypeId(101), &cond, TypeId(202), Some(&deferred_cond));

        let after = kind_snapshot_for_tests(idx);
        let d_computes = after.0 - before.0;
        let d_inputs = after.1 - before.1;
        let d_results = after.2 - before.2;
        let d_deferred = after.3 - before.3;

        assert_eq!(d_computes, 3, "three computes recorded");
        assert_eq!(d_inputs, 2, "two distinct input TypeIds (100, 101)");
        assert_eq!(
            d_results, 3,
            "three distinct result TypeIds (200, 201, 202)"
        );
        assert_eq!(d_deferred, 1, "one result stayed a conditional (deferred)");
        // recompute headroom = computes - distinct_inputs = 3 - 2 = 1.
        assert_eq!(
            d_computes - d_inputs,
            1,
            "one recompute of an existing input"
        );
    }

    /// Non eval-engine kinds are ignored; mapped/application route to their
    /// own buckets.
    #[test]
    fn record_compute_ignores_non_lever_kinds_and_routes_per_kind() {
        FORCE_PROBE_FOR_TESTS.store(true, Ordering::Relaxed);
        let m_idx = ProbeKind::Mapped as usize;
        let a_idx = ProbeKind::Application as usize;
        let m_before = kind_snapshot_for_tests(m_idx);
        let a_before = kind_snapshot_for_tests(a_idx);

        // An object input is not a lever kind: must be ignored entirely.
        let object = TypeData::Array(TypeId(1));
        record_compute(TypeId(300), &object, TypeId(301), None);

        let mapped = TypeData::Mapped(MappedTypeId(3));
        let app = TypeData::Application(TypeApplicationId(4));
        record_compute(TypeId(400), &mapped, TypeId(401), None);
        record_compute(TypeId(500), &app, TypeId(501), None);

        let m_after = kind_snapshot_for_tests(m_idx);
        let a_after = kind_snapshot_for_tests(a_idx);
        // `>` (not `==`): under a shared-process test runner a sibling test
        // may record into the application bucket concurrently. The object
        // input must contribute nothing to either lever bucket, which the
        // mapped-bucket delta (no sibling writes mapped) pins exactly.
        assert_eq!(
            m_after.0 - m_before.0,
            1,
            "one mapped compute, object ignored"
        );
        assert!(a_after.0 > a_before.0, "at least our application compute");
    }

    /// The report is empty when counters are disabled and the gate is the
    /// only check (default-behavior-unchanged contract).
    #[test]
    fn dump_report_nonempty_only_under_gate() {
        FORCE_PROBE_FOR_TESTS.store(true, Ordering::Relaxed);
        let app = TypeData::Application(TypeApplicationId(11));
        record_compute(TypeId(900), &app, TypeId(901), None);
        record_application_entry(true, true);
        record_application_cache_lookup(ApplicationLookupSite::RawArgs, false);
        record_application_cache_lookup(ApplicationLookupSite::ExpandedArgs, true);
        record_application_body_path(ApplicationBodyPath::KnownParams);
        record_application_cache_insert(true, true);
        let report = dump_report();
        assert!(
            report.contains("eval-materialization probe"),
            "report should render under the gate"
        );
        assert!(
            report.contains("recompute headroom"),
            "report should expose recompute headroom"
        );
        assert!(
            report.contains("application cache eligibility"),
            "report should expose the application cache eligibility split"
        );
    }

    /// Application cache counters split `(DefId,args)` eligibility from opaque
    /// and tainted paths so #13250 follow-ups can tell whether the existing
    /// application-eval cache key is the right reuse layer.
    #[test]
    fn application_cache_eligibility_counters_record_deltas() {
        FORCE_PROBE_FOR_TESTS.store(true, Ordering::Relaxed);
        let before = application_cache_snapshot_for_tests();

        record_application_entry(true, false);
        record_application_entry(false, false);
        record_application_cache_lookup(ApplicationLookupSite::RawArgs, false);
        record_application_cache_lookup(ApplicationLookupSite::ExpandedArgs, true);
        record_application_body_path(ApplicationBodyPath::OpaqueResolvedUnknown);
        record_application_body_path(ApplicationBodyPath::ExtractedParams);
        record_application_cache_insert(false, true);
        record_application_cache_insert(true, false);
        record_application_cache_insert(true, true);

        let after = application_cache_snapshot_for_tests();
        assert_eq!(after.entries_with_def_id - before.entries_with_def_id, 1);
        assert_eq!(
            after.entries_without_def_id - before.entries_without_def_id,
            1
        );
        assert_eq!(
            after.entries_without_query_db - before.entries_without_query_db,
            1
        );
        assert_eq!(after.raw_lookup_misses - before.raw_lookup_misses, 1);
        assert_eq!(after.expanded_lookup_hits - before.expanded_lookup_hits, 1);
        assert_eq!(
            after.body_opaque_resolved_unknown - before.body_opaque_resolved_unknown,
            1
        );
        assert_eq!(
            after.body_extracted_params - before.body_extracted_params,
            1
        );
        assert_eq!(
            after.cache_insert_skipped_limit - before.cache_insert_skipped_limit,
            1
        );
        assert_eq!(
            after.cache_insert_skipped_no_query_db - before.cache_insert_skipped_no_query_db,
            1
        );
        assert_eq!(
            after.cache_insert_eligible - before.cache_insert_eligible,
            1
        );
    }

    use crate::TypeInterner;
    use crate::types::{ObjectFlags, PropertyInfo};
    use tsz_binder::SymbolId;

    /// Build an object `{ a: string; b: number }` with the given nominal
    /// `symbol` brand. Every property is also branded with `parent_id = symbol`
    /// so both symbol-strip targets vary together.
    fn branded_object(interner: &TypeInterner, brand: Option<SymbolId>) -> TypeId {
        let a = interner.intern_string("a");
        let b = interner.intern_string("b");
        let mut p_a = PropertyInfo::new(a, TypeId::STRING);
        p_a.parent_id = brand;
        let mut p_b = PropertyInfo::new(b, TypeId::NUMBER);
        p_b.parent_id = brand;
        interner.object_with_flags_and_symbol(vec![p_a, p_b], ObjectFlags::empty(), brand)
    }

    /// The fingerprint ignores the nominal `symbol` brand: two objects with
    /// identical structure but different `ObjectShape.symbol` (and different
    /// per-property `parent_id`) must hash EQUAL.
    #[test]
    fn symbol_stripped_fingerprint_ignores_object_symbol() {
        let interner = TypeInterner::new();
        let o1 = branded_object(&interner, Some(SymbolId(11)));
        let o2 = branded_object(&interner, Some(SymbolId(22)));
        // Different brands => distinct interned TypeIds (nominal identity).
        assert_ne!(o1, o2, "branded objects must intern distinctly");
        let db: &dyn TypeDatabase = &interner;
        assert_eq!(
            symbol_stripped_fingerprint(db, o1),
            symbol_stripped_fingerprint(db, o2),
            "symbol/parent_id brand must not affect the fingerprint"
        );
    }

    /// The fingerprint distinguishes a real structural difference (a property
    /// type change) even when the brand is identical.
    #[test]
    fn symbol_stripped_fingerprint_distinguishes_structure() {
        let interner = TypeInterner::new();
        let a = interner.intern_string("a");
        let b = interner.intern_string("b");
        let brand = Some(SymbolId(7));
        let o1 = {
            let mut p_a = PropertyInfo::new(a, TypeId::STRING);
            p_a.parent_id = brand;
            let mut p_b = PropertyInfo::new(b, TypeId::NUMBER);
            p_b.parent_id = brand;
            interner.object_with_flags_and_symbol(vec![p_a, p_b], ObjectFlags::empty(), brand)
        };
        // Same brand, but `b: boolean` instead of `b: number`.
        let o2 = {
            let mut p_a = PropertyInfo::new(a, TypeId::STRING);
            p_a.parent_id = brand;
            let mut p_b = PropertyInfo::new(b, TypeId::BOOLEAN);
            p_b.parent_id = brand;
            interner.object_with_flags_and_symbol(vec![p_a, p_b], ObjectFlags::empty(), brand)
        };
        let db: &dyn TypeDatabase = &interner;
        assert_ne!(
            symbol_stripped_fingerprint(db, o1),
            symbol_stripped_fingerprint(db, o2),
            "a property type difference must change the fingerprint"
        );
    }

    /// Recursion strips the symbol brand at depth: wrapping each of two
    /// symbol-distinct objects in an `Array` produces EQUAL fingerprints.
    #[test]
    fn symbol_stripped_fingerprint_strips_symbol_at_depth() {
        let interner = TypeInterner::new();
        let o1 = branded_object(&interner, Some(SymbolId(33)));
        let o2 = branded_object(&interner, Some(SymbolId(44)));
        let a1 = interner.array(o1);
        let a2 = interner.array(o2);
        assert_ne!(a1, a2, "arrays of branded objects intern distinctly");
        let db: &dyn TypeDatabase = &interner;
        assert_eq!(
            symbol_stripped_fingerprint(db, a1),
            symbol_stripped_fingerprint(db, a2),
            "symbol brand must be stripped through the array element"
        );
    }

    /// `record_canon_headroom` is first-sight per distinct result id: a repeat
    /// of the same result does not re-sample, and a distinct structural result
    /// adds one raw form. `query_db = None` is exercised.
    #[test]
    fn record_canon_headroom_dedups_and_counts() {
        FORCE_PROBE_FOR_TESTS.store(true, Ordering::Relaxed);
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;
        let idx = ProbeKind::Application as usize;
        let key = TypeData::Application(TypeApplicationId(101));

        let before = canon_headroom_snapshot_for_tests(idx);
        let o1 = branded_object(&interner, Some(SymbolId(101)));
        // First sight of o1 => one new sampled result and one raw form.
        record_canon_headroom(&key, o1, db, None);
        // Repeat of o1 => first-sight gate rejects, no new sample.
        record_canon_headroom(&key, o1, db, None);
        // A structurally distinct result => one more sample + one more raw form.
        let o3 = {
            let c = interner.intern_string("c");
            interner.object(vec![PropertyInfo::new(c, TypeId::STRING)])
        };
        record_canon_headroom(&key, o3, db, None);
        let after = canon_headroom_snapshot_for_tests(idx);

        assert_eq!(
            after.0 - before.0,
            2,
            "two distinct result ids sampled (o1, o3); the repeat of o1 is gated"
        );
        // o1 and o3 are structurally distinct => two distinct raw fingerprints.
        assert_eq!(
            after.1 - before.1,
            2,
            "two distinct raw symbol-stripped forms"
        );
        assert_eq!(after.2 - before.2, 2, "two first-sight samples counted");
    }
}
