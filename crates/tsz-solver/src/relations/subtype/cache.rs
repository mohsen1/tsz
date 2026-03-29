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

use crate::TypeDatabase;
use crate::def::DefId;
use crate::def::resolver::TypeResolver;
use crate::relations::subtype::{SubtypeChecker, SubtypeResult, is_disjoint_unit_type};
use crate::types::{IntrinsicKind, TypeApplicationId, TypeData, TypeId};
use crate::visitor::{application_id, enum_components, lazy_def_id, union_list_id};

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

/// Reset subtype depth and fuel counters.
/// Called between compilation sessions to prevent stale state from a previous
/// compilation (e.g., if it panicked and left counters dirty).
pub fn reset_subtype_thread_local_state() {
    GLOBAL_SUBTYPE_STATE.with(|s| s.set(0));
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
        // =========================================================================
        // Fast paths (no cycle tracking needed)
        // =========================================================================
        let allow_any = self.any_propagation.allows_any_at_depth(self.guard.depth());
        let mut source = source;
        let mut target = target;
        if !allow_any {
            if source == TypeId::ANY {
                // In strict mode, any doesn't match everything structurally.
                // We demote it to STRICT_ANY so it only matches top types or itself.
                source = TypeId::STRICT_ANY;
            }
            if target == TypeId::ANY {
                target = TypeId::STRICT_ANY;
            }
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
        if allow_any
            && (source == TypeId::ANY || source == TypeId::STRICT_ANY)
            && target != TypeId::NEVER
        {
            return SubtypeResult::True;
        }

        // Everything is assignable to any (when allowed)
        if allow_any && (target == TypeId::ANY || target == TypeId::STRICT_ANY) {
            return SubtypeResult::True;
        }

        // If not allowing any (nested strict any / identity mode), STRICT_ANY
        // can only match STRICT_ANY, ANY, or UNKNOWN as a top-type source.
        // Crucially, non-any types are NOT assignable to STRICT_ANY in this mode.
        // This ensures that bidirectional subtype checks used for identity (TS2403)
        // correctly reject `number <: any` at nested depths, matching tsc's
        // isTypeIdenticalTo where `any` is only identical to `any`.
        if !allow_any
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
        if source == TypeId::ERROR || target == TypeId::ERROR {
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
            let has_null = member_list.iter().any(|&m| m == TypeId::NULL);
            let has_undef = member_list.iter().any(|&m| m == TypeId::UNDEFINED);
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
        if !self.identity_cycle_check
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

        let def_pair = if both_same_base_app {
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
                let is_nullable = source.is_nullable();
                if !is_nullable {
                    let result = self.check_object_contract(source, target);
                    if let Some(dp) = def_entered {
                        self.def_guard.leave(dp);
                    }
                    self.guard.leave(pair);
                    leave_global!();
                    return result;
                }
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
        // =========================================================================
        // Pre-evaluation variance fast path for Application types.
        //
        // When both types are Application types (e.g., FunctionComponent<X> vs
        // FunctionComponent<Y>), check type argument compatibility using variance
        // BEFORE evaluation. This is critical because evaluation converts
        // Application -> Object, losing the generic identity needed for variance-
        // based rejection. Without this, recursive generic interfaces like
        // FunctionComponent<P> get structurally compared with coinductive cycle
        // detection, which incorrectly assumes compatibility when type arguments
        // differ (e.g., SomePropsCloneX vs SomeProps).
        //
        // Also handles the common case where the target is a Union containing an
        // Application (e.g., from optional properties: FC<SomeProps> | undefined).
        // Without this, the source Application gets evaluated to an Object before
        // the union is unwrapped, losing the generic identity.
        //
        // GUARD: Skip this fast path when we're already inside a recursive type
        // expansion (def_guard has entries). In that case, variance rejection may
        // be incorrect because the types participate in a recursive structure where
        // coinductive cycle detection should determine the result instead.
        // =========================================================================
        let outer_def_count =
            self.def_guard.visiting_count() - if def_entered.is_some() { 1 } else { 0 };
        if !self.bypass_evaluation && outer_def_count == 0 {
            let variance_result = if let (Some(s_app_id), Some(t_app_id)) = (s_app_id, t_app_id) {
                self.try_variance_fast_path(s_app_id, t_app_id)
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
                if let Some(db) = self.query_db {
                    let key = self.make_cache_key(source, target);
                    match result {
                        SubtypeResult::True => db.insert_subtype_cache(key, true),
                        SubtypeResult::False => db.insert_subtype_cache(key, false),
                        _ => {}
                    }
                }
                leave_global!();
                return result;
            }
        }

        // =========================================================================
        // Meta-type evaluation (after cycle detection is set up)
        // =========================================================================
        let result = if self.bypass_evaluation {
            if target == TypeId::NEVER {
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
            } else if target == TypeId::NEVER {
                SubtypeResult::False
            } else {
                self.check_subtype_inner(source, target)
            }
        };

        // Cleanup: leave both guards
        if let Some(dp) = def_entered {
            self.def_guard.leave(dp);
        }
        self.guard.leave(pair);

        // Cache definitive results for cross-checker memoization.
        if let Some(db) = self.query_db {
            let key = self.make_cache_key(source, target);
            match result {
                SubtypeResult::True => db.insert_subtype_cache(key, true),
                SubtypeResult::False => db.insert_subtype_cache(key, false),
                SubtypeResult::CycleDetected | SubtypeResult::DepthExceeded => {}
            }
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
        if self.identity_cycle_check {
            self.identity_cycle_result(source, target)
        } else {
            self.cycle_result()
        }
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
}
