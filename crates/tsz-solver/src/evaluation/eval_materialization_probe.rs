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

use crate::types::{TypeData, TypeId};
use dashmap::{DashMap, DashSet};
use rustc_hash::FxBuildHasher;
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
}

fn new_dashset() -> DashSet<u32, FxBuildHasher> {
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
}
