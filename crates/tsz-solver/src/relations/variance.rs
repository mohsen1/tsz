//! Variance calculation for type parameters.
//!
//! This module implements variance analysis for generic type parameters,
//! enabling O(1) generic assignability checks by determining whether type
//! parameters are covariant, contravariant, invariant, or independent.
//!
//! ## Variance (Task #41)
//!
//! Variance determines how subtyping of generic types relates to subtyping
//! of their type arguments:
//!
//! - **Covariant**: `Box<Dog>` <: `Box<Animal>` if `Dog` <: `Animal`
//! - **Contravariant**: `Writer<Animal>` <: `Writer<Dog>` if `Dog` <: `Animal`
//! - **Invariant**: `MutableBox<Dog>` <: `MutableBox<Animal>` only if `Dog === Animal`
//! - **Independent**: Type parameter not used - can be skipped in checks
//!
//! ## Implementation
//!
//! The `VarianceVisitor` traverses types while tracking polarity:
//! - **Positive polarity** (covariant positions): function returns, array elements
//! - **Negative polarity** (contravariant positions): function parameters
//! - **Both polarity** (invariant): mutable properties with different read/write types
//!
//! Cycle detection uses `(TypeId, Polarity)` pairs to allow correct variance
//! calculation for recursive types like `type List<T> = { head: T; tail: List<T> }`.
//!
//! Also supports lazy type resolution, recursive variance composition,
//! and Ref(SymbolRef) type handling.

use crate::caches::db::QueryDatabase;
use crate::construction::TypeDatabase;
use crate::def::DefId;
use crate::def::resolver::TypeResolver;
use crate::types::{
    CallableShapeId, ConditionalTypeId, FunctionShapeId, IntrinsicKind, LiteralValue, MappedTypeId,
    ObjectShapeId, StringIntrinsicKind, SymbolRef, TemplateLiteralId, TemplateSpan, TupleListId,
    TypeApplicationId, TypeData, TypeId, TypeListId, TypeParamInfo, Variance,
};
use crate::visitor::lazy_def_id;
use crate::visitors::visitor::TypeVisitor;

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_common::interner::Atom;

/// Outcome of the shared per-argument Application-variance loop.
///
/// This is the single source of truth for how a same-base/same-arity
/// `Application`-vs-`Application` pair is walked against its declared
/// per-parameter [`Variance`]. Both Application-variance fast paths
/// (`relation_queries::check_application_variance` at the relation-query
/// boundary, and `SubtypeChecker::try_variance_fast_path` in the engine)
/// run this exact loop so they cannot drift on which argument orientation a
/// given variance position checks. Each caller still owns the *relation* used
/// to relate two argument types (the boundary uses the lawyer
/// `CompatChecker::is_assignable`; the engine uses the raw judge
/// `check_subtype`) and the mapping from this outcome to its own
/// accept/reject/fall-through verdict — only the variance walk itself is
/// shared.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VarianceArgLoopOutcome {
    /// At least one argument was in a variance-relevant (non-independent)
    /// position and was therefore relation-checked.
    pub any_checked: bool,
    /// Every relation check performed so far succeeded (no mismatch).
    pub all_ok: bool,
    /// A *forward* (source-relates-to-target) check failed. Set for covariant
    /// and invariant positions; never set by the contravariant orientation
    /// (which only performs a reverse check). Used by the engine's
    /// recursive-mapped-alias rejection refinement.
    pub forward_rejected: bool,
}

/// Walk an `Application`-vs-`Application` argument list against its declared
/// per-parameter variances, relating argument types through the supplied
/// `arg_related` relation, and report the [`VarianceArgLoopOutcome`].
///
/// `arg_related(a, b)` must answer "is `a` related to `b`" under the caller's
/// chosen relation. The loop relates arguments per position:
/// - invariant: `arg_related(s, t)` then `arg_related(t, s)` (forward first);
/// - covariant: `arg_related(s, t)` (forward);
/// - contravariant: `arg_related(t, s)` (reverse only);
/// - independent: skipped.
///
/// The loop stops at the first mismatch, matching the historical
/// short-circuit behavior of both fast paths.
pub(crate) fn run_application_variance_arg_loop(
    variances: &[Variance],
    source_args: &[TypeId],
    target_args: &[TypeId],
    mut arg_related: impl FnMut(TypeId, TypeId) -> bool,
) -> VarianceArgLoopOutcome {
    let mut any_checked = false;
    let mut all_ok = true;
    let mut forward_rejected = false;

    for (i, variance) in variances.iter().enumerate() {
        let s_arg = source_args[i];
        let t_arg = target_args[i];

        if variance.is_invariant() {
            any_checked = true;
            if !arg_related(s_arg, t_arg) {
                forward_rejected = true;
                all_ok = false;
                break;
            }
            if !arg_related(t_arg, s_arg) {
                all_ok = false;
                break;
            }
        } else if variance.is_covariant() {
            any_checked = true;
            if !arg_related(s_arg, t_arg) {
                forward_rejected = true;
                all_ok = false;
                break;
            }
        } else if variance.is_contravariant() {
            any_checked = true;
            if !arg_related(t_arg, s_arg) {
                all_ok = false;
                break;
            }
        }
    }

    VarianceArgLoopOutcome {
        any_checked,
        all_ok,
        forward_rejected,
    }
}

/// Compute the variance of a type parameter within a type.
///
/// This is the main entry point for variance calculation. It analyzes how
/// a specific type parameter (identified by its name) is used within a type
/// to determine whether it's covariant, contravariant, invariant, or independent.
///
/// # Parameters
///
/// * `db` - The type database for looking up type structures
/// * `type_id` - The type to analyze (e.g., the body of a generic type)
/// * `target_param` - The name of the type parameter to find (e.g., "T")
///
/// # Returns
///
/// A `Variance` bitmask indicating how the type parameter is used:
///
/// # Examples
///
/// ```text
/// use crate::relations::variance::compute_variance;
/// use crate::types::*;
///
/// // For type ReadonlyArray<T> = { readonly [index: number]: T }
/// // T is in a covariant position (array element)
/// let variance = compute_variance(db, array_body, "T");
/// assert!(variance.is_covariant());
///
/// // For type Writer<T> = { write(x: T): void }
/// // T is in a contravariant position (function parameter)
/// let variance = compute_variance(db, writer_body, "T");
/// assert!(variance.is_contravariant());
///
/// // For type Box<T> = { get(): T; set(x: T): void }
/// // T is in both positions -> invariant
/// let variance = compute_variance(db, box_body, "T");
/// assert!(variance.is_invariant());
/// ```
pub fn compute_variance(db: &dyn QueryDatabase, type_id: TypeId, target_param: Atom) -> Variance {
    let mut computer = VarianceComputer::new(db.as_type_database(), db.as_type_resolver());
    computer.compute(type_id, target_param)
}

/// Compute the variance of a type parameter using an explicit resolver.
///
/// This is the resolver-aware equivalent of `compute_variance`. It is used by
/// relation checks that need to preserve local alias identity even when the
/// shared query cache cannot resolve those alias definitions.
pub fn compute_variance_with_resolver(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    type_id: TypeId,
    target_param: Atom,
) -> Variance {
    let mut computer = VarianceComputer::new(db, resolver);
    computer.compute(type_id, target_param)
}

/// Compute the full variance mask for a generic definition using an explicit resolver.
///
/// Returns `None` when the definition cannot be resolved to a generic body.
pub fn compute_type_param_variances_with_resolver(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    def_id: DefId,
) -> Option<Arc<[Variance]>> {
    let mut computer = VarianceComputer::new(db, resolver);
    computer.compute_def_variances(def_id)
}

pub fn compute_actual_type_param_variances_with_resolver(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    def_id: DefId,
) -> Option<Arc<[Variance]>> {
    let mut computer = VarianceComputer::new_actual(db, resolver);
    computer.compute_def_variances(def_id)
}

/// Session-cached form of [`compute_type_param_variances_with_resolver`].
///
/// Variance of a generic `DefId` is a pure function of that definition's
/// resolved body, so for a fixed checking session the declared-variance mask is
/// stable across every reference to the generic. Without memoization, each type
/// reference that validates its type arguments rebuilds a fresh
/// [`VarianceComputer`] and re-walks the entire (possibly deep, lazy-ref-heavy)
/// type graph from scratch — the dominant cost when checking large
/// generic-alias-heavy projects.
///
/// This helper computes the mask **using the supplied resolver** (never the
/// query database's own resolver, which may not see local alias bodies) and
/// wires two persistent tiers into the [`VarianceComputer`]:
///
/// 1. the universe-shared interner store (`TypeDatabase::shared_def_variance`),
///    reachable from every relation/evaluation path including those without a
///    `QueryDatabase`, shared across files and child checkers; and
/// 2. the per-checker session cache exposed by [`QueryDatabase`], kept for the
///    query database's own resolver paths.
///
/// ## Soundness gates (canonical masks and resolution fingerprints)
///
/// A [`VarianceComputer`] tracks `active_defs` to truncate cyclic
/// self-references (returning the "independent" placeholder mask). The mask a
/// nested `DefId` produces mid-walk is a pure function of its resolved body
/// **only when its subtree never back-edged into a def that was already on the
/// recursion stack below its own frame**: such a back-edge would be truncated
/// at the ancestor instead of at the def itself, yielding a context-dependent
/// (provisional) mask. The visitor-level nesting counters (`mapped_depth`,
/// `method_bivariant_depth`, `inside_unreliable`, `bound_type_params`) never
/// cross a `compute_def_variances` boundary — each nested def starts fresh
/// visitors at base context — so in-flight def dependency is the exact
/// condition for context sensitivity.
///
/// The computer therefore tracks, per def frame, the minimum stack depth of
/// any in-flight dependency observed in its subtree (including dependencies
/// inherited by consuming a provisional per-walk cache entry). A frame whose
/// subtree minimum is not below its own depth produced the same mask a fresh
/// top-level query would produce — canonical — and is promoted regardless of
/// nesting depth. Provisional (cycle-tentative) masks are never promoted;
/// they stay in the per-walk map exactly as before.
///
/// Resolver dependence is handled by a resolution-failure fingerprint: each
/// frame records the defs whose params/body/lazy resolution failed in its
/// subtree. A canonical mask is stored in the shared tier together with that
/// fingerprint, and a consumer replays it only after validating that every
/// fingerprint def still fails to resolve under its own resolver — under that
/// condition the stored mask is exactly what the consumer's fresh walk would
/// compute. Walks that hit non-fingerprintable gaps (failed `SymbolRef`
/// resolution, session-cache masks of unknown cleanliness) are opaque and
/// never reach the shared tier.
///
/// Only fully-resolved (`Some`) results are memoized. A `None` (unresolved or
/// non-generic) result is not cached, so a later reference made after the
/// definition's body becomes resolvable still recomputes.
pub fn compute_type_param_variances_with_resolver_cached(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    query_db: Option<&dyn QueryDatabase>,
    def_id: DefId,
) -> Option<Arc<[Variance]>> {
    let session_cache = query_db.filter(|_| variance_cache_enabled());
    let mut computer = VarianceComputer::new(db, resolver);
    computer.session_cache = session_cache;
    computer.compute_def_variances(def_id)
}

/// Debug kill-switch for the session-level computed-variance cache.
///
/// Set `TSZ_DISABLE_VARIANCE_CACHE=1` to bypass both reads and writes so the
/// resolver-aware variance walk runs uncached on every reference. Used to
/// bisect regressions and prove byte-identical diagnostics; defaults to enabled.
fn variance_cache_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("TSZ_DISABLE_VARIANCE_CACHE").is_err())
}

/// One per-walk memo entry for a def's computed variance mask.
///
/// `dep` is the canonicality witness: `None` means the mask is canonical —
/// identical to what a fresh top-level query for the def would produce (its
/// walk never depended on a def that was in-flight below its own frame).
/// `Some((depth, def))` records the shallowest in-flight dependency observed
/// when the mask was computed: the def at stack `depth` whose truncation made
/// this mask provisional. Consumers re-validate the witness against the
/// current stack and inherit the taint (see `compute_def_variances`).
struct DefVarianceEntry {
    result: Option<Arc<[Variance]>>,
    dep: Option<(usize, DefId)>,
    /// Resolution-failure fingerprint: the defs whose lazy resolution failed
    /// during this mask's computation. Consuming the entry replays these into
    /// the consumer's frame so ancestors inherit the fingerprint. `None`
    /// means no fingerprintable gaps.
    gaps: Option<Arc<[DefId]>>,
    /// Opaque resolver-dependence witness: `true` when the mask's computation
    /// hit a gap that cannot be expressed as a def fingerprint (failed
    /// `SymbolRef` resolution, a per-checker session-cache mask of unknown
    /// cleanliness, or a fingerprint overflow). Consuming such an entry makes
    /// the consumer opaque too, so it must not reach the universe-shared
    /// store.
    opaque: bool,
}

/// Maximum number of distinct failed defs a shared-store fingerprint may
/// carry. Each fingerprint def costs one `resolve_lazy` probe per store read;
/// walks with more distinct failures than this are treated as opaque
/// (per-walk reuse only).
const MAX_GAP_FINGERPRINT: usize = 16;

struct VarianceComputer<'a> {
    db: &'a dyn TypeDatabase,
    resolver: &'a dyn TypeResolver,
    use_declared_variance: bool,
    /// In-flight defs on the recursion stack, mapped to their stack depth.
    active_defs: FxHashMap<DefId, usize>,
    /// Stack-ordered in-flight defs (`depth -> DefId`), parallel to
    /// `active_defs`. Used to re-validate provisional-entry dependency
    /// witnesses against the current stack.
    active_stack: Vec<DefId>,
    cached_def_variances: FxHashMap<DefId, DefVarianceEntry>,
    /// Minimum stack depth of any in-flight dependency observed since the
    /// current frame was entered (`usize::MAX` = none). A frame whose subtree
    /// minimum stays at or above its own depth produced a canonical mask.
    min_inflight_dep: usize,
    /// Append-only log of fingerprintable resolution gaps observed during this
    /// computer's walks: defs whose params/body/lazy resolution failed. A
    /// frame's gap fingerprint is the slice appended while it was open
    /// (children's gaps stay in the log, so ancestors inherit them). Masks
    /// are pure functions of (def structure, failure set): they may be
    /// promoted to the universe-shared interner store together with their
    /// fingerprint, and replayed by any consumer whose resolver still fails
    /// the same defs.
    gap_log: Vec<DefId>,
    /// Monotonic count of non-fingerprintable gaps (failed `SymbolRef`
    /// resolutions, per-checker session-cache masks of unknown cleanliness).
    /// A frame that observed one is opaque and never reaches the shared store.
    opaque_gaps: u64,
    /// Whether canonical, resolution-clean masks may be read from / written to
    /// the universe-shared interner store (`TypeDatabase::shared_def_variance`).
    /// Only set for `use_declared_variance` computers (the store holds declared
    /// masks, so the `new_actual` computer must never touch it) and disabled by
    /// the `TSZ_DISABLE_VARIANCE_CACHE` kill switch.
    use_shared_store: bool,
    /// Optional session-persistent declared-variance cache.
    ///
    /// When present, `compute_def_variances` reads from and writes to this map
    /// for every def whose mask is canonical (see
    /// [`compute_type_param_variances_with_resolver_cached`]). Only wired in for
    /// `use_declared_variance` computers: the map stores declared masks, so the
    /// `new_actual` computer must never touch it.
    session_cache: Option<&'a dyn QueryDatabase>,
}

impl<'a> VarianceComputer<'a> {
    fn new(db: &'a dyn TypeDatabase, resolver: &'a dyn TypeResolver) -> Self {
        Self {
            db,
            resolver,
            use_declared_variance: true,
            active_defs: FxHashMap::default(),
            active_stack: Vec::new(),
            cached_def_variances: FxHashMap::default(),
            min_inflight_dep: usize::MAX,
            gap_log: Vec::new(),
            opaque_gaps: 0,
            use_shared_store: variance_cache_enabled(),
            session_cache: None,
        }
    }

    fn new_actual(db: &'a dyn TypeDatabase, resolver: &'a dyn TypeResolver) -> Self {
        Self {
            db,
            resolver,
            use_declared_variance: false,
            active_defs: FxHashMap::default(),
            active_stack: Vec::new(),
            cached_def_variances: FxHashMap::default(),
            min_inflight_dep: usize::MAX,
            gap_log: Vec::new(),
            opaque_gaps: 0,
            use_shared_store: false,
            session_cache: None,
        }
    }

    fn compute(&mut self, type_id: TypeId, target_param: Atom) -> Variance {
        let visitor = VarianceVisitor::new(self, target_param);
        visitor.compute(type_id)
    }

    fn compute_def_variances(&mut self, def_id: DefId) -> Option<Arc<[Variance]>> {
        if self.use_declared_variance
            && let Some(declared) = self.resolver.get_type_param_variance(def_id)
        {
            return Some(declared);
        }

        if let Some(entry) = self.cached_def_variances.get(&def_id) {
            // Inherit the canonicality witness: consuming a provisional mask
            // makes the consumer's mask provisional too. Re-validate the
            // witness against the current stack — if the recorded in-flight
            // frame is still active at the same depth, the dependency is live
            // at that depth; otherwise the entry was computed against a frame
            // that has since completed, so any frame consuming it now (other
            // than the always-canonical walk root) diverges from a fresh
            // computation and must not be promoted (depth 0 taints every
            // nested frame).
            if let Some((depth, dep_def)) = entry.dep {
                let live = self.active_stack.get(depth) == Some(&dep_def);
                let observed = if live { depth } else { 0 };
                self.min_inflight_dep = self.min_inflight_dep.min(observed);
            }
            if entry.opaque {
                self.opaque_gaps += 1;
            }
            let (result, gaps) = (entry.result.clone(), entry.gaps.clone());
            if let Some(gaps) = gaps {
                self.gap_log.extend_from_slice(&gaps);
            }
            return result;
        }

        if let Some(&depth) = self.active_defs.get(&def_id) {
            // Recursive self-reference: return independent (empty) variance for
            // each type parameter. This tells visit_application to skip the
            // recursive arguments entirely, so only non-recursive appearances of
            // the type parameter determine the variance. This avoids the previous
            // behavior of returning None which caused NEEDS_STRUCTURAL_FALLBACK
            // to be set, incorrectly forcing structural comparison for types like
            // Promise<T> that are clearly covariant from their direct usages.
            //
            // The truncated frame is an in-flight dependency of the current
            // subtree: record its depth for the canonicality gate.
            self.min_inflight_dep = self.min_inflight_dep.min(depth);
            let params = self.resolver.get_lazy_type_params(def_id);
            return params.map(|p| Arc::from(vec![Variance::empty(); p.len()]));
        }

        // The def is not in-flight, so a universe-shared mask is a candidate.
        // Stored masks are canonical and carry their resolution-failure
        // fingerprint; the mask equals what a fresh top-level query would
        // compute under any resolver that fails the same defs. Validate the
        // fingerprint against the current resolver, replay it into the
        // current frame on success, and recompute on mismatch.
        if self.use_shared_store
            && let Some((mask, gaps)) = self.db.shared_def_variance(def_id)
        {
            let valid = gaps.iter().all(|d| {
                self.resolver
                    .resolve_lazy_lookup_only(*d, self.db)
                    .is_none()
            });
            if valid {
                self.gap_log.extend_from_slice(&gaps);
                let gaps = if gaps.is_empty() { None } else { Some(gaps) };
                self.cached_def_variances.insert(
                    def_id,
                    DefVarianceEntry {
                        result: Some(mask.clone()),
                        dep: None,
                        gaps,
                        opaque: false,
                    },
                );
                return Some(mask);
            }
        }

        // Per-checker session mask: canonical (safe to replay at any nesting
        // depth for this checker) but of unknown resolver-cleanliness, so
        // consuming it counts as a resolution gap — the consumer's mask stays
        // out of the universe-shared store.
        if let Some(qdb) = self.session_cache
            && let Some(cached) = qdb.get_cached_type_param_variance(def_id)
        {
            self.opaque_gaps += 1;
            self.cached_def_variances.insert(
                def_id,
                DefVarianceEntry {
                    result: Some(cached.clone()),
                    dep: None,
                    gaps: None,
                    opaque: true,
                },
            );
            return Some(cached);
        }

        let my_depth = self.active_stack.len();
        self.active_defs.insert(def_id, my_depth);
        self.active_stack.push(def_id);
        let saved_min = self.min_inflight_dep;
        self.min_inflight_dep = usize::MAX;
        let gap_log_at_entry = self.gap_log.len();
        let opaque_at_entry = self.opaque_gaps;

        let result: Option<Arc<[Variance]>> = (|| {
            let params = self.resolver.get_lazy_type_params(def_id)?;
            if params.is_empty() {
                return None;
            }

            let body = self.resolver.resolve_lazy(def_id, self.db)?;
            let mut variances = Vec::with_capacity(params.len());
            for param in &params {
                variances.push(self.compute(body, param.name));
            }
            Some(Arc::from(variances))
        })();

        // A `None` result means params or body did not resolve (or the def is
        // non-generic) — resolver-dependent territory either way. Record the
        // def itself as a fingerprintable gap: the parent's mask is valid for
        // any resolver under which this def still does not resolve.
        if result.is_none() {
            self.gap_log.push(def_id);
        }

        self.active_stack.pop();
        self.active_defs.remove(&def_id);
        let subtree_min = self.min_inflight_dep;
        // Canonical iff the subtree never depended on a frame strictly below
        // this one. Back-edges to this frame itself (`subtree_min == my_depth`)
        // are the deterministic self-truncation a fresh top-level query would
        // also perform, so they do not make the mask provisional. The walk
        // root (`my_depth == 0`) is canonical by definition: it IS a fresh
        // top-level query.
        let canonical = subtree_min >= my_depth;
        // Dependencies on frames below this one remain in-flight dependencies
        // of the parent; those resolved at or within this frame do not.
        self.min_inflight_dep = saved_min.min(if subtree_min < my_depth {
            subtree_min
        } else {
            usize::MAX
        });

        let opaque = self.opaque_gaps != opaque_at_entry;
        // Deduplicated resolution-failure fingerprint of this frame's subtree
        // (children's gaps stay in the log, so this slice includes them).
        let mut fingerprint: smallvec::SmallVec<[DefId; 4]> = smallvec::SmallVec::new();
        for d in &self.gap_log[gap_log_at_entry..] {
            if !fingerprint.contains(d) {
                fingerprint.push(*d);
            }
        }
        let fingerprint_overflow = fingerprint.len() > MAX_GAP_FINGERPRINT;

        // Promote to the session cache only canonical, fully resolved masks.
        // A `None` (unresolved/non-generic) is not cached so a later reference
        // after the body resolves still recomputes. The stored mask equals
        // what a fresh uncached top-level query would return, so replaying it
        // cannot change a diagnostic.
        if canonical
            && let Some(qdb) = self.session_cache
            && let Some(variances) = result.as_ref()
        {
            qdb.insert_type_param_variance(def_id, variances.clone());
        }

        // Promote to the universe-shared interner store only canonical,
        // non-opaque masks, together with their resolution-failure
        // fingerprint. The stored value is a pure function of (def structure,
        // failure set): consumers validate the fingerprint against their own
        // resolver before replaying, so the cache is cross-checker
        // deterministic.
        let shared_gaps: Option<Arc<[DefId]>> = if fingerprint.is_empty() {
            None
        } else {
            Some(Arc::from(fingerprint.as_slice()))
        };
        if canonical
            && !opaque
            && !fingerprint_overflow
            && self.use_shared_store
            && let Some(variances) = result.as_ref()
        {
            self.db.insert_shared_def_variance(
                def_id,
                variances.clone(),
                shared_gaps.clone().unwrap_or_else(|| Arc::from([])),
            );
        }

        let dep = if canonical {
            None
        } else {
            Some((subtree_min, self.active_stack[subtree_min]))
        };
        self.cached_def_variances.insert(
            def_id,
            DefVarianceEntry {
                result: result.clone(),
                dep,
                gaps: shared_gaps,
                opaque: opaque || fingerprint_overflow,
            },
        );
        result
    }
}

/// Visitor that computes variance for a specific type parameter.
///
/// The visitor tracks the current polarity (positive for covariant positions,
/// negative for contravariant positions) as it traverses the type graph.
/// When it encounters the target type parameter, it records the current polarity.
struct VarianceVisitor<'a, 'b> {
    /// Shared variance computation host.
    computer: &'b mut VarianceComputer<'a>,
    /// The name of the type parameter we're searching for (e.g., 'T').
    target_param: Atom,
    /// The accumulated variance result so far.
    result: Variance,
    /// Unified recursion guard for (`TypeId`, Polarity) cycle detection.
    guard: crate::recursion::RecursionGuard<(TypeId, bool)>,
    /// Stack of polarities to track current position in the type graph.
    /// true = Positive (Covariant), false = Negative (Contravariant)
    polarity_stack: Vec<bool>,
    /// Names of bound type parameters (mapped type iteration variables) whose
    /// constraints should be skipped during variance computation. In a mapped
    /// type `{ [K in keyof S]: S[K] }`, K is a bound variable. Its constraint
    /// `keyof S` is already accounted for by visiting `mapped.constraint`.
    /// Without this, visiting K's constraint again through the template would
    /// double-count S's variance contribution (adding a spurious contravariant
    /// occurrence through the keyof reversal).
    bound_type_params: smallvec::SmallVec<[Atom; 2]>,
    /// Whether the target parameter was seen as the object of an indexed access.
    /// Used to detect when indexed access can normalize away type argument differences.
    seen_target_in_index_access: bool,
    /// Depth counter for mapped type nesting. When > 0, occurrences of the target
    /// parameter are inside a mapped type and should not set `DIRECT_USAGE`.
    inside_mapped_depth: u32,
    /// Depth counter for method-bivariant traversal. When > 0, we're inside
    /// method parameter types. TypeScript methods have bivariant parameter
    /// checking, and tsc's variance computation (via marker types) always
    /// produces BIVARIANT for type params found through method params, which
    /// is then checked using the COVARIANT direction. To match this, we
    /// record all target param occurrences as COVARIANT when inside method
    /// params, regardless of actual nesting depth/polarity.
    method_bivariant_depth: u32,
    /// When true, suppress `method_bivariant_depth` from being set. This is
    /// used inside indexed access types where `{ m(x: T): any }['m']`
    /// extracts the method as a plain function, stripping method-ness.
    suppress_method_bivariance: bool,
    /// True if the target parameter was found at a strictly-checked position —
    /// outside any method-bivariant context AND outside any application visit
    /// that already inherited `REJECTION_UNRELIABLE` from a nested generic.
    /// A strict occurrence provides a reliable variance signal that overrides
    /// the unreliability set by sibling method-bivariant occurrences: e.g.
    /// `{ m(x: T, cb: (x: T) => void) }` should be COVARIANT, not bivariant,
    /// because the callback occurrence pins the variance.
    strict_occurrence_seen: bool,
    /// Depth counter for visiting type arguments of an Application whose base
    /// generic has `REJECTION_UNRELIABLE` in its variance. Inside such a visit
    /// we do not treat the leaf occurrence as a strict signal — the
    /// unreliability has already been inherited from the wrapping application
    /// (e.g. `{ container: C1<T> }` should remain bivariant when `C1` is
    /// bivariant).
    inside_unreliable_application: u32,
    /// Completed type/context pairs within this target-param walk.
    ///
    /// The recursion guard only catches active cycles. Drizzle-like declaration
    /// graphs also contain wide diamonds where the same resolved subtree appears
    /// many times after its first visit has completed. Re-entering those
    /// completed subtrees is redundant because variance accumulation is
    /// monotonic/idempotent for a fixed visitor context.
    completed: FxHashSet<VarianceVisitKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct VarianceVisitKey {
    type_id: TypeId,
    polarity: bool,
    method_bivariant: bool,
    suppress_method_bivariance: bool,
    inside_mapped: bool,
    inside_unreliable_application: bool,
}

impl<'a, 'b> VarianceVisitor<'a, 'b> {
    /// Create a new `VarianceVisitor`.
    fn new(computer: &'b mut VarianceComputer<'a>, target_param: Atom) -> Self {
        Self {
            computer,
            target_param,
            result: Variance::empty(),
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::Variance,
            ),
            polarity_stack: vec![true], // Start with positive (covariant) polarity
            bound_type_params: smallvec::SmallVec::new(),
            seen_target_in_index_access: false,
            inside_mapped_depth: 0,
            method_bivariant_depth: 0,
            suppress_method_bivariance: false,
            strict_occurrence_seen: false,
            inside_unreliable_application: 0,
            completed: FxHashSet::default(),
        }
    }

    /// Entry point: computes the variance of `target_param` within `type_id`.
    fn compute(mut self, type_id: TypeId) -> Variance {
        self.visit_with_polarity(type_id, true);
        // When the type parameter is used as the object of an indexed access
        // AND a mapped type with modifiers is present (NEEDS_STRUCTURAL_FALLBACK),
        // the variance-based rejection becomes unreliable. Indexed access types
        // combined with intersections can normalize away differences between type
        // arguments, producing structurally equivalent instantiations even when
        // the type arguments themselves are not assignable.
        if self.seen_target_in_index_access && self.result.needs_structural_fallback() {
            self.result |= Variance::REJECTION_UNRELIABLE;
        }
        // If we found at least one strict (non-method-bivariant, non-inherited)
        // occurrence of the target parameter, the variance signal is reliable —
        // clear `REJECTION_UNRELIABLE` that may have been added by sibling
        // method-bivariant occurrences. This matches tsc, where a callback or
        // direct-position occurrence of T pins the variance even when T also
        // appears as a direct method parameter.
        //
        // Skip the clear when the target was seen as the object of an indexed
        // access: in that case `REJECTION_UNRELIABLE` is set for an unrelated
        // reason (indexed-access + intersection normalisation can collapse
        // distinct type arguments into structurally equal results — see
        // `DerivedTable<S>` in `variancePropagation`), and a sibling strict
        // occurrence does NOT make that rejection reliable.
        if self.strict_occurrence_seen && !self.seen_target_in_index_access {
            self.result.remove(Variance::REJECTION_UNRELIABLE);
        }
        self.result
    }

    /// Core recursive step with polarity tracking.
    fn visit_with_polarity(&mut self, type_id: TypeId, polarity: bool) {
        // Fast path: intrinsic types contribute no variance information —
        // they have no nested type parameters anywhere inside them. The
        // visitor's `visit_intrinsic` handler is `{}`, so the guard
        // enter/leave, polarity-stack push/pop, and dispatch are all
        // wasted work for them. `TypeId::is_intrinsic` is a free range
        // check. Mirrors #2001 / #2005 / #2008 / #2009.
        if type_id.is_intrinsic() {
            return;
        }

        let completed_key = self.completed_key(type_id, polarity);
        if completed_key.is_some_and(|key| self.completed.contains(&key)) {
            return;
        }

        // Unified enter: cycle detection + depth/iteration limits
        let key = (type_id, polarity);
        match self.guard.enter(key) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return, // Cycle or limits exceeded
        }

        // Push new polarity onto stack
        self.polarity_stack.push(polarity);

        // Dispatch via TypeVisitor trait - the visitor implementations below
        // will use get_current_polarity() to get the current polarity
        self.visit_type(self.computer.db, type_id);

        // Pop polarity from stack
        self.polarity_stack.pop();

        self.guard.leave(key);

        if !self.guard.is_exceeded()
            && let Some(key) = completed_key
        {
            self.completed.insert(key);
        }
    }

    fn completed_key(&self, type_id: TypeId, polarity: bool) -> Option<VarianceVisitKey> {
        if !self.bound_type_params.is_empty() {
            return None;
        }
        Some(VarianceVisitKey {
            type_id,
            polarity,
            method_bivariant: self.method_bivariant_depth > 0,
            suppress_method_bivariance: self.suppress_method_bivariance,
            inside_mapped: self.inside_mapped_depth > 0,
            inside_unreliable_application: self.inside_unreliable_application > 0,
        })
    }

    /// Get the current polarity from the stack.
    fn get_current_polarity(&self) -> bool {
        *self.polarity_stack.last().unwrap_or(&true)
    }

    /// Record an occurrence of the target parameter at the current polarity.
    fn add_occurrence(&mut self, polarity: bool) {
        if self.method_bivariant_depth > 0 {
            // Inside method parameter types, always record as COVARIANT.
            // This matches tsc behavior: method bivariance makes T appear in
            // both co and contra positions (BIVARIANT), but tsc checks bivariant
            // type args using the covariant direction first. The net effect is
            // that method-param occurrences act as covariant for variance checking.
            self.result |= Variance::COVARIANT;
            self.result |= Variance::REJECTION_UNRELIABLE;
        } else if polarity {
            self.result |= Variance::COVARIANT;
        } else {
            self.result |= Variance::CONTRAVARIANT;
        }
        // Mark as direct usage when outside mapped type contexts.
        // Direct usage (function params, return types, properties) provides
        // reliable variance signal, unlike mapped type keyof/template positions.
        if self.inside_mapped_depth == 0 {
            self.result |= Variance::DIRECT_USAGE;
        }
        // Track whether we've found T at a strict position. A strict occurrence
        // is one that's outside method bivariance AND outside an application
        // visit that already inherited unreliability. Such an occurrence pins
        // the variance signal — see `compute()` for how this is consumed.
        if self.method_bivariant_depth == 0 && self.inside_unreliable_application == 0 {
            self.strict_occurrence_seen = true;
        }
    }

    /// Check if a constraint type uses `keyof` of the target type parameter.
    /// For mapped types like `{ [K in keyof S]: Template }`, the key set depends
    /// on S via keyof, so the variance shortcut is unreliable even without modifiers.
    fn constraint_uses_keyof_of_target(&self, constraint: TypeId) -> bool {
        if let Some(crate::types::TypeData::KeyOf(inner)) = self.computer.db.lookup(constraint) {
            self.type_references_target_param(inner)
        } else {
            false
        }
    }

    /// Check if a type references the target type parameter (directly or nested).
    fn type_references_target_param(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        match self.computer.db.lookup(type_id) {
            Some(crate::types::TypeData::TypeParameter(info)) => info.name == self.target_param,
            Some(crate::types::TypeData::KeyOf(inner)) => self.type_references_target_param(inner),
            Some(crate::types::TypeData::IndexAccess(obj, idx)) => {
                self.type_references_target_param(obj) || self.type_references_target_param(idx)
            }
            _ => false,
        }
    }
}

impl<'a, 'b> TypeVisitor for VarianceVisitor<'a, 'b> {
    type Output = ();

    fn default_output() -> Self::Output {}

    // ===== Intrinsic types (no type parameters) =====
    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) {}

    fn visit_literal(&mut self, _value: &LiteralValue) {}

    fn visit_unique_symbol(&mut self, _symbol_ref: u32) {}

    fn visit_error(&mut self) {}

    fn visit_this_type(&mut self) {}

    // ===== Composite types =====

    /// Union types: variance is the union of variances from all members.
    fn visit_union(&mut self, list_id: u32) {
        let members = self.computer.db.type_list(TypeListId(list_id));
        // For unions, collect variance from all members
        // The union of covariant/contravariant gives us the overall variance
        for &member in members.iter() {
            // Polarity is preserved for union members
            self.visit_type(self.computer.db, member);
        }
    }

    /// Intersection types: variance is the union of variances from all members.
    fn visit_intersection(&mut self, list_id: u32) {
        let members = self.computer.db.type_list(TypeListId(list_id));
        // For intersections, collect variance from all members
        for &member in members.iter() {
            // Polarity is preserved for intersection members
            self.visit_type(self.computer.db, member);
        }
    }

    /// Array types: element type is in covariant position.
    fn visit_array(&mut self, element_type: TypeId) {
        // Array<T> is covariant in T
        // Current polarity preserved
        let current_polarity = self.get_current_polarity();
        self.visit_with_polarity(element_type, current_polarity);
    }

    /// Tuple types: element types are in covariant position.
    fn visit_tuple(&mut self, list_id: u32) {
        let elements = self.computer.db.tuple_list(TupleListId(list_id));
        let current_polarity = self.get_current_polarity();
        for element in elements.iter() {
            self.visit_with_polarity(element.type_id, current_polarity);
        }
    }

    /// Function types: parameters are contravariant, return type is covariant.
    fn visit_function(&mut self, shape_id: u32) {
        let shape = self.computer.db.function_shape(FunctionShapeId(shape_id));
        let current_polarity = self.get_current_polarity();

        let saved_method_depth = self.method_bivariant_depth;

        // Method parameters are bivariant at assignability time, but variance
        // probing still has to see type parameters used only in method
        // parameters. Treat those occurrences as covariant-first so generic
        // application checks do not classify Promise-like interfaces as
        // independent.
        if shape.is_method {
            if !self.suppress_method_bivariance {
                self.method_bivariant_depth = saved_method_depth + 1;
                for param in &shape.params {
                    self.visit_with_polarity(param.type_id, !current_polarity);
                }
                self.method_bivariant_depth = saved_method_depth;
            }
            // Method return type stays at the outer (saved) depth — strict
            // covariant position (matches tsc, where `interface C<T> { m():
            // T }` is COVARIANT).
            self.visit_with_polarity(shape.return_type, current_polarity);
            if let Some(this_ty) = shape.this_type {
                self.visit_with_polarity(this_ty, current_polarity);
            }
        } else {
            // Nested non-method function: T occurrences inside its
            // parameters, return type, and `this` are NOT method-bivariant,
            // even if this function value is itself a parameter of a
            // surrounding method. Reset `method_bivariant_depth` for ALL
            // child visits so leaf occurrences record their actual variance
            // polarity (e.g. `Promise<T>.then(cb: (x: T) => T)` records the
            // return-position T as COVARIANT, not bivariant).
            self.method_bivariant_depth = 0;
            for param in &shape.params {
                self.visit_with_polarity(param.type_id, !current_polarity);
            }
            self.visit_with_polarity(shape.return_type, current_polarity);
            if let Some(this_ty) = shape.this_type {
                self.visit_with_polarity(this_ty, !current_polarity);
            }
            self.method_bivariant_depth = saved_method_depth;
        }
    }

    /// Callable types: same variance rules as functions.
    fn visit_callable(&mut self, shape_id: u32) {
        let callable = self.computer.db.callable_shape(CallableShapeId(shape_id));
        let current_polarity = self.get_current_polarity();
        let saved_method_depth = self.method_bivariant_depth;

        // Call signatures
        for sig in &callable.call_signatures {
            // For methods (see visit_function for full rationale).
            if sig.is_method {
                if !self.suppress_method_bivariance {
                    self.method_bivariant_depth = saved_method_depth + 1;
                    for param in &sig.params {
                        self.visit_with_polarity(param.type_id, !current_polarity);
                    }
                    self.method_bivariant_depth = saved_method_depth;
                }
                // Method return type stays at the outer (saved) depth.
                self.visit_with_polarity(sig.return_type, current_polarity);
                if let Some(this_ty) = sig.this_type {
                    self.visit_with_polarity(this_ty, current_polarity);
                }
            } else {
                // Non-method call signature: reset method bivariance for the
                // entire signature — parameters, return type, and `this` —
                // matching `visit_function` for non-method shapes.
                self.method_bivariant_depth = 0;
                for param in &sig.params {
                    self.visit_with_polarity(param.type_id, !current_polarity);
                }
                self.visit_with_polarity(sig.return_type, current_polarity);
                if let Some(this_ty) = sig.this_type {
                    self.visit_with_polarity(this_ty, !current_polarity);
                }
                self.method_bivariant_depth = saved_method_depth;
            }
        }

        // Construct signatures follow the same rules. We deliberately do NOT
        // reset `method_bivariant_depth` here: a constructor signature inside
        // a generic interface (e.g. `interface ObjectContaining<T> { new
        // (sample: Partial<T>): Partial<T> }`) is not a callback in a
        // surrounding method, so the legacy depth propagation is the safe
        // behaviour. Changing this changed several `Partial<T>` /
        // `nongenericPartialInstantiations*` baselines without a clear win.
        for sig in &callable.construct_signatures {
            for param in &sig.params {
                self.visit_with_polarity(param.type_id, !current_polarity);
            }
            self.visit_with_polarity(sig.return_type, current_polarity);
            if let Some(this_ty) = sig.this_type {
                self.visit_with_polarity(this_ty, !current_polarity);
            }
        }

        // Properties follow the same rules as regular objects
        for prop in &callable.properties {
            // Read type is always checked at current polarity
            self.visit_with_polarity(prop.type_id, current_polarity);

            // CRITICAL FIX: Mutable properties are ALWAYS invariant
            if !prop.readonly {
                let write_ty = if prop.write_type != TypeId::NONE {
                    prop.write_type
                } else {
                    prop.type_id
                };
                self.visit_with_polarity(write_ty, !current_polarity);
            }
        }
    }

    /// Object types: properties are covariant (readonly) or invariant (mutable).
    fn visit_object(&mut self, shape_id: u32) {
        let shape = self.computer.db.object_shape(ObjectShapeId(shape_id));
        let current_polarity = self.get_current_polarity();

        for prop in &shape.properties {
            // TypeScript treats all properties as covariant for variance inference,
            // regardless of mutability. This matches tsc behavior where `{ x: T }`
            // is covariant in T even though the property is mutable (a well-known
            // unsoundness in TS for usability). Only explicit write_type differences
            // (set accessors with different types) contribute contravariant position.
            self.visit_with_polarity(prop.type_id, current_polarity);

            if prop.has_split_accessor() {
                self.visit_with_polarity(prop.write_type, !current_polarity);
            }
        }

        // Index signatures: same covariant-only rule for tsc parity
        if let Some(ref idx) = shape.string_index {
            self.visit_with_polarity(idx.value_type, current_polarity);
        }

        if let Some(ref idx) = shape.number_index {
            self.visit_with_polarity(idx.value_type, current_polarity);
        }
    }

    /// Object with index signatures: same variance rules as regular objects.
    fn visit_object_with_index(&mut self, shape_id: u32) {
        self.visit_object(shape_id);
    }

    /// Type parameters: check if this is our target.
    fn visit_type_parameter(&mut self, info: &TypeParamInfo) {
        if info.name == self.target_param {
            let current_polarity = self.get_current_polarity();
            self.add_occurrence(current_polarity);
        }

        // Skip constraint/default for bound type parameters (mapped type iteration
        // variables like K in `{ [K in keyof S]: S[K] }`). Their constraints are
        // already accounted for by visit_mapped visiting mapped.constraint directly.
        let is_bound = self.bound_type_params.contains(&info.name);
        if !is_bound {
            // Also check constraint (at current polarity).
            // Constraints affect structural shape: `<U extends T>` means T
            // constrains what U can be, so T's variance is affected.
            if let Some(constraint) = info.constraint {
                let current_polarity = self.get_current_polarity();
                self.visit_with_polarity(constraint, current_polarity);
            }

            // Type parameter defaults are NOT visited for variance: a default
            // like `<TResult1 = T>` is only used when the caller omits the
            // type argument, and even then it expresses an *instantiation
            // rule*, not an occurrence of T in the generic body. Counting it
            // as an occurrence over-constrains variance — for example,
            // `Promise<T>.then<TResult1 = T>(cb: (v: T) => TResult1):
            // Promise<TResult1>` would record T as both contravariant (via
            // the cb return's default) and covariant (via the Promise<TR1>
            // return's default), making T invariant and rejecting valid
            // `Promise<never>` → `Promise<X>` assignments.
            //
            // Constraints (`<U extends T>`) ARE visited — those genuinely
            // constrain U's structural shape and propagate T's variance.
        }
    }

    /// Bound parameters: not handled in variance (used for canonicalization).
    fn visit_bound_parameter(&mut self, _de_bruijn_index: u32) {}

    /// Resolve Lazy(DefId) types to analyze variance of the underlying type.
    fn visit_lazy(&mut self, def_id: u32) {
        // Resolve the Lazy(DefId) to its underlying TypeId
        let def_id = DefId(def_id);
        // Lookup-only: variance masks carry a resolution-failure fingerprint that
        // is validated with `resolve_lazy_lookup_only` (see `def_variances`), so
        // the gap set recorded here must be computed the same way — on-demand
        // miss-forcing (#12101) must not perturb the fingerprint.
        if let Some(resolved) = self
            .computer
            .resolver
            .resolve_lazy_lookup_only(def_id, self.computer.db)
        {
            let current_polarity = self.get_current_polarity();
            self.visit_with_polarity(resolved, current_polarity);
        } else {
            // Unresolved lazy reference: record the def in the frame's
            // resolution-failure fingerprint. The resulting mask is valid for
            // any resolver under which this def still does not resolve.
            self.computer.gap_log.push(def_id);
        }
    }

    /// Resolve Ref(SymbolRef) types to analyze variance (legacy path).
    fn visit_ref(&mut self, symbol_ref: u32) {
        let symbol_ref = SymbolRef(symbol_ref);

        // Try to convert Ref to DefId (migration path)
        if let Some(def_id) = self.computer.resolver.symbol_to_def_id(symbol_ref) {
            // Convert to Lazy and resolve (lookup-only: keep the variance
            // fingerprint insulated from #12101 miss-forcing — see `visit_lazy`).
            if let Some(resolved) = self
                .computer
                .resolver
                .resolve_lazy_lookup_only(def_id, self.computer.db)
            {
                let current_polarity = self.get_current_polarity();
                self.visit_with_polarity(resolved, current_polarity);
                return;
            }
        }

        // Fallback: resolve legacy symbols when DefId is unavailable.
        if let Some(resolved) = self
            .computer
            .resolver
            .resolve_symbol_ref(symbol_ref, self.computer.db)
        {
            let current_polarity = self.get_current_polarity();
            self.visit_with_polarity(resolved, current_polarity);
        } else {
            // Unresolved symbol reference: resolver-dependent and not
            // expressible as a def fingerprint — keep the resulting mask out
            // of the shared store.
            self.computer.opaque_gaps += 1;
        }
    }

    /// Recursive types: skip (already handled by cycle detection).
    fn visit_recursive(&mut self, _de_bruijn_index: u32) {}

    /// Enum types: check member type variance.
    fn visit_enum(&mut self, _def_id: u32, member_type: TypeId) {
        let current_polarity = self.get_current_polarity();
        self.visit_with_polarity(member_type, current_polarity);
    }

    /// Look up the base type's variance and compose it with current polarity.
    /// This enables recursive variance calculation for nested generics like
    /// `type Wrapper<T> = Box<T>` where `Box` is covariant, so `Wrapper` should also be covariant.
    fn visit_application(&mut self, app_id: u32) {
        let app = self.computer.db.type_application(TypeApplicationId(app_id));
        let current_polarity = self.get_current_polarity();

        // 1. Extract DefId from the base type
        let base_def_id = lazy_def_id(self.computer.db, app.base);
        let variances = base_def_id.and_then(|def_id| self.computer.compute_def_variances(def_id));

        if let Some(variances) = variances {
            // 3. Compose variance: for each argument, apply base param's variance rules
            for (i, &arg) in app.args.iter().enumerate() {
                // Default to invariance if base type has more args than variance entries
                let base_param_variance = variances
                    .get(i)
                    .copied()
                    .unwrap_or(Variance::COVARIANT | Variance::CONTRAVARIANT);

                // Propagate NEEDS_STRUCTURAL_FALLBACK and REJECTION_UNRELIABLE
                // from nested applications. If Required<T> needs structural fallback
                // due to modifiers, then Foo<T> = { a: Required<T> } also needs it.
                if base_param_variance.needs_structural_fallback() {
                    self.result |= Variance::NEEDS_STRUCTURAL_FALLBACK;
                }
                let inherits_unreliable = base_param_variance.rejection_unreliable();
                if inherits_unreliable {
                    self.result |= Variance::REJECTION_UNRELIABLE;
                }

                // Composition Rules:
                // - Covariant base param: Argument inherits current polarity
                // - Contravariant base param: Argument flips current polarity
                // While visiting `arg`, mark that any leaf occurrence of T
                // appearing through this application should not count as a
                // "strict signal": the wrapping application has already
                // decided this position is unreliable, so the leaf merely
                // re-emits that unreliability. Without this, a structurally
                // bivariant wrapper such as `{ container: C1<T> }` would be
                // incorrectly demoted to strict covariance once we clear
                // `REJECTION_UNRELIABLE` based on `strict_occurrence_seen`.
                if inherits_unreliable {
                    self.inside_unreliable_application += 1;
                }
                if base_param_variance.contains(Variance::COVARIANT) {
                    self.visit_with_polarity(arg, current_polarity);
                }
                if base_param_variance.contains(Variance::CONTRAVARIANT) {
                    self.visit_with_polarity(arg, !current_polarity);
                }
                if inherits_unreliable {
                    self.inside_unreliable_application -= 1;
                }
                // Note: Invariant (both bits) visits both. Independent (no bits) visits neither.
            }
        } else if base_def_id.is_some() {
            // Can't compute — assume invariance + structural fallback.
            // We have a DefId but can't resolve the body/params, so we
            // can't verify whether the inner type has mapped type modifiers
            // that would make the variance shortcut unsound.
            self.result |= Variance::NEEDS_STRUCTURAL_FALLBACK;
            for &arg in &app.args {
                self.visit_with_polarity(arg, current_polarity);
                self.visit_with_polarity(arg, !current_polarity);
            }
        } else {
            // No DefId available — assume invariance (safest choice)
            for &arg in &app.args {
                self.visit_with_polarity(arg, current_polarity);
                self.visit_with_polarity(arg, !current_polarity);
            }
        }
    }

    /// Conditional types: branch types contribute variance; the `check_type`
    /// selects a branch and the `extends_type` is a bound.
    fn visit_conditional(&mut self, cond_id: u32) {
        let cond = self.computer.db.get_conditional(ConditionalTypeId(cond_id));
        let current_polarity = self.get_current_polarity();

        // The branch types X and Y are ordinary covariant usage positions.
        self.visit_with_polarity(cond.true_type, current_polarity);
        self.visit_with_polarity(cond.false_type, current_polarity);

        // The `extends_type` U is only a bound on the branch selection. tsc
        // treats a parameter that appears *solely* in an extends position as
        // bivariant: two instantiations differing only in that argument relate
        // in either direction (e.g. `M<DB, TB>` whose member returns
        // `R extends TB[] ? X : Y` accepts `M<{a:1}, "a">` against `M<DB, TB>`).
        // So the extends position is intentionally NOT visited — leaving such a
        // parameter independent (bivariant).
        //
        // The `check_type` T, however, selects the branch: tsc measures a
        // parameter appearing there (two instantiations differing in it do
        // NOT relate, even when both branches collapse to the same type).
        //
        // When the parameter also occurs in a branch it is already measured by
        // the covariant visits above (so it is not independent and the
        // "all positions independent" shortcut cannot fire for it); only a
        // parameter that appears *solely* in the check position would otherwise
        // be left independent and wrongly treated as bivariant. The exact
        // measured variance of a distributive check position is subtle (it
        // depends on branch-vs-bound probing), so for that solely-check case we
        // mark the result as needing the structural fallback rather than
        // asserting a co/contra/invariant direction we cannot reliably model —
        // the structural comparison then judges the pair.
        let in_check = crate::contains_type_parameter_named_shallow(
            self.computer.db,
            cond.check_type,
            self.target_param,
        );
        if in_check {
            let in_branch = crate::contains_type_parameter_named_shallow(
                self.computer.db,
                cond.true_type,
                self.target_param,
            ) || crate::contains_type_parameter_named_shallow(
                self.computer.db,
                cond.false_type,
                self.target_param,
            );
            if !in_branch {
                self.result |= Variance::NEEDS_STRUCTURAL_FALLBACK;
            }
        }
    }

    /// Mapped types: constraint is contravariant, template is covariant.
    fn visit_mapped(&mut self, mapped_id: u32) {
        let mapped = self.computer.db.get_mapped(MappedTypeId(mapped_id));
        let current_polarity = self.get_current_polarity();

        // Mapped types with modifiers (-?/+?/-readonly/+readonly) require structural
        // fallback because mutually-assignable type arguments can produce structurally
        // incompatible results after modifier application (e.g., Required<{a?; x}> vs
        // Required<{b?; x}> — the args are assignable but the results differ).
        //
        // Additionally, mapped types whose constraint uses `keyof` of the target
        // type parameter (e.g., `{ [K in keyof S]: Type<S[K]> }`) need structural
        // fallback because the key set depends on S via `keyof S`, making the
        // variance check insufficient: a variance failure (e.g., invariant check
        // fails because `{a: 1} <: {}` but not `{} <: {a: 1}`) doesn't mean the
        // expanded mapped types are incompatible (`{ a: Type<1> }` IS assignable to `{}`).
        //
        // Plain mapped types like `Record<P, T> = { [K in P]: T }` do NOT need
        // fallback because the key set P is a direct type argument, not derived
        // through `keyof`, so variance correctly captures the relationship.
        if mapped.optional_modifier.is_some()
            || mapped.readonly_modifier.is_some()
            || self.constraint_uses_keyof_of_target(mapped.constraint)
        {
            self.result |= Variance::NEEDS_STRUCTURAL_FALLBACK;
        }

        // Homomorphic mapped types with non-identity templates need structural
        // fallback. For identity mapped types (`{ [K in keyof S]: S[K] }`), the
        // variance is purely covariant and reliable. But for non-identity templates
        // like `{ [K in keyof S]: Type<S[K]> }`, the template may introduce
        // contravariant positions (e.g., Type<A> with A in function parameter
        // position), making the variance invariant. However, the STRUCTURAL result
        // can still be compatible: `ToA<{x:n}>` is assignable to `ToA<{}>`
        // because `ToA<{}>` evaluates to `{}` (no keys, so structurally empty).
        //
        // This matches tsc's variance probing behavior: when probing gives
        // unreliable results for complex mapped types, tsc falls through to
        // structural comparison rather than definitively rejecting.
        {
            use crate::types::TypeData;
            if let Some(TypeData::KeyOf(source)) = self.computer.db.lookup(mapped.constraint) {
                // Check if the template is identity: T[K] where T is the keyof source
                // and K is the iteration variable.
                let is_identity = if let Some(TypeData::IndexAccess(obj, idx)) =
                    self.computer.db.lookup(mapped.template)
                {
                    obj == source
                        && matches!(
                            self.computer.db.lookup(idx),
                            Some(TypeData::TypeParameter(tp)) if tp.name == mapped.type_param.name
                        )
                } else {
                    false
                };
                if !is_identity {
                    self.result |= Variance::NEEDS_STRUCTURAL_FALLBACK;
                }
            }
        }

        // Type parameter constraint: check if it's our target
        if mapped.type_param.name == self.target_param {
            // The iteration variable K itself doesn't contribute to variance
            // It's a binder, not a usage of T
        }

        // Track that we're inside a mapped type so occurrences are not
        // marked as DIRECT_USAGE. Mapped type positions (keyof constraint,
        // template) can give unreliable variance signals.
        self.inside_mapped_depth += 1;

        // Constraint (K in keyof T) is CONTRAVARIANT with respect to T
        self.visit_with_polarity(mapped.constraint, !current_polarity);

        // Mark the iteration variable K as bound. When visiting the template,
        // encountering K should NOT trigger visiting K's constraint again —
        // the constraint is already accounted for above. Without this,
        // `{ [K in keyof S]: S[K] }` would give S an invariant variance
        // because K's constraint `keyof S` would add a spurious contravariant
        // contribution through the keyof reversal.
        let iter_var_name = mapped.type_param.name;
        self.bound_type_params.push(iter_var_name);

        // Template type is COVARIANT with respect to T.
        self.visit_with_polarity(mapped.template, current_polarity);

        // Name type (if present) is COVARIANT
        if let Some(name_type) = mapped.name_type {
            self.visit_with_polarity(name_type, current_polarity);
        }

        // Remove the bound variable
        self.bound_type_params.pop();

        self.inside_mapped_depth -= 1;
    }

    /// Index access: both object and key are at current polarity.
    ///
    /// When the target type parameter appears inside an indexed access (either as
    /// the object or the key), we mark the variance as needing structural fallback.
    /// This matches tsc's behavior where indexed access through a type parameter
    /// produces "unmeasurable" variance — the relationship between the type argument
    /// and the indexed access result is too complex for static variance analysis.
    ///
    /// Example: `S["base"] & S["new"]` in `DerivedTable<S>` — even though S is used
    /// covariantly, different instantiations like `{base: B, new: N}` and
    /// `{base: B, new: N & B}` can produce structurally equivalent indexed access
    /// results despite the type arguments not being subtypes of each other.
    fn visit_index_access(&mut self, object_type: TypeId, key_type: TypeId) {
        let current_polarity = self.get_current_polarity();
        // Track when the target parameter appears as the object of an indexed
        // access. This indicates that the type mapping S → S["key"] may
        // normalize away differences between type arguments.
        if let Some(TypeData::TypeParameter(tp)) = self.computer.db.lookup(object_type)
            && tp.name == self.target_param
        {
            self.seen_target_in_index_access = true;
        }
        let before = self.result;
        // Suppress method bivariance inside indexed access. When a type uses
        // the `bivarianceHack` pattern like `{ m(x: T): any }['m']`, the
        // indexed access extracts the method as a plain function, stripping
        // method-ness. Method params should be skipped here (INDEPENDENT),
        // matching the original behavior that allowed structural comparison
        // with proper method bivariance at the assignability level.
        let saved_smb = self.suppress_method_bivariance;
        self.suppress_method_bivariance = true;
        self.visit_with_polarity(object_type, current_polarity);
        self.visit_with_polarity(key_type, current_polarity);
        self.suppress_method_bivariance = saved_smb;
        // If the target parameter was found inside this indexed access,
        // the variance shortcut may be unreliable — require structural fallback.
        //
        // However, for simple literal-key indexed accesses like A['witness'],
        // the access is a straightforward property projection that preserves
        // variance reliability. The variance of A['key'] directly follows from
        // A's variance for that property. Only non-literal keys (type params,
        // keyof, unions, etc.) can cause non-obvious normalization that makes
        // the variance shortcut unreliable.
        if self.result != before {
            let is_literal_key = matches!(
                self.computer.db.lookup(key_type),
                Some(TypeData::Literal(_))
            );
            if !is_literal_key {
                self.result |= Variance::NEEDS_STRUCTURAL_FALLBACK;
            }
        }
    }

    /// Template literals: types in spans are at current polarity.
    fn visit_template_literal(&mut self, template_id: u32) {
        let before = self.result;
        let spans = self
            .computer
            .db
            .template_list(TemplateLiteralId(template_id));
        let current_polarity = self.get_current_polarity();
        self.inside_unreliable_application += 1;

        for span in spans.iter() {
            if let TemplateSpan::Type(type_id) = span {
                self.visit_with_polarity(*type_id, current_polarity);
            }
        }
        self.inside_unreliable_application -= 1;
        if self.result != before {
            self.result |= Variance::REJECTION_UNRELIABLE;
        }
    }

    /// Type query: not handled (would need symbol resolution).
    fn visit_type_query(&mut self, _symbol_ref: u32) {}

    /// Keyof: operand is CONTRAVARIANT.
    ///
    /// keyof reverses the variance relationship:
    /// - If T <: U (T is subtype of U), then keyof T has MORE properties than keyof U
    /// - Therefore keyof T is NOT a subtype of keyof U (it's a supertype)
    /// - Example: { a: 1, b: 2 } <: { a: 1 }, but "a" | "b" is NOT <: "a"
    fn visit_keyof(&mut self, type_id: TypeId) {
        let current_polarity = self.get_current_polarity();
        // keyof T reverses the variance (contravariant position)
        self.visit_with_polarity(type_id, !current_polarity);
    }

    /// Readonly types: inner type is at current polarity.
    fn visit_readonly_type(&mut self, inner_type: TypeId) {
        let current_polarity = self.get_current_polarity();
        self.visit_with_polarity(inner_type, current_polarity);
    }

    /// Infer types: declaration is not a usage.
    fn visit_infer(&mut self, info: &TypeParamInfo) {
        // FIX: Do not check info.name == self.target_param.
        // 'infer X' declares X, it doesn't use the outer target param.
        // If 'infer T' shadows outer 'T', it's still a declaration, not a usage.

        // Check constraint
        if let Some(constraint) = info.constraint {
            let current_polarity = self.get_current_polarity();
            self.visit_with_polarity(constraint, current_polarity);
        }
    }

    /// String intrinsics: type argument is at current polarity.
    fn visit_string_intrinsic(&mut self, _kind: StringIntrinsicKind, type_arg: TypeId) {
        let current_polarity = self.get_current_polarity();
        self.visit_with_polarity(type_arg, current_polarity);
    }

    /// Module namespace: not handled.
    fn visit_module_namespace(&mut self, _symbol_ref: u32) {}
}
