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
use crate::caches::db::QueryDatabase;
use crate::def::DefId;
use crate::diagnostics::{DynSubtypeTracer, SubtypeFailureReason};
#[cfg(test)]
use crate::types::*;
use crate::types::{
    IntrinsicKind, LiteralValue, ObjectFlags, ObjectShape, SymbolRef, TypeData, TypeId, TypeListId,
};
use crate::visitor::{
    TypeVisitor, application_id, array_element_type, callable_shape_id, conditional_type_id,
    enum_components, function_shape_id, index_access_parts, intersection_list_id, intrinsic_kind,
    is_this_type, keyof_inner_type, lazy_def_id, literal_value, mapped_type_id, object_shape_id,
    object_with_index_shape_id, readonly_inner_type, string_intrinsic_components,
    template_literal_id, tuple_list_id, type_param_info, type_query_symbol, union_list_id,
    unique_symbol_ref,
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
    /// `type T<X> = T<Box<X>>`. Following tsc's semantics, this is treated
    /// as `true` (Ternary.Maybe) — when the relation checker cannot determine
    /// the answer within depth limits, it assumes the types are related.
    /// This matches tsc's `isRelatedTo` overflow behavior and prevents false
    /// TS2344 errors on recursive/circular generic constraints.
    DepthExceeded,
}

impl SubtypeResult {
    pub const fn is_true(self) -> bool {
        matches!(self, Self::True | Self::CycleDetected | Self::DepthExceeded)
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
pub(crate) fn is_disjoint_unit_type(types: &dyn TypeDatabase, ty: TypeId) -> bool {
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

// TypeResolver, NoopResolver, and TypeEnvironment are defined in def/resolver.rs
pub use crate::def::resolver::{NoopResolver, TypeEnvironment, TypeResolver};

use super::rules::intrinsics::boxable_intrinsic_kind;
use super::visitor::SubtypeVisitor;

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
    pub inheritance_graph: Option<&'a crate::classes::inheritance::InheritanceGraph>,
    /// Optional callback to check if a symbol is a class (for nominal subtyping).
    /// Returns true if the symbol has the CLASS flag set.
    pub is_class_symbol: Option<&'a dyn Fn(SymbolRef) -> bool>,
    /// Controls how `any` is treated during subtype checks.
    pub any_propagation: AnyPropagationMode,
    /// Whether to enforce weak type checking during nested structural comparisons.
    /// When true, object comparisons will reject assignments where the target is a
    /// "weak type" (all optional properties) and the source has no common properties.
    /// This is set by `CompatChecker` to propagate TS2559 detection into nested property checks.
    /// Default: false (`SubtypeChecker` alone doesn't enforce weak types).
    pub enforce_weak_types: bool,
    /// Tracks whether we're inside a property type comparison. When true, the weak
    /// type check applies to object-to-object comparisons. This prevents the `SubtypeChecker`
    /// from applying weak checks at the top level (where the `CompatChecker` already handles
    /// them with proper exemptions like global Object and union-level policies).
    pub(crate) in_property_check: bool,
    /// When true, we're checking source <: individual members of an intersection target.
    /// Weak type checks (TS2559) are suppressed for individual members because the
    /// source may have no common properties with one member but still be assignable
    /// to the combined intersection (e.g., `A <: A & WeakType` should pass).
    pub(crate) in_intersection_member_check: bool,
    /// Whether recursive relation cycles and overflow should be treated as
    /// assumed-related (`true`) or definitive failure (`false`).
    pub assume_related_on_cycle: bool,
    /// When `true`, DefId-level cycle detection compares Application type
    /// arguments before assuming related. This prevents false identity matches
    /// for recursive generic interfaces like `IPromise<T>` vs `Promise<T>`
    /// where the structures are identical but the type arguments at the cycle
    /// point differ (e.g., `IPromise2<W, U>` vs `Promise2<any, W>`).
    /// Used by `are_types_identical_for_redeclaration` for TS2403 identity checks.
    pub identity_cycle_check: bool,
    /// Cache for `evaluate_type` results within this `SubtypeChecker`'s lifetime.
    /// This prevents O(n²) behavior when the same type (e.g., a large union) is
    /// evaluated multiple times across different subtype checks.
    /// Key is (`TypeId`, `no_unchecked_indexed_access`) since that flag affects evaluation.
    pub(crate) eval_cache: FxHashMap<(TypeId, bool), TypeId>,
    /// Optional tracer for collecting subtype failure diagnostics.
    /// When `Some`, enables detailed failure reason collection for error messages.
    /// When `None`, disables tracing for maximum performance (default).
    pub tracer: Option<&'a mut dyn DynSubtypeTracer>,
    /// When true (default), non-generic functions may be compared to generic functions
    /// by erasing the target's type parameters to their constraints. This matches tsc's
    /// default `eraseGenerics` behavior for structural type comparison.
    /// When false, a non-generic function is NOT assignable to a generic function —
    /// the target's `TypeParameter` types are left in place, causing the comparison to
    /// fail for concrete types. Used for implements/extends member type checking
    /// where tsc's `compareSignaturesRelated` does NOT erase.
    pub erase_generics: bool,
    /// Type parameter equivalences established during generic function subtype checking.
    ///
    /// When alpha-renaming in `check_function_subtype` maps target type params to source
    /// type params (e.g., B→D), the substitution may fail to penetrate pre-evaluated Object
    /// types due to name-based shadowing from inner functions with same-named type params.
    /// These equivalences allow structural comparison to treat the mapped type params as
    /// identical, fixing false TS2416 for generic methods with structurally identical signatures
    /// but different type param names (e.g., `<D>(f: (t: C) => D) => IList<D>` vs
    /// `<B>(f: (t: C) => B) => IList<B>`).
    pub(crate) type_param_equivalences: Vec<(TypeId, TypeId)>,
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
            enforce_weak_types: false,
            in_property_check: false,
            in_intersection_member_check: false,
            assume_related_on_cycle: true,
            identity_cycle_check: false,
            bypass_evaluation: false,
            max_depth: MAX_SUBTYPE_DEPTH,
            erase_generics: true,
            eval_cache: FxHashMap::default(),
            tracer: None,
            type_param_equivalences: Vec::new(),
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
            enforce_weak_types: false,
            in_property_check: false,
            in_intersection_member_check: false,
            assume_related_on_cycle: true,
            identity_cycle_check: false,
            bypass_evaluation: false,
            max_depth: MAX_SUBTYPE_DEPTH,
            erase_generics: true,
            eval_cache: FxHashMap::default(),
            tracer: None,
            type_param_equivalences: Vec::new(),
        }
    }

    /// Set the inheritance graph for O(1) nominal class subtype checking.
    pub const fn with_inheritance_graph(
        mut self,
        graph: &'a crate::classes::inheritance::InheritanceGraph,
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

    /// Configure whether recursive relation cycles should be assumed related.
    pub const fn with_assume_related_on_cycle(mut self, assume: bool) -> Self {
        self.assume_related_on_cycle = assume;
        self
    }

    pub(crate) const fn cycle_result(&self) -> SubtypeResult {
        if self.assume_related_on_cycle {
            SubtypeResult::CycleDetected
        } else {
            SubtypeResult::False
        }
    }

    pub(crate) const fn depth_result(&self) -> SubtypeResult {
        if self.assume_related_on_cycle {
            SubtypeResult::DepthExceeded
        } else {
            SubtypeResult::False
        }
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
        self.erase_generics = (flags & crate::RelationCacheKey::FLAG_NO_ERASE_GENERICS) == 0;
        self
    }

    pub(crate) fn resolve_lazy_type(&self, type_id: TypeId) -> TypeId {
        if let Some(def_id) = lazy_def_id(self.interner, type_id) {
            self.resolver
                .resolve_lazy(def_id, self.interner)
                .map(|resolved| self.bind_polymorphic_this(type_id, resolved))
                .unwrap_or(type_id)
        } else {
            type_id
        }
    }

    pub(crate) fn bind_polymorphic_this(&self, receiver: TypeId, resolved: TypeId) -> TypeId {
        if crate::contains_this_type(self.interner, resolved) {
            crate::substitute_this_type(self.interner, resolved, receiver)
        } else {
            resolved
        }
    }

    /// Inner subtype check (after cycle detection and type evaluation).
    ///
    /// Wrapped with `stacker::maybe_grow()` so that deeply recursive structural
    /// comparisons (e.g. ts-toolbelt type-level tests) grow the stack dynamically
    /// instead of crashing even when the logical `RecursionGuard` has headroom.
    pub(crate) fn check_subtype_inner(&mut self, source: TypeId, target: TypeId) -> SubtypeResult {
        stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || {
            self.check_subtype_inner_impl(source, target)
        })
    }

    /// Actual structural comparison -- separated so `stacker::maybe_grow` can wrap it.
    fn check_homomorphic_mapped_source_to_type_param(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        // Try raw mapped type first
        if let Some(mapped_id) = mapped_type_id(self.interner, source) {
            return self.check_homomorphic_mapped_to_target(mapped_id, target);
        }

        // Try application that resolves to a mapped type (e.g., Readonly<T>, Partial<T>)
        if let Some(app_id) = application_id(self.interner, source)
            && let Some(expanded) = self.try_expand_application(app_id)
            && let Some(mapped_id) = mapped_type_id(self.interner, expanded)
        {
            return self.check_homomorphic_mapped_to_target(mapped_id, target);
        }

        false
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

mod algorithm;

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
        // Bivariant callback checking disables strict_function_types so parameter
        // types are checked bivariantly (both directions). But the parameter COUNT
        // check must still apply — a callback with more required params than the
        // target accepts is always an error (TS2345), regardless of bivariance.
        let prev_strict = self.strict_function_types;
        self.strict_function_types = false;
        let result = SubtypeChecker::is_assignable_to(self, source, target);
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
#[path = "../../../tests/subtype_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/index_signature_tests.rs"]
mod index_signature_tests;

#[cfg(test)]
#[path = "../../../tests/generics_rules_tests.rs"]
mod generics_rules_tests;

#[cfg(test)]
#[path = "../../../tests/callable_tests.rs"]
mod callable_tests;

#[cfg(test)]
#[path = "../../../tests/union_tests.rs"]
mod union_tests;

#[cfg(test)]
#[path = "../../../tests/typescript_quirks_tests.rs"]
mod typescript_quirks_tests;

#[cfg(test)]
#[path = "../../../tests/type_predicate_tests.rs"]
mod type_predicate_tests;

#[cfg(test)]
#[path = "../../../tests/overlap_tests.rs"]
mod overlap_tests;

#[cfg(test)]
#[path = "../../../tests/intersection_optional_subtype_tests.rs"]
mod intersection_optional_subtype_tests;
