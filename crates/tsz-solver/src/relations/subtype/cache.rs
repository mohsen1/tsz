//! Subtype check caching and cycle detection layer.
//!
//! This module implements the outer `check_subtype` method which wraps the
//! structural dispatch in `check_subtype_inner` with:
//! - Fast paths (identity, `any`, `unknown`, `never`, `error`)
//! - Cross-checker memoization via `QueryDatabase`
//! - Coinductive cycle detection via `RecursionGuard`
//! - DefId-level and SymbolId-level cycle detection for recursive types
//! - Pre-evaluation intrinsic checks (Object/Function interfaces)
//! - Meta-type evaluation bridging

use crate::construction::TypeDatabase;
use crate::def::DefId;
use crate::def::resolver::TypeResolver;
use crate::limits::limit_result_cache_enabled;
use crate::relations::subtype::{SubtypeChecker, SubtypeResult, is_disjoint_unit_type};
use crate::types::{
    IntrinsicKind, RelationCacheKey, RelationCacheValue, TypeApplicationId, TypeData, TypeId,
};
use crate::visitor::{
    application_id, array_element_type, conditional_type_id, contains_this_type, enum_components,
    lazy_def_id, literal_value, type_param_info, union_list_id,
};

// The global subtype chain fuel/depth state, the cache-poisoning sentinel
// counters, and all limit thresholds live in the consolidated `crate::limits`
// module (issue #13091). The re-exports below keep this module the stable
// import path for relation-side callers.
pub(crate) use crate::limits::{
    MAX_GLOBAL_SUBTYPE_FUEL, note_lazy_resolve_failure, note_weak_type_sensitivity,
    remaining_global_subtype_fuel,
};
pub use crate::limits::{lazy_resolve_failure_count, reset_subtype_thread_local_state};

/// One recorded `Ternary.Maybe`-style relation outcome awaiting validation by
/// the outermost frame of its checker instance (tsc `maybeKeys` parity).
///
/// `fuel_band: None` marks a cycle-derived Maybe: on outermost success the
/// coinductive assumption is validated and the key is promoted to a
/// definitive `true`. `fuel_band: Some(band)` marks a fuel-limit Maybe: on
/// outermost success it is promoted to a budget-conditional
/// [`RelationCacheValue::LimitTrue`] entry honest up to `band`.
#[derive(Copy, Clone, Debug)]
pub(crate) struct MaybeRelationEntry {
    key: RelationCacheKey,
    fuel_band: Option<u32>,
}

/// Frame-entry snapshot captured by `check_subtype` and consumed by
/// `finish_relation_frame` at every frame exit: the maybe-stack watermark,
/// the promotability of this frame's verdicts, whether the budget chain was
/// pristine at entry (fuel-band honesty), and the cache-poisoning sentinel
/// counters whose stability gates promotion.
#[derive(Copy, Clone, Debug)]
struct RelationFrameSnapshot {
    maybe_start: usize,
    frame_promotable: bool,
    pristine_budget_chain: bool,
    lazy_failures_at_entry: u64,
    weak_sensitivity_at_entry: u64,
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check if a Lazy type resolved to an Enum with the same DefId.
    ///
    /// When `Lazy(DefId(X))` resolves to `Enum(DefId(X), ...)`, the recursive call
    /// in `check_subtype` would extract the same DefId and falsely detect a cycle.
    /// This helper identifies that case so the caller can release the `def_guard`.
    /// For non-enum resolutions (e.g., recursive interfaces), the `def_guard` is
    /// critical for preventing infinite recursion and must NOT be released.
    fn is_lazy_to_same_enum(&self, original: TypeId, resolved: TypeId) -> bool {
        if original.is_intrinsic() || resolved.is_intrinsic() {
            return false;
        }
        if let Some(lazy_def) = lazy_def_id(self.interner, original)
            && let Some((enum_def, _)) = enum_components(self.interner, resolved)
        {
            return lazy_def == enum_def;
        }
        false
    }

    /// Guard against evaluation collapsing a compound type (union/intersection).
    ///
    /// When evaluation simplifies a union/intersection to a non-compound type
    /// (e.g., subtype reduction removes a member), we must preserve the original
    /// so the visitor can iterate over all members. Without this, a union like
    /// `{} | Dictionary<string>` could collapse to just `{}`, losing the constraint
    /// that ALL members must satisfy the target.
    fn guard_compound_collapse(&self, original: TypeId, evaluated: TypeId) -> TypeId {
        if evaluated == original {
            return evaluated;
        }
        let original_is_compound = union_list_id(self.interner, original).is_some()
            || matches!(
                self.interner.lookup(original),
                Some(TypeData::Intersection(_))
            );
        if !original_is_compound {
            return evaluated;
        }
        let eval_is_compound = union_list_id(self.interner, evaluated).is_some()
            || matches!(
                self.interner.lookup(evaluated),
                Some(TypeData::Intersection(_))
            );
        if eval_is_compound {
            evaluated
        } else {
            original
        }
    }

    /// When a cycle is detected, we return `CycleDetected` (coinductive semantics)
    /// which implements greatest fixed point semantics - the correct behavior for
    /// recursive type checking. When depth/iteration limits are exceeded, we return
    /// `DepthExceeded` (conservative false) for soundness.
    pub fn check_subtype(&mut self, source: TypeId, target: TypeId) -> SubtypeResult {
        let _span = tracing::trace_span!(
            "check_subtype",
            src = source.0,
            tgt = target.0,
            depth = self.guard.depth(),
        )
        .entered();

        // =========================================================================
        // Fast paths (no cycle tracking needed)
        // =========================================================================
        let allow_any_source = self
            .any_propagation
            .allows_any_source_at_depth(self.guard.depth());
        let allow_any_target = self
            .any_propagation
            .allows_any_target_at_depth(self.guard.depth());
        let mut source = source;
        let mut target = target;
        if !allow_any_source && source == TypeId::ANY {
            // In strict mode, any doesn't match everything structurally.
            // We demote it to STRICT_ANY so it only matches top types or itself.
            source = TypeId::STRICT_ANY;
        }
        if !allow_any_target && target == TypeId::ANY {
            target = TypeId::STRICT_ANY;
        }

        // Same type is always a subtype of itself
        if source == target {
            return SubtypeResult::True;
        }

        // Check type parameter equivalences established during generic function
        // subtype checking (alpha-renaming). When both types are TypeParameters
        // in the equivalence set, treat them as identical.
        if !self.type_param_equivalences.is_empty()
            && matches!(
                self.interner.lookup(source),
                Some(TypeData::TypeParameter(_))
            )
            && matches!(
                self.interner.lookup(target),
                Some(TypeData::TypeParameter(_))
            )
        {
            for &(eq_a, eq_b) in &self.type_param_equivalences {
                if (source == eq_a && target == eq_b) || (source == eq_b && target == eq_a) {
                    return SubtypeResult::True;
                }
            }
        }

        // PERF: Intrinsic disjointness fast-path for common primitive pairs.
        // Avoids cache lookup, canonical_id, and structural dispatch for the most
        // common "obviously not a subtype" cases like number vs string.
        // Both source and target are intrinsic (id < 100) and already known != each other.
        if source.is_intrinsic() && target.is_intrinsic() {
            // Intrinsic types that are known to be disjoint from each other.
            // If both are "concrete" intrinsics (not any/unknown/never/error/void/undefined/null
            // which have special assignability), they're disjoint.
            // Concrete primitive types that are mutually disjoint:
            // BOOLEAN(8), NUMBER(9), STRING(10), BIGINT(11), SYMBOL(12), OBJECT(13)
            const fn is_concrete_primitive(id: TypeId) -> bool {
                matches!(id.0, 8..=13)
            }
            if is_concrete_primitive(source) && is_concrete_primitive(target) {
                return SubtypeResult::False;
            }
        }

        // Any is assignable to anything except never (when allowed).
        // tsc: `if (s & TypeFlags.Any) return !(t & TypeFlags.Never);`
        if allow_any_source
            && (source == TypeId::ANY || source == TypeId::STRICT_ANY)
            && target != TypeId::NEVER
        {
            return SubtypeResult::True;
        }

        // Everything is assignable to any (when allowed)
        if allow_any_target && (target == TypeId::ANY || target == TypeId::STRICT_ANY) {
            return SubtypeResult::True;
        }

        // If not allowing any sources (nested strict any / identity mode /
        // overload subtype pass), STRICT_ANY can only match STRICT_ANY, ANY,
        // or UNKNOWN as a top-type source. Crucially, non-any types are NOT
        // assignable to STRICT_ANY in the symmetric strict modes.
        // This ensures that bidirectional subtype checks used for identity (TS2403)
        // correctly reject `number <: any` at nested depths, matching tsc's
        // isTypeIdenticalTo where `any` is only identical to `any`.
        if !allow_any_source
            && (source == TypeId::ANY || source == TypeId::STRICT_ANY)
            && (target == TypeId::ANY || target == TypeId::STRICT_ANY || target == TypeId::UNKNOWN)
        {
            return SubtypeResult::True;
        }
        // When strict any is active, STRICT_ANY as target is NOT a universal sink.
        // Non-any source types fall through to structural checking, which will fail
        // because STRICT_ANY has no structural properties to match against.

        // Everything is assignable to unknown
        if target == TypeId::UNKNOWN {
            return SubtypeResult::True;
        }

        // Never is assignable to everything
        if source == TypeId::NEVER {
            return SubtypeResult::True;
        }

        // Error types are assignable to/from everything (like `any` in tsc).
        // This prevents cascading diagnostics when type resolution fails.
        if crate::visitor::is_error_type(self.interner, source)
            || crate::visitor::is_error_type(self.interner, target)
        {
            return SubtypeResult::True;
        }

        // In TypeScript, `unknown` equals `{} | null | undefined`. When the
        // source is `unknown` and the target is a union containing all three
        // constituents, unknown is assignable. This is also handled by the compat
        // layer (empty_object_with_nullish_target), but the subtype layer needs
        // it too for nested checks that bypass compat.
        if source == TypeId::UNKNOWN
            && let Some(members) = union_list_id(self.interner, target)
        {
            let member_list = self.interner.type_list(members);
            // PERF: Check null and undefined first (O(1) identity checks).
            // Only intern the empty object if the nullish members are present,
            // avoiding a Vec allocation + hash lookup on the common non-matching path.
            let has_null = member_list.contains(&TypeId::NULL);
            let has_undef = member_list.contains(&TypeId::UNDEFINED);
            if has_null && has_undef {
                let empty_obj = self.interner.object(vec![]);
                let has_empty_obj = member_list
                    .iter()
                    .any(|&m| m == empty_obj || self.check_subtype(empty_obj, m).is_true());
                if has_empty_obj {
                    return SubtypeResult::True;
                }
            }
        }

        // Fast path: distinct disjoint unit types are never subtypes.
        // This avoids expensive structural checks for large unions of literals/enum members.
        // Guard: when both are Literal types with the same value but different TypeIds
        // (can happen when the same literal is interned from different contexts, e.g.,
        // JSDoc annotations on export default vs the expression type), they ARE equal.
        if is_disjoint_unit_type(self.interner, source)
            && is_disjoint_unit_type(self.interner, target)
        {
            // Check if both are literals with the same value
            if let (Some(TypeData::Literal(s_lit)), Some(TypeData::Literal(t_lit))) =
                (self.interner.lookup(source), self.interner.lookup(target))
                && s_lit == t_lit
            {
                return SubtypeResult::True;
            }
            return SubtypeResult::False;
        }

        // =========================================================================
        // Cross-checker memoization (QueryCache lookup) — BEFORE fuel tracking.
        // =========================================================================
        // Check the shared cache for a previously computed result BEFORE
        // incrementing the global fuel/depth counters. This avoids 4 TLS accesses
        // (2 enter + 2 leave) for every cache-hit check, which is significant
        // when the cache hit rate is high (e.g., repeated assignability checks
        // in generic function bodies).
        //
        // Skip when identity_cycle_check is active: the cache key doesn't encode
        // the identity-mode flag, so a cached `true` from a normal subtype check
        // would incorrectly short-circuit the identity check (which needs stricter
        // Application type-argument comparison at cycle points for TS2403).
        // Types containing ThisType are context-dependent (they depend on which class
        // is currently being checked via the resolver's this_type_stack). Caching them
        // in the shared cache would poison it with results computed outside of any
        // class context (e.g., during class type construction), causing later
        // legitimate checks inside class bodies to get the wrong cached result. Such
        // pairs instead use the instance-local fallback memo (issue #13828), which is
        // valid for the lifetime of this checker — the `this` binding is fixed for one
        // top-level query — and dropped afterward, so it cannot poison sibling checks.
        //
        // Use contains_this_type (not just is_this_type) because even after
        // substitute_this_type_if_needed the *target* may still be a class instance
        // type whose method signatures carry `ThisType` return types.
        let has_this_type =
            contains_this_type(self.interner, source) || contains_this_type(self.interner, target);
        // Class-symbol classification is also context-dependent: the callback
        // can make the same object shape behave as a named class/interface in
        // one checker and as a plain structural object in another. Since the
        // relation cache key cannot encode an arbitrary predicate, those answers
        // are excluded from the cross-checker shared cache and likewise served by
        // the instance-local fallback memo, whose `is_class_symbol` closure is fixed
        // for the checker's lifetime.
        let has_class_check_context = self.is_class_symbol.is_some();
        let can_use_shared_relation_cache = !has_this_type && !has_class_check_context;
        // The `in_callback_param_check` state is encoded in
        // `RelationFlags::IN_CALLBACK_PARAM_CHECK` via `make_cache_key`, so
        // callback-mode results live in a separate cache slot from
        // non-callback-mode results and cannot poison each other.
        if !self.identity_cycle_check {
            if can_use_shared_relation_cache {
                if let Some(db) = self.query_db {
                    let key = self.make_cache_key(source, target);
                    match db.lookup_subtype_cache_value(key) {
                        Some(RelationCacheValue::True) => return SubtypeResult::True,
                        Some(RelationCacheValue::False) => return SubtypeResult::False,
                        // Budget-conditional assumed-related verdict (tsc
                        // `Ternary.Maybe` parity): honest only when this query's
                        // remaining fuel budget is no larger than the recorded
                        // run's. Under a raised budget, fall through and recompute
                        // (fuel-band cache honesty).
                        Some(RelationCacheValue::LimitTrue { fuel_band })
                            if limit_result_cache_enabled()
                                && remaining_global_subtype_fuel() <= fuel_band =>
                        {
                            tsz_common::perf_counters::record_relation_limit_cache_hit();
                            return SubtypeResult::DepthExceeded;
                        }
                        Some(RelationCacheValue::LimitTrue { .. }) | None => {}
                    }
                }
            } else if !self.local_relation_cache.is_empty() {
                // Context-dependent pair (polymorphic `this` / class-check
                // context): excluded from the cross-checker shared cache, but
                // memoizable for this checker instance's lifetime (issue
                // #13828). The context — the resolver's `this` binding and the
                // `is_class_symbol` closure — is fixed for the instance, so a
                // definitive verdict for this pair holds for every repeat of it
                // in the same query's recursive structural walk. The memo is
                // dropped with the checker (and cleared by `reset`), so it can
                // never serve a verdict to a later query under a different
                // context. A non-empty memo implies `query_db` was present at
                // write time (and it never reverts to `None`), so no `query_db`
                // recheck is needed before building the key below.
                let key = self.make_cache_key(source, target);
                if let Some(&related) = self.local_relation_cache.get(&key) {
                    return if related {
                        SubtypeResult::True
                    } else {
                        SubtypeResult::False
                    };
                }
            }
        }

        // Structural Identity Fast-Path (O(1) after canonicalization)
        // Check if source and target canonicalize to the same TypeId, which means
        // they are structurally identical. This avoids expensive structural walks
        // for types that are the same structure but were interned separately.
        //
        // PERF: Placed AFTER cache lookup because cache is a simple hash check,
        // while canonical_id may allocate a Canonicalizer and traverse the type.
        // Guarded by bypass_evaluation to prevent infinite recursion when called
        // from TypeEvaluator during simplification (evaluation has already been done).
        if !self.bypass_evaluation
            && let Some(db) = self.query_db
        {
            let source_canon = db.canonical_id(source);
            let target_canon = db.canonical_id(target);
            if source_canon == target_canon {
                return SubtypeResult::True;
            }
        }

        // =========================================================================
        // Global fuel guard (cross-instance work limiter)
        // =========================================================================
        // Track nesting depth and consume fuel for every non-trivial check.
        // Fuel is monotonically consumed; depth tracks when we're back at root.
        // PERF: A single consolidated TLS access (`crate::limits`) reads and
        // updates the packed depth/fuel state AND snapshots the two
        // cache-poisoning sentinel counters and the shared solver-frame depth:
        //
        // - The unresolved-`Lazy` snapshot: if it changes while computing this
        //   pair's result, the result depended on a `Lazy` whose body was not
        //   yet registered, so a `False` is undetermined and must not be cached.
        // - The weak-type-sensitivity snapshot: if it changes, the result
        //   depended on weak-type enforcement state (TS2559), which the
        //   flag-agnostic `RelationCacheKey` does not encode. Caching it would
        //   let a result computed under one enforcement state be served to a
        //   sibling check under another.
        let frame_entry = crate::limits::enter_subtype_frame();
        let global_depth = frame_entry.global_depth;
        let fuel = frame_entry.fuel;
        let lazy_failures_at_entry = frame_entry.lazy_failures;
        let weak_sensitivity_at_entry = frame_entry.weak_sensitivity;

        // ── Limit-hit maybe-stack (tsc `maybeKeys` parity, issue #13241) ────
        // Frame-entry snapshot of the maybe stack. Every completion path of
        // this frame (after the recursion guard is entered) routes through
        // `finish_relation_frame`, which:
        //   - truncates the stack to this snapshot when the frame resolves to
        //     a definitive `False` (Maybe entries recorded inside this frame's
        //     subtree depended on an in-flight assumption this failure
        //     invalidates — tsc discards those maybeKeys the same way);
        //   - records this frame's key when it resolves to a Maybe verdict
        //     (`CycleDetected` / `DepthExceeded`) in a promotable context;
        //   - promotes (on overall success) or discards (on failure) all
        //     surviving entries when the outermost frame of this checker
        //     instance completes.
        let maybe_start = self.maybe_keys.len();
        let frame_promotable = can_use_shared_relation_cache
            && !self.bypass_evaluation
            && !self.identity_cycle_check
            && self.query_db.is_some()
            && limit_result_cache_enabled();
        // A fuel-limit Maybe verdict may only be recorded when every budget
        // dimension was pristine at this frame's entry: full global fuel
        // (`global_depth == 0`), a fresh per-instance iteration budget, and no
        // enclosing cross-operation solver frames. Any later query then holds
        // an equal-or-smaller budget in every dimension, so reusing the
        // assumed-related verdict is monotonically safe — a smaller budget
        // can only bail earlier with the same answer (fuel-band honesty).
        // The solver-frame depth was read under the same TLS resolution as the
        // fuel state in `enter_subtype_frame`, keeping it off the hot path.
        let pristine_budget_chain = global_depth == 0
            && self.guard.iterations() == 0
            && frame_entry.solver_stack_frames == 0;

        let frame_snapshot = RelationFrameSnapshot {
            maybe_start,
            frame_promotable,
            pristine_budget_chain,
            lazy_failures_at_entry,
            weak_sensitivity_at_entry,
        };

        // Helper macro: run the maybe-stack completion protocol for this
        // frame. Must be invoked at every exit taken after `guard.enter`
        // succeeded, after the corresponding `guard.leave`.
        macro_rules! finish_frame {
            ($result:expr) => {
                self.finish_relation_frame($result, frame_snapshot, source, target);
            };
        }

        // Helper macro to decrement global depth and optionally reset fuel on
        // early returns (fuel resets when the outermost chain frame exits).
        macro_rules! leave_global {
            () => {
                crate::limits::leave_subtype_frame(global_depth == 0);
            };
        }

        if fuel >= MAX_GLOBAL_SUBTYPE_FUEL {
            leave_global!();
            return self.depth_result();
        }

        // =========================================================================
        // Cycle detection (coinduction) via RecursionGuard - BEFORE evaluation!
        //
        // RecursionGuard handles iteration limits, depth limits, cycle detection,
        // and visiting set size limits in one call.
        // =========================================================================

        let pair = (source, target);

        // Check reversed pair for bivariant cross-recursion detection.
        if self.guard.is_visiting(&(target, source)) {
            leave_global!();
            return self.result_on_cycle(source, target);
        }

        use crate::recursion::RecursionResult;
        match self.guard.enter(pair) {
            RecursionResult::Cycle => {
                leave_global!();
                return self.result_on_cycle(source, target);
            }
            RecursionResult::DepthExceeded | RecursionResult::IterationExceeded => {
                leave_global!();
                return self.depth_result();
            }
            RecursionResult::Entered => {}
        }

        // =======================================================================
        // DefId-level cycle detection (before evaluation!)
        // Catches cycles in recursive type aliases BEFORE they expand.
        //
        // For non-Application types: extract DefId directly from Lazy/Enum.
        // For Application types (e.g., List<T>): extract the BASE DefId from
        // the Application's base type. This enables coinductive cycle detection
        // for recursive generic interfaces like List<T> extends Sequence<T>
        // where method return types create infinite expansion chains
        // (e.g., List<Pair<T,S>> <: Seq<Pair<T,S>> → List<Pair<...>> <: ...).
        //
        // For Application types with the SAME base DefId (e.g., Array<number>
        // vs Array<string>), we skip cycle detection because these are legitimate
        // comparisons that should not be treated as cycles.
        // =======================================================================

        // Extract DefId and Application info in a single pass per type.
        // This consolidates 3+ lookups per type into a single lookup + match.
        let s_app_id = application_id(self.interner, source);
        let t_app_id = application_id(self.interner, target);

        let extract_def_id = |interner: &dyn TypeDatabase,
                              type_id: TypeId,
                              app_id: Option<TypeApplicationId>|
         -> Option<DefId> {
            if let Some(def) = lazy_def_id(interner, type_id) {
                return Some(def);
            }
            if let Some((def, _)) = enum_components(interner, type_id) {
                return Some(def);
            }
            if let Some(app_id) = app_id {
                let app = interner.type_application(app_id);
                if let Some(def) = lazy_def_id(interner, app.base) {
                    return Some(def);
                }
            }
            None
        };

        let s_def_id = extract_def_id(self.interner, source, s_app_id);
        let t_def_id = extract_def_id(self.interner, target, t_app_id);

        // Skip DefId-level cycle detection when both are Application types with
        // the SAME base DefId (e.g., Box<number> vs Box<string>).
        let both_same_base_app = if let (Some(s_app_id), Some(t_app_id)) = (s_app_id, t_app_id) {
            let s_app = self.interner.type_application(s_app_id);
            let t_app = self.interner.type_application(t_app_id);
            s_app.base == t_app.base
                || {
                    let s_def = lazy_def_id(self.interner, s_app.base);
                    let t_def = lazy_def_id(self.interner, t_app.base);
                    matches!((s_def, t_def), (Some(sd), Some(td)) if self.resolver.defs_are_equivalent(sd, td))
                }
        } else {
            false
        };

        // For conditional type aliases that are same-base-app with identical
        // arguments, we still enter the def_guard (unlike non-conditional
        // same-base-app where def_pair = None). This implements tsc's recursion
        // identity mechanism for self-comparisons while still allowing
        // `DeepReadonly<number>` vs `DeepReadonly<string>` to compare the
        // differing arguments instead of cycling on the alias DefId alone.
        let is_cond_same_base_app = both_same_base_app
            && if let (Some(s_app_id), Some(t_app_id)) = (s_app_id, t_app_id) {
                let s_app = self.interner.type_application(s_app_id);
                let t_app = self.interner.type_application(t_app_id);
                s_app.args == t_app.args && self.is_conditional_alias_base_inline(s_app.base)
            } else {
                false
            };
        let def_pair = if both_same_base_app && !is_cond_same_base_app {
            None
        } else if let (Some(s_def), Some(t_def)) = (s_def_id, t_def_id) {
            Some((s_def, t_def))
        } else {
            None
        };

        // =======================================================================
        // Symbol-level cycle detection for cross-context DefId aliasing.
        //
        // The same interface (e.g., Promise) may get different DefIds in different
        // checker contexts (lib vs user file). When comparing recursive generic
        // interfaces, the DefId-level cycle detection can miss cycles because
        // the inner comparison uses different DefIds than the outer one.
        //
        // Fix: resolve DefIds to their underlying SymbolIds (stored in
        // DefinitionInfo). If a (SymbolId, SymbolId) pair is already being
        // visited via a different DefId pair, treat it as a cycle.
        // =======================================================================
        if let (Some(s_def), Some(t_def)) = (s_def_id, t_def_id) {
            let s_sym = self.resolver.def_to_symbol_id(s_def);
            let t_sym = self.resolver.def_to_symbol_id(t_def);
            if let (Some(s_sid), Some(t_sid)) = (s_sym, t_sym) {
                // Check if any visiting DefId pair maps to the same SymbolId pair
                let found_cycle = self.def_guard.is_visiting_any(|&(visiting_s, visiting_t)| {
                    let different_pair = visiting_s != s_def || visiting_t != t_def;
                    if !different_pair {
                        return false;
                    }
                    // Forward match: visiting (A, B) matches new (A', B') at SymbolId level
                    let s_sym_match = self.resolver.def_to_symbol_id(visiting_s) == Some(s_sid);
                    let t_sym_match = self.resolver.def_to_symbol_id(visiting_t) == Some(t_sid);
                    if s_sym_match && t_sym_match {
                        return true;
                    }
                    // Reversed match: visiting (A, B) matches new (B', A') at SymbolId level.
                    // This catches bivariant cross-recursion with aliased DefIds, e.g.,
                    // when checking IteratorObject<...> <: Generator<...> while
                    // Generator<...> <: IteratorObject<...> is being visited with
                    // different DefIds for the same SymbolIds.
                    let s_rev_match = self.resolver.def_to_symbol_id(visiting_s) == Some(t_sid);
                    let t_rev_match = self.resolver.def_to_symbol_id(visiting_t) == Some(s_sid);
                    s_rev_match && t_rev_match
                });
                if found_cycle {
                    self.guard.leave(pair);
                    let result = self.result_on_cycle(source, target);
                    finish_frame!(result);
                    leave_global!();
                    return result;
                }
            }
        }

        let mut def_entered = if let Some((s_def, t_def)) = def_pair {
            // Check reversed pair for bivariant cross-recursion
            if self.def_guard.is_visiting(&(t_def, s_def)) {
                self.guard.leave(pair);
                let result = self.result_on_cycle(source, target);
                finish_frame!(result);
                leave_global!();
                return result;
            }
            match self.def_guard.enter((s_def, t_def)) {
                RecursionResult::Cycle => {
                    self.guard.leave(pair);
                    let result = self.result_on_cycle(source, target);
                    finish_frame!(result);
                    leave_global!();
                    return result;
                }
                RecursionResult::Entered => Some((s_def, t_def)),
                _ => None,
            }
        } else {
            None
        };

        // =========================================================================
        // Pre-evaluation intrinsic checks
        // =========================================================================
        // Object interface: any non-nullable source is assignable.
        // In TypeScript, the Object interface from lib.d.ts is the root of
        // the prototype chain — all types except null/undefined/void are
        // assignable to it. We must check BEFORE evaluate_type() because
        // evaluation may change the target TypeId, losing the boxed identity.
        {
            let is_object_interface_target =
                crate::type_queries::is_global_interface_by_identity_with_resolver(
                    self.interner,
                    self.resolver,
                    target,
                    IntrinsicKind::Object,
                );
            if is_object_interface_target {
                // is_nullable() short-circuits before the interner lookup for common null/undefined/void cases.
                if source.is_nullable() || !self.is_global_object_interface_type(source) {
                    if let Some(dp) = def_entered {
                        self.def_guard.leave(dp);
                    }
                    self.guard.leave(pair);
                    finish_frame!(SubtypeResult::False);
                    leave_global!();
                    return SubtypeResult::False;
                }
                let result = self.check_object_contract(source, target);
                if let Some(dp) = def_entered {
                    self.def_guard.leave(dp);
                }
                self.guard.leave(pair);
                finish_frame!(result);
                leave_global!();
                return result;
            }
        }

        // Check if target is the Function interface from lib.d.ts.
        // We must check BEFORE evaluate_type() because evaluation resolves
        // Lazy(DefId) → ObjectShape, losing the DefId identity needed to
        // recognize the type as an intrinsic interface.
        if !self.bypass_evaluation
            && crate::type_queries::is_global_interface_by_identity_with_resolver(
                self.interner,
                self.resolver,
                target,
                IntrinsicKind::Function,
            )
        {
            let source_eval = self.evaluate_type(source);
            if self.is_callable_type(source_eval) {
                // North Star Fix: is_callable_type now respects allow_any correctly.
                // If it returned true, it means either we're in permissive mode OR
                // the source is genuinely a callable type.
                if let Some(dp) = def_entered {
                    self.def_guard.leave(dp);
                }
                self.guard.leave(pair);
                finish_frame!(SubtypeResult::True);
                leave_global!();
                return SubtypeResult::True;
            }
        }

        // Deferred conditional targets containing `infer` must be checked in their
        // raw form when the check context is still generic. Eager evaluation can
        // bind the infer variables from a broad constraint and incorrectly accept
        // assignments that tsc rejects until instantiation.
        if !self.bypass_evaluation
            && let Some(target_cond_id) = conditional_type_id(self.interner, target)
        {
            let target_cond = self.interner.get_conditional(target_cond_id);
            let target_has_infer = crate::type_queries::contains_infer_types_db(
                self.interner,
                target_cond.extends_type,
            ) || crate::type_queries::contains_infer_types_db(
                self.interner,
                target_cond.true_type,
            ) || crate::type_queries::contains_infer_types_db(
                self.interner,
                target_cond.false_type,
            );
            let target_is_deferred_context =
                crate::visitor::contains_type_parameters(self.interner, target_cond.check_type)
                    || crate::visitor::contains_type_parameters(
                        self.interner,
                        target_cond.extends_type,
                    )
                    || crate::visitor::contains_type_parameters(
                        self.interner,
                        target_cond.true_type,
                    )
                    || crate::visitor::contains_type_parameters(
                        self.interner,
                        target_cond.false_type,
                    )
                    || contains_this_type(self.interner, target_cond.check_type)
                    || contains_this_type(self.interner, target_cond.extends_type)
                    || contains_this_type(self.interner, target_cond.true_type)
                    || contains_this_type(self.interner, target_cond.false_type);

            if target_has_infer && target_is_deferred_context {
                let evaluated_source = self.evaluate_type(source);
                if evaluated_source == target
                    || conditional_type_id(self.interner, evaluated_source).is_some_and(
                        |source_cond_id| {
                            let source_cond = self.interner.get_conditional(source_cond_id);
                            source_cond.check_type == target_cond.check_type
                                && source_cond.extends_type == target_cond.extends_type
                                && source_cond.true_type == target_cond.true_type
                                && source_cond.false_type == target_cond.false_type
                                && source_cond.is_distributive == target_cond.is_distributive
                        },
                    )
                {
                    if let Some(dp) = def_entered {
                        self.def_guard.leave(dp);
                    }
                    self.guard.leave(pair);
                    finish_frame!(SubtypeResult::True);
                    leave_global!();
                    return SubtypeResult::True;
                }
                let result = self.subtype_of_conditional_target(source, &target_cond);
                if let Some(dp) = def_entered {
                    self.def_guard.leave(dp);
                }
                self.guard.leave(pair);
                finish_frame!(result);
                leave_global!();
                return result;
            }
        }

        // =========================================================================
        // Pre-evaluation variance fast path for Application types.
        //
        // When both types are Application types (e.g., FunctionComponent<X> vs
        // FunctionComponent<Y>), check type argument compatibility using variance
        // BEFORE evaluation. This is critical because evaluation converts
        // Application → Object, losing the generic identity needed for variance-
        // based rejection. Without this, recursive generic interfaces like
        // FunctionComponent<P> get structurally compared with coinductive cycle
        // detection, which incorrectly assumes compatibility when type arguments
        // differ (e.g., SomePropsCloneX vs SomeProps).
        //
        // Also handles the common case where the target is a Union containing an
        // Application (e.g., from optional properties: FC<SomeProps> | undefined).
        // Without this, the source Application gets evaluated to an Object before
        // the union is unwrapped, losing the generic identity.
        // =========================================================================
        if !self.bypass_evaluation {
            // When source is the evaluated (non-Application) form of a generic type but
            // target is still an Application, recover the source's original Application
            // via display_alias for variance checking. Without this, a source like
            // ICEP<unknown,unknown> (a Callable evaluated from the Application) would
            // miss the variance fast path against ICEP<any,any> (still an Application),
            // causing an unnecessary full structural expansion that fails on self-referential
            // types.
            let s_app_id_for_variance = s_app_id.or_else(|| {
                self.interner
                    .get_display_alias(source)
                    .and_then(|alias| application_id(self.interner, alias))
            });
            // When the TARGET also lost its Application identity to evaluation
            // (e.g. a parameter type like `Kysely<any>` already expanded to an
            // Object), recover it via display provenance as well — but trust
            // it only in the PERMISSIVE direction. Provenance is
            // display-grade, so a provenance-recovered target is never used
            // to reject: the all-`any`-target-args lawyer shortcut and the
            // accept-only variance check below may conclude `True`; every
            // failure falls through to the structural comparison unchanged.
            let t_app_id_for_recovered_target = t_app_id
                .or_else(|| {
                    self.interner
                        .get_display_alias(target)
                        .and_then(|alias| application_id(self.interner, alias))
                })
                .or_else(|| {
                    self.interner
                        .get_application_eval_origin(target)
                        .and_then(|origin| application_id(self.interner, origin))
                });
            let variance_result = if let (Some(s_app_id), Some(t_app_id)) =
                (s_app_id_for_variance, t_app_id)
            {
                self.try_variance_fast_path(s_app_id, t_app_id)
            } else if let Some(t_app_id) = t_app_id_for_recovered_target {
                // Accept-only variance on a provenance-recovered same-base
                // pair: tsc relates two instantiations of the same generic
                // reference by per-argument variance (`relateVariances`)
                // before any structural expansion, so an `any` argument
                // relates bidirectionally and silences the member walk
                // (kysely `ExpressionWrapper<DB, TB, any>` vs
                // `ExpressionWrapper<DB, TB, O[K]>`, whose `and` member is a
                // deferred conditional that can never relate structurally).
                // tsz's checker computes class member types in evaluated
                // form, so BOTH sides may have lost their `Application`
                // identity here; recover each via display provenance and
                // honor only a conclusive `True` — rejections from
                // display-grade identity are discarded and the relation
                // falls through to the structural path.
                //
                // The display-alias and the semantic eval-origin channels
                // can name DIFFERENT bases for one value: a value typed
                // through a generic alias over a self-referential generic
                // (e.g. `Async<T> = Promise<T>`) records the user alias
                // `Async` as its display provenance but the underlying
                // `Promise` as its eval origin, while the target only
                // recovered its underlying base. Try both source candidates
                // so the one that shares the target's definition
                // (`Promise<any>` vs `Promise<X>`) is found and its `any`
                // argument relates the pair before the order-dependent
                // structural expansion of the recursive `then` member can
                // spuriously reject it (the false `TS2416` on method
                // overrides whose return type is a generic alias over
                // `Promise` — zod's `_parse`).
                let s_candidates = [
                    s_app_id_for_variance,
                    self.interner
                        .get_application_eval_origin(source)
                        .and_then(|origin| application_id(self.interner, origin)),
                ];
                let mut vr = None;
                for s_app_id in s_candidates.into_iter().flatten() {
                    vr = self.try_same_base_all_any_target_args(source, Some(s_app_id), t_app_id);
                    if matches!(vr, Some(SubtypeResult::True)) {
                        break;
                    }
                    // Identical argument lists prove nothing here: the sides
                    // are distinct shapes for a non-argument reason
                    // (context-dependent evaluation of the same application,
                    // e.g. under exactOptionalPropertyTypes), and only the
                    // structural comparison can judge that.
                    {
                        let s_app = self.interner.type_application(s_app_id);
                        let t_app = self.interner.type_application(t_app_id);
                        if s_app.args == t_app.args {
                            continue;
                        }
                    }
                    vr = self
                        .try_same_base_args_identical_or_any(s_app_id, t_app_id)
                        .or_else(|| self.try_variance_fast_path(s_app_id, t_app_id));
                    if matches!(vr, Some(SubtypeResult::True)) {
                        break;
                    }
                }
                match vr {
                    Some(SubtypeResult::True) => Some(SubtypeResult::True),
                    _ => None,
                }
            } else if let Some(s_app_id) = s_app_id {
                // Source is Application, target might be Union containing an Application.
                // This handles optional properties where target is App<X> | undefined.
                self.try_variance_against_union_target(source, s_app_id, target)
            } else {
                None
            };

            if let Some(result) = variance_result {
                if let Some(dp) = def_entered {
                    self.def_guard.leave(dp);
                }
                self.guard.leave(pair);
                self.record_definitive_verdict(
                    source,
                    target,
                    result,
                    can_use_shared_relation_cache,
                    lazy_failures_at_entry,
                    weak_sensitivity_at_entry,
                );
                finish_frame!(result);
                leave_global!();
                return result;
            }
        }

        let readonly_array_bridge_result = if let Some(source_members) =
            union_list_id(self.interner, source)
            && self.type_contains_readonly_array_syntax(target)
            && self
                .interner
                .type_list(source_members)
                .iter()
                .any(|&member| self.readonly_array_application_element(member).is_some())
        {
            let member_list = self.interner.type_list(source_members);
            let all_related = member_list
                .iter()
                .all(|&member| self.check_subtype(member, target).is_true());
            all_related.then_some(SubtypeResult::True)
        } else if let (Some(s_elem), Some(t_elem)) = (
            self.readonly_array_application_element(source),
            self.readonly_array_syntax_element(target),
        ) {
            Some(self.check_subtype(s_elem, t_elem))
        } else if let (Some(s_elem), Some(t_elem)) = (
            self.readonly_array_syntax_element(source),
            self.readonly_array_application_element(target),
        ) {
            Some(self.check_subtype(s_elem, t_elem))
        } else if let Some(s_elem) = self.readonly_array_application_element(source)
            && let Some(target_members) = union_list_id(self.interner, target)
        {
            let member_list = self.interner.type_list(target_members);
            if member_list
                .iter()
                .any(|&member| self.readonly_array_syntax_element(member).is_some())
            {
                let any_related = member_list.iter().any(|&member| {
                    self.readonly_array_syntax_element(member)
                        .is_some_and(|t_elem| self.check_subtype(s_elem, t_elem).is_true())
                        || self.check_subtype(source, member).is_true()
                });
                any_related.then_some(SubtypeResult::True)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(result) = readonly_array_bridge_result {
            if let Some(dp) = def_entered {
                self.def_guard.leave(dp);
            }
            self.guard.leave(pair);
            self.record_definitive_verdict(
                source,
                target,
                result,
                can_use_shared_relation_cache,
                lazy_failures_at_entry,
                weak_sensitivity_at_entry,
            );
            finish_frame!(result);
            leave_global!();
            return result;
        }

        // Arrays must compare their element types before evaluation turns them into
        // structural Array interface objects. Otherwise a generic mapped element type
        // can be accepted through the recursive Array shape even when the direct
        // element relation fails.
        if let (Some(s_elem), Some(t_elem)) = (
            array_element_type(self.interner, source),
            array_element_type(self.interner, target),
        ) {
            let result = self.check_subtype(s_elem, t_elem);
            if let Some(dp) = def_entered {
                self.def_guard.leave(dp);
            }
            self.guard.leave(pair);
            self.record_definitive_verdict(
                source,
                target,
                result,
                can_use_shared_relation_cache,
                lazy_failures_at_entry,
                weak_sensitivity_at_entry,
            );
            finish_frame!(result);
            leave_global!();
            return result;
        }

        // =========================================================================
        // Meta-type evaluation (after cycle detection is set up)
        // =========================================================================
        let result = if self.bypass_evaluation {
            if target == TypeId::NEVER && type_param_info(self.interner, source).is_none() {
                SubtypeResult::False
            } else {
                // Even with bypass_evaluation (used by the evaluator to prevent
                // infinite recursion), we must still resolve Lazy(DefId) types to
                // their structural forms. The visitor pattern resolves Lazy SOURCE
                // types via visit_lazy, but Lazy TARGET types are never resolved
                // by the visitor. Without this, subtype checks between types whose
                // nested components (e.g., index signature value types) are Lazy
                // will give incorrect results — causing simplify_union_members to
                // incorrectly collapse distinct union members.
                let source_resolved = self.resolve_lazy_type(source);
                let target_resolved = self.resolve_lazy_type(target);
                if source_resolved != source || target_resolved != target {
                    if (self.is_lazy_to_same_enum(source, source_resolved)
                        || self.is_lazy_to_same_enum(target, target_resolved))
                        && let Some(dp) = def_entered.take()
                    {
                        self.def_guard.leave(dp);
                    }
                    self.check_subtype(source_resolved, target_resolved)
                } else {
                    self.check_subtype_inner(source, target)
                }
            }
        } else {
            let literal_against_object_union = literal_value(self.interner, source).is_some()
                && union_list_id(self.interner, target).is_some_and(|members| {
                    self.interner.type_list(members).contains(&TypeId::OBJECT)
                });
            if literal_against_object_union {
                let result = self.check_subtype_inner(source, target);
                if let Some(dp) = def_entered {
                    self.def_guard.leave(dp);
                }
                self.guard.leave(pair);
                finish_frame!(result);
                leave_global!();
                return result;
            }

            let source_raw = self.evaluate_type(source);
            let target_raw = self.evaluate_type(target);
            let source_eval = self.guard_compound_collapse(source, source_raw);
            let target_eval = self.guard_compound_collapse(target, target_raw);

            if source_eval != source || target_eval != target {
                if (self.is_lazy_to_same_enum(source, source_eval)
                    || self.is_lazy_to_same_enum(target, target_eval))
                    && let Some(dp) = def_entered.take()
                {
                    self.def_guard.leave(dp);
                }
                self.check_subtype(source_eval, target_eval)
            } else if target == TypeId::NEVER && type_param_info(self.interner, source).is_none() {
                SubtypeResult::False
            } else {
                self.check_subtype_inner(source, target)
            }
        };

        // Cleanup: leave both guards.
        if let Some(dp) = def_entered {
            self.def_guard.leave(dp);
        }
        self.guard.leave(pair);

        tracing::trace!(
            src = source.0,
            tgt = target.0,
            ?result,
            "check_subtype dispatch result"
        );

        // Cache definitive results for cross-checker memoization. Context-
        // dependent pairs route to the instance-local fallback memo instead of
        // the shared cache (see `record_definitive_verdict` and the lookup
        // guard above).
        self.record_definitive_verdict(
            source,
            target,
            result,
            can_use_shared_relation_cache,
            lazy_failures_at_entry,
            weak_sensitivity_at_entry,
        );

        finish_frame!(result);

        // Decrement global depth; reset fuel when outermost call completes.
        // PERF: Single TLS access for both depth and fuel.
        crate::limits::leave_subtype_frame(global_depth == 0);

        result
    }

    /// Record a definitive subtype verdict for later reuse.
    ///
    /// Context-independent pairs are written to the cross-checker shared
    /// `QueryCache`. Context-dependent pairs — those carrying a polymorphic
    /// `this` or checked inside a class-check context, for which
    /// `can_use_shared_relation_cache` is `false` — are written to the
    /// instance-local fallback memo instead (issue #13828): their verdict
    /// depends on the resolver's current `this` binding and on the
    /// `is_class_symbol` closure, neither of which the flag-agnostic
    /// [`RelationCacheKey`] encodes, so sharing them across checker instances
    /// could poison sibling checks. Both inputs are fixed for the lifetime of
    /// one checker instance, so the local memo safely serves repeats of the
    /// same pair within one query and is dropped when the instance (or its
    /// `reset`) ends.
    ///
    /// Only definitive `True`/`False` verdicts are recorded; `CycleDetected` /
    /// `DepthExceeded` are budget-conditional and handled by the maybe-keys
    /// promotion path.
    ///
    /// This mirrors the discipline of the former `cache_definitive!` macro and
    /// is intentionally conservative: the poison-sentinel snapshots are
    /// process-wide (thread-local), so *any* unresolved-`Lazy` event or
    /// weak-type-sensitivity event anywhere in this top-level call's subtree —
    /// even in a branch whose result did not feed the final answer —
    /// suppresses the write. That only ever skips a write
    /// (correctness-preserving: the result is recomputed later), never
    /// produces a wrong answer. Results computed under `bypass_evaluation` are
    /// likewise never written: that mode compares raw alias/meta forms without
    /// expanding them, so its answers can differ from full-evaluation answers
    /// for the same pair, and the flag-agnostic `RelationCacheKey` cannot tell
    /// the two modes apart.
    fn record_definitive_verdict(
        &mut self,
        source: TypeId,
        target: TypeId,
        result: SubtypeResult,
        can_use_shared_relation_cache: bool,
        lazy_failures_at_entry: u64,
        weak_sensitivity_at_entry: u64,
    ) {
        let related = match result {
            SubtypeResult::True => true,
            SubtypeResult::False => false,
            SubtypeResult::CycleDetected | SubtypeResult::DepthExceeded => return,
        };
        if self.bypass_evaluation
            || crate::limits::poison_sentinel_counts()
                != (lazy_failures_at_entry, weak_sensitivity_at_entry)
        {
            return;
        }
        let Some(db) = self.query_db else {
            return;
        };
        let key = self.make_cache_key(source, target);
        if can_use_shared_relation_cache {
            db.insert_subtype_cache(key, related);
        } else {
            self.local_relation_cache.insert(key, related);
        }
    }

    /// Maybe-stack completion protocol for one `check_subtype` frame
    /// (tsc `maybeKeys` parity, issue #13241).
    ///
    /// Called at every frame exit taken after the recursion guard was entered
    /// (after the matching `guard.leave`). Semantics, mirroring tsc's
    /// `recursiveTypeRelatedTo` / `resetMaybeStack`:
    ///
    /// - A definitive `False` invalidates every Maybe entry recorded within
    ///   this frame's subtree: those verdicts may have leaned on an in-flight
    ///   coinductive assumption that this failure refutes, so they are
    ///   discarded (truncated back to `maybe_start`).
    /// - A Maybe verdict (`CycleDetected` / `DepthExceeded`) records this
    ///   frame's relation key for later validation. Cycle verdicts are
    ///   recorded at any depth (their assumption frame is an ancestor in this
    ///   same instance and will either succeed — validating them — or fail
    ///   and truncate them). Fuel verdicts are recorded only for frames that
    ///   started a pristine budget chain (`pristine_budget_chain`), where the
    ///   fuel band is the full budget and every other budget dimension
    ///   (instance depth, iteration count, shared solver frames) is at its
    ///   pristine maximum — so any later query reuses the verdict from an
    ///   equal-or-smaller budget, which is monotonically safe.
    /// - When the outermost frame of this checker instance completes
    ///   (`guard.depth() == 0`), surviving entries are promoted on overall
    ///   success — cycle entries to definitive `true` (the coinductive
    ///   assumption set is self-consistent), fuel entries to band-conditional
    ///   `LimitTrue` — or discarded on failure. Promotion is additionally
    ///   gated on the unresolved-`Lazy` / weak-type-sensitivity counters
    ///   having been stable across the whole outermost window, the same
    ///   discipline `record_definitive_verdict` applies to definitive writes.
    fn finish_relation_frame(
        &mut self,
        result: SubtypeResult,
        frame: RelationFrameSnapshot,
        source: TypeId,
        target: TypeId,
    ) {
        match result {
            SubtypeResult::False => {
                self.maybe_keys.truncate(frame.maybe_start);
            }
            SubtypeResult::CycleDetected => {
                if frame.frame_promotable {
                    self.maybe_keys.push(MaybeRelationEntry {
                        key: self.make_cache_key(source, target),
                        fuel_band: None,
                    });
                }
            }
            SubtypeResult::DepthExceeded => {
                if frame.frame_promotable && frame.pristine_budget_chain {
                    self.maybe_keys.push(MaybeRelationEntry {
                        key: self.make_cache_key(source, target),
                        fuel_band: Some(MAX_GLOBAL_SUBTYPE_FUEL),
                    });
                }
            }
            SubtypeResult::True => {}
        }

        // Outermost frame of this checker instance: validate or discard.
        if self.guard.depth() == 0 && !self.maybe_keys.is_empty() {
            let entries = std::mem::take(&mut self.maybe_keys);
            if result.is_true()
                && crate::limits::poison_sentinel_counts()
                    == (
                        frame.lazy_failures_at_entry,
                        frame.weak_sensitivity_at_entry,
                    )
                && let Some(db) = self.query_db
            {
                for entry in entries {
                    match entry.fuel_band {
                        None => db.promote_subtype_cache_true(entry.key),
                        Some(band) => db.insert_subtype_limit_true(entry.key, band),
                    }
                    tsz_common::perf_counters::record_relation_maybe_promotion();
                }
            }
        }
    }

    /// Returns the appropriate cycle result based on the current mode.
    ///
    /// In identity mode (TS2403), delegates to `identity_cycle_result` which
    /// compares Application type arguments before assuming related.
    /// In normal mode, delegates to `cycle_result` (coinductive assumption).
    fn result_on_cycle(&self, source: TypeId, target: TypeId) -> SubtypeResult {
        let result = if self.identity_cycle_check {
            self.identity_cycle_result(source, target)
        } else {
            self.cycle_result()
        };
        tracing::trace!(
            src = source.0,
            tgt = target.0,
            ?result,
            "relation cycle hit"
        );
        result
    }

    /// Identity-mode cycle result: check Application type arguments at cycle points.
    ///
    /// When a cycle is detected during identity checking (TS2403), we compare
    /// Application type arguments before assuming the types are related.
    ///
    /// Recursive generic interfaces like `IPromise2<T, V>` and `Promise2<T, V>`
    /// share the same structural pattern but may differ in their type arguments
    /// at the cycle point. For example:
    ///   - `IPromise2<W, U>` vs `Promise2<any, W>` → args differ → NOT identical
    ///   - `IPromise<U>` vs `Promise<U>` → args [U] == [U] → assume identical
    ///
    /// For non-Application types (evaluated objects, callables), falls back to
    /// the standard coinductive assumption (`CycleDetected` = True).
    pub(crate) fn identity_cycle_result(&self, source: TypeId, target: TypeId) -> SubtypeResult {
        let s_app = application_id(self.interner, source);
        let t_app = application_id(self.interner, target);
        if let (Some(s_app_id), Some(t_app_id)) = (s_app, t_app) {
            let s_app_data = self.interner.type_application(s_app_id);
            let t_app_data = self.interner.type_application(t_app_id);
            if s_app_data.args.len() != t_app_data.args.len() {
                return SubtypeResult::False;
            }
            for (s_arg, t_arg) in s_app_data.args.iter().zip(t_app_data.args.iter()) {
                if s_arg != t_arg {
                    return SubtypeResult::False;
                }
            }
            // All type arguments match — assume related at the cycle point
            self.cycle_result()
        } else {
            // Not both Application types — fall back to coinductive assumption
            self.cycle_result()
        }
    }

    /// Check whether an Application base TypeId belongs to a conditional type alias.
    ///
    /// First checks the pre-populated `conditional_alias_bases` cache (fast path).
    /// Falls back to the raw definition-store body for the `DefId`, bypassing the
    /// full `resolve_lazy` chain which can return a cached `Application` form or a
    /// self-`Lazy` wrapper for generic type aliases (hiding the real `Conditional`
    /// body). Only when the raw body is unavailable does this fall back to
    /// `resolve_lazy`, filtering out self-wrappers there too.
    pub(crate) fn is_conditional_alias_base_inline(&self, base: TypeId) -> bool {
        if self.interner.is_conditional_alias_base(base) {
            return true;
        }
        let Some(def_id) = lazy_def_id(self.interner, base) else {
            tracing::trace!(
                base = base.0,
                "is_conditional_alias_base_inline: no lazy def_id"
            );
            return false;
        };
        let def_kind = self.resolver.get_def_kind(def_id);
        if !matches!(def_kind, Some(crate::def::DefKind::TypeAlias)) {
            tracing::trace!(
                base = base.0,
                def_id = def_id.0,
                def_kind = ?def_kind,
                "is_conditional_alias_base_inline: not TypeAlias"
            );
            return false;
        }

        // Prefer the raw definition-store body, which always holds the
        // un-evaluated structural body registered at alias-definition time.
        // `resolve_lazy` for generic aliases can return a cached `Application`
        // TypeId (from `symbol_types`) that obscures the real `Conditional` body.
        let raw_body = self.resolver.get_def_raw_body(def_id, self.interner);
        let body_opt = raw_body.or_else(|| {
            let body = self.resolver.resolve_lazy(def_id, self.interner)?;
            // Filter out self-wrappers: a Lazy(def_id) returned by resolve_lazy
            // for a TypeAlias means the body wasn't available; treat as unknown.
            if lazy_def_id(self.interner, body) == Some(def_id) {
                None
            } else {
                Some(body)
            }
        });

        let Some(body) = body_opt else {
            // Same undetermined-result event as `resolve_lazy_type`: the
            // conditional-alias-base decision derived from an unresolvable
            // `Lazy` must not poison the subtype cache in the enclosing call.
            tracing::trace!(
                base = base.0,
                def_id = def_id.0,
                "is_conditional_alias_base_inline: no body found"
            );
            note_lazy_resolve_failure();
            return false;
        };
        let body_kind = self.interner.lookup(body);
        if matches!(body_kind, Some(TypeData::Conditional(_))) {
            tracing::trace!(
                base = base.0,
                def_id = def_id.0,
                body = body.0,
                "is_conditional_alias_base_inline: found Conditional body → true"
            );
            self.interner.mark_conditional_alias_base(base);
            return true;
        }
        tracing::trace!(
            base = base.0,
            def_id = def_id.0,
            body = body.0,
            body_kind = ?body_kind,
            "is_conditional_alias_base_inline: body is not Conditional → false"
        );
        false
    }

    /// Whether an `Application` base is an *indexed-access* type alias that must
    /// be expanded structurally rather than compared through the same-base
    /// variance fast path.
    ///
    /// `DefKind::TypeAlias` is transparent: `tsc` never compares two
    /// applications of a type alias nominally — it substitutes the arguments and
    /// relates the resulting structural types. For an alias whose body is an
    /// `IndexAccess` transform — the `TypeBox` shape
    /// `Static<T, P> = (T & { params: P })['static']` — the alias has no sound
    /// declared variance: the same-base variance fast path would compare the raw
    /// arguments (`typeof Input` vs `typeof Output`) and, through their nested
    /// same-base applications, hit the coinductive cycle assumption — silently
    /// reporting `Static<typeof Input>` assignable to `Static<typeof Output>`
    /// even when the expanded objects differ (a missing property). Skipping the
    /// fast path routes the comparison through structural expansion, which
    /// evaluates both applications to their concrete shapes and relates those,
    /// matching `tsc`.
    ///
    /// Conditional-bodied aliases are handled separately (their variance path is
    /// intentionally retained for differing arguments so genuine leaf mismatches
    /// are caught). Plain (union/object/tuple-bodied) type aliases and nominal
    /// interface/class applications keep the variance fast path; mapped-type
    /// alias bodies rely on the variance prober's structural-fallback signal.
    pub(crate) fn is_indexed_access_alias_base_inline(&self, base: TypeId) -> bool {
        let Some(def_id) = lazy_def_id(self.interner, base) else {
            return false;
        };
        if !matches!(
            self.resolver.get_def_kind(def_id),
            Some(crate::def::DefKind::TypeAlias)
        ) {
            return false;
        }
        let Some(body) = self.resolver.resolve_lazy(def_id, self.interner) else {
            note_lazy_resolve_failure();
            return false;
        };
        matches!(
            self.interner.lookup(body),
            Some(TypeData::IndexAccess(_, _))
        )
    }
}
