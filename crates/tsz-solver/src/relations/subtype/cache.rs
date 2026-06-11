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
use crate::relations::subtype::{SubtypeChecker, SubtypeResult, is_disjoint_unit_type};
use crate::types::{IntrinsicKind, TypeApplicationId, TypeData, TypeId};
use crate::visitor::{
    application_id, array_element_type, conditional_type_id, contains_this_type, enum_components,
    lazy_def_id, literal_value, type_param_info, union_list_id,
};

// Global thread-local fuel counter for cross-instance subtype check termination.
//
// Unlike depth counters (which unwind), fuel is monotonically consumed and never
// restored until the outermost check_subtype call completes. This prevents the
// "infinite hang" scenario where each property comparison in an implements check
// triggers a deep evaluation chain — the total work across ALL properties is bounded.
//
// The depth counter tracks nesting level (incremented on enter, decremented on leave)
// to detect when we're back at the outermost call and can reset the fuel.
//
// PERF: Depth and fuel are packed into a single u64 to halve the TLS access count
// (2 per check_subtype call instead of 4). Layout: high 32 bits = fuel, low 32 bits = depth.
thread_local! {
    static GLOBAL_SUBTYPE_STATE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    // Monotonic counter bumped whenever a `Lazy(DefId)` could not be resolved
    // (its body is not yet registered — typically a re-entrant lib-resolution
    // window). A subtype result computed while this counter changed depended on
    // an undetermined type and must NOT be cached as definitive, or it poisons
    // every later structural check that shares the same member type.
    static LAZY_RESOLVE_FAILURES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    // Monotonic counter bumped whenever a structural comparison hits the
    // weak-type (TS2559) trigger: a non-empty, non-weak source compared against
    // a weak-type target with no common property names. Such a pair yields
    // DIFFERENT results depending on whether weak-type enforcement is active
    // (`SubtypeChecker::enforce_weak_types` plus the `in_property_check` /
    // `in_intersection_member_check` gating context). That enforcement state is
    // operation-local and is NOT encoded in the flag-agnostic
    // `RelationCacheKey`, so a result computed while this counter changed must
    // NOT be memoized in the shared relation cache or it poisons a sibling
    // check that runs under a different enforcement state. Mirrors the
    // unresolved-`Lazy` snapshot mechanism above.
    static WEAK_TYPE_SENSITIVITY: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Record that a `Lazy(DefId)` failed to resolve during a relation check.
#[inline]
pub(crate) fn note_lazy_resolve_failure() {
    LAZY_RESOLVE_FAILURES.with(|c| c.set(c.get().wrapping_add(1)));
}

/// Current value of the unresolved-`Lazy` counter; compare a snapshot taken
/// before computing a result with the value after to detect whether the
/// computation depended on an unresolved `Lazy`.
#[inline]
pub(crate) fn lazy_resolve_failure_count() -> u64 {
    LAZY_RESOLVE_FAILURES.with(std::cell::Cell::get)
}

/// Record that a structural comparison reached the weak-type (TS2559) trigger,
/// making the in-flight result sensitive to the active weak-type enforcement
/// state. See [`WEAK_TYPE_SENSITIVITY`].
#[inline]
pub(crate) fn note_weak_type_sensitivity() {
    WEAK_TYPE_SENSITIVITY.with(|c| c.set(c.get().wrapping_add(1)));
}

/// Current value of the weak-type-sensitivity counter; compare a snapshot taken
/// before computing a result with the value after to detect whether the
/// computation depended on weak-type enforcement state (which the relation
/// cache key does not encode).
#[inline]
pub(crate) fn weak_type_sensitivity_count() -> u64 {
    WEAK_TYPE_SENSITIVITY.with(std::cell::Cell::get)
}

/// Pack depth (low 32) and fuel (high 32) into a single u64.
#[inline(always)]
const fn pack_depth_fuel(depth: u32, fuel: u32) -> u64 {
    (fuel as u64) << 32 | depth as u64
}

/// Extract depth from packed state.
#[inline(always)]
const fn unpack_depth(state: u64) -> u32 {
    state as u32
}

/// Extract fuel from packed state.
#[inline(always)]
const fn unpack_fuel(state: u64) -> u32 {
    (state >> 32) as u32
}

/// Reset subtype depth, fuel, and unresolved-`Lazy`-failure counters.
/// Called between compilation sessions to prevent stale state from a previous
/// compilation (e.g., if it panicked and left counters dirty).
pub fn reset_subtype_thread_local_state() {
    GLOBAL_SUBTYPE_STATE.with(|s| s.set(0));
    LAZY_RESOLVE_FAILURES.with(|c| c.set(0));
    WEAK_TYPE_SENSITIVITY.with(|c| c.set(0));
}

// Maximum number of non-trivial subtype checks per top-level call chain.
// Generous enough for complex real-world types (react, fp-ts) but restrictive
// enough to prevent runaway recursion from hanging.
const MAX_GLOBAL_SUBTYPE_FUEL: u32 = 10_000;

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

        // TEMP-TRACE (remove before PR): log raw TypeData of both sides.
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!(
                src = source.0,
                tgt = target.0,
                src_data = ?self.interner.lookup(source),
                tgt_data = ?self.interner.lookup(target),
                "check_subtype entry data"
            );
        }

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
        // would poison the cache with results computed outside of any class context
        // (e.g., during class type construction), causing later legitimate checks
        // inside class bodies to get the wrong cached result.
        //
        // Use contains_this_type (not just is_this_type) because even after
        // substitute_this_type_if_needed the *target* may still be a class instance
        // type whose method signatures carry `ThisType` return types.
        let has_this_type =
            contains_this_type(self.interner, source) || contains_this_type(self.interner, target);
        // Class-symbol classification is also context-dependent: the callback
        // can make the same object shape behave as a named class/interface in
        // one checker and as a plain structural object in another. Since the
        // relation cache key cannot encode an arbitrary predicate, avoid
        // sharing those answers across checker instances.
        let has_class_check_context = self.is_class_symbol.is_some();
        let can_use_shared_relation_cache = !has_this_type && !has_class_check_context;
        // The `in_callback_param_check` state is encoded in
        // `RelationFlags::IN_CALLBACK_PARAM_CHECK` via `make_cache_key`, so
        // callback-mode results live in a separate cache slot from
        // non-callback-mode results and cannot poison each other.
        if !self.identity_cycle_check
            && can_use_shared_relation_cache
            && let Some(db) = self.query_db
        {
            let key = self.make_cache_key(source, target);
            if let Some(cached) = db.lookup_subtype_cache(key) {
                return if cached {
                    SubtypeResult::True
                } else {
                    SubtypeResult::False
                };
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
        // PERF: Single TLS access reads both depth and fuel; single access writes both.
        let (global_depth, fuel) = GLOBAL_SUBTYPE_STATE.with(|s| {
            let prev = s.get();
            let depth = unpack_depth(prev);
            let fuel = unpack_fuel(prev);
            s.set(pack_depth_fuel(depth + 1, fuel + 1));
            (depth, fuel)
        });

        // Snapshot the unresolved-`Lazy` counter. If it changes while computing
        // this pair's result, the result depended on a `Lazy` whose body was not
        // yet registered, so a `False` is undetermined and must not be cached.
        let lazy_failures_at_entry = lazy_resolve_failure_count();

        // Snapshot the weak-type-sensitivity counter. If it changes while
        // computing this pair's result, the result depended on weak-type
        // enforcement state (TS2559), which the flag-agnostic `RelationCacheKey`
        // does not encode. Caching it would let a result computed under one
        // enforcement state be served to a sibling check under another.
        let weak_sensitivity_at_entry = weak_type_sensitivity_count();

        // Helper macro to decrement global depth and optionally reset fuel on early returns.
        macro_rules! leave_global {
            () => {
                GLOBAL_SUBTYPE_STATE.with(|s| {
                    let prev = s.get();
                    let depth = unpack_depth(prev).saturating_sub(1);
                    if global_depth == 0 {
                        // Outermost call completed — reset fuel
                        s.set(pack_depth_fuel(depth, 0));
                    } else {
                        s.set(pack_depth_fuel(depth, unpack_fuel(prev)));
                    }
                });
            };
        }

        // Helper macro to cache a definitive subtype result, but only when the
        // computation did not depend on a `Lazy` whose body was unresolved
        // (which would make the result undetermined and poison the cache).
        //
        // This is best-effort and intentionally conservative: the snapshots are
        // process-wide (thread-local), so *any* unresolved-`Lazy` event or
        // weak-type-sensitivity event anywhere in this top-level call's subtree
        // — even in a branch whose result did not feed the final answer —
        // suppresses the write. That only ever skips a cache write
        // (correctness-preserving: the result is recomputed later), never
        // produces a wrong answer. Captures `lazy_failures_at_entry` and
        // `weak_sensitivity_at_entry` from the enclosing scope by name.
        macro_rules! cache_definitive {
            ($db:expr, $key:expr, $result:expr) => {
                if lazy_resolve_failure_count() == lazy_failures_at_entry
                    && weak_type_sensitivity_count() == weak_sensitivity_at_entry
                {
                    match $result {
                        SubtypeResult::True => $db.insert_subtype_cache($key, true),
                        SubtypeResult::False => $db.insert_subtype_cache($key, false),
                        SubtypeResult::CycleDetected | SubtypeResult::DepthExceeded => {}
                    }
                }
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
                    leave_global!();
                    return self.result_on_cycle(source, target);
                }
            }
        }

        let mut def_entered = if let Some((s_def, t_def)) = def_pair {
            // Check reversed pair for bivariant cross-recursion
            if self.def_guard.is_visiting(&(t_def, s_def)) {
                self.guard.leave(pair);
                leave_global!();
                return self.result_on_cycle(source, target);
            }
            match self.def_guard.enter((s_def, t_def)) {
                RecursionResult::Cycle => {
                    self.guard.leave(pair);
                    leave_global!();
                    return self.result_on_cycle(source, target);
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
            let is_object_interface_target = self
                .resolver
                .is_boxed_type_id(target, IntrinsicKind::Object)
                || self
                    .resolver
                    .get_boxed_type(IntrinsicKind::Object)
                    .is_some_and(|boxed| boxed == target)
                || lazy_def_id(self.interner, target).is_some_and(|def_id| {
                    self.resolver.is_boxed_def_id(def_id, IntrinsicKind::Object)
                });
            if is_object_interface_target {
                // is_nullable() short-circuits before the interner lookup for common null/undefined/void cases.
                if source.is_nullable() || !self.is_global_object_interface_type(source) {
                    if let Some(dp) = def_entered {
                        self.def_guard.leave(dp);
                    }
                    self.guard.leave(pair);
                    leave_global!();
                    return SubtypeResult::False;
                }
                let result = self.check_object_contract(source, target);
                if let Some(dp) = def_entered {
                    self.def_guard.leave(dp);
                }
                self.guard.leave(pair);
                leave_global!();
                return result;
            }
        }

        // Check if target is the Function interface from lib.d.ts.
        // We must check BEFORE evaluate_type() because evaluation resolves
        // Lazy(DefId) → ObjectShape, losing the DefId identity needed to
        // recognize the type as an intrinsic interface.
        if !self.bypass_evaluation
            && (lazy_def_id(self.interner, target).is_some_and(|t_def| {
                self.resolver
                    .is_boxed_def_id(t_def, IntrinsicKind::Function)
            }) || self
                .resolver
                .is_boxed_type_id(target, IntrinsicKind::Function))
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
                    leave_global!();
                    return SubtypeResult::True;
                }
                let result = self.subtype_of_conditional_target(source, &target_cond);
                if let Some(dp) = def_entered {
                    self.def_guard.leave(dp);
                }
                self.guard.leave(pair);
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
            let variance_result =
                if let (Some(s_app_id), Some(t_app_id)) = (s_app_id_for_variance, t_app_id) {
                    self.try_variance_fast_path(s_app_id, t_app_id)
                } else if let Some(t_app_id) = t_app_id_for_recovered_target {
                    self.try_same_base_all_any_target_args(source, s_app_id_for_variance, t_app_id)
                        .or_else(|| {
                            // Accept-only variance on a provenance-recovered
                            // same-base pair: tsc relates two instantiations of
                            // the same generic reference by per-argument
                            // variance (`relateVariances`) before any
                            // structural expansion, so an `any` argument
                            // relates bidirectionally and silences the member
                            // walk (kysely `ExpressionWrapper<DB, TB, any>` vs
                            // `ExpressionWrapper<DB, TB, O[K]>`, whose `and`
                            // member is a deferred conditional that can never
                            // relate structurally). tsz's checker computes
                            // class member types in evaluated form, so BOTH
                            // sides may have lost their `Application` identity
                            // here; recover each via display provenance and
                            // honor only a conclusive `True` — rejections from
                            // display-grade identity are discarded and the
                            // relation falls through to the structural path.
                            // The semantic eval-origin map may recover the
                            // source where display provenance declined to
                            // record (generic-arg repaint guards); both
                            // channels are trusted here because this branch
                            // is accept-only.
                            let s_app_id = s_app_id_for_variance.or_else(|| {
                                self.interner
                                    .get_application_eval_origin(source)
                                    .and_then(|origin| application_id(self.interner, origin))
                            })?;
                            // TEMP-TRACE (remove before PR)
                            tracing::trace!(
                                src = source.0,
                                tgt = target.0,
                                s_app = ?s_app_id,
                                t_app = ?t_app_id,
                                "recovered-target variance attempt"
                            );
                            let vr = self.try_variance_fast_path(s_app_id, t_app_id);
                            // TEMP-TRACE (remove before PR)
                            tracing::trace!(
                                src = source.0,
                                tgt = target.0,
                                ?vr,
                                "recovered-target variance result"
                            );
                            match vr {
                                Some(SubtypeResult::True) => Some(SubtypeResult::True),
                                _ => None,
                            }
                        })
                } else if let Some(s_app_id) = s_app_id {
                    // Source is Application, target might be Union containing an Application.
                    // This handles optional properties where target is App<X> | undefined.
                    self.try_variance_against_union_target(s_app_id, target)
                } else {
                    None
                };

            if let Some(result) = variance_result {
                if let Some(dp) = def_entered {
                    self.def_guard.leave(dp);
                }
                self.guard.leave(pair);
                if can_use_shared_relation_cache && let Some(db) = self.query_db {
                    let key = self.make_cache_key(source, target);
                    cache_definitive!(db, key, result);
                }
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
            if can_use_shared_relation_cache && let Some(db) = self.query_db {
                let key = self.make_cache_key(source, target);
                cache_definitive!(db, key, result);
            }
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
            if can_use_shared_relation_cache && let Some(db) = self.query_db {
                let key = self.make_cache_key(source, target);
                cache_definitive!(db, key, result);
            }
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

        // Cache definitive results for cross-checker memoization.
        // Skip context-dependent results (see lookup guard above).
        if can_use_shared_relation_cache && let Some(db) = self.query_db {
            let key = self.make_cache_key(source, target);
            cache_definitive!(db, key, result);
        }

        // Decrement global depth; reset fuel when outermost call completes.
        // PERF: Single TLS access for both depth and fuel.
        GLOBAL_SUBTYPE_STATE.with(|s| {
            let prev = s.get();
            let depth = unpack_depth(prev).saturating_sub(1);
            if global_depth == 0 {
                s.set(pack_depth_fuel(depth, 0));
            } else {
                s.set(pack_depth_fuel(depth, unpack_fuel(prev)));
            }
        });

        result
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
