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

use std::sync::Arc;

use crate::caches::db::QueryDatabase;
use crate::construction::TypeDatabase;
use crate::def::DefId;
use crate::diagnostics::{DynSubtypeTracer, SubtypeFailureReason};
use crate::objects::{PropertyCollectionResult, collect_properties};
use crate::operations::AssignabilityChecker;
#[cfg(test)]
use crate::types::*;
use crate::types::{
    IntrinsicKind, LiteralValue, ObjectFlags, ObjectShape, PropertyInfo, SymbolRef, TemplateSpan,
    TypeData, TypeId, TypeListId,
};
use crate::visitor::{
    TypeVisitor, application_id, array_element_type, callable_shape_id, conditional_type_id,
    enum_components, function_shape_id, index_access_parts, intersection_list_id, intrinsic_kind,
    is_this_type, keyof_inner_type, lazy_def_id, literal_value, mapped_type_id, object_shape_id,
    object_with_index_shape_id, readonly_inner_type, string_intrinsic_components,
    template_literal_id, template_literal_spans_full_string_domain, tuple_list_id, type_param_info,
    type_query_symbol, union_list_id, unique_symbol_ref,
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
    // BOOLEAN_TRUE / BOOLEAN_FALSE are reserved intrinsic TypeIds whose
    // TypeData::lookup returns Literal(Boolean), so they ARE disjoint unit
    // types. All other intrinsics lookup to Intrinsic(_) which falls
    // through to `_ => false`.
    if ty == TypeId::BOOLEAN_TRUE || ty == TypeId::BOOLEAN_FALSE {
        return true;
    }
    if ty.is_intrinsic() {
        return false;
    }
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
    /// Per-`DefId` one-sided application expansion depth; see `enter_app_expansion_depth`.
    pub(crate) app_expand_depth: FxHashMap<DefId, u32>,
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
    /// Strict identity mode for the readonly modifier. When true, two
    /// otherwise-structurally-equal types whose readonly state differs are
    /// treated as non-related. This is asymmetric to ordinary assignability,
    /// which is permissive about readonly. Toggled inside the bidirectional
    /// identity check used by conditional `extends` clause comparison
    /// (the `IfEquals` pattern), where `{ readonly x: T }` and `{ x: T }` must
    /// be observably distinct.
    /// Default: false.
    pub strict_readonly_identity: bool,
    /// Whether null/undefined are treated as separate types.
    /// Default: true (strict null checks).
    pub strict_null_checks: bool,
    /// Whether indexed access includes `undefined`.
    /// Default: false (legacy TS behavior).
    pub no_unchecked_indexed_access: bool,
    // When true, disables method bivariance (methods use contravariance).
    // Default: false (methods are bivariant in TypeScript for compatibility).
    pub disable_method_bivariance: bool,
    /// When true, the immediate function comparison about to start is happening
    /// inside a callback parameter check. This corresponds to tsc's
    /// `SignatureCheckMode.Callback` bit: even if both signatures are method
    /// declarations, params are checked contravariantly (no method-bivariance
    /// loosening). This flag is set by `are_parameters_compatible_impl` before
    /// recursing into callable parameter types and is consumed (reset) on entry
    /// to `check_function_subtype_impl`, matching tsc's behavior where
    /// `getSingleCallSignature` returns undefined inside callback mode and the
    /// next callback recursion starts fresh through `compareTypes`.
    pub(crate) in_callback_param_check: bool,
    /// The immediate callback signature comparison should use
    /// `BivariantCallback` return compatibility. This is set only when the
    /// callback comparison is reached from a bivariant method/constructor slot.
    pub(crate) in_bivariant_callback_return_check: bool,
    /// True only while the immediate callback signature comparison is checking
    /// its own parameter list. Nested function comparisons reached through
    /// `compareTypes` must start fresh, so `are_parameters_compatible_impl`
    /// clears this around recursive subtype calls.
    pub(crate) force_strict_callback_param_variance: bool,
    /// Type arguments from the current generic application receiver while
    /// comparing a method property. If a callable method parameter is exactly
    /// one of these args, it originated as a generic type-parameter slot and
    /// should keep normal method bivariance, matching tsc's
    /// `isInstantiatedGenericParameter` guard.
    pub(crate) instantiated_generic_method_args: Vec<TypeId>,
    /// Original `strict_function_types` values saved while
    /// `check_subtype_with_method_variance` temporarily enables bivariant
    /// method parameters. Return-type comparisons consult this so method
    /// bivariance does not leak into returned function types.
    pub(crate) method_bivariance_strict_stack: Vec<bool>,
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
    /// Apparent object shapes for primitive wrapper fallback.
    ///
    /// Primitive structural subtype checks can ask for the same wrapper shape
    /// thousands of times. Cache the shape once per checker so those
    /// checks avoid rebuilding method signatures and property vectors.
    pub(crate) apparent_primitive_shapes: [Option<Arc<ObjectShape>>; 5],
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
    /// When true, a failed contextual inference for two generic signatures with
    /// different arity falls through to erased-signature comparison.
    ///
    /// This is intentionally opt-in: interface property compatibility needs the
    /// retry, but ordinary assignments must keep the failed inference as a real
    /// mismatch so invalid reverse generic assignments still report TS2322.
    pub allow_erased_generic_signature_retry: bool,
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

/// Operation-local cache statistics for [`SubtypeChecker`].
///
/// Owner: one subtype-checking request family. The evaluation memo is dropped
/// with the checker or cleared by [`SubtypeChecker::reset`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SubtypeCheckerCacheStatistics {
    /// Entries in the evaluation memo keyed by input `TypeId` and evaluation mode.
    pub eval_entries: usize,
    estimated_size_bytes: usize,
}

impl SubtypeCheckerCacheStatistics {
    /// Estimated heap bytes owned by subtype checker memo tables.
    #[must_use]
    pub const fn estimated_size_bytes(self) -> usize {
        self.estimated_size_bytes
    }
}

// Large structural dispatch (`check_subtype_inner_impl`) lives in a child
// module to keep this shard under the §19 file-size cap; see `core_dispatch`.
#[path = "core_dispatch.rs"]
mod core_dispatch;

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
            app_expand_depth: FxHashMap::default(),
            sym_visiting: FxHashSet::default(),
            strict_function_types: true, // Default to strict (sound) behavior
            allow_void_return: false,
            allow_bivariant_rest: false,
            allow_bivariant_param_count: false,
            exact_optional_property_types: false,
            strict_readonly_identity: false,
            strict_null_checks: true,
            no_unchecked_indexed_access: false,
            disable_method_bivariance: false,
            in_callback_param_check: false,
            in_bivariant_callback_return_check: false,
            force_strict_callback_param_variance: false,
            instantiated_generic_method_args: Vec::new(),
            method_bivariance_strict_stack: Vec::new(),
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
            allow_erased_generic_signature_retry: false,
            eval_cache: FxHashMap::default(),
            apparent_primitive_shapes: std::array::from_fn(|_| None),
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
            app_expand_depth: FxHashMap::default(),
            sym_visiting: FxHashSet::default(),
            strict_function_types: true,
            allow_void_return: false,
            allow_bivariant_rest: false,
            allow_bivariant_param_count: false,
            exact_optional_property_types: false,
            strict_readonly_identity: false,
            strict_null_checks: true,
            no_unchecked_indexed_access: false,
            disable_method_bivariance: false,
            in_callback_param_check: false,
            in_bivariant_callback_return_check: false,
            force_strict_callback_param_variance: false,
            instantiated_generic_method_args: Vec::new(),
            method_bivariance_strict_stack: Vec::new(),
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
            allow_erased_generic_signature_retry: false,
            eval_cache: FxHashMap::default(),
            apparent_primitive_shapes: std::array::from_fn(|_| None),
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

    fn intersection_has_incompatible_target_property(
        &mut self,
        source_intersection: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(target_shape_id) = object_shape_id(self.interner, target)
            .or_else(|| object_with_index_shape_id(self.interner, target))
        else {
            return false;
        };
        let target_shape = self.interner.object_shape(target_shape_id);
        let PropertyCollectionResult::Properties {
            properties: source_props,
            ..
        } = collect_properties(source_intersection, self.interner, self.resolver)
        else {
            return false;
        };

        for target_prop in &target_shape.properties {
            let Some(source_prop) = self.lookup_property(&source_props, None, target_prop.name)
            else {
                continue;
            };

            let saved_tracer = self.tracer.take();
            let compatible =
                self.check_property_compatibility(source_prop, target_prop, None, None);
            self.tracer = saved_tracer;

            if compatible.is_false() {
                return true;
            }
        }

        false
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
        self.app_expand_depth.clear();
        self.sym_visiting.clear();
        self.eval_cache.clear();
    }

    /// Return entry and size accounting for this checker's operation-local caches.
    #[must_use]
    pub fn cache_statistics(&self) -> SubtypeCheckerCacheStatistics {
        let eval_entries = self.eval_cache.len();
        let estimated_size_bytes =
            eval_entries.saturating_mul(std::mem::size_of::<((TypeId, bool), TypeId)>());
        SubtypeCheckerCacheStatistics {
            eval_entries,
            estimated_size_bytes,
        }
    }

    /// Whether any recursion limit (depth or iteration count) was exceeded.
    ///
    /// Use [`iteration_exceeded`] to distinguish complexity overflow (TS2859) from
    /// stack-depth overflow (TS2321).
    pub const fn depth_exceeded(&self) -> bool {
        self.guard.is_exceeded()
    }

    /// Whether the iteration (relation-count) budget was exhausted.
    ///
    /// When true the caller should emit TS2859 "Excessive complexity comparing
    /// types". When false but [`depth_exceeded`] is true, the stack depth was
    /// exceeded and the caller should emit TS2321 "Excessive stack depth".
    pub const fn iteration_exceeded(&self) -> bool {
        self.guard.iteration_exceeded()
    }

    /// Run `f` with subtype flags configured for tsc's `isTypeIdenticalTo`
    /// identity checking (TS2403 and related redeclaration/identity paths).
    ///
    /// Temporarily sets `any_propagation = TopLevelOnly`, enables
    /// `identity_cycle_check`, disables method bivariance, and forces
    /// `strict_function_types = true` — matching tsc's strict bidirectional
    /// structural equality. All four flags are restored on return, even if
    /// `f` returns early.
    pub fn with_identity_check_mode<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved_any_mode = self.any_propagation;
        let saved_identity_cycle = self.identity_cycle_check;
        let saved_method_bivariance = self.disable_method_bivariance;
        let saved_strict_fn = self.strict_function_types;
        self.any_propagation = AnyPropagationMode::TopLevelOnly;
        self.identity_cycle_check = true;
        self.disable_method_bivariance = true;
        self.strict_function_types = true;
        let result = f(self);
        self.any_propagation = saved_any_mode;
        self.identity_cycle_check = saved_identity_cycle;
        self.disable_method_bivariance = saved_method_bivariance;
        self.strict_function_types = saved_strict_fn;
        result
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
            crate::instantiation::instantiate::substitute_this_type_cached(
                self.interner,
                self.query_db,
                resolved,
                receiver,
            )
        } else {
            resolved
        }
    }

    fn constrained_projection_for_template_source(&mut self, source: TypeId) -> Option<TypeId> {
        let template_id = template_literal_id(self.interner, source)?;
        let spans = self.interner.template_list(template_id);
        let mut projected = Vec::with_capacity(spans.len());
        let mut changed = false;

        for span in spans.iter() {
            match span {
                TemplateSpan::Text(atom) => projected.push(TemplateSpan::Text(*atom)),
                TemplateSpan::Type(type_id) => {
                    let projected_type =
                        self.constrained_projection_for_template_span_type(*type_id);
                    changed |= projected_type != *type_id;
                    projected.push(TemplateSpan::Type(projected_type));
                }
            }
        }

        if !changed {
            return None;
        }

        let projected = self.interner.template_literal(projected);
        Some(self.evaluate_type(projected))
    }

    fn constrained_projection_for_template_span_type(&mut self, type_id: TypeId) -> TypeId {
        if let Some(info) = type_param_info(self.interner, type_id)
            && let Some(constraint) = info.constraint
        {
            return self.evaluate_type(constraint);
        }

        match self.interner.lookup(type_id) {
            Some(TypeData::StringIntrinsic { kind, type_arg }) => {
                let projected_arg = self.constrained_projection_for_template_span_type(type_arg);
                if projected_arg != type_arg {
                    return self.evaluate_type(self.interner.string_intrinsic(kind, projected_arg));
                }
                type_id
            }
            Some(TypeData::TemplateLiteral(template_id)) => {
                let spans = self.interner.template_list(template_id);
                let mut projected = Vec::with_capacity(spans.len());
                let mut changed = false;
                for span in spans.iter() {
                    match span {
                        TemplateSpan::Text(atom) => projected.push(TemplateSpan::Text(*atom)),
                        TemplateSpan::Type(inner) => {
                            let projected_inner =
                                self.constrained_projection_for_template_span_type(*inner);
                            changed |= projected_inner != *inner;
                            projected.push(TemplateSpan::Type(projected_inner));
                        }
                    }
                }
                if changed {
                    self.evaluate_type(self.interner.template_literal(projected))
                } else {
                    type_id
                }
            }
            _ => type_id,
        }
    }

    fn object_shape_def_id(&self, type_id: TypeId) -> Option<DefId> {
        let shape_id = object_shape_id(self.interner, type_id)
            .or_else(|| object_with_index_shape_id(self.interner, type_id))?;
        let shape = self.interner.object_shape(shape_id);
        let symbol = shape.symbol?;
        self.resolver.symbol_to_def_id(SymbolRef(symbol.0))
    }

    fn non_generic_object_shape_def_id(&self, type_id: TypeId) -> Option<DefId> {
        let def_id = self.object_shape_def_id(type_id)?;
        let has_type_params = self
            .resolver
            .get_lazy_type_params(def_id)
            .is_some_and(|params| !params.is_empty());
        (!has_type_params).then_some(def_id)
    }

    pub(crate) fn readonly_array_application_base(&self, base: TypeId) -> bool {
        match self.interner.lookup(base) {
            Some(TypeData::Lazy(def_id)) => self.resolver.is_builtin_readonly_array_def(def_id),
            Some(TypeData::UnresolvedTypeName(name)) => {
                self.interner.resolve_atom_ref(name).as_ref() == "ReadonlyArray"
            }
            _ => self
                .interner
                .get_display_alias(base)
                .is_some_and(|alias| self.readonly_array_application_base(alias)),
        }
    }

    pub(crate) fn readonly_array_application_element(&self, type_id: TypeId) -> Option<TypeId> {
        let app_id = application_id(self.interner, type_id)?;
        let app = self.interner.type_application(app_id);
        (app.args.len() == 1 && self.readonly_array_application_base(app.base))
            .then_some(app.args[0])
    }

    pub(crate) fn readonly_array_syntax_element(&self, type_id: TypeId) -> Option<TypeId> {
        let inner = readonly_inner_type(self.interner, type_id)?;
        array_element_type(self.interner, inner)
    }

    pub(crate) fn type_contains_readonly_array_syntax(&self, type_id: TypeId) -> bool {
        if self.readonly_array_syntax_element(type_id).is_some() {
            return true;
        }
        union_list_id(self.interner, type_id).is_some_and(|members| {
            self.interner
                .type_list(members)
                .iter()
                .any(|&member| self.type_contains_readonly_array_syntax(member))
        })
    }

    fn array_source_satisfies_minimal_indexed_array_target(
        &mut self,
        source_elem: TypeId,
        target_elem: TypeId,
        target_props: &[PropertyInfo],
    ) -> bool {
        if !self.check_subtype(source_elem, target_elem).is_true() {
            return false;
        }

        let length = self.interner.intern_string("length");
        target_props.iter().all(|prop| {
            prop.optional
                || (prop.name == length
                    && self.check_subtype(TypeId::NUMBER, prop.type_id).is_true())
        })
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

    pub(crate) fn readonly_application_or_display_alias_inner(
        &self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let app_id = application_id(self.interner, type_id).or_else(|| {
            self.interner
                .get_display_alias(type_id)
                .and_then(|alias| application_id(self.interner, alias))
        })?;
        let app = self.interner.type_application(app_id);
        let def_id = match self.interner.lookup(app.base) {
            Some(TypeData::Lazy(def_id)) => Some(def_id),
            Some(TypeData::TypeQuery(symbol_ref)) => {
                let def_id = self.resolver.symbol_to_def_id(symbol_ref)?;
                matches!(
                    self.resolver.get_def_kind(def_id),
                    Some(crate::def::DefKind::Interface | crate::def::DefKind::TypeAlias)
                )
                .then_some(def_id)
            }
            _ => None,
        }?;
        let name = self.resolver.get_def_name(def_id)?;
        let inner = app.args.first().copied()?;
        (self.interner.resolve_atom_ref(name).as_ref() == "Readonly").then_some(inner)
    }

    /// Check if a source type is a homomorphic mapped type that is assignable
    /// to a type parameter target.
    ///
    /// In tsc 6.0, homomorphic mapped types like `Readonly<T>`, `Partial<T>`,
    /// `Required<T>`, and identity mapped types `{ [K in keyof T]: T[K] }` are
    /// bidirectionally assignable to their source type parameter T.
    ///
    /// This handles the case where source is:
    /// - A raw Mapped type: `{ readonly [K in keyof T]: T[K] }`
    /// - An Application that expands to a Mapped type: `Readonly<T>`, `Partial<T>`
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

/// Check if two types are structurally identical with an outer type-parameter
/// scope visible to both sides.
///
/// This generalizes [`are_types_structurally_identical`] for callers comparing
/// type expressions whose `TypeData::TypeParameter` references are bound
/// outside the supplied types — for example, constraints attached to merged
/// interface declarations, where each declaration's `T` resolves to a distinct
/// underlying `TypeParameter` `TypeId` even though both should be treated as
/// the "same" positional parameter under tsc's declaration-merge rule.
///
/// `param_names` lists the outer-scope parameter names in declaration order.
/// References to those names inside `a` or `b` are rewritten to the matching
/// `BoundParameter(n)` before structural equality is checked.
pub fn are_types_structurally_identical_in_param_scope<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    resolver: &R,
    a: TypeId,
    b: TypeId,
    param_names: &[tsz_common::interner::Atom],
) -> bool {
    if a == b {
        return true;
    }
    let mut canonicalizer = crate::canonicalize::Canonicalizer::new(interner, resolver);
    let canon_a = canonicalizer.canonicalize_with_param_scope(a, param_names);
    let canon_b = canonicalizer.canonicalize_with_param_scope(b, param_names);
    canon_a == canon_b
}

/// Convenience function for one-off subtype checks routed through a `QueryDatabase`.
/// The `QueryDatabase` enables Salsa memoization when available.
pub fn is_subtype_of_with_db(db: &dyn QueryDatabase, source: TypeId, target: TypeId) -> bool {
    let mut checker = SubtypeChecker::new(db.as_type_database()).with_query_db(db);
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

#[cfg(test)]
#[path = "../../../tests/intrinsic_object_tests.rs"]
mod intrinsic_object_tests;

#[cfg(test)]
mod with_identity_check_mode_tests {
    use super::*;
    use crate::construction::TypeInterner;

    #[test]
    fn restores_flags_after_closure() {
        let interner = TypeInterner::new();
        let mut checker = SubtypeChecker::new(&interner);
        checker.any_propagation = AnyPropagationMode::All;
        checker.identity_cycle_check = false;
        checker.disable_method_bivariance = false;
        checker.strict_function_types = false;

        let inside = checker.with_identity_check_mode(|sub| {
            (
                sub.any_propagation,
                sub.identity_cycle_check,
                sub.disable_method_bivariance,
                sub.strict_function_types,
            )
        });

        assert_eq!(inside.0, AnyPropagationMode::TopLevelOnly);
        assert!(inside.1);
        assert!(inside.2);
        assert!(inside.3);

        assert_eq!(checker.any_propagation, AnyPropagationMode::All);
        assert!(!checker.identity_cycle_check);
        assert!(!checker.disable_method_bivariance);
        assert!(!checker.strict_function_types);
    }
}
