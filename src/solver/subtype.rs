//! Structural subtype checking.
//!
//! This module implements the core logic engine for TypeScript's structural
//! subtyping. It uses coinductive semantics to handle recursive types.
//!
//! Key features:
//! - O(1) equality check via TypeId comparison
//! - Cycle detection for recursive types (coinductive)
//! - Set-theoretic operations for unions and intersections
//! - TypeResolver trait for lazy symbol resolution
//! - Tracer pattern for zero-cost diagnostic abstraction

use crate::limits;
use crate::solver::AssignabilityChecker;
use crate::solver::TypeDatabase;
use crate::solver::diagnostics::SubtypeFailureReason;
use crate::solver::types::*;
use crate::solver::utils;
use std::collections::HashSet;

#[cfg(test)]
use crate::solver::TypeInterner;

/// Maximum recursion depth for subtype checking.
/// This prevents OOM/stack overflow from infinitely expanding recursive types.
/// Examples: `interface AA<T extends AA<T>>`, `interface List<T> { next: List<T> }`
pub(crate) const MAX_SUBTYPE_DEPTH: u32 = limits::MAX_SUBTYPE_DEPTH;

/// Result of a subtype check
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SubtypeResult {
    /// The relationship is definitely true
    True,
    /// The relationship is definitely false
    False,
    /// We're in a cycle and assuming true (provisional)
    Provisional,
}

impl SubtypeResult {
    pub fn is_true(self) -> bool {
        matches!(self, SubtypeResult::True | SubtypeResult::Provisional)
    }

    pub fn is_false(self) -> bool {
        matches!(self, SubtypeResult::False)
    }
}

/// Trait for resolving type references to their structural types.
/// This allows the SubtypeChecker to lazily resolve Ref types
/// without being tightly coupled to the binder/checker.
pub trait TypeResolver {
    /// Resolve a symbol reference to its structural type.
    /// Returns None if the symbol cannot be resolved.
    fn resolve_ref(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId>;

    /// Get type parameters for a symbol (for generic type aliases/interfaces).
    /// Returns None by default; implementations can override to support
    /// Application type expansion.
    fn get_type_params(&self, _symbol: SymbolRef) -> Option<Vec<TypeParamInfo>> {
        None
    }

    /// Get the boxed interface type for a primitive intrinsic (Rule #33).
    /// For example, IntrinsicKind::Number -> TypeId of the Number interface.
    /// This enables primitives to be subtypes of their boxed interfaces.
    fn get_boxed_type(&self, _kind: IntrinsicKind) -> Option<TypeId> {
        None
    }
}

/// A no-op resolver that doesn't resolve any references.
/// Useful for tests or when symbol resolution isn't needed.
pub struct NoopResolver;

impl TypeResolver for NoopResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }
}

/// A type environment that maps symbol refs to their resolved types.
/// This is populated before type checking and passed to the SubtypeChecker.
#[derive(Clone, Debug, Default)]
pub struct TypeEnvironment {
    /// Maps symbol references to their resolved structural types.
    types: std::collections::HashMap<u32, TypeId>,
    /// Maps symbol references to their type parameters (for generic types).
    type_params: std::collections::HashMap<u32, Vec<TypeParamInfo>>,
    /// Maps primitive intrinsic kinds to their boxed interface types (Rule #33).
    /// e.g., IntrinsicKind::Number -> TypeId of the Number interface
    boxed_types: std::collections::HashMap<IntrinsicKind, TypeId>,
}

impl TypeEnvironment {
    pub fn new() -> Self {
        TypeEnvironment {
            types: std::collections::HashMap::new(),
            type_params: std::collections::HashMap::new(),
            boxed_types: std::collections::HashMap::new(),
        }
    }

    /// Register a symbol's resolved type.
    pub fn insert(&mut self, symbol: SymbolRef, type_id: TypeId) {
        self.types.insert(symbol.0, type_id);
    }

    /// Register a boxed type for a primitive (Rule #33).
    /// e.g., set_boxed_type(IntrinsicKind::Number, type_id_of_Number_interface)
    pub fn set_boxed_type(&mut self, kind: IntrinsicKind, type_id: TypeId) {
        self.boxed_types.insert(kind, type_id);
    }

    /// Get the boxed type for a primitive.
    pub fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        self.boxed_types.get(&kind).copied()
    }

    /// Register a symbol's resolved type with type parameters.
    pub fn insert_with_params(
        &mut self,
        symbol: SymbolRef,
        type_id: TypeId,
        params: Vec<TypeParamInfo>,
    ) {
        self.types.insert(symbol.0, type_id);
        if !params.is_empty() {
            self.type_params.insert(symbol.0, params);
        }
    }

    /// Get a symbol's resolved type.
    pub fn get(&self, symbol: SymbolRef) -> Option<TypeId> {
        self.types.get(&symbol.0).copied()
    }

    /// Get a symbol's type parameters.
    pub fn get_params(&self, symbol: SymbolRef) -> Option<&Vec<TypeParamInfo>> {
        self.type_params.get(&symbol.0)
    }

    /// Check if the environment contains a symbol.
    pub fn contains(&self, symbol: SymbolRef) -> bool {
        self.types.contains_key(&symbol.0)
    }

    /// Number of resolved types.
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

impl TypeResolver for TypeEnvironment {
    fn resolve_ref(&self, symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.get(symbol)
    }

    fn get_type_params(&self, symbol: SymbolRef) -> Option<Vec<TypeParamInfo>> {
        self.get_params(symbol).cloned()
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        TypeEnvironment::get_boxed_type(self, kind)
    }
}

/// Maximum number of unique type pairs to track in cycle detection.
/// Prevents unbounded memory growth in pathological cases.
pub const MAX_IN_PROGRESS_PAIRS: usize = limits::MAX_IN_PROGRESS_PAIRS as usize;

/// Subtype checking context.
/// Maintains the "seen" set for cycle detection.
pub struct SubtypeChecker<'a, R: TypeResolver = NoopResolver> {
    pub(crate) interner: &'a dyn TypeDatabase,
    pub(crate) resolver: &'a R,
    /// Active subtype pairs being checked (for cycle detection)
    pub(crate) in_progress: HashSet<(TypeId, TypeId)>,
    /// Current recursion depth (for stack overflow prevention)
    pub(crate) depth: u32,
    /// Total number of check_subtype calls (iteration limit)
    pub(crate) total_checks: u32,
    /// Whether the recursion depth limit was exceeded (for TS2589 diagnostic)
    pub depth_exceeded: bool,
    /// Whether to use strict function types (contravariant parameters).
    /// Default: true (sound, correct behavior)
    pub strict_function_types: bool,
    /// Whether to allow any return type when the target return is void.
    pub allow_void_return: bool,
    /// Whether rest parameters of any/unknown should be treated as bivariant.
    /// See https://github.com/microsoft/TypeScript/issues/20007.
    pub allow_bivariant_rest: bool,
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
}

/// Maximum total subtype checks allowed per SubtypeChecker instance.
/// Prevents infinite loops in pathological type comparison scenarios.
pub const MAX_TOTAL_SUBTYPE_CHECKS: u32 = 100_000;

impl<'a> SubtypeChecker<'a, NoopResolver> {
    /// Create a new SubtypeChecker without a resolver (basic mode).
    pub fn new(interner: &'a dyn TypeDatabase) -> SubtypeChecker<'a, NoopResolver> {
        static NOOP: NoopResolver = NoopResolver;
        SubtypeChecker {
            interner,
            resolver: &NOOP,
            in_progress: HashSet::new(),
            depth: 0,
            total_checks: 0,
            depth_exceeded: false,
            strict_function_types: true, // Default to strict (sound) behavior
            allow_void_return: false,
            allow_bivariant_rest: false,
            allow_bivariant_param_count: false,
            exact_optional_property_types: false,
            strict_null_checks: true,
            no_unchecked_indexed_access: false,
            disable_method_bivariance: false,
        }
    }
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Create a new SubtypeChecker with a custom resolver.
    pub fn with_resolver(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        SubtypeChecker {
            interner,
            resolver,
            in_progress: HashSet::new(),
            depth: 0,
            total_checks: 0,
            depth_exceeded: false,
            strict_function_types: true,
            allow_void_return: false,
            allow_bivariant_rest: false,
            allow_bivariant_param_count: false,
            exact_optional_property_types: false,
            strict_null_checks: true,
            no_unchecked_indexed_access: false,
            disable_method_bivariance: false,
        }
    }

    /// Set whether strict null checks are enabled.
    /// When false, null and undefined are assignable to any type.
    pub fn with_strict_null_checks(mut self, strict_null_checks: bool) -> Self {
        self.strict_null_checks = strict_null_checks;
        self
    }

    pub(crate) fn resolve_ref_type(&self, type_id: TypeId) -> TypeId {
        match self.interner.lookup(type_id) {
            Some(TypeKey::Ref(symbol)) => self
                .resolver
                .resolve_ref(symbol, self.interner)
                .unwrap_or(type_id),
            _ => type_id,
        }
    }

    /// Check if `source` is a subtype of `target`.
    /// This is the main entry point for subtype checking.
    pub fn is_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
        self.check_subtype(source, target).is_true()
    }

    /// Check if `source` is assignable to `target`.
    /// This is a strict structural check; use CompatChecker for TypeScript assignability rules.
    pub fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_subtype_of(source, target)
    }

    /// Internal subtype check with cycle detection
    pub fn check_subtype(&mut self, source: TypeId, target: TypeId) -> SubtypeResult {
        // =========================================================================
        // Fast paths
        // =========================================================================

        // Same type is always a subtype of itself
        if source == target {
            return SubtypeResult::True;
        }

        // Any is assignable to anything
        if source == TypeId::ANY {
            return SubtypeResult::True;
        }

        // Everything is assignable to any
        if target == TypeId::ANY {
            return SubtypeResult::True;
        }

        // Everything is assignable to unknown
        if target == TypeId::UNKNOWN {
            return SubtypeResult::True;
        }

        // Never is assignable to everything
        if source == TypeId::NEVER {
            return SubtypeResult::True;
        }

        // =========================================================================
        // Meta-type evaluation (must happen before NEVER target check)
        // =========================================================================
        // Evaluate meta-types (KeyOf, Conditional, etc.) before the NEVER check
        // because keyof {} = never, and we need to evaluate that first
        let source_eval = self.evaluate_type(source);
        let target_eval = self.evaluate_type(target);

        // If evaluation changed anything, recurse with the simplified types
        if source_eval != source || target_eval != target {
            return self.check_subtype(source_eval, target_eval);
        }

        // =========================================================================
        // Post-evaluation fast paths
        // =========================================================================

        // Nothing (except never) is assignable to never
        if target == TypeId::NEVER {
            return SubtypeResult::False;
        }

        // Error types ARE compatible to suppress cascading errors
        // This treats ERROR as more permissive than Any to avoid error storms
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return SubtypeResult::True;
        }

        // =========================================================================
        // Iteration limit check (timeout prevention)
        // =========================================================================

        self.total_checks += 1;
        if self.total_checks > MAX_TOTAL_SUBTYPE_CHECKS {
            // Too many checks - likely in an infinite expansion scenario
            // Return false to break out of the loop
            self.depth_exceeded = true;
            return SubtypeResult::False;
        }

        // =========================================================================
        // Depth Check (stack overflow prevention)
        // =========================================================================

        if self.depth > MAX_SUBTYPE_DEPTH {
            // Recursion too deep - mark as exceeded and return false to prevent stack overflow
            // The caller can check depth_exceeded to emit TS2589 diagnostic
            // Note: This differs from coinductive cycle detection which returns Provisional
            self.depth_exceeded = true;
            return SubtypeResult::False;
        }

        // =========================================================================
        // Cycle detection (coinduction)
        // =========================================================================

        let pair = (source, target);
        if self.in_progress.contains(&pair) {
            // We're in a cycle - return provisional true
            // This implements coinductive semantics for recursive types
            return SubtypeResult::Provisional;
        }

        // Also check the reversed pair to detect cycles in bivariant parameter checking.
        // When checking bivariant parameters, we check both (A, B) and (B, A), which can
        // create cross-recursion that the normal cycle detection doesn't catch.
        let reversed_pair = (target, source);
        if self.in_progress.contains(&reversed_pair) {
            // We're in a cross-recursion cycle from bivariant checking
            return SubtypeResult::Provisional;
        }

        // Memory safety: limit the number of in-progress pairs to prevent unbounded growth
        if self.in_progress.len() >= MAX_IN_PROGRESS_PAIRS {
            // Too many pairs being tracked - likely pathological case
            self.depth_exceeded = true;
            return SubtypeResult::False;
        }

        // Mark as in-progress and increment depth
        self.in_progress.insert(pair);
        self.depth += 1;

        // Do the actual check
        let result = self.check_subtype_inner(source, target);

        // Remove from in-progress and decrement depth
        self.depth -= 1;
        self.in_progress.remove(&pair);

        result
    }

    /// Inner subtype check (after cycle detection and type evaluation)
    fn check_subtype_inner(&mut self, source: TypeId, target: TypeId) -> SubtypeResult {
        // Types are already evaluated in check_subtype, so no need to re-evaluate here

        if !self.strict_null_checks && (source == TypeId::NULL || source == TypeId::UNDEFINED) {
            return SubtypeResult::True;
        }

        // Look up the type keys
        let source_key = match self.interner.lookup(source) {
            Some(k) => k,
            None => return SubtypeResult::False,
        };
        let target_key = match self.interner.lookup(target) {
            Some(k) => k,
            None => return SubtypeResult::False,
        };

        // Note: Weak type checking is handled by CompatChecker (compat.rs:167-170).
        // Removed redundant check here to avoid double-checking which caused false positives.

        if let Some(shape) = self.apparent_primitive_shape_for_key(&source_key) {
            match &target_key {
                TypeKey::Object(t_shape_id) => {
                    let t_shape = self.interner.object_shape(*t_shape_id);
                    return self.check_object_subtype(&shape.properties, None, &t_shape.properties);
                }
                TypeKey::ObjectWithIndex(t_shape_id) => {
                    let t_shape = self.interner.object_shape(*t_shape_id);
                    return self.check_object_with_index_subtype(&shape, None, &t_shape);
                }
                _ => {}
            }
        }

        if let TypeKey::Conditional(source_cond_id) = &source_key {
            if let TypeKey::Conditional(target_cond_id) = &target_key {
                let source_cond = self.interner.conditional_type(*source_cond_id);
                let target_cond = self.interner.conditional_type(*target_cond_id);
                return self.check_conditional_subtype(source_cond.as_ref(), target_cond.as_ref());
            }

            let source_cond = self.interner.conditional_type(*source_cond_id);
            return self.conditional_branches_subtype(source_cond.as_ref(), target);
        }

        if let TypeKey::Conditional(target_cond_id) = &target_key {
            let target_cond = self.interner.conditional_type(*target_cond_id);
            return self.subtype_of_conditional_target(source, target_cond.as_ref());
        }

        // =========================================================================
        // Structural checks
        // =========================================================================

        match (&source_key, &target_key) {
            // Intrinsic to intrinsic
            (TypeKey::Intrinsic(s), TypeKey::Intrinsic(t)) => self.check_intrinsic_subtype(*s, *t),

            // Rule #33: Primitive to boxed interface (e.g., number to Number)
            // Primitives are subtypes of their boxed wrapper interfaces
            (TypeKey::Intrinsic(s_kind), _) => {
                if self.is_boxed_primitive_subtype(*s_kind, target) {
                    SubtypeResult::True
                } else {
                    SubtypeResult::False
                }
            }

            // Literal to intrinsic
            (TypeKey::Literal(lit), TypeKey::Intrinsic(t)) => {
                self.check_literal_to_intrinsic(lit, *t)
            }

            // Literal to literal
            (TypeKey::Literal(s), TypeKey::Literal(t)) => {
                if s == t {
                    SubtypeResult::True
                } else {
                    SubtypeResult::False
                }
            }

            // Literal string to template literal - check if literal matches pattern
            (TypeKey::Literal(LiteralValue::String(s_lit)), TypeKey::TemplateLiteral(t_spans)) => {
                self.check_literal_matches_template_literal(*s_lit, *t_spans)
            }

            // Union source: all members must be subtypes of target
            (TypeKey::Union(members), _) => {
                self.check_union_source_subtype(*members, target, &target_key)
            }

            // Union target: source must be subtype of at least one member
            (_, TypeKey::Union(members)) => {
                self.check_union_target_subtype(source, &source_key, *members)
            }

            // Intersection source: source is subtype if any constituent is
            (TypeKey::Intersection(members), _) => {
                self.check_intersection_source_subtype(*members, target)
            }

            // Intersection target: all members must be satisfied
            (_, TypeKey::Intersection(members)) => {
                self.check_intersection_target_subtype(source, *members)
            }

            (TypeKey::TypeParameter(s_info), target_key) | (TypeKey::Infer(s_info), target_key) => {
                self.check_type_parameter_subtype(s_info, target, target_key)
            }

            // Rule #31: Base Constraint Assignability - concrete type to TypeParameter
            // source <: T where T is a type parameter
            // TypeScript allows this if source is a subtype of T's base constraint
            (_, TypeKey::TypeParameter(t_info)) | (_, TypeKey::Infer(t_info)) => {
                if let Some(constraint) = t_info.constraint {
                    // Source must be a subtype of T's constraint
                    self.check_subtype(source, constraint)
                } else {
                    // Unconstrained type parameter: any concrete type can't be assigned
                    // to an unconstrained type parameter (T could be anything)
                    SubtypeResult::False
                }
            }

            // object keyword accepts any non-primitive type
            (_, TypeKey::Intrinsic(IntrinsicKind::Object)) => {
                if self.is_object_keyword_type(source) {
                    SubtypeResult::True
                } else {
                    SubtypeResult::False
                }
            }

            // Rule #29: The Global Function type - Intrinsic(Function) as untyped callable supertype
            // Any callable type (function or callable) is a subtype of Function
            (_, TypeKey::Intrinsic(IntrinsicKind::Function)) => {
                if self.is_callable_type(source) {
                    SubtypeResult::True
                } else {
                    SubtypeResult::False
                }
            }

            // Array to array
            (TypeKey::Array(s_elem), TypeKey::Array(t_elem)) => {
                // Arrays are covariant in TypeScript
                self.check_subtype(*s_elem, *t_elem)
            }

            // Tuple to tuple
            (TypeKey::Tuple(s_elems), TypeKey::Tuple(t_elems)) => {
                let s_elems = self.interner.tuple_list(*s_elems);
                let t_elems = self.interner.tuple_list(*t_elems);
                self.check_tuple_subtype(&s_elems, &t_elems)
            }

            // Tuple to array
            (TypeKey::Tuple(elems), TypeKey::Array(t_elem)) => {
                self.check_tuple_to_array_subtype(*elems, *t_elem)
            }

            // Array to tuple (variadic tuples with no required fixed elements only)
            (TypeKey::Array(s_elem), TypeKey::Tuple(t_elems)) => {
                let t_elems = self.interner.tuple_list(*t_elems);
                self.check_array_to_tuple_subtype(*s_elem, &t_elems)
            }

            // Object to object
            (TypeKey::Object(s_shape_id), TypeKey::Object(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.check_object_subtype(
                    &s_shape.properties,
                    Some(*s_shape_id),
                    &t_shape.properties,
                )
            }

            // Object with index to object with index
            (TypeKey::ObjectWithIndex(s_shape_id), TypeKey::ObjectWithIndex(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.check_object_with_index_subtype(&s_shape, Some(*s_shape_id), &t_shape)
            }

            // Object with index to simple object (index signatures can satisfy missing properties)
            (TypeKey::ObjectWithIndex(s_shape_id), TypeKey::Object(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.check_object_with_index_to_object(&s_shape, *s_shape_id, &t_shape.properties)
            }

            // Simple object to object with index
            (TypeKey::Object(s_shape_id), TypeKey::ObjectWithIndex(t_shape_id)) => {
                // All source properties must satisfy target's index signature
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.check_object_to_indexed(&s_shape.properties, Some(*s_shape_id), &t_shape)
            }

            // Function to function
            (TypeKey::Function(s_fn_id), TypeKey::Function(t_fn_id)) => {
                let s_fn = self.interner.function_shape(*s_fn_id);
                let t_fn = self.interner.function_shape(*t_fn_id);
                self.check_function_subtype(&s_fn, &t_fn)
            }

            // Callable to callable (overloaded signatures)
            (TypeKey::Callable(s_callable_id), TypeKey::Callable(t_callable_id)) => {
                let s_callable = self.interner.callable_shape(*s_callable_id);
                let t_callable = self.interner.callable_shape(*t_callable_id);
                self.check_callable_subtype(&s_callable, &t_callable)
            }

            // Function to callable (single signature to overloaded)
            (TypeKey::Function(s_fn_id), TypeKey::Callable(t_callable_id)) => {
                self.check_function_to_callable_subtype(*s_fn_id, *t_callable_id)
            }

            // Callable to function (overloaded to single)
            (TypeKey::Callable(s_callable_id), TypeKey::Function(t_fn_id)) => {
                self.check_callable_to_function_subtype(*s_callable_id, *t_fn_id)
            }

            // Generic application to application
            (TypeKey::Application(s_app_id), TypeKey::Application(t_app_id)) => {
                self.check_application_to_application_subtype(*s_app_id, *t_app_id)
            }

            // Source is Application, target is structural - try to expand and compare
            (TypeKey::Application(app_id), _) => {
                self.check_application_expansion_target(source, target, *app_id)
            }

            // Target is Application, source is structural - try to expand and compare
            (_, TypeKey::Application(app_id)) => {
                self.check_source_to_application_expansion(source, target, *app_id)
            }

            // Source is Mapped, target is structural - try to expand and compare
            (TypeKey::Mapped(mapped_id), _) => {
                self.check_mapped_expansion_target(source, target, *mapped_id)
            }

            // Target is Mapped, source is structural - try to expand and compare
            (_, TypeKey::Mapped(mapped_id)) => {
                self.check_source_to_mapped_expansion(source, target, *mapped_id)
            }

            // Reference types - try to resolve and compare structurally
            (TypeKey::Ref(s_sym), TypeKey::Ref(t_sym)) => {
                self.check_ref_ref_subtype(source, target, s_sym, t_sym)
            }

            // Source is Ref, target is structural - resolve and check
            (TypeKey::Ref(s_sym), _) => self.check_ref_subtype(source, target, s_sym),

            // Source is structural, target is Ref - resolve and check
            (_, TypeKey::Ref(t_sym)) => self.check_to_ref_subtype(source, target, t_sym),

            // Index access types
            (TypeKey::IndexAccess(s_obj, s_idx), TypeKey::IndexAccess(t_obj, t_idx)) => {
                if self.check_subtype(*s_obj, *t_obj).is_true()
                    && self.check_subtype(*s_idx, *t_idx).is_true()
                {
                    SubtypeResult::True
                } else {
                    SubtypeResult::False
                }
            }

            // Type query (typeof) - resolve to structural types when possible
            (TypeKey::TypeQuery(s_sym), TypeKey::TypeQuery(t_sym)) => {
                self.check_typequery_typequery_subtype(source, target, s_sym, t_sym)
            }

            // Source is TypeQuery, target is structural - resolve and check
            (TypeKey::TypeQuery(s_sym), _) => self.check_typequery_subtype(source, target, s_sym),

            // Source is structural, target is TypeQuery - resolve and check
            (_, TypeKey::TypeQuery(t_sym)) => {
                self.check_to_typequery_subtype(source, target, t_sym)
            }

            // KeyOf types - keyof T <: keyof U if T :> U (contravariant)
            (TypeKey::KeyOf(s_inner), TypeKey::KeyOf(t_inner)) => {
                // keyof T <: keyof U when U <: T (contravariant in T)
                self.check_subtype(*t_inner, *s_inner)
            }
            // Note: KeyOf vs Union is handled by the general Union target case above

            // Readonly types - readonly T[] <: readonly U[] if T <: U
            (TypeKey::ReadonlyType(s_inner), TypeKey::ReadonlyType(t_inner)) => {
                self.check_subtype(*s_inner, *t_inner)
            }
            // Readonly array/tuple is NOT assignable to mutable version
            // This must come after the ReadonlyType-ReadonlyType case above
            (TypeKey::ReadonlyType(_), TypeKey::Array(_)) => SubtypeResult::False,
            (TypeKey::ReadonlyType(_), TypeKey::Tuple(_)) => SubtypeResult::False,
            // Mutable arrays/tuples are assignable to readonly versions
            (TypeKey::Array(_), TypeKey::ReadonlyType(t_inner)) => {
                self.check_subtype(source, *t_inner)
            }
            (TypeKey::Tuple(_), TypeKey::ReadonlyType(t_inner)) => {
                self.check_subtype(source, *t_inner)
            }

            // Unique symbol - only equal to itself
            (TypeKey::UniqueSymbol(s_sym), TypeKey::UniqueSymbol(t_sym)) => {
                if s_sym == t_sym {
                    SubtypeResult::True
                } else {
                    SubtypeResult::False
                }
            }
            // Unique symbol is a subtype of symbol
            (TypeKey::UniqueSymbol(_), TypeKey::Intrinsic(IntrinsicKind::Symbol)) => {
                SubtypeResult::True
            }

            // This type - identity only
            (TypeKey::ThisType, TypeKey::ThisType) => SubtypeResult::True,

            // Template literal types - structural comparison
            (TypeKey::TemplateLiteral(s_spans), TypeKey::TemplateLiteral(t_spans)) => {
                if s_spans == t_spans {
                    SubtypeResult::True
                } else {
                    SubtypeResult::False
                }
            }
            // Template literal is a subtype of string
            (TypeKey::TemplateLiteral(_), TypeKey::Intrinsic(IntrinsicKind::String)) => {
                SubtypeResult::True
            }

            // Default: not a subtype
            _ => SubtypeResult::False,
        }
    }
}

// =============================================================================
// Error Explanation API
// =============================================================================

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Explain why `source` is not assignable to `target`.
    ///
    /// This is the "slow path" - called only when `is_assignable_to` returns false
    /// and we need to generate an error message. Re-runs the subtype logic with
    /// tracing enabled to produce a structured failure reason.
    ///
    /// Returns `None` if the types are actually compatible (shouldn't happen
    /// if called correctly after a failed check).
    pub fn explain_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        // Fast path: if types are equal, no failure
        if source == target {
            return None;
        }

        if !self.strict_null_checks && (source == TypeId::NULL || source == TypeId::UNDEFINED) {
            return None;
        }

        // Check for any/unknown/never special cases
        if source == TypeId::ANY || target == TypeId::ANY || target == TypeId::UNKNOWN {
            return None;
        }
        if source == TypeId::NEVER {
            return None;
        }
        // ERROR types should NOT produce diagnostics - suppress cascading errors
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return None;
        }

        // Look up the type keys
        let source_key = self.interner.lookup(source)?;
        let target_key = self.interner.lookup(target)?;

        // Note: Weak type checking is handled by CompatChecker (compat.rs:167-170).
        // Removed redundant check here to avoid double-checking which caused false positives.

        self.explain_failure_inner(source, target, &source_key, &target_key)
    }

    fn explain_failure_inner(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_key: &TypeKey,
        target_key: &TypeKey,
    ) -> Option<SubtypeFailureReason> {
        if let Some(shape) = self.apparent_primitive_shape_for_key(source_key) {
            match target_key {
                TypeKey::Object(t_shape_id) => {
                    let t_shape = self.interner.object_shape(*t_shape_id);
                    return self.explain_object_failure(
                        source,
                        target,
                        &shape.properties,
                        None,
                        &t_shape.properties,
                    );
                }
                TypeKey::ObjectWithIndex(t_shape_id) => {
                    let t_shape = self.interner.object_shape(*t_shape_id);
                    return self
                        .explain_indexed_object_failure(source, target, &shape, None, &t_shape);
                }
                _ => {}
            }
        }

        match (source_key, target_key) {
            // Object to object - find the specific missing/mismatched property
            (TypeKey::Object(s_shape_id), TypeKey::Object(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.explain_object_failure(
                    source,
                    target,
                    &s_shape.properties,
                    Some(*s_shape_id),
                    &t_shape.properties,
                )
            }

            // Object with index to object with index
            (TypeKey::ObjectWithIndex(s_shape_id), TypeKey::ObjectWithIndex(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.explain_indexed_object_failure(
                    source,
                    target,
                    &s_shape,
                    Some(*s_shape_id),
                    &t_shape,
                )
            }

            // Object with index to object
            (TypeKey::ObjectWithIndex(s_shape_id), TypeKey::Object(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.explain_object_with_index_to_object_failure(
                    source,
                    target,
                    &s_shape,
                    *s_shape_id,
                    &t_shape.properties,
                )
            }

            // Simple object to indexed object
            (TypeKey::Object(s_shape_id), TypeKey::ObjectWithIndex(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                if let Some(reason) = self.explain_object_failure(
                    source,
                    target,
                    &s_shape.properties,
                    Some(*s_shape_id),
                    &t_shape.properties,
                ) {
                    return Some(reason);
                }
                // Then check index signature constraints
                if let Some(ref string_idx) = t_shape.string_index {
                    for prop in &s_shape.properties {
                        let prop_type = self.optional_property_type(prop);
                        if !self
                            .check_subtype(prop_type, string_idx.value_type)
                            .is_true()
                        {
                            return Some(SubtypeFailureReason::IndexSignatureMismatch {
                                index_kind: "string",
                                source_value_type: prop_type,
                                target_value_type: string_idx.value_type,
                            });
                        }
                    }
                }
                None
            }

            // Function to function
            (TypeKey::Function(s_fn_id), TypeKey::Function(t_fn_id)) => {
                let s_fn = self.interner.function_shape(*s_fn_id);
                let t_fn = self.interner.function_shape(*t_fn_id);
                self.explain_function_failure(&s_fn, &t_fn)
            }

            // Array to array
            (TypeKey::Array(s_elem), TypeKey::Array(t_elem)) => {
                if !self.check_subtype(*s_elem, *t_elem).is_true() {
                    Some(SubtypeFailureReason::ArrayElementMismatch {
                        source_element: *s_elem,
                        target_element: *t_elem,
                    })
                } else {
                    None
                }
            }

            // Tuple to tuple
            (TypeKey::Tuple(s_elems), TypeKey::Tuple(t_elems)) => {
                let s_elems = self.interner.tuple_list(*s_elems);
                let t_elems = self.interner.tuple_list(*t_elems);
                self.explain_tuple_failure(&s_elems, &t_elems)
            }

            // Union target - source must match at least one member
            (_, TypeKey::Union(members)) => {
                let members = self.interner.type_list(*members);
                Some(SubtypeFailureReason::NoUnionMemberMatches {
                    source_type: source,
                    target_union_members: members.as_ref().to_vec(),
                })
            }

            // Intrinsic to intrinsic mismatch (e.g., string vs number)
            (TypeKey::Intrinsic(s_kind), TypeKey::Intrinsic(t_kind)) => {
                if s_kind != t_kind {
                    Some(SubtypeFailureReason::IntrinsicTypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                } else {
                    None
                }
            }

            // Literal to literal mismatch (e.g., "hello" vs "world")
            (TypeKey::Literal(_), TypeKey::Literal(_)) => {
                Some(SubtypeFailureReason::LiteralTypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            }

            // Literal to incompatible intrinsic (e.g., "hello" vs number)
            (TypeKey::Literal(lit), TypeKey::Intrinsic(t_kind)) => {
                let compatible = match lit {
                    LiteralValue::String(_) => *t_kind == IntrinsicKind::String,
                    LiteralValue::Number(_) => *t_kind == IntrinsicKind::Number,
                    LiteralValue::BigInt(_) => *t_kind == IntrinsicKind::Bigint,
                    LiteralValue::Boolean(_) => *t_kind == IntrinsicKind::Boolean,
                };
                if !compatible {
                    Some(SubtypeFailureReason::LiteralTypeMismatch {
                        source_type: source,
                        target_type: target,
                    })
                } else {
                    None
                }
            }

            // Intrinsic to literal (e.g., string vs "hello") - always incompatible
            (TypeKey::Intrinsic(_), TypeKey::Literal(_)) => {
                Some(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                })
            }

            // Default: generic type mismatch
            _ => Some(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            }),
        }
    }

    /// Explain why an object type assignment failed.
    fn explain_object_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_props: &[PropertyInfo],
        source_shape_id: Option<ObjectShapeId>,
        target_props: &[PropertyInfo],
    ) -> Option<SubtypeFailureReason> {
        for t_prop in target_props {
            let s_prop = self.lookup_property(source_props, source_shape_id, t_prop.name);

            match s_prop {
                Some(sp) => {
                    // Check optional/required mismatch
                    if sp.optional && !t_prop.optional {
                        return Some(SubtypeFailureReason::OptionalPropertyRequired {
                            property_name: t_prop.name,
                        });
                    }
                    // Check readonly mismatch
                    if sp.readonly && !t_prop.readonly {
                        return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                            property_name: t_prop.name,
                        });
                    }

                    // Check property type compatibility
                    let source_type = self.optional_property_type(sp);
                    let target_type = self.optional_property_type(t_prop);
                    let allow_bivariant = sp.is_method || t_prop.is_method;
                    if !self
                        .check_subtype_with_method_variance(
                            source_type,
                            target_type,
                            allow_bivariant,
                        )
                        .is_true()
                    {
                        // Recursively explain the nested failure
                        let nested = self.explain_failure_with_method_variance(
                            source_type,
                            target_type,
                            allow_bivariant,
                        );
                        return Some(SubtypeFailureReason::PropertyTypeMismatch {
                            property_name: t_prop.name,
                            source_property_type: source_type,
                            target_property_type: target_type,
                            nested_reason: nested.map(Box::new),
                        });
                    }
                    if !t_prop.readonly
                        && (sp.write_type != sp.type_id || t_prop.write_type != t_prop.type_id)
                    {
                        let source_write = self.optional_property_write_type(sp);
                        let target_write = self.optional_property_write_type(t_prop);
                        if !self
                            .check_subtype_with_method_variance(
                                target_write,
                                source_write,
                                allow_bivariant,
                            )
                            .is_true()
                        {
                            let nested = self.explain_failure_with_method_variance(
                                target_write,
                                source_write,
                                allow_bivariant,
                            );
                            return Some(SubtypeFailureReason::PropertyTypeMismatch {
                                property_name: t_prop.name,
                                source_property_type: source_write,
                                target_property_type: target_write,
                                nested_reason: nested.map(Box::new),
                            });
                        }
                    }
                }
                None => {
                    // Required property is missing
                    if !t_prop.optional {
                        return Some(SubtypeFailureReason::MissingProperty {
                            property_name: t_prop.name,
                            source_type: source,
                            target_type: target,
                        });
                    }
                }
            }
        }

        None
    }

    /// Explain why an indexed object type assignment failed.
    fn explain_indexed_object_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_shape: &ObjectShape,
        source_shape_id: Option<ObjectShapeId>,
        target_shape: &ObjectShape,
    ) -> Option<SubtypeFailureReason> {
        // First check properties
        if let Some(reason) = self.explain_object_failure(
            source,
            target,
            &source_shape.properties,
            source_shape_id,
            &target_shape.properties,
        ) {
            return Some(reason);
        }

        // Check string index signature
        if let Some(ref t_string_idx) = target_shape.string_index {
            match &source_shape.string_index {
                Some(s_string_idx) => {
                    if s_string_idx.readonly && !t_string_idx.readonly {
                        return Some(SubtypeFailureReason::TypeMismatch {
                            source_type: source,
                            target_type: target,
                        });
                    }
                    if !self
                        .check_subtype(s_string_idx.value_type, t_string_idx.value_type)
                        .is_true()
                    {
                        return Some(SubtypeFailureReason::IndexSignatureMismatch {
                            index_kind: "string",
                            source_value_type: s_string_idx.value_type,
                            target_value_type: t_string_idx.value_type,
                        });
                    }
                }
                None => {
                    for prop in &source_shape.properties {
                        let prop_type = self.optional_property_type(prop);
                        if !self
                            .check_subtype(prop_type, t_string_idx.value_type)
                            .is_true()
                        {
                            return Some(SubtypeFailureReason::IndexSignatureMismatch {
                                index_kind: "string",
                                source_value_type: prop_type,
                                target_value_type: t_string_idx.value_type,
                            });
                        }
                    }
                }
            }
        }

        // Check number index signature
        if let Some(ref t_number_idx) = target_shape.number_index
            && let Some(ref s_number_idx) = source_shape.number_index
        {
            if s_number_idx.readonly && !t_number_idx.readonly {
                return Some(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                });
            }
            if !self
                .check_subtype(s_number_idx.value_type, t_number_idx.value_type)
                .is_true()
            {
                return Some(SubtypeFailureReason::IndexSignatureMismatch {
                    index_kind: "number",
                    source_value_type: s_number_idx.value_type,
                    target_value_type: t_number_idx.value_type,
                });
            }
        }

        if let Some(reason) =
            self.explain_properties_against_index_signatures(&source_shape.properties, target_shape)
        {
            return Some(reason);
        }

        None
    }

    fn explain_object_with_index_to_object_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_shape: &ObjectShape,
        source_shape_id: ObjectShapeId,
        target_props: &[PropertyInfo],
    ) -> Option<SubtypeFailureReason> {
        for t_prop in target_props {
            if let Some(sp) =
                self.lookup_property(&source_shape.properties, Some(source_shape_id), t_prop.name)
            {
                if sp.optional && !t_prop.optional {
                    return Some(SubtypeFailureReason::OptionalPropertyRequired {
                        property_name: t_prop.name,
                    });
                }
                if sp.readonly && !t_prop.readonly {
                    return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                        property_name: t_prop.name,
                    });
                }

                let source_type = self.optional_property_type(sp);
                let target_type = self.optional_property_type(t_prop);
                let allow_bivariant = sp.is_method || t_prop.is_method;
                if !self
                    .check_subtype_with_method_variance(source_type, target_type, allow_bivariant)
                    .is_true()
                {
                    let nested = self.explain_failure_with_method_variance(
                        source_type,
                        target_type,
                        allow_bivariant,
                    );
                    return Some(SubtypeFailureReason::PropertyTypeMismatch {
                        property_name: t_prop.name,
                        source_property_type: source_type,
                        target_property_type: target_type,
                        nested_reason: nested.map(Box::new),
                    });
                }
                if !t_prop.readonly
                    && (sp.write_type != sp.type_id || t_prop.write_type != t_prop.type_id)
                {
                    let source_write = self.optional_property_write_type(sp);
                    let target_write = self.optional_property_write_type(t_prop);
                    if !self
                        .check_subtype_with_method_variance(
                            target_write,
                            source_write,
                            allow_bivariant,
                        )
                        .is_true()
                    {
                        let nested = self.explain_failure_with_method_variance(
                            target_write,
                            source_write,
                            allow_bivariant,
                        );
                        return Some(SubtypeFailureReason::PropertyTypeMismatch {
                            property_name: t_prop.name,
                            source_property_type: source_write,
                            target_property_type: target_write,
                            nested_reason: nested.map(Box::new),
                        });
                    }
                }
                continue;
            }

            let mut checked = false;
            let target_type = self.optional_property_type(t_prop);

            if utils::is_numeric_property_name(self.interner, t_prop.name)
                && let Some(number_idx) = &source_shape.number_index
            {
                checked = true;
                if number_idx.readonly && !t_prop.readonly {
                    return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                        property_name: t_prop.name,
                    });
                }
                if !self
                    .check_subtype_with_method_variance(
                        number_idx.value_type,
                        target_type,
                        t_prop.is_method,
                    )
                    .is_true()
                {
                    return Some(SubtypeFailureReason::IndexSignatureMismatch {
                        index_kind: "number",
                        source_value_type: number_idx.value_type,
                        target_value_type: target_type,
                    });
                }
            }

            if let Some(string_idx) = &source_shape.string_index {
                checked = true;
                if string_idx.readonly && !t_prop.readonly {
                    return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                        property_name: t_prop.name,
                    });
                }
                if !self
                    .check_subtype_with_method_variance(
                        string_idx.value_type,
                        target_type,
                        t_prop.is_method,
                    )
                    .is_true()
                {
                    return Some(SubtypeFailureReason::IndexSignatureMismatch {
                        index_kind: "string",
                        source_value_type: string_idx.value_type,
                        target_value_type: target_type,
                    });
                }
            }

            if !checked && !t_prop.optional {
                return Some(SubtypeFailureReason::MissingProperty {
                    property_name: t_prop.name,
                    source_type: source,
                    target_type: target,
                });
            }
        }

        None
    }

    fn explain_properties_against_index_signatures(
        &mut self,
        source: &[PropertyInfo],
        target: &ObjectShape,
    ) -> Option<SubtypeFailureReason> {
        let string_index = target.string_index.as_ref();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return None;
        }

        for prop in source {
            let prop_type = self.optional_property_type(prop);
            let allow_bivariant = prop.is_method;

            if let Some(number_idx) = number_index {
                let is_numeric = utils::is_numeric_property_name(self.interner, prop.name);
                if is_numeric {
                    if !number_idx.readonly && prop.readonly {
                        return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                            property_name: prop.name,
                        });
                    }
                    if !self
                        .check_subtype_with_method_variance(
                            prop_type,
                            number_idx.value_type,
                            allow_bivariant,
                        )
                        .is_true()
                    {
                        return Some(SubtypeFailureReason::IndexSignatureMismatch {
                            index_kind: "number",
                            source_value_type: prop_type,
                            target_value_type: number_idx.value_type,
                        });
                    }
                }
            }

            if let Some(string_idx) = string_index {
                if !string_idx.readonly && prop.readonly {
                    return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                        property_name: prop.name,
                    });
                }
                if !self
                    .check_subtype_with_method_variance(
                        prop_type,
                        string_idx.value_type,
                        allow_bivariant,
                    )
                    .is_true()
                {
                    return Some(SubtypeFailureReason::IndexSignatureMismatch {
                        index_kind: "string",
                        source_value_type: prop_type,
                        target_value_type: string_idx.value_type,
                    });
                }
            }
        }

        None
    }

    /// Explain why a function type assignment failed.
    fn explain_function_failure(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> Option<SubtypeFailureReason> {
        // Check return type
        if !(self
            .check_subtype(source.return_type, target.return_type)
            .is_true()
            || self.allow_void_return && target.return_type == TypeId::VOID)
        {
            let nested = self.explain_failure(source.return_type, target.return_type);
            return Some(SubtypeFailureReason::ReturnTypeMismatch {
                source_return: source.return_type,
                target_return: target.return_type,
                nested_reason: nested.map(Box::new),
            });
        }

        // Check parameter count
        let target_has_rest = target.params.last().is_some_and(|p| p.rest);
        let rest_elem_type = if target_has_rest {
            target
                .params
                .last()
                .map(|param| self.get_array_element_type(param.type_id))
        } else {
            None
        };
        let rest_is_top = self.allow_bivariant_rest
            && matches!(rest_elem_type, Some(TypeId::ANY | TypeId::UNKNOWN));
        let source_required = self.required_param_count(&source.params);
        let target_required = self.required_param_count(&target.params);
        let extra_required_ok = target_has_rest
            && source_required > target_required
            && self.extra_required_accepts_undefined(
                &source.params,
                target_required,
                source_required,
            );
        let too_many_params = !self.allow_bivariant_param_count
            && !rest_is_top
            && source_required > target_required
            && (!target_has_rest || !extra_required_ok);
        if !target_has_rest && too_many_params {
            return Some(SubtypeFailureReason::TooManyParameters {
                source_count: source_required,
                target_count: target_required,
            });
        }

        // Check parameter types
        let source_has_rest = source.params.last().is_some_and(|p| p.rest);
        let target_fixed_count = if target_has_rest {
            target.params.len().saturating_sub(1)
        } else {
            target.params.len()
        };
        let source_fixed_count = if source_has_rest {
            source.params.len().saturating_sub(1)
        } else {
            source.params.len()
        };
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source.params[i];
            let t_param = &target.params[i];
            // Check parameter compatibility (contravariant in strict mode, bivariant in legacy)
            if !self.are_parameters_compatible(s_param.type_id, t_param.type_id) {
                return Some(SubtypeFailureReason::ParameterTypeMismatch {
                    param_index: i,
                    source_param: s_param.type_id,
                    target_param: t_param.type_id,
                });
            }
        }

        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return None; // Invalid rest parameter
            };
            if rest_is_top {
                return None;
            }

            for i in target_fixed_count..source_fixed_count {
                let s_param = &source.params[i];
                if !self.are_parameters_compatible(s_param.type_id, rest_elem_type) {
                    return Some(SubtypeFailureReason::ParameterTypeMismatch {
                        param_index: i,
                        source_param: s_param.type_id,
                        target_param: rest_elem_type,
                    });
                }
            }

            if source_has_rest {
                let Some(s_rest_param) = source.params.last() else {
                    return None;
                };
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                if !self.are_parameters_compatible(s_rest_elem, rest_elem_type) {
                    return Some(SubtypeFailureReason::ParameterTypeMismatch {
                        param_index: source_fixed_count,
                        source_param: s_rest_elem,
                        target_param: rest_elem_type,
                    });
                }
            }
        }

        if source_has_rest {
            let rest_param = source.params.last()?;
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            let rest_is_top = self.allow_bivariant_rest
                && (rest_elem_type == TypeId::ANY || rest_elem_type == TypeId::UNKNOWN);

            if !rest_is_top {
                for i in source_fixed_count..target_fixed_count {
                    let t_param = &target.params[i];
                    if !self.are_parameters_compatible(rest_elem_type, t_param.type_id) {
                        return Some(SubtypeFailureReason::ParameterTypeMismatch {
                            param_index: i,
                            source_param: rest_elem_type,
                            target_param: t_param.type_id,
                        });
                    }
                }
            }
        }

        if target_has_rest && too_many_params {
            return Some(SubtypeFailureReason::TooManyParameters {
                source_count: source_required,
                target_count: target_required,
            });
        }

        None
    }

    /// Explain why a tuple type assignment failed.
    fn explain_tuple_failure(
        &mut self,
        source: &[TupleElement],
        target: &[TupleElement],
    ) -> Option<SubtypeFailureReason> {
        let source_required = source.iter().filter(|e| !e.optional && !e.rest).count();
        let target_required = target.iter().filter(|e| !e.optional && !e.rest).count();

        if source_required < target_required {
            return Some(SubtypeFailureReason::TupleElementMismatch {
                source_count: source.len(),
                target_count: target.len(),
            });
        }

        for (i, t_elem) in target.iter().enumerate() {
            if t_elem.rest {
                let expansion = self.expand_tuple_rest(t_elem.type_id);
                let outer_tail = &target[i + 1..];
                // Combined suffix = expansion.tail + outer_tail
                let combined_suffix: Vec<_> = expansion
                    .tail
                    .iter()
                    .chain(outer_tail.iter())
                    .cloned()
                    .collect();

                let mut source_end = source.len();
                for tail_elem in combined_suffix.iter().rev() {
                    if source_end <= i {
                        if !tail_elem.optional {
                            return Some(SubtypeFailureReason::TupleElementMismatch {
                                source_count: source.len(),
                                target_count: target.len(),
                            });
                        }
                        break;
                    }
                    let s_elem = &source[source_end - 1];
                    if s_elem.rest {
                        if !tail_elem.optional {
                            return Some(SubtypeFailureReason::TupleElementMismatch {
                                source_count: source.len(),
                                target_count: target.len(),
                            });
                        }
                        break;
                    }
                    let assignable = self
                        .check_subtype(s_elem.type_id, tail_elem.type_id)
                        .is_true();
                    if tail_elem.optional && !assignable {
                        break;
                    }
                    if !assignable {
                        return Some(SubtypeFailureReason::TupleElementTypeMismatch {
                            index: source_end - 1,
                            source_element: s_elem.type_id,
                            target_element: tail_elem.type_id,
                        });
                    }
                    source_end -= 1;
                }

                let mut source_iter = source.iter().enumerate().take(source_end).skip(i);

                for t_fixed in &expansion.fixed {
                    match source_iter.next() {
                        Some((j, s_elem)) => {
                            if s_elem.rest {
                                return Some(SubtypeFailureReason::TupleElementMismatch {
                                    source_count: source.len(),
                                    target_count: target.len(),
                                });
                            }
                            if !self
                                .check_subtype(s_elem.type_id, t_fixed.type_id)
                                .is_true()
                            {
                                return Some(SubtypeFailureReason::TupleElementTypeMismatch {
                                    index: j,
                                    source_element: s_elem.type_id,
                                    target_element: t_fixed.type_id,
                                });
                            }
                        }
                        None => {
                            if !t_fixed.optional {
                                return Some(SubtypeFailureReason::TupleElementMismatch {
                                    source_count: source.len(),
                                    target_count: target.len(),
                                });
                            }
                        }
                    }
                }

                if let Some(variadic) = expansion.variadic {
                    let variadic_array = self.interner.array(variadic);
                    for (j, s_elem) in source_iter {
                        let target_type = if s_elem.rest {
                            variadic_array
                        } else {
                            variadic
                        };
                        if !self.check_subtype(s_elem.type_id, target_type).is_true() {
                            return Some(SubtypeFailureReason::TupleElementTypeMismatch {
                                index: j,
                                source_element: s_elem.type_id,
                                target_element: target_type,
                            });
                        }
                    }
                    return None;
                }

                if source_iter.next().is_some() {
                    return Some(SubtypeFailureReason::TupleElementMismatch {
                        source_count: source.len(),
                        target_count: target.len(),
                    });
                }
                return None;
            }

            if let Some(s_elem) = source.get(i) {
                if s_elem.rest {
                    // Source has rest but target expects fixed element
                    return Some(SubtypeFailureReason::TupleElementMismatch {
                        source_count: source.len(), // Approximate "infinity"
                        target_count: target.len(),
                    });
                }

                if !self.check_subtype(s_elem.type_id, t_elem.type_id).is_true() {
                    return Some(SubtypeFailureReason::TupleElementTypeMismatch {
                        index: i,
                        source_element: s_elem.type_id,
                        target_element: t_elem.type_id,
                    });
                }
            } else if !t_elem.optional {
                return Some(SubtypeFailureReason::TupleElementMismatch {
                    source_count: source.len(),
                    target_count: target.len(),
                });
            }
        }

        // Target is closed. Check for extra elements in source.
        if source.len() > target.len() {
            return Some(SubtypeFailureReason::TupleElementMismatch {
                source_count: source.len(),
                target_count: target.len(),
            });
        }

        for s_elem in source {
            if s_elem.rest {
                return Some(SubtypeFailureReason::TupleElementMismatch {
                    source_count: source.len(), // implies open
                    target_count: target.len(),
                });
            }
        }

        None
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
}

/// Convenience function for one-off subtype checks with a resolver
pub fn is_subtype_of_with_resolver<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
) -> bool {
    let mut checker = SubtypeChecker::with_resolver(interner, resolver);
    checker.is_subtype_of(source, target)
}

// Re-enabled subtype tests - verifying API compatibility
#[cfg(test)]
#[path = "subtype_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "index_signature_tests.rs"]
mod index_signature_tests;

#[cfg(test)]
#[path = "callable_tests.rs"]
mod callable_tests;

#[cfg(test)]
#[path = "union_tests.rs"]
mod union_tests;

#[cfg(test)]
#[path = "typescript_quirks_tests.rs"]
mod typescript_quirks_tests;

#[cfg(test)]
#[path = "type_predicate_tests.rs"]
mod type_predicate_tests;
