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
use crate::types::{IntrinsicKind, TypeId};
use crate::visitor::{application_id, enum_components, lazy_def_id};

// Global thread-local fuel counter for cross-instance subtype check termination.
//
// Unlike depth counters (which unwind), fuel is monotonically consumed and never
// restored until the outermost check_subtype call completes. This prevents the
// "infinite hang" scenario where each property comparison in an implements check
// triggers a deep evaluation chain — the total work across ALL properties is bounded.
//
// The depth counter tracks nesting level (incremented on enter, decremented on leave)
// to detect when we're back at the outermost call and can reset the fuel.
thread_local! {
    static GLOBAL_SUBTYPE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static GLOBAL_SUBTYPE_FUEL: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

// Maximum number of non-trivial subtype checks per top-level call chain.
// Generous enough for complex real-world types (react, fp-ts) but restrictive
// enough to prevent runaway recursion from hanging.
const MAX_GLOBAL_SUBTYPE_FUEL: u32 = 10_000;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
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

        // Task #54: Structural Identity Fast-Path (O(1) after canonicalization)
        // Check if source and target canonicalize to the same TypeId, which means
        // they are structurally identical. This avoids expensive structural walks
        // for types that are the same structure but were interned separately.
        //
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

        // Fast path: distinct disjoint unit types are never subtypes.
        // This avoids expensive structural checks for large unions of literals/enum members.
        if is_disjoint_unit_type(self.interner, source)
            && is_disjoint_unit_type(self.interner, target)
        {
            return SubtypeResult::False;
        }

        // =========================================================================
        // Global fuel guard (cross-instance work limiter)
        // =========================================================================
        // Track nesting depth and consume fuel for every non-trivial check.
        // Fuel is monotonically consumed; depth tracks when we're back at root.
        let global_depth = GLOBAL_SUBTYPE_DEPTH.with(|d| {
            let v = d.get();
            d.set(v + 1);
            v
        });
        let fuel = GLOBAL_SUBTYPE_FUEL.with(|f| {
            let v = f.get();
            f.set(v + 1);
            v
        });

        // Helper macro to decrement global depth and optionally reset fuel on early returns.
        macro_rules! leave_global {
            () => {
                GLOBAL_SUBTYPE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
                if global_depth == 0 {
                    GLOBAL_SUBTYPE_FUEL.with(|f| f.set(0));
                }
            };
        }

        if fuel >= MAX_GLOBAL_SUBTYPE_FUEL {
            leave_global!();
            return self.depth_result();
        }

        // =========================================================================
        // Cross-checker memoization (QueryCache lookup)
        // =========================================================================
        // Check the shared cache for a previously computed result.
        // This avoids re-doing expensive structural checks for type pairs
        // already resolved by a prior SubtypeChecker instance.
        if let Some(db) = self.query_db {
            let key = self.make_cache_key(source, target);
            if let Some(cached) = db.lookup_subtype_cache(key) {
                leave_global!();
                return if cached {
                    SubtypeResult::True
                } else {
                    SubtypeResult::False
                };
            }
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
            return self.cycle_result();
        }

        use crate::recursion::RecursionResult;
        match self.guard.enter(pair) {
            RecursionResult::Cycle => {
                leave_global!();
                return self.cycle_result();
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

        let extract_def_id = |interner: &dyn TypeDatabase, type_id: TypeId| -> Option<DefId> {
            // First try direct Lazy/Enum DefId
            if let Some(def) = lazy_def_id(interner, type_id) {
                return Some(def);
            }
            if let Some((def, _)) = enum_components(interner, type_id) {
                return Some(def);
            }
            // For Application types, extract the base DefId
            if let Some(app_id) = application_id(interner, type_id) {
                let app = interner.type_application(app_id);
                if let Some(def) = lazy_def_id(interner, app.base) {
                    return Some(def);
                }
            }
            None
        };

        let s_def_id = extract_def_id(self.interner, source);
        let t_def_id = extract_def_id(self.interner, target);

        // Skip DefId-level cycle detection when both are Application types with
        // the SAME base DefId (e.g., Box<number> vs Box<string>). These are
        // legitimate comparisons that differ only in type arguments — they are
        // NOT recursive cycles. check_application_to_application_subtype has its
        // own cycle detection that handles cross-base recursion correctly.
        // Without this guard, the def_guard entry here conflicts with the one in
        // check_application_to_application_subtype, causing false CycleDetected
        // results that collapse unions (e.g., Box<number> | Box<string> | Box<boolean>
        // incorrectly reduced to Box<number>).
        let both_same_base_app = if let (Some(s_app_id), Some(t_app_id)) = (
            application_id(self.interner, source),
            application_id(self.interner, target),
        ) {
            let s_app = self.interner.type_application(s_app_id);
            let t_app = self.interner.type_application(t_app_id);
            s_app.base == t_app.base
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
                if self.def_guard.is_visiting_any(|&(visiting_s, visiting_t)| {
                    visiting_s != s_def
                        && visiting_t != t_def
                        && self.resolver.def_to_symbol_id(visiting_s) == Some(s_sid)
                        && self.resolver.def_to_symbol_id(visiting_t) == Some(t_sid)
                }) {
                    self.guard.leave(pair);
                    leave_global!();
                    return self.cycle_result();
                }
            }
        }

        let mut def_entered = if let Some((s_def, t_def)) = def_pair {
            // Check reversed pair for bivariant cross-recursion
            if self.def_guard.is_visiting(&(t_def, s_def)) {
                self.guard.leave(pair);
                leave_global!();
                return self.cycle_result();
            }
            match self.def_guard.enter((s_def, t_def)) {
                RecursionResult::Cycle => {
                    self.guard.leave(pair);
                    leave_global!();
                    return self.cycle_result();
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
                    // Leave def_guard before recursing ONLY when a Lazy type resolved to
                    // an Enum with the same DefId. This prevents false cycle detection when
                    // Lazy(DefId(X)) resolves to Enum(DefId(X), ...) — the recursive call
                    // would extract the same DefId and falsely detect a cycle. For non-enum
                    // resolutions (e.g., recursive interfaces), the def_guard is critical.
                    let has_lazy_to_enum = |orig: TypeId, resolved: TypeId| -> bool {
                        if let Some(lazy_def) = lazy_def_id(self.interner, orig)
                            && let Some((enum_def, _)) = enum_components(self.interner, resolved)
                        {
                            return lazy_def == enum_def;
                        }
                        false
                    };
                    if (has_lazy_to_enum(source, source_resolved)
                        || has_lazy_to_enum(target, target_resolved))
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
            let source_eval = self.evaluate_type(source);
            let target_eval = self.evaluate_type(target);

            if source_eval != source || target_eval != target {
                // Leave def_guard before recursing ONLY when a Lazy type resolved to an
                // Enum with the same DefId. When Lazy(DefId(X)) evaluates to Enum(DefId(X), ...),
                // the recursive call extracts the same DefId and falsely detects a cycle.
                // For non-enum Lazy resolutions (e.g., recursive generic interfaces), the
                // def_guard is critical for preventing infinite recursion — don't release it.
                let has_lazy_to_enum = |orig: TypeId, eval: TypeId| -> bool {
                    if let Some(lazy_def) = lazy_def_id(self.interner, orig)
                        && let Some((enum_def, _)) = enum_components(self.interner, eval)
                    {
                        return lazy_def == enum_def;
                    }
                    false
                };
                if (has_lazy_to_enum(source, source_eval) || has_lazy_to_enum(target, target_eval))
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
        GLOBAL_SUBTYPE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        if global_depth == 0 {
            GLOBAL_SUBTYPE_FUEL.with(|f| f.set(0));
        }

        result
    }
}
