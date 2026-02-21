//! Structural subtype checking.
//!
//! This module implements the core logic engine for TypeScript's structural
//! subtyping. It uses coinductive semantics to handle recursive types.
//!
//! Key features:
//! - O(1) equality check via `TypeId` comparison
//! - Cycle detection for recursive types (coinductive)
//! - Set-theoretic operations for unions and intersections
//! - `TypeResolver` trait for lazy symbol resolution
//! - Tracer pattern for zero-cost diagnostic abstraction

use crate::AssignabilityChecker;
use crate::TypeDatabase;
use crate::db::QueryDatabase;
use crate::def::DefId;
use crate::diagnostics::{DynSubtypeTracer, SubtypeFailureReason};
#[cfg(test)]
use crate::types::*;
use crate::types::{
    IntrinsicKind, LiteralValue, ObjectFlags, ObjectShape, SymbolRef, TypeData, TypeId, TypeListId,
};
use crate::visitor::{
    TypeVisitor, application_id, array_element_type, callable_shape_id, conditional_type_id,
    enum_components, function_shape_id, intersection_list_id, intrinsic_kind, is_this_type,
    keyof_inner_type, lazy_def_id, literal_value, mapped_type_id, object_shape_id,
    object_with_index_shape_id, readonly_inner_type, ref_symbol, template_literal_id,
    tuple_list_id, type_param_info, type_query_symbol, union_list_id, unique_symbol_ref,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::limits;

/// Maximum recursion depth for subtype checking.
/// This prevents OOM/stack overflow from infinitely expanding recursive types.
/// Examples: `interface AA<T extends AA<T>>`, `interface List<T> { next: List<T> }`
pub(crate) const MAX_SUBTYPE_DEPTH: u32 = limits::MAX_SUBTYPE_DEPTH;
pub(crate) const INTERSECTION_OBJECT_FAST_PATH_THRESHOLD: usize = 8;

/// Result of a subtype check
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SubtypeResult {
    /// The relationship is definitely true
    True,
    /// The relationship is definitely false
    False,
    /// We're in a valid cycle (coinductive recursion)
    ///
    /// This represents finite/cyclic recursion like `interface List { next: List }`.
    /// The type graph forms a closed loop, which is valid in TypeScript.
    CycleDetected,
    /// We've exceeded the recursion depth limit
    ///
    /// This represents expansive recursion that grows indefinitely like
    /// `type T<X> = T<Box<X>>`. TypeScript rejects these as "excessively deep".
    ///
    /// This is treated as `false` for soundness - if we can't prove subtyping within
    /// reasonable limits, we reject the relationship rather than accepting unsoundly.
    DepthExceeded,
}

impl SubtypeResult {
    pub const fn is_true(self) -> bool {
        matches!(self, Self::True | Self::CycleDetected)
    }

    pub const fn is_false(self) -> bool {
        matches!(self, Self::False)
    }
}

/// Returns true for unit types where `source != target` implies disjointness.
///
/// This intentionally excludes:
/// - null/undefined/void/never (special-cased assignability semantics)
/// - Tuples (labeled tuples like [a: 1] vs [b: 1] are compatible despite different `TypeIds`)
///
/// Only safe for primitives where identity implies structural equality.
fn is_disjoint_unit_type(types: &dyn TypeDatabase, ty: TypeId) -> bool {
    match types.lookup(ty) {
        Some(TypeData::Literal(_) | TypeData::UniqueSymbol(_)) => true,
        // Note: Tuples removed to avoid labeled tuple bug
        // TypeScript treats [a: 1] and [b: 1] as compatible even though they have different TypeIds
        _ => false,
    }
}

/// Controls how `any` is treated during subtype checks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AnyPropagationMode {
    /// `any` is treated as top/bottom everywhere (TypeScript default).
    All,
    /// `any` is treated as top/bottom only at the top-level comparison.
    TopLevelOnly,
}

impl AnyPropagationMode {
    #[inline]
    pub(crate) const fn allows_any_at_depth(self, depth: u32) -> bool {
        match self {
            Self::All => true,
            Self::TopLevelOnly => depth == 0,
        }
    }
}

// TypeResolver, NoopResolver, and TypeEnvironment are defined in type_resolver.rs
pub use crate::type_resolver::{NoopResolver, TypeEnvironment, TypeResolver};

// SubtypeVisitor is defined in subtype_visitor.rs
pub use crate::relations::subtype_visitor::SubtypeVisitor;

/// Subtype checking context.
/// Maintains the "seen" set for cycle detection.
pub struct SubtypeChecker<'a, R: TypeResolver = NoopResolver> {
    pub(crate) interner: &'a dyn TypeDatabase,
    /// Optional query database for Salsa-backed memoization.
    /// When set, routes `evaluate_type` and `is_subtype_of` through Salsa.
    pub(crate) query_db: Option<&'a dyn QueryDatabase>,
    pub(crate) resolver: &'a R,
    /// Unified recursion guard for TypeId-pair cycle detection, depth, and iteration limits.
    pub(crate) guard: crate::recursion::RecursionGuard<(TypeId, TypeId)>,
    /// Active `SymbolRef` pairs being checked (for DefId-level cycle detection)
    /// This catches cycles in Ref types before they're resolved, preventing
    /// infinite expansion of recursive type aliases and interfaces.
    pub(crate) seen_refs: FxHashSet<(SymbolRef, SymbolRef)>,
    /// Unified recursion guard for DefId-pair cycle detection.
    /// Catches cycles in Lazy(DefId) types before they're resolved.
    pub(crate) def_guard: crate::recursion::RecursionGuard<(DefId, DefId)>,
    /// Symbol-pair visiting set for Object-level cycle detection.
    /// Catches cycles when comparing evaluated Object types with symbols
    /// (e.g., `Promise<X>` vs `PromiseLike<Y>`) where `DefId` information is lost
    /// after type evaluation. Without this, recursive interfaces like `Promise`
    /// cause infinite expansion when comparing `then` method return types.
    sym_visiting: FxHashSet<(tsz_binder::SymbolId, tsz_binder::SymbolId)>,
    /// Whether to use strict function types (contravariant parameters).
    /// Default: true (sound, correct behavior)
    pub strict_function_types: bool,
    /// Whether to allow any return type when the target return is void.
    pub allow_void_return: bool,
    /// Whether rest parameters of any/unknown should be treated as bivariant.
    /// See <https://github.com/microsoft/TypeScript/issues/20007>.
    pub allow_bivariant_rest: bool,
    /// When true, skip the `evaluate_type()` call in `check_subtype`.
    /// This prevents infinite recursion when `TypeEvaluator` calls `SubtypeChecker`
    /// for simplification, since `TypeEvaluator` has already evaluated the types.
    pub bypass_evaluation: bool,
    /// Maximum recursion depth for subtype checking.
    /// Used by `TypeEvaluator` simplification to prevent stack overflow.
    /// Default: `MAX_SUBTYPE_DEPTH` (100)
    pub max_depth: u32,
    /// Whether required parameter count mismatches are allowed for bivariant methods.
    pub allow_bivariant_param_count: bool,
    /// Whether optional properties are exact (exclude implicit `undefined`).
    /// Default: false (legacy TS behavior).
    pub exact_optional_property_types: bool,
    /// Whether null/undefined are treated as separate types.
    /// Default: true (strict null checks).
    pub strict_null_checks: bool,
    /// Whether indexed access includes `undefined`.
    /// Default: false (legacy TS behavior).
    pub no_unchecked_indexed_access: bool,
    // When true, disables method bivariance (methods use contravariance).
    // Default: false (methods are bivariant in TypeScript for compatibility).
    pub disable_method_bivariance: bool,
    /// Optional inheritance graph for O(1) nominal class subtype checking.
    /// When provided, enables fast nominal checks for class inheritance.
    pub inheritance_graph: Option<&'a crate::inheritance::InheritanceGraph>,
    /// Optional callback to check if a symbol is a class (for nominal subtyping).
    /// Returns true if the symbol has the CLASS flag set.
    pub is_class_symbol: Option<&'a dyn Fn(SymbolRef) -> bool>,
    /// Controls how `any` is treated during subtype checks.
    pub any_propagation: AnyPropagationMode,
    /// Cache for `evaluate_type` results within this `SubtypeChecker`'s lifetime.
    /// This prevents O(n²) behavior when the same type (e.g., a large union) is
    /// evaluated multiple times across different subtype checks.
    /// Key is (`TypeId`, `no_unchecked_indexed_access`) since that flag affects evaluation.
    pub(crate) eval_cache: FxHashMap<(TypeId, bool), TypeId>,
    /// Optional tracer for collecting subtype failure diagnostics.
    /// When `Some`, enables detailed failure reason collection for error messages.
    /// When `None`, disables tracing for maximum performance (default).
    pub tracer: Option<&'a mut dyn DynSubtypeTracer>,
}

impl<'a> SubtypeChecker<'a, NoopResolver> {
    /// Create a new `SubtypeChecker` without a resolver (basic mode).
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        static NOOP: NoopResolver = NoopResolver;
        SubtypeChecker {
            interner,
            query_db: None,
            resolver: &NOOP,
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::SubtypeCheck,
            ),
            seen_refs: FxHashSet::default(),
            def_guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::SubtypeCheck,
            ),
            sym_visiting: FxHashSet::default(),
            strict_function_types: true, // Default to strict (sound) behavior
            allow_void_return: false,
            allow_bivariant_rest: false,
            allow_bivariant_param_count: false,
            exact_optional_property_types: false,
            strict_null_checks: true,
            no_unchecked_indexed_access: false,
            disable_method_bivariance: false,
            inheritance_graph: None,
            is_class_symbol: None,
            any_propagation: AnyPropagationMode::All,
            bypass_evaluation: false,
            max_depth: MAX_SUBTYPE_DEPTH,
            eval_cache: FxHashMap::default(),
            tracer: None,
        }
    }
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Create a new `SubtypeChecker` with a custom resolver.
    pub fn with_resolver(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        SubtypeChecker {
            interner,
            query_db: None,
            resolver,
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::SubtypeCheck,
            ),
            seen_refs: FxHashSet::default(),
            def_guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::SubtypeCheck,
            ),
            sym_visiting: FxHashSet::default(),
            strict_function_types: true,
            allow_void_return: false,
            allow_bivariant_rest: false,
            allow_bivariant_param_count: false,
            exact_optional_property_types: false,
            strict_null_checks: true,
            no_unchecked_indexed_access: false,
            disable_method_bivariance: false,
            inheritance_graph: None,
            is_class_symbol: None,
            any_propagation: AnyPropagationMode::All,
            bypass_evaluation: false,
            max_depth: MAX_SUBTYPE_DEPTH,
            eval_cache: FxHashMap::default(),
            tracer: None,
        }
    }

    /// Set the inheritance graph for O(1) nominal class subtype checking.
    pub const fn with_inheritance_graph(
        mut self,
        graph: &'a crate::inheritance::InheritanceGraph,
    ) -> Self {
        self.inheritance_graph = Some(graph);
        self
    }

    /// Set the callback to check if a symbol is a class.
    pub fn with_class_check(mut self, check: &'a dyn Fn(SymbolRef) -> bool) -> Self {
        self.is_class_symbol = Some(check);
        self
    }

    /// Configure how `any` is treated during subtype checks.
    pub const fn with_any_propagation_mode(mut self, mode: AnyPropagationMode) -> Self {
        self.any_propagation = mode;
        self
    }

    /// Set the tracer for collecting subtype failure diagnostics.
    /// When set, enables detailed failure reason collection for error messages.
    pub fn with_tracer(mut self, tracer: &'a mut dyn DynSubtypeTracer) -> Self {
        self.tracer = Some(tracer);
        self
    }

    /// Set the query database for Salsa-backed memoization.
    /// When set, routes `evaluate_type` and `is_subtype_of` through Salsa.
    pub fn with_query_db(mut self, db: &'a dyn QueryDatabase) -> Self {
        self.query_db = Some(db);
        self
    }

    /// Set whether strict null checks are enabled.
    /// When false, null and undefined are assignable to any type.
    pub const fn with_strict_null_checks(mut self, strict_null_checks: bool) -> Self {
        self.strict_null_checks = strict_null_checks;
        self
    }

    /// Reset per-check state so this checker can be reused for another subtype check.
    ///
    /// This clears cycle detection sets and counters while preserving configuration
    /// (`strict_function_types`, `allow_void_return`, etc.) and borrowed references
    /// (interner, resolver, `inheritance_graph`, etc.).
    ///
    /// Uses `.clear()` instead of re-allocating, so hash set memory is reused.
    #[inline]
    pub fn reset(&mut self) {
        self.guard.reset();
        self.seen_refs.clear();
        self.def_guard.reset();
        self.sym_visiting.clear();
        self.eval_cache.clear();
    }

    /// Whether the recursion depth was exceeded during subtype checking.
    pub const fn depth_exceeded(&self) -> bool {
        self.guard.is_exceeded()
    }

    /// Apply compiler flags from a packed u16 bitmask.
    ///
    /// This unpacks the flags used by `RelationCacheKey` and applies them to the checker.
    /// The bit layout matches the cache key definition in types.rs:
    /// - bit 0: `strict_null_checks`
    /// - bit 1: `strict_function_types`
    /// - bit 2: `exact_optional_property_types`
    /// - bit 3: `no_unchecked_indexed_access`
    /// - bit 4: `disable_method_bivariance`
    /// - bit 5: `allow_void_return`
    /// - bit 6: `allow_bivariant_rest`
    /// - bit 7: `allow_bivariant_param_count`
    pub(crate) const fn apply_flags(mut self, flags: u16) -> Self {
        self.strict_null_checks = (flags & (1 << 0)) != 0;
        self.strict_function_types = (flags & (1 << 1)) != 0;
        self.exact_optional_property_types = (flags & (1 << 2)) != 0;
        self.no_unchecked_indexed_access = (flags & (1 << 3)) != 0;
        self.disable_method_bivariance = (flags & (1 << 4)) != 0;
        self.allow_void_return = (flags & (1 << 5)) != 0;
        self.allow_bivariant_rest = (flags & (1 << 6)) != 0;
        self.allow_bivariant_param_count = (flags & (1 << 7)) != 0;
        self
    }

    pub(crate) fn resolve_ref_type(&self, type_id: TypeId) -> TypeId {
        // Handle DefId-based Lazy types (new API)
        if let Some(def_id) = lazy_def_id(self.interner, type_id) {
            return self
                .resolver
                .resolve_lazy(def_id, self.interner)
                .unwrap_or(type_id);
        }

        // Handle legacy SymbolRef-based types (old API)
        if let Some(symbol) = ref_symbol(self.interner, type_id) {
            self.resolver
                .resolve_symbol_ref(symbol, self.interner)
                .unwrap_or(type_id)
        } else {
            type_id
        }
    }

    pub(crate) fn resolve_lazy_type(&self, type_id: TypeId) -> TypeId {
        if let Some(def_id) = lazy_def_id(self.interner, type_id) {
            self.resolver
                .resolve_lazy(def_id, self.interner)
                .unwrap_or(type_id)
        } else {
            type_id
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

        // Any is assignable to anything (when allowed)
        if allow_any && (source == TypeId::ANY || source == TypeId::STRICT_ANY) {
            return SubtypeResult::True;
        }

        // Everything is assignable to any (when allowed)
        if allow_any && (target == TypeId::ANY || target == TypeId::STRICT_ANY) {
            return SubtypeResult::True;
        }

        // If not allowing any (nested strict any), any still matches Top types as source,
        // but any as target ALWAYS matches (it's a top type).
        if !allow_any
            && (source == TypeId::ANY || source == TypeId::STRICT_ANY)
            && (target == TypeId::ANY || target == TypeId::STRICT_ANY || target == TypeId::UNKNOWN)
        {
            return SubtypeResult::True;
        }
        // Fall through to structural check (which will fail for STRICT_ANY)
        if !allow_any && (target == TypeId::ANY || target == TypeId::STRICT_ANY) {
            return SubtypeResult::True;
        }
        // Fall through to structural check (which will fail for STRICT_ANY)

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
        // Cross-checker memoization (QueryCache lookup)
        // =========================================================================
        // Check the shared cache for a previously computed result.
        // This avoids re-doing expensive structural checks for type pairs
        // already resolved by a prior SubtypeChecker instance.
        if let Some(db) = self.query_db {
            let key = self.make_cache_key(source, target);
            if let Some(cached) = db.lookup_subtype_cache(key) {
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
            return SubtypeResult::CycleDetected;
        }

        use crate::recursion::RecursionResult;
        match self.guard.enter(pair) {
            RecursionResult::Cycle => return SubtypeResult::CycleDetected,
            RecursionResult::DepthExceeded | RecursionResult::IterationExceeded => {
                return SubtypeResult::DepthExceeded;
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

        let extract_def_id =
            |interner: &dyn crate::TypeDatabase, type_id: TypeId| -> Option<DefId> {
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

        let def_pair = if let (Some(s_def), Some(t_def)) = (s_def_id, t_def_id) {
            // Skip same-base Application cycle detection to avoid false positives
            // (e.g., Array<number> vs Array<string> share the same base)
            if s_def == t_def
                && application_id(self.interner, source).is_some()
                && application_id(self.interner, target).is_some()
            {
                None
            } else {
                Some((s_def, t_def))
            }
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
                    return SubtypeResult::CycleDetected;
                }
            }
        }

        let def_entered = if let Some((s_def, t_def)) = def_pair {
            // Check reversed pair for bivariant cross-recursion
            if self.def_guard.is_visiting(&(t_def, s_def)) {
                self.guard.leave(pair);
                return SubtypeResult::CycleDetected;
            }
            match self.def_guard.enter((s_def, t_def)) {
                RecursionResult::Cycle => {
                    self.guard.leave(pair);
                    return SubtypeResult::CycleDetected;
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
                let is_nullable = matches!(source, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID);
                if !is_nullable {
                    let result = self.check_object_contract(source, target);
                    if let Some(dp) = def_entered {
                        self.def_guard.leave(dp);
                    }
                    self.guard.leave(pair);
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
                self.check_subtype_inner(source, target)
            }
        } else {
            let source_eval = self.evaluate_type(source);
            let target_eval = self.evaluate_type(target);

            if source_eval != source || target_eval != target {
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

        result
    }

    /// Inner subtype check (after cycle detection and type evaluation)
    fn check_subtype_inner(&mut self, source: TypeId, target: TypeId) -> SubtypeResult {
        // Types are already evaluated in check_subtype, so no need to re-evaluate here

        if !self.strict_null_checks && source.is_nullish() {
            return SubtypeResult::True;
        }

        // Note: Canonicalization-based structural identity (Task #36) was previously
        // called here as a "fast path", but it was actually SLOWER than the normal path
        // because it allocated a fresh Canonicalizer per call (FxHashMap + Vecs) and
        // triggered O(n²) union reduction via interner.union(). The existing QueryCache
        // already provides O(1) memoization for repeated subtype checks.
        // The Canonicalizer remains available for its intended purpose: detecting
        // structural identity of recursive type aliases (graph isomorphism).
        // See: are_types_structurally_identical() and isomorphism_tests.rs

        // Note: Weak type checking is handled by CompatChecker (compat.rs:167-170).
        // Removed redundant check here to avoid double-checking which caused false positives.

        if let Some(shape) = self.apparent_primitive_shape_for_type(source) {
            if let Some(t_shape_id) = object_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                return self.check_object_subtype(&shape, None, &t_shape);
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                return self.check_object_with_index_subtype(&shape, None, &t_shape);
            }
        }

        if let Some(source_cond_id) = conditional_type_id(self.interner, source) {
            if let Some(target_cond_id) = conditional_type_id(self.interner, target) {
                let source_cond = self.interner.conditional_type(source_cond_id);
                let target_cond = self.interner.conditional_type(target_cond_id);
                return self.check_conditional_subtype(source_cond.as_ref(), target_cond.as_ref());
            }

            let source_cond = self.interner.conditional_type(source_cond_id);
            return self.conditional_branches_subtype(source_cond.as_ref(), target);
        }

        if let Some(target_cond_id) = conditional_type_id(self.interner, target) {
            let target_cond = self.interner.conditional_type(target_cond_id);
            return self.subtype_of_conditional_target(source, target_cond.as_ref());
        }

        // Note: Source union/intersection handling is consolidated as follows:
        //
        // 1. Source union: Kept here (not moved to visitor) because it must run BEFORE
        //    the target union check. This order dependency is critical for correct
        //    union-to-union semantics: Union(A,B) <: Union(C,D) means ALL members of
        //    source must be subtypes of the target union (delegating to target union check).
        //
        // 2. Source intersection: Moved to visitor pattern (visit_intersection) which
        //    handles both the "at least one member" check AND the property merging logic
        //    for object targets. This removed ~50 lines of duplicate code.
        //
        // Source union check must run BEFORE target union check to handle union-to-union cases:
        // Union(A, B) <: Union(C, D) means (A <: Union(C, D)) AND (B <: Union(C, D))
        // This is different from the target union check which does: Source <: C OR Source <: D
        if let Some(members) = union_list_id(self.interner, source) {
            let member_list = self.interner.type_list(members);
            for &member in member_list.iter() {
                if !self.check_subtype(member, target).is_true() {
                    // Trace: No union member matches target
                    if let Some(tracer) = &mut self.tracer
                        && !tracer.on_mismatch_dyn(SubtypeFailureReason::NoUnionMemberMatches {
                            source_type: source,
                            target_union_members: vec![target],
                        })
                    {
                        return SubtypeResult::False;
                    }
                    return SubtypeResult::False;
                }
            }
            return SubtypeResult::True;
        }

        if let Some(members) = union_list_id(self.interner, target) {
            if keyof_inner_type(self.interner, source).is_some()
                && self.is_keyof_subtype_of_string_number_symbol_union(members)
            {
                return SubtypeResult::True;
            }

            // Rule #7: Open Numeric Enums - number is assignable to unions containing numeric enums
            if source == TypeId::NUMBER {
                let member_list = self.interner.type_list(members);
                for &member in member_list.iter() {
                    let def_id = lazy_def_id(self.interner, member)
                        .or_else(|| enum_components(self.interner, member).map(|(d, _)| d));
                    if let Some(def_id) = def_id
                        && self.resolver.is_numeric_enum(def_id)
                    {
                        return SubtypeResult::True;
                    }
                }
            }

            let member_list = self.interner.type_list(members);

            // Fast path: TypeId equality pre-scan before expensive structural checks.
            // If source has the same TypeId as any union member, it's trivially a subtype.
            // This avoids O(n × cost) structural comparisons when the match is by identity.
            for &member in member_list.iter() {
                if source == member {
                    return SubtypeResult::True;
                }
            }

            for &member in member_list.iter() {
                if self.check_subtype(source, member).is_true() {
                    return SubtypeResult::True;
                }
            }

            // Type parameter constraint check: if source is a type parameter with a constraint,
            // check if its constraint is assignable to the entire target union.
            // e.g., Bottom extends T | U should be assignable to T | U
            if let Some(s_info) = type_param_info(self.interner, source)
                && let Some(constraint) = s_info.constraint
                && self.check_subtype(constraint, target).is_true()
            {
                return SubtypeResult::True;
            }

            // Distributive intersection factoring:
            // S <: (A & S) | (B & S) is equivalent to S <: A | B
            let s_arc;
            let source_members: &[TypeId] =
                if let Some(s_list) = intersection_list_id(self.interner, source) {
                    s_arc = self.interner.type_list(s_list);
                    &s_arc
                } else {
                    std::slice::from_ref(&source)
                };

            let mut factored_members = Vec::new();
            let mut all_contain_source = true;
            for &member in member_list.iter() {
                let i_arc;
                let i_list: &[TypeId] =
                    if let Some(i_members) = intersection_list_id(self.interner, member) {
                        i_arc = self.interner.type_list(i_members);
                        &i_arc
                    } else {
                        std::slice::from_ref(&member)
                    };

                let mut contains_all = true;
                for &s_m in source_members.iter() {
                    if !i_list.contains(&s_m) {
                        contains_all = false;
                        break;
                    }
                }

                // DEBUG LOGGING
                // println!("source: {:?}, target member: {:?}, source_members: {:?}, i_list: {:?}, contains: {}",
                //          self.interner.lookup(source), self.interner.lookup(member), source_members, i_list, contains_all);

                if contains_all {
                    let mut rem = Vec::new();
                    for &i_m in i_list.iter() {
                        if !source_members.contains(&i_m) {
                            rem.push(i_m);
                        }
                    }
                    factored_members.push(self.interner.intersection(rem));
                } else {
                    all_contain_source = false;
                    break;
                }
            }

            if all_contain_source && !factored_members.is_empty() {
                let factored_target = self.interner.union(factored_members);
                // println!("ALL CONTAIN SOURCE! checking subtype against factored target: {:?}", self.interner.lookup(factored_target));
                if self.check_subtype(source, factored_target).is_true() {
                    return SubtypeResult::True;
                }
            }

            // Discriminated union check: if the source has discriminant properties
            // that distinguish between target union members, check each discriminant
            // value against the matching target members with a narrowed source.
            // See TypeScript's typeRelatedToDiscriminatedType.
            if self
                .type_related_to_discriminated_type(source, &member_list)
                .is_true()
            {
                return SubtypeResult::True;
            }

            // Intersection source check: if source is an intersection, check if any
            // member is assignable to the target union as a whole.
            // e.g., (A & B) <: C | D if A <: C | D
            if let Some(s_list) = intersection_list_id(self.interner, source) {
                let s_member_list = self.interner.type_list(s_list);
                for &s_member in s_member_list.iter() {
                    if self.check_subtype(s_member, target).is_true() {
                        return SubtypeResult::True;
                    }
                }
            }

            // Trace: Source is not a subtype of any union member
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::NoUnionMemberMatches {
                    source_type: source,
                    target_union_members: member_list.iter().copied().collect(),
                })
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::False;
        }

        // Note: Source intersection check removed - handled by visitor pattern dispatch
        // at the end of this function. The visitor includes the property merging logic.

        if let Some(members) = intersection_list_id(self.interner, target) {
            let member_list = self.interner.type_list(members);

            // Keep diagnostic precision when collecting mismatch reasons via tracer.
            if self.tracer.is_none()
                && self.can_use_object_intersection_fast_path(&member_list)
                && let Some(merged_target) = self.build_object_intersection_target(target)
            {
                return self.check_subtype(source, merged_target);
            }

            for &member in member_list.iter() {
                if !self.check_subtype(source, member).is_true() {
                    return SubtypeResult::False;
                }
            }
            return SubtypeResult::True;
        }

        if let (Some(s_kind), Some(t_kind)) = (
            intrinsic_kind(self.interner, source),
            intrinsic_kind(self.interner, target),
        ) {
            return self.check_intrinsic_subtype(s_kind, t_kind);
        }

        // Type parameter checks BEFORE boxed primitive check
        // Unconstrained type parameters should be handled before other checks
        if let Some(s_info) = type_param_info(self.interner, source) {
            return self.check_type_parameter_subtype(&s_info, target);
        }

        if let Some(_t_info) = type_param_info(self.interner, target) {
            // Special case: T & SomeType <: T
            // If source is an intersection containing the target type parameter,
            // the intersection is a more specific version (excluding null/undefined)
            // and is assignable. This handles the common pattern: T & {} <: T.
            if let Some(members) = intersection_list_id(self.interner, source) {
                let member_list = self.interner.type_list(members);
                for &member in member_list.iter() {
                    if member == target {
                        return SubtypeResult::True;
                    }
                }
            }

            // A concrete type is never a subtype of an opaque type parameter.
            // The type parameter T could be instantiated as any type satisfying its constraint,
            // so we cannot guarantee that source <: T unless source is never/any (handled above).
            //
            // This is the correct TypeScript behavior:
            // - "hello" is NOT assignable to T extends string (T could be "world")
            // - { value: number } is NOT assignable to unconstrained T (T defaults to unknown)
            //
            // Note: When the type parameter is the SOURCE (e.g., T <: string), we check
            // against its constraint. But as TARGET, we return False.

            // Trace: Concrete type not assignable to type parameter
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::False;
        }

        if let Some(s_kind) = intrinsic_kind(self.interner, source) {
            if self.is_boxed_primitive_subtype(s_kind, target) {
                return SubtypeResult::True;
            }
            // `object` keyword is structurally equivalent to `{}` (empty object).
            // It's assignable to any object type where all properties are optional,
            // since no required properties need to be satisfied.
            if s_kind == IntrinsicKind::Object {
                let target_shape = object_shape_id(self.interner, target)
                    .or_else(|| object_with_index_shape_id(self.interner, target));
                if let Some(t_shape_id) = target_shape {
                    let t_shape = self.interner.object_shape(t_shape_id);
                    if t_shape.properties.iter().all(|p| p.optional) {
                        return SubtypeResult::True;
                    }
                }
            }
            // Trace: Intrinsic type mismatch (boxed primitive check failed)
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::False;
        }

        if let (Some(lit), Some(t_kind)) = (
            literal_value(self.interner, source),
            intrinsic_kind(self.interner, target),
        ) {
            return self.check_literal_to_intrinsic(&lit, t_kind);
        }

        if let (Some(s_lit), Some(t_lit)) = (
            literal_value(self.interner, source),
            literal_value(self.interner, target),
        ) {
            if s_lit == t_lit {
                return SubtypeResult::True;
            }
            // Trace: Literal type mismatch
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::LiteralTypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::False;
        }

        if let (Some(LiteralValue::String(s_lit)), Some(t_spans)) = (
            literal_value(self.interner, source),
            template_literal_id(self.interner, target),
        ) {
            return self.check_literal_matches_template_literal(s_lit, t_spans);
        }

        if intrinsic_kind(self.interner, target) == Some(IntrinsicKind::Object) {
            if self.is_object_keyword_type(source) {
                return SubtypeResult::True;
            }
            // Trace: Source is not object-compatible
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::False;
        }

        // Check if target is the Function intrinsic (TypeId::FUNCTION) or the
        // Function interface from lib.d.ts. We check three ways:
        // 1. Target is the Function intrinsic (TypeId::FUNCTION)
        // 2. Target matches the registered boxed Function TypeId
        // 3. Target was resolved from a Lazy(DefId) whose DefId is a known Function DefId
        //    (handles the case where get_type_of_symbol and resolve_lib_type_by_name
        //    produce different TypeIds for the same Function interface)
        let is_function_target = intrinsic_kind(self.interner, target)
            == Some(IntrinsicKind::Function)
            || self
                .resolver
                .is_boxed_type_id(target, IntrinsicKind::Function)
            || self
                .resolver
                .get_boxed_type(IntrinsicKind::Function)
                .is_some_and(|boxed| boxed == target)
            || lazy_def_id(self.interner, target).is_some_and(|def_id| {
                self.resolver
                    .is_boxed_def_id(def_id, IntrinsicKind::Function)
            });
        if is_function_target {
            if self.is_callable_type(source) {
                return SubtypeResult::True;
            }
            // Trace: Source is not function-compatible
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::False;
        }

        // Check if target is the global `Object` interface from lib.d.ts.
        // This is a separate path from intrinsic `object`:
        // - `object` (lowercase) includes callable values.
        // - `Object` (capitalized interface) should follow TS structural rules and
        //   exclude bare callable types from primitive-style object assignability.
        let is_global_object_target = self
            .resolver
            .is_boxed_type_id(target, IntrinsicKind::Object)
            || self
                .resolver
                .get_boxed_type(IntrinsicKind::Object)
                .is_some_and(|boxed| boxed == target)
            || lazy_def_id(self.interner, target)
                .is_some_and(|t_def| self.resolver.is_boxed_def_id(t_def, IntrinsicKind::Object));
        if is_global_object_target {
            let source_eval = self.evaluate_type(source);
            if self.is_global_object_interface_type(source_eval) {
                return SubtypeResult::True;
            }

            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::False;
        }

        if let (Some(s_elem), Some(t_elem)) = (
            array_element_type(self.interner, source),
            array_element_type(self.interner, target),
        ) {
            return self.check_subtype(s_elem, t_elem);
        }

        if let (Some(s_elems), Some(t_elems)) = (
            tuple_list_id(self.interner, source),
            tuple_list_id(self.interner, target),
        ) {
            // OPTIMIZATION: Unit-tuple disjointness fast-path (O(1) cached lookup)
            // Two different unit tuples (tuples of literals/enums only) are guaranteed disjoint.
            // Since we already checked source == target at the top and returned True,
            // reaching here means source != target. If both are unit tuples, they're disjoint.
            // This avoids O(N) structural recursion for each comparison in BCT's O(N²) loop.
            if self.interner.is_unit_type(source) && self.interner.is_unit_type(target) {
                return SubtypeResult::False;
            }
            let s_elems = self.interner.tuple_list(s_elems);
            let t_elems = self.interner.tuple_list(t_elems);
            return self.check_tuple_subtype(&s_elems, &t_elems);
        }

        if let (Some(s_elems), Some(t_elem)) = (
            tuple_list_id(self.interner, source),
            array_element_type(self.interner, target),
        ) {
            return self.check_tuple_to_array_subtype(s_elems, t_elem);
        }

        if let (Some(s_elem), Some(t_elems)) = (
            array_element_type(self.interner, source),
            tuple_list_id(self.interner, target),
        ) {
            let t_elems = self.interner.tuple_list(t_elems);
            return self.check_array_to_tuple_subtype(s_elem, &t_elems);
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, source),
            object_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);

            // Symbol-level cycle detection for recursive interface types.
            // When both objects have symbols (e.g., Promise<X> vs PromiseLike<Y>),
            // check if we're already comparing objects with the same symbol pair.
            // This catches cycles where type evaluation loses DefId identity:
            // Promise<never> evaluates to Object(51) which has no DefId, but its
            // `then` method returns Promise<TResult> which, after instantiation and
            // evaluation, produces another Object with the same Promise symbol.
            if let (Some(s_sym), Some(t_sym)) = (s_shape.symbol, t_shape.symbol)
                && s_sym != t_sym
            {
                let sym_pair = (s_sym, t_sym);
                if !self.sym_visiting.insert(sym_pair) {
                    // Already visiting this symbol pair — coinductive cycle
                    return SubtypeResult::CycleDetected;
                }
                let result = self.check_object_subtype(&s_shape, Some(s_shape_id), &t_shape);
                self.sym_visiting.remove(&sym_pair);
                return result;
            }

            return self.check_object_subtype(&s_shape, Some(s_shape_id), &t_shape);
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, source),
            object_with_index_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_with_index_subtype(&s_shape, Some(s_shape_id), &t_shape);
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, source),
            object_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_with_index_to_object(
                &s_shape,
                s_shape_id,
                &t_shape.properties,
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, source),
            object_with_index_shape_id(self.interner, target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.check_object_to_indexed(&s_shape.properties, Some(s_shape_id), &t_shape);
        }

        if let (Some(s_fn_id), Some(t_fn_id)) = (
            function_shape_id(self.interner, source),
            function_shape_id(self.interner, target),
        ) {
            let s_fn = self.interner.function_shape(s_fn_id);
            let t_fn = self.interner.function_shape(t_fn_id);
            return self.check_function_subtype(&s_fn, &t_fn);
        }

        // Compatibility bridge: function-like values are assignable to interfaces
        // that only require Function members like `call`/`apply`.
        // This aligns with tsc behavior for:
        //   interface Callable { call(blah: any): any }
        //   const x: Callable = () => {}
        let source_function_like = function_shape_id(self.interner, source).is_some()
            || callable_shape_id(self.interner, source).is_some_and(|sid| {
                let shape = self.interner.callable_shape(sid);
                !shape.call_signatures.is_empty()
            })
            || source == TypeId::FUNCTION;
        if source_function_like {
            if let Some(t_callable_id) = callable_shape_id(self.interner, target) {
                let t_shape = self.interner.callable_shape(t_callable_id);
                if t_shape.call_signatures.is_empty() && t_shape.construct_signatures.is_empty() {
                    let required_props: Vec<_> =
                        t_shape.properties.iter().filter(|p| !p.optional).collect();
                    if required_props.len() == 1 {
                        let name = self.interner.resolve_atom(required_props[0].name);
                        if name == "call" || name == "apply" {
                            return SubtypeResult::True;
                        }
                    }
                }
            }
            if let Some(t_shape_id) = object_shape_id(self.interner, target)
                .or_else(|| object_with_index_shape_id(self.interner, target))
            {
                let t_shape = self.interner.object_shape(t_shape_id);
                let required_props: Vec<_> =
                    t_shape.properties.iter().filter(|p| !p.optional).collect();
                if required_props.len() == 1 {
                    let name = self.interner.resolve_atom(required_props[0].name);
                    if name == "call" || name == "apply" {
                        return SubtypeResult::True;
                    }
                }
            }
        }

        if let (Some(s_callable_id), Some(t_callable_id)) = (
            callable_shape_id(self.interner, source),
            callable_shape_id(self.interner, target),
        ) {
            let s_callable = self.interner.callable_shape(s_callable_id);
            let t_callable = self.interner.callable_shape(t_callable_id);
            return self.check_callable_subtype(&s_callable, &t_callable);
        }

        if let (Some(s_fn_id), Some(t_callable_id)) = (
            function_shape_id(self.interner, source),
            callable_shape_id(self.interner, target),
        ) {
            return self.check_function_to_callable_subtype(s_fn_id, t_callable_id);
        }

        if let (Some(s_callable_id), Some(t_fn_id)) = (
            callable_shape_id(self.interner, source),
            function_shape_id(self.interner, target),
        ) {
            return self.check_callable_to_function_subtype(s_callable_id, t_fn_id);
        }

        if let (Some(s_app_id), Some(t_app_id)) = (
            application_id(self.interner, source),
            application_id(self.interner, target),
        ) {
            return self.check_application_to_application_subtype(s_app_id, t_app_id);
        }

        // When both source and target are applications, try mapped-to-mapped
        // comparison before falling through to one-sided expansion. This handles
        // cases like Readonly<T> <: Partial<T> where both resolve to mapped types
        // over a generic type parameter that can't be concretely expanded.
        if let (Some(s_app_id), Some(t_app_id)) = (
            application_id(self.interner, source),
            application_id(self.interner, target),
        ) {
            let result = self.check_application_to_application(source, target, s_app_id, t_app_id);
            if result != SubtypeResult::False {
                return result;
            }
            // Fall through to one-sided expansion
        }

        if let Some(app_id) = application_id(self.interner, source) {
            return self.check_application_expansion_target(source, target, app_id);
        }

        if let Some(app_id) = application_id(self.interner, target) {
            return self.check_source_to_application_expansion(source, target, app_id);
        }

        // Check mapped-to-mapped structural comparison (for raw mapped types).
        if let (Some(source_mapped_id), Some(target_mapped_id)) = (
            mapped_type_id(self.interner, source),
            mapped_type_id(self.interner, target),
        ) {
            let result =
                self.check_mapped_to_mapped(source, target, source_mapped_id, target_mapped_id);
            if result != SubtypeResult::False {
                return result;
            }
        }

        if let Some(mapped_id) = mapped_type_id(self.interner, source) {
            return self.check_mapped_expansion_target(source, target, mapped_id);
        }

        if let Some(mapped_id) = mapped_type_id(self.interner, target) {
            return self.check_source_to_mapped_expansion(source, target, mapped_id);
        }

        // =======================================================================
        // ENUM TYPE CHECKING (Nominal Identity)
        // =======================================================================
        // Enums are nominal types - two different enums with the same member types
        // are NOT compatible. Enum(DefId, MemberType) preserves both:
        // - DefId: For nominal identity (E1 != E2)
        // - MemberType: For structural assignability to primitives (E1 <: number)
        // =======================================================================

        if let (Some((s_def_id, _s_members)), Some((t_def_id, _t_members))) = (
            enum_components(self.interner, source),
            enum_components(self.interner, target),
        ) {
            // Enum to Enum: Nominal check - DefIds must match
            if s_def_id == t_def_id {
                return SubtypeResult::True;
            }

            // Check for member-to-parent relationship (e.g., E.A -> E)
            // If source is a member of the target enum, it is a subtype
            if self.resolver.get_enum_parent_def_id(s_def_id) == Some(t_def_id) {
                // Source is a member of target enum
                // Only allow if target is the full enum type (not a different member)
                if self.resolver.is_enum_type(target, self.interner) {
                    return SubtypeResult::True;
                }
            }

            // Different enums are NOT compatible (nominal typing)
            // Trace: Enum nominal mismatch
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::False;
        }

        // Source is Enum, Target is not - check structural member type
        if let Some((_s_def_id, s_members)) = enum_components(self.interner, source) {
            return self.check_subtype(s_members, target);
        }

        // Target is Enum, Source is not - check Rule #7 first, then structural member type
        if let Some((t_def_id, t_members)) = enum_components(self.interner, target) {
            // Rule #7: number is assignable to numeric enums
            if source == TypeId::NUMBER && self.resolver.is_numeric_enum(t_def_id) {
                return SubtypeResult::True;
            }
            if matches!(
                self.interner.lookup(source),
                Some(TypeData::Literal(LiteralValue::Number(_)))
            ) && self.resolver.is_numeric_enum(t_def_id)
            {
                return SubtypeResult::True;
            }
            return self.check_subtype(source, t_members);
        }

        // =======================================================================
        // PHASE 3.2: PRIORITIZE DefId (Lazy) OVER SymbolRef (Ref)
        // =======================================================================
        // We now check Lazy(DefId) types before Ref(SymbolRef) types to establish
        // DefId as the primary type identity system. The InheritanceGraph bridge
        // enables Lazy types to use O(1) nominal subtype checking.
        // =======================================================================

        if let (Some(s_def), Some(t_def)) = (
            lazy_def_id(self.interner, source),
            lazy_def_id(self.interner, target),
        ) {
            // Use DefId-level cycle detection (checked before Ref types)
            return self.check_lazy_lazy_subtype(source, target, s_def, t_def);
        }

        // =======================================================================
        // Rule #7: Open Numeric Enums - Number <-> Numeric Enum Assignability
        // =======================================================================
        // In TypeScript, numeric enums are "open" - they allow bidirectional
        // assignability with the number type. This is unsound but matches tsc behavior.
        // See docs/specs/TS_UNSOUNDNESS_CATALOG.md Item #7.

        // Helper to extract DefId from Enum or Lazy types
        let get_enum_def_id = |type_id: TypeId| -> Option<DefId> {
            match self.interner.lookup(type_id) {
                Some(TypeData::Enum(def_id, _)) | Some(TypeData::Lazy(def_id)) => Some(def_id),
                _ => None,
            }
        };

        // Check: source is numeric enum, target is Number
        if let Some(s_def) = get_enum_def_id(source)
            && target == TypeId::NUMBER
            && self.resolver.is_numeric_enum(s_def)
        {
            return SubtypeResult::True;
        }

        // Check: source is Number (or numeric literal), target is numeric enum
        if let Some(t_def) = get_enum_def_id(target) {
            if source == TypeId::NUMBER && self.resolver.is_numeric_enum(t_def) {
                return SubtypeResult::True;
            }
            // Also check for numeric literals (subtypes of number)
            if matches!(
                self.interner.lookup(source),
                Some(TypeData::Literal(LiteralValue::Number(_)))
            ) && self.resolver.is_numeric_enum(t_def)
            {
                // For numeric literals, we need to check if they're assignable to the enum
                // Fall through to structural check (e.g., 0 -> E.A might succeed if E.A = 0)
                return self.check_subtype(source, self.resolve_lazy_type(target));
            }
        }

        if lazy_def_id(self.interner, source).is_some() {
            let resolved = self.resolve_lazy_type(source);
            return if resolved != source {
                self.check_subtype(resolved, target)
            } else {
                SubtypeResult::False
            };
        }

        if lazy_def_id(self.interner, target).is_some() {
            let resolved = self.resolve_lazy_type(target);
            return if resolved != target {
                self.check_subtype(source, resolved)
            } else {
                SubtypeResult::False
            };
        }

        // =======================================================================
        // Ref(SymbolRef) checks - now secondary to Lazy(DefId)
        // =======================================================================

        if let (Some(s_sym), Some(t_sym)) = (
            ref_symbol(self.interner, source),
            ref_symbol(self.interner, target),
        ) {
            return self.check_ref_ref_subtype(source, target, s_sym, t_sym);
        }

        if let Some(s_sym) = ref_symbol(self.interner, source) {
            return self.check_ref_subtype(source, target, s_sym);
        }

        if let Some(t_sym) = ref_symbol(self.interner, target) {
            return self.check_to_ref_subtype(source, target, t_sym);
        }

        if let (Some(s_sym), Some(t_sym)) = (
            type_query_symbol(self.interner, source),
            type_query_symbol(self.interner, target),
        ) {
            return self.check_typequery_typequery_subtype(source, target, s_sym, t_sym);
        }

        if let Some(s_sym) = type_query_symbol(self.interner, source) {
            return self.check_typequery_subtype(source, target, s_sym);
        }

        if let Some(t_sym) = type_query_symbol(self.interner, target) {
            return self.check_to_typequery_subtype(source, target, t_sym);
        }

        if let (Some(s_inner), Some(t_inner)) = (
            keyof_inner_type(self.interner, source),
            keyof_inner_type(self.interner, target),
        ) {
            return self.check_subtype(t_inner, s_inner);
        }

        if let (Some(s_inner), Some(t_inner)) = (
            readonly_inner_type(self.interner, source),
            readonly_inner_type(self.interner, target),
        ) {
            return self.check_subtype(s_inner, t_inner);
        }

        // Readonly target peeling: T <: Readonly<U> if T <: U
        // A mutable type can always be treated as readonly (readonly is a supertype)
        // CRITICAL: Only peel if source is NOT Readonly. If source IS Readonly, we must
        // fall through to the visitor to compare Readonly<S> vs Readonly<T>.
        if let Some(t_inner) = readonly_inner_type(self.interner, target)
            && readonly_inner_type(self.interner, source).is_none()
        {
            return self.check_subtype(source, t_inner);
        }

        // Readonly source to mutable target case is handled by SubtypeVisitor::visit_readonly_type
        // which returns False (correctly, because Readonly is not assignable to Mutable)

        if let (Some(s_sym), Some(t_sym)) = (
            unique_symbol_ref(self.interner, source),
            unique_symbol_ref(self.interner, target),
        ) {
            return if s_sym == t_sym {
                SubtypeResult::True
            } else {
                SubtypeResult::False
            };
        }

        if unique_symbol_ref(self.interner, source).is_some()
            && intrinsic_kind(self.interner, target) == Some(IntrinsicKind::Symbol)
        {
            return SubtypeResult::True;
        }

        if is_this_type(self.interner, source) && is_this_type(self.interner, target) {
            return SubtypeResult::True;
        }

        if let (Some(s_spans), Some(t_spans)) = (
            template_literal_id(self.interner, source),
            template_literal_id(self.interner, target),
        ) {
            return self.check_template_assignable_to_template(s_spans, t_spans);
        }

        if template_literal_id(self.interner, source).is_some()
            && intrinsic_kind(self.interner, target) == Some(IntrinsicKind::String)
        {
            return SubtypeResult::True;
        }

        let source_is_callable = function_shape_id(self.interner, source).is_some()
            || callable_shape_id(self.interner, source).is_some();
        if source_is_callable {
            // Build a source ObjectShape from callable properties for structural comparison.
            // IMPORTANT: Sort properties by name (Atom) to match the merge scan's expectation.
            let source_props = if let Some(callable_id) = callable_shape_id(self.interner, source) {
                let callable = self.interner.callable_shape(callable_id);
                let mut props = callable.properties.clone();
                props.sort_by_key(|a| a.name);
                Some(ObjectShape {
                    flags: ObjectFlags::empty(),
                    properties: props,
                    string_index: callable.string_index.clone(),
                    number_index: callable.number_index.clone(),
                    symbol: callable.symbol,
                })
            } else {
                None
            };

            if let Some(t_shape_id) = object_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                if t_shape.properties.is_empty() {
                    return SubtypeResult::True;
                }
                // If source is a CallableShape with properties, check structural compatibility
                if let Some(ref s_shape) = source_props {
                    return self.check_object_subtype(s_shape, None, &t_shape);
                }
                // FunctionShape has no properties - not assignable to non-empty object
                if let Some(tracer) = &mut self.tracer
                    && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                {
                    return SubtypeResult::False;
                }
                return SubtypeResult::False;
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                if t_shape.properties.is_empty() && t_shape.string_index.is_none() {
                    return SubtypeResult::True;
                }
                // If source is a CallableShape with properties, check structural compatibility
                if let Some(ref s_shape) = source_props {
                    return self.check_object_subtype(s_shape, None, &t_shape);
                }
                // FunctionShape has no properties - not assignable to non-empty indexed object
                if let Some(tracer) = &mut self.tracer
                    && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                {
                    return SubtypeResult::False;
                }
                return SubtypeResult::False;
            }
        }

        let source_is_array_or_tuple = array_element_type(self.interner, source).is_some()
            || tuple_list_id(self.interner, source).is_some();
        if source_is_array_or_tuple {
            if let Some(t_shape_id) = object_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                if t_shape.properties.is_empty() {
                    return SubtypeResult::True;
                }
                // Check if all target properties are satisfiable by the array.
                // First try a quick check for length-only targets.
                let only_length = t_shape
                    .properties
                    .iter()
                    .all(|p| self.interner.resolve_atom(p.name) == "length");
                if only_length {
                    let all_ok = t_shape
                        .properties
                        .iter()
                        .all(|p| self.check_subtype(TypeId::NUMBER, p.type_id).is_true());
                    if all_ok {
                        return SubtypeResult::True;
                    }
                }
                // Try the Array<T> interface for full structural comparison.
                // This handles cases like: number[] <: { toString(): string }
                if let Some(elem) = array_element_type(self.interner, source)
                    && let Some(result) = self.check_array_interface_subtype(elem, target)
                {
                    return result;
                }
                // Trace: Array/tuple not compatible with object
                if let Some(tracer) = &mut self.tracer
                    && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                {
                    return SubtypeResult::False;
                }
                return SubtypeResult::False;
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                if t_shape.properties.is_empty() && t_shape.string_index.is_none() {
                    if let Some(ref num_idx) = t_shape.number_index {
                        let elem_type =
                            array_element_type(self.interner, source).unwrap_or(TypeId::ANY);
                        if !self.check_subtype(elem_type, num_idx.value_type).is_true() {
                            // Trace: Array element type mismatch with index signature
                            if let Some(tracer) = &mut self.tracer
                                && !tracer.on_mismatch_dyn(
                                    SubtypeFailureReason::IndexSignatureMismatch {
                                        index_kind: "number",
                                        source_value_type: elem_type,
                                        target_value_type: num_idx.value_type,
                                    },
                                )
                            {
                                return SubtypeResult::False;
                            }
                            return SubtypeResult::False;
                        }
                    }
                    return SubtypeResult::True;
                }
                // Target has non-empty properties + index signature.
                // Try the Array<T> interface for full structural comparison.
                if let Some(elem) = array_element_type(self.interner, source)
                    && let Some(result) = self.check_array_interface_subtype(elem, target)
                {
                    return result;
                }
                // Trace: Array/tuple not compatible with indexed object with non-empty properties
                if let Some(tracer) = &mut self.tracer
                    && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                {
                    return SubtypeResult::False;
                }
                return SubtypeResult::False;
            }
        }

        // =======================================================================
        // VISITOR PATTERN DISPATCH (Task #48.4)
        // =======================================================================
        // After all special-case checks above, dispatch to the visitor for
        // general structural type checking. The visitor implements double-
        // dispatch pattern to handle source type variants and their interaction
        // with the target type.
        // =======================================================================

        // Extract the interner reference FIRST (Copy trait)
        // This must happen before creating the visitor which mutably borrows self
        let interner = self.interner;

        // Create the visitor with a mutable reborrow of self
        let mut visitor = SubtypeVisitor {
            checker: self,
            source,
            target,
        };

        // Dispatch to the visitor using the extracted interner
        let result = visitor.visit_type(interner, source);

        if result == SubtypeResult::False && self.check_generic_index_access_subtype(source, target)
        {
            return SubtypeResult::True;
        }

        // Trace: Generic fallback type mismatch (no specific reason matched above)
        if result == SubtypeResult::False
            && let Some(tracer) = &mut self.tracer
            && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            })
        {
            return SubtypeResult::False;
        }

        result
    }

    /// Check if a deferred keyof type is a subtype of string | number | symbol.
    /// This handles the case where `keyof T` (T is a type parameter) should be
    /// considered a subtype of `string | number | symbol` because in TypeScript,
    /// keyof always produces a subtype of those three types.
    fn is_keyof_subtype_of_string_number_symbol_union(&self, members: TypeListId) -> bool {
        let member_list = self.interner.type_list(members);
        // Check if the union contains string, number, and symbol
        let mut has_string = false;
        let mut has_number = false;
        let mut has_symbol = false;
        for &member in member_list.iter() {
            if member == TypeId::STRING {
                has_string = true;
            } else if member == TypeId::NUMBER {
                has_number = true;
            } else if member == TypeId::SYMBOL {
                has_symbol = true;
            }
        }
        has_string && has_number && has_symbol
    }
}

/// Convenience function for one-off subtype checks (without resolver)
pub fn is_subtype_of(interner: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    let mut checker = SubtypeChecker::new(interner);
    checker.is_subtype_of(source, target)
}

impl<'a, R: TypeResolver> AssignabilityChecker for SubtypeChecker<'a, R> {
    fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        SubtypeChecker::is_assignable_to(self, source, target)
    }

    fn is_assignable_to_bivariant_callback(&mut self, source: TypeId, target: TypeId) -> bool {
        let prev_strict = self.strict_function_types;
        let prev_param_count = self.allow_bivariant_param_count;
        self.strict_function_types = false;
        self.allow_bivariant_param_count = true;
        let result = SubtypeChecker::is_assignable_to(self, source, target);
        self.allow_bivariant_param_count = prev_param_count;
        self.strict_function_types = prev_strict;
        result
    }

    fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        SubtypeChecker::evaluate_type(self, type_id)
    }
}

/// Check if two types are structurally identical using De Bruijn indices for cycles.
///
/// This is the O(1) alternative to bidirectional subtyping for identity checks.
/// It transforms cyclic graphs into trees to solve the Graph Isomorphism problem.
pub fn are_types_structurally_identical<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    resolver: &R,
    a: TypeId,
    b: TypeId,
) -> bool {
    if a == b {
        return true;
    }
    let mut canonicalizer = crate::canonicalize::Canonicalizer::new(interner, resolver);
    let canon_a = canonicalizer.canonicalize(a);
    let canon_b = canonicalizer.canonicalize(b);

    // After canonicalization, structural identity reduces to TypeId equality
    canon_a == canon_b
}

/// Convenience function for one-off subtype checks routed through a `QueryDatabase`.
/// The `QueryDatabase` enables Salsa memoization when available.
pub fn is_subtype_of_with_db(db: &dyn QueryDatabase, source: TypeId, target: TypeId) -> bool {
    let mut checker = SubtypeChecker::new(db.as_type_database()).with_query_db(db);
    checker.is_subtype_of(source, target)
}

/// Convenience function for one-off subtype checks with compiler flags.
/// The flags are a packed u16 bitmask matching RelationCacheKey.flags.
pub fn is_subtype_of_with_flags(
    interner: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    flags: u16,
) -> bool {
    let mut checker = SubtypeChecker::new(interner).apply_flags(flags);
    checker.is_subtype_of(source, target)
}

// Re-enabled subtype tests - verifying API compatibility
#[cfg(test)]
#[path = "../../tests/subtype_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/index_signature_tests.rs"]
mod index_signature_tests;

#[cfg(test)]
#[path = "../../tests/generics_rules_tests.rs"]
mod generics_rules_tests;

#[cfg(test)]
#[path = "../../tests/callable_tests.rs"]
mod callable_tests;

#[cfg(test)]
#[path = "../../tests/union_tests.rs"]
mod union_tests;

#[cfg(test)]
#[path = "../../tests/typescript_quirks_tests.rs"]
mod typescript_quirks_tests;

#[cfg(test)]
#[path = "../../tests/type_predicate_tests.rs"]
mod type_predicate_tests;

#[cfg(test)]
#[path = "../../tests/overlap_tests.rs"]
mod overlap_tests;
