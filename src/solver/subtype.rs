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

use crate::interner::Atom;
use crate::solver::types::*;
use crate::solver::utils;
use crate::solver::{
    ApparentMemberKind, AssignabilityChecker, TypeDatabase, apparent_primitive_members,
};
use std::collections::HashSet;

#[cfg(test)]
use crate::solver::TypeInterner;

/// Maximum recursion depth for subtype checking.
/// This prevents OOM/stack overflow from infinitely expanding recursive types.
/// Examples: `interface AA<T extends AA<T>>`, `interface List<T> { next: List<T> }`
pub(crate) const MAX_SUBTYPE_DEPTH: u32 = 100;

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
}

impl TypeEnvironment {
    pub fn new() -> Self {
        TypeEnvironment {
            types: std::collections::HashMap::new(),
            type_params: std::collections::HashMap::new(),
        }
    }

    /// Register a symbol's resolved type.
    pub fn insert(&mut self, symbol: SymbolRef, type_id: TypeId) {
        self.types.insert(symbol.0, type_id);
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
}

struct TupleRestExpansion {
    /// Fixed elements before the variadic portion (prefix)
    fixed: Vec<TupleElement>,
    /// The variadic element type (e.g., T for ...T[])
    variadic: Option<TypeId>,
    /// Fixed elements after the variadic portion (suffix/tail)
    tail: Vec<TupleElement>,
}

/// Maximum number of unique type pairs to track in cycle detection.
/// Prevents unbounded memory growth in pathological cases.
pub const MAX_IN_PROGRESS_PAIRS: usize = 10_000;

/// Subtype checking context.
/// Maintains the "seen" set for cycle detection.
pub struct SubtypeChecker<'a, R: TypeResolver = NoopResolver> {
    interner: &'a dyn TypeDatabase,
    resolver: &'a R,
    /// Active subtype pairs being checked (for cycle detection)
    in_progress: HashSet<(TypeId, TypeId)>,
    /// Current recursion depth (for stack overflow prevention)
    depth: u32,
    /// Total number of check_subtype calls (iteration limit)
    total_checks: u32,
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

        // Error types are NOT compatible (propagate errors instead of silencing)
        // This treats ERROR as more strict than Any/Unknown to catch type errors
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return SubtypeResult::False;
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

    /// Try to expand an Application type to its structural form.
    /// Returns None if the application cannot be expanded (missing type params or body).
    fn try_expand_application(&mut self, app_id: TypeApplicationId) -> Option<TypeId> {
        use crate::solver::{TypeSubstitution, instantiate_type};

        let app = self.interner.type_application(app_id);

        // Look up the base type key
        let base_key = self.interner.lookup(app.base)?;

        // If the base is a Ref, try to resolve and instantiate
        if let TypeKey::Ref(symbol) = base_key {
            // Get type parameters for this symbol
            let type_params = self.resolver.get_type_params(symbol)?;

            // Resolve the base type to get the body
            let resolved = self.resolver.resolve_ref(symbol, self.interner)?;

            // Skip expansion if the resolved type is just this Application
            // (prevents infinite recursion on self-referential types)
            let resolved_key = self.interner.lookup(resolved);
            if let Some(TypeKey::Application(resolved_app_id)) = resolved_key
                && resolved_app_id == app_id
            {
                return None;
            }

            // Create substitution and instantiate
            let substitution = TypeSubstitution::from_args(&type_params, &app.args);
            let instantiated = instantiate_type(self.interner, resolved, &substitution);

            // Return the instantiated type for recursive checking
            Some(instantiated)
        } else {
            // Base is not a Ref - can't expand
            None
        }
    }

    /// Try to expand a Mapped type to its structural form.
    /// Returns None if the mapped type cannot be expanded (unresolvable constraint).
    fn try_expand_mapped(&mut self, mapped_id: MappedTypeId) -> Option<TypeId> {
        use crate::solver::{
            LiteralValue, MappedModifier, PropertyInfo, TypeSubstitution, evaluate_type,
            instantiate_type,
        };

        let mapped = self.interner.mapped_type(mapped_id);

        // Get concrete keys from the constraint
        let keys = self.try_evaluate_mapped_constraint(mapped.constraint)?;
        if keys.is_empty() {
            return None;
        }

        // Check if this is a homomorphic mapped type (template is T[K])
        // If so, we should preserve the original property modifiers
        let is_homomorphic = match self.interner.lookup(mapped.template) {
            Some(TypeKey::IndexAccess(_obj, idx)) => match self.interner.lookup(idx) {
                Some(TypeKey::TypeParameter(param)) => param.name == mapped.type_param.name,
                _ => false,
            },
            _ => false,
        };

        // Extract source object type for homomorphic mapped types
        let source_object = if is_homomorphic {
            match self.interner.lookup(mapped.template) {
                Some(TypeKey::IndexAccess(obj, _idx)) => Some(obj),
                _ => None,
            }
        } else {
            None
        };

        // Helper to get original property modifiers
        let get_original_modifiers = |key_name: crate::interner::Atom| -> (bool, bool) {
            if let Some(source_obj) = source_object {
                if let Some(TypeKey::Object(shape_id)) = self.interner.lookup(source_obj) {
                    let shape = self.interner.object_shape(shape_id);
                    for prop in &shape.properties {
                        if prop.name == key_name {
                            return (prop.optional, prop.readonly);
                        }
                    }
                } else if let Some(TypeKey::ObjectWithIndex(shape_id)) =
                    self.interner.lookup(source_obj)
                {
                    let shape = self.interner.object_shape(shape_id);
                    for prop in &shape.properties {
                        if prop.name == key_name {
                            return (prop.optional, prop.readonly);
                        }
                    }
                }
            }
            (false, false)
        };

        // Build properties by instantiating template for each key
        let mut properties = Vec::new();
        for key_name in keys {
            let key_literal = self
                .interner
                .intern(TypeKey::Literal(LiteralValue::String(key_name)));

            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, key_literal);

            let instantiated_type = instantiate_type(self.interner, mapped.template, &subst);
            // Evaluate the instantiated type to resolve conditionals like T[K] extends object ? ... : T[K]
            let property_type = evaluate_type(self.interner, instantiated_type);

            // Determine modifiers based on mapped type configuration
            let (original_optional, original_readonly) = get_original_modifiers(key_name);
            let optional = match mapped.optional_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => {
                    // For homomorphic types, preserve original
                    if is_homomorphic {
                        original_optional
                    } else {
                        false
                    }
                }
            };
            let readonly = match mapped.readonly_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => {
                    // For homomorphic types, preserve original
                    if is_homomorphic {
                        original_readonly
                    } else {
                        false
                    }
                }
            };

            properties.push(PropertyInfo {
                name: key_name,
                type_id: property_type,
                write_type: property_type,
                optional,
                readonly,
                is_method: false,
            });
        }

        Some(self.interner.object(properties))
    }

    /// Try to evaluate a mapped type constraint to get concrete string keys.
    /// Returns None if the constraint can't be resolved to concrete keys.
    fn try_evaluate_mapped_constraint(
        &self,
        constraint: TypeId,
    ) -> Option<Vec<crate::interner::Atom>> {
        use crate::solver::LiteralValue;

        let key = self.interner.lookup(constraint)?;

        match key {
            TypeKey::KeyOf(operand) => {
                // Try to resolve the operand to get concrete keys
                self.try_get_keyof_keys(operand)
            }
            TypeKey::Literal(LiteralValue::String(name)) => Some(vec![name]),
            TypeKey::Union(list_id) => {
                let members = self.interner.type_list(list_id);
                let mut keys = Vec::new();
                for &member in members.iter() {
                    if let Some(TypeKey::Literal(LiteralValue::String(name))) =
                        self.interner.lookup(member)
                    {
                        keys.push(name);
                    }
                }
                if keys.is_empty() { None } else { Some(keys) }
            }
            _ => None,
        }
    }

    /// Try to get keys from keyof an operand type.
    fn try_get_keyof_keys(&self, operand: TypeId) -> Option<Vec<crate::interner::Atom>> {
        let key = self.interner.lookup(operand)?;

        match key {
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                if shape.properties.is_empty() {
                    return None;
                }
                Some(shape.properties.iter().map(|p| p.name).collect())
            }
            TypeKey::Ref(symbol) => {
                // Try to resolve the ref and get keys from the resolved type
                let resolved = self.resolver.resolve_ref(symbol, self.interner)?;
                if resolved == operand {
                    return None; // Avoid infinite recursion
                }
                self.try_get_keyof_keys(resolved)
            }
            _ => None,
        }
    }

    /// Check intrinsic type to intrinsic type subtyping.
    ///
    /// Handles subtype relationships between TypeScript's built-in primitive types:
    /// - **Same type**: Always compatible (number <: number, string <: string)
    /// - **Void accepts undefined**: undefined <: void (in non-strict mode)
    /// - **Object keyword**: Handled in check_subtype_inner with structural rules
    ///
    /// ## TypeScript Intrinsic Hierarchy:
    /// ```
    /// never <: null <: undefined <: void
    /// never <: bigint <: boolean <: number <: string
    /// never <: object
    /// ```
    ///
    /// Note: Most intrinsic type checking is done via direct equality in check_subtype_inner.
    /// This function handles special cases and the void/undefined relationship.
    fn check_intrinsic_subtype(
        &self,
        source: IntrinsicKind,
        target: IntrinsicKind,
    ) -> SubtypeResult {
        if source == target {
            return SubtypeResult::True;
        }

        // null and undefined are subtypes of their non-strict counterparts
        match (source, target) {
            // void accepts undefined
            (IntrinsicKind::Undefined, IntrinsicKind::Void) => SubtypeResult::True,

            // object keyword handling is in check_subtype_inner
            _ => SubtypeResult::False,
        }
    }

    /// Helper for resolving two Ref/TypeQuery symbols and checking subtype.
    /// Handles the common pattern of:
    /// - Both resolved: check s_type <: t_type
    /// - Only source resolved: check s_type <: target
    /// - Only target resolved: check source <: t_type
    /// - Neither resolved: False
    fn check_resolved_pair_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
        s_resolved: Option<TypeId>,
        t_resolved: Option<TypeId>,
    ) -> SubtypeResult {
        match (s_resolved, t_resolved) {
            (Some(s_type), Some(t_type)) => self.check_subtype(s_type, t_type),
            (Some(s_type), None) => self.check_subtype(s_type, target),
            (None, Some(t_type)) => self.check_subtype(source, t_type),
            (None, None) => SubtypeResult::False,
        }
    }

    /// Check Ref to Ref subtype with optional identity shortcut.
    fn check_ref_ref_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
        s_sym: &SymbolRef,
        t_sym: &SymbolRef,
    ) -> SubtypeResult {
        if s_sym == t_sym {
            return SubtypeResult::True;
        }

        let s_resolved = self.resolver.resolve_ref(*s_sym, self.interner);
        let t_resolved = self.resolver.resolve_ref(*t_sym, self.interner);
        self.check_resolved_pair_subtype(source, target, s_resolved, t_resolved)
    }

    /// Check TypeQuery to TypeQuery subtype with optional identity shortcut.
    fn check_typequery_typequery_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
        s_sym: &SymbolRef,
        t_sym: &SymbolRef,
    ) -> SubtypeResult {
        if s_sym == t_sym {
            return SubtypeResult::True;
        }

        let s_resolved = self.resolver.resolve_ref(*s_sym, self.interner);
        let t_resolved = self.resolver.resolve_ref(*t_sym, self.interner);
        self.check_resolved_pair_subtype(source, target, s_resolved, t_resolved)
    }

    /// Check Ref to structural type subtype.
    ///
    /// When the source type is a nominal reference (Ref), we must resolve it to
    /// its structural type and then check subtyping against the target.
    ///
    /// ## Resolution Process:
    /// 1. Look up the symbol referenced by the Ref
    /// 2. If found, check if the resolved type is a subtype of target
    /// 3. If not found, the types are incompatible
    ///
    /// ## Example:
    /// ```typescript
    /// interface Animal { name: string; }
    /// interface Dog extends Animal { bark(): void; }
    /// let dog: Dog;
    /// let animal: Animal = dog; // Resolves Dog, checks Dog <: Animal
    /// ```
    fn check_ref_subtype(
        &mut self,
        _source: TypeId,
        target: TypeId,
        sym: &SymbolRef,
    ) -> SubtypeResult {
        match self.resolver.resolve_ref(*sym, self.interner) {
            Some(s_resolved) => self.check_subtype(s_resolved, target),
            None => SubtypeResult::False,
        }
    }

    /// Check structural type to Ref subtype.
    ///
    /// When the target type is a nominal reference (Ref), we must resolve it to
    /// its structural type and then check if the source is a subtype of that.
    ///
    /// ## Resolution Process:
    /// 1. Look up the symbol referenced by the target Ref
    /// 2. If found, check if source is a subtype of the resolved type
    /// 3. If not found, the types are incompatible
    ///
    /// ## Example:
    /// ```typescript
    /// interface Animal { name: string; }
    /// interface Dog extends Animal { bark(): void; }
    /// let animal: Animal;
    /// let dog: Dog = animal; // Error: Resolves Dog, checks Animal <: Dog (fails)
    /// ```
    fn check_to_ref_subtype(
        &mut self,
        source: TypeId,
        _target: TypeId,
        sym: &SymbolRef,
    ) -> SubtypeResult {
        match self.resolver.resolve_ref(*sym, self.interner) {
            Some(t_resolved) => self.check_subtype(source, t_resolved),
            None => SubtypeResult::False,
        }
    }

    /// Check TypeQuery to structural type subtype.
    fn check_typequery_subtype(
        &mut self,
        _source: TypeId,
        target: TypeId,
        sym: &SymbolRef,
    ) -> SubtypeResult {
        match self.resolver.resolve_ref(*sym, self.interner) {
            Some(s_resolved) => self.check_subtype(s_resolved, target),
            None => SubtypeResult::False,
        }
    }

    /// Check structural type to TypeQuery subtype.
    fn check_to_typequery_subtype(
        &mut self,
        source: TypeId,
        _target: TypeId,
        sym: &SymbolRef,
    ) -> SubtypeResult {
        match self.resolver.resolve_ref(*sym, self.interner) {
            Some(t_resolved) => self.check_subtype(source, t_resolved),
            None => SubtypeResult::False,
        }
    }

    /// Check Application to Application subtype.
    fn check_application_to_application_subtype(
        &mut self,
        s_app_id: TypeApplicationId,
        t_app_id: TypeApplicationId,
    ) -> SubtypeResult {
        let s_app = self.interner.type_application(s_app_id);
        let t_app = self.interner.type_application(t_app_id);
        if s_app.args.len() != t_app.args.len() {
            return SubtypeResult::False;
        }
        if !self.check_subtype(s_app.base, t_app.base).is_true() {
            return SubtypeResult::False;
        }
        for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
            if !self.check_subtype(*s_arg, *t_arg).is_true() {
                return SubtypeResult::False;
            }
        }
        SubtypeResult::True
    }

    /// Check Application expansion to target (one-sided Application case).
    fn check_application_expansion_target(
        &mut self,
        _source: TypeId,
        target: TypeId,
        app_id: TypeApplicationId,
    ) -> SubtypeResult {
        match self.try_expand_application(app_id) {
            Some(expanded) => self.check_subtype(expanded, target),
            None => SubtypeResult::False,
        }
    }

    /// Check source to Application expansion (one-sided Application case).
    fn check_source_to_application_expansion(
        &mut self,
        source: TypeId,
        _target: TypeId,
        app_id: TypeApplicationId,
    ) -> SubtypeResult {
        match self.try_expand_application(app_id) {
            Some(expanded) => self.check_subtype(source, expanded),
            None => SubtypeResult::False,
        }
    }

    /// Check Mapped expansion to target (one-sided Mapped case).
    fn check_mapped_expansion_target(
        &mut self,
        _source: TypeId,
        target: TypeId,
        mapped_id: MappedTypeId,
    ) -> SubtypeResult {
        match self.try_expand_mapped(mapped_id) {
            Some(expanded) => self.check_subtype(expanded, target),
            None => SubtypeResult::False,
        }
    }

    /// Check source to Mapped expansion (one-sided Mapped case).
    fn check_source_to_mapped_expansion(
        &mut self,
        source: TypeId,
        _target: TypeId,
        mapped_id: MappedTypeId,
    ) -> SubtypeResult {
        match self.try_expand_mapped(mapped_id) {
            Some(expanded) => self.check_subtype(source, expanded),
            None => SubtypeResult::False,
        }
    }

    /// Check conditional type to conditional type subtyping.
    ///
    /// Validates that two conditional types are equivalent in their structure and
    /// that their true/false branches are subtype-compatible.
    ///
    /// ## Conditional Type Structure:
    /// ```typescript
    /// T extends U ? X : Y
    /// ```
    ///
    /// ## Subtyping Rules:
    /// 1. **Distributive flags must match**: Both must be distributive or non-distributive
    ///    - `T extends U ? X : Y` ≢ `T extends U ? A : B` ❌ (different distributivity)
    ///
    /// 2. **Check type must be equivalent**: `check_type` parameters must be the same
    ///    - Extends clause must match structurally
    ///
    /// 3. **Branch compatibility**: Both true and false branches must be compatible
    ///    - `X1 <: X2` AND `Y1 <: Y2`
    ///
    /// ## Examples:
    /// - `T extends string ? number : boolean` ≡ `T extends string ? number : boolean` ✅
    /// - `T extends U ? number` ≢ `T extends U ? string` ❌ (different branches)
    fn check_conditional_subtype(
        &mut self,
        source: &ConditionalType,
        target: &ConditionalType,
    ) -> SubtypeResult {
        if source.is_distributive != target.is_distributive {
            return SubtypeResult::False;
        }

        if !self.types_equivalent(source.check_type, target.check_type) {
            return SubtypeResult::False;
        }

        if !self.types_equivalent(source.extends_type, target.extends_type) {
            return SubtypeResult::False;
        }

        if self
            .check_subtype(source.true_type, target.true_type)
            .is_true()
            && self
                .check_subtype(source.false_type, target.false_type)
                .is_true()
        {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    fn conditional_branches_subtype(
        &mut self,
        cond: &ConditionalType,
        target: TypeId,
    ) -> SubtypeResult {
        if self.check_subtype(cond.true_type, target).is_true()
            && self.check_subtype(cond.false_type, target).is_true()
        {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    fn subtype_of_conditional_target(
        &mut self,
        source: TypeId,
        target: &ConditionalType,
    ) -> SubtypeResult {
        if self.check_subtype(source, target.true_type).is_true()
            && self.check_subtype(source, target.false_type).is_true()
        {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Check if two types are equivalent (bidirectional subtyping).
    ///
    /// Types are equivalent if each is a subtype of the other:
    /// - `left <: right` AND `right <: left`
    ///
    /// This is stronger than simple equality - it handles structural equivalence
    /// for complex types like objects, arrays, etc.
    ///
    /// ## Examples:
    /// - `number` ≡ `number` ✅ (same type)
    /// - `{ x: number }` ≡ `{ x: number }` ✅ (structural equivalence)
    /// - `{ x: number }` ≢ `{ x: number; y: number }` ❌ (different properties)
    /// - `T & U` ≡ `U & T` ✅ (intersection commutes)
    ///
    /// Note: For most type checking, unidirectional subtyping (`<:`) is used.
    /// Equivalence (`≡`) is primarily for type parameter constraints and exact matching.
    fn types_equivalent(&mut self, left: TypeId, right: TypeId) -> bool {
        self.check_subtype(left, right).is_true() && self.check_subtype(right, left).is_true()
    }

    /// Check if a union includes all primitive types (string, number, symbol).
    ///
    /// This is used to optimize `keyof` type checking. When a union contains
    /// all three primitives, `keyof` returns the union of all their keys.
    ///
    /// ## Example:
    /// ```typescript
    /// type T = string | number | symbol;
    /// type Keys = keyof T; // Returns string | number | symbol
    /// ```
    ///
    /// Returns true if all three primitives are present in the union.
    fn union_includes_keyof_primitives(&self, members: TypeListId) -> bool {
        let members = self.interner.type_list(members);
        let mut has_string = false;
        let mut has_number = false;
        let mut has_symbol = false;

        for &member in members.iter() {
            match member {
                TypeId::STRING => has_string = true,
                TypeId::NUMBER => has_number = true,
                TypeId::SYMBOL => has_symbol = true,
                _ => {}
            }
            if has_string && has_number && has_symbol {
                return true;
            }
        }

        false
    }

    /// Check if a type is the "object" keyword type or compatible.
    ///
    /// The "object" keyword type represents any non-primitive type and is
    /// compatible with most types in structural type checking.
    ///
    /// ## Compatible with object keyword:
    /// - `object` (the intrinsic type itself)
    /// - `any` (top type)
    /// - `never` (bottom type)
    /// - `error` (error type)
    /// - Most structural types (objects, arrays, functions, etc.)
    ///
    /// ## NOT compatible with object keyword:
    /// - Primitives (string, number, boolean, bigint, symbol, null, undefined, void)
    ///
    /// This is used in subtype checking to determine when structural typing rules apply.
    fn is_object_keyword_type(&mut self, source: TypeId) -> bool {
        match source {
            TypeId::ANY | TypeId::NEVER | TypeId::ERROR | TypeId::OBJECT => return true,
            TypeId::UNKNOWN
            | TypeId::VOID
            | TypeId::NULL
            | TypeId::UNDEFINED
            | TypeId::BOOLEAN
            | TypeId::NUMBER
            | TypeId::STRING
            | TypeId::BIGINT
            | TypeId::SYMBOL => return false,
            _ => {}
        }

        let key = match self.interner.lookup(source) {
            Some(key) => key,
            None => return false,
        };

        match &key {
            TypeKey::Object(_)
            | TypeKey::ObjectWithIndex(_)
            | TypeKey::Array(_)
            | TypeKey::Tuple(_)
            | TypeKey::Function(_)
            | TypeKey::Callable(_)
            | TypeKey::Mapped(_)
            | TypeKey::Application(_)
            | TypeKey::ThisType => true,
            TypeKey::ReadonlyType(inner) => self.check_subtype(*inner, TypeId::OBJECT).is_true(),
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => match info.constraint {
                Some(constraint) => self.check_subtype(constraint, TypeId::OBJECT).is_true(),
                None => false,
            },
            TypeKey::Ref(sym) => {
                if let Some(resolved) = self.resolver.resolve_ref(*sym, self.interner) {
                    self.check_subtype(resolved, TypeId::OBJECT).is_true()
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if a type is callable (function or callable type).
    /// Rule #29: Function intrinsic accepts any callable type as a subtype.
    fn is_callable_type(&mut self, source: TypeId) -> bool {
        match source {
            TypeId::ANY | TypeId::NEVER | TypeId::ERROR => return true,
            TypeId::FUNCTION => return true,
            _ => {}
        }

        let key = match self.interner.lookup(source) {
            Some(key) => key,
            None => return false,
        };

        match &key {
            TypeKey::Function(_) | TypeKey::Callable(_) => true,
            TypeKey::Union(members) => {
                let members = self.interner.type_list(*members);
                // A union is callable if all members are callable
                members.iter().all(|&m| self.is_callable_type(m))
            }
            TypeKey::Intersection(members) => {
                let members = self.interner.type_list(*members);
                // An intersection is callable if at least one member is callable
                members.iter().any(|&m| self.is_callable_type(m))
            }
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                // Type parameters are not inherently callable without a callable constraint
                match info.constraint {
                    Some(constraint) => self.is_callable_type(constraint),
                    None => false,
                }
            }
            TypeKey::Ref(sym) => {
                if let Some(resolved) = self.resolver.resolve_ref(*sym, self.interner) {
                    self.is_callable_type(resolved)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn apparent_primitive_shape_for_key(&mut self, key: &TypeKey) -> Option<ObjectShape> {
        let kind = self.apparent_primitive_kind(key)?;
        Some(self.apparent_primitive_shape(kind))
    }

    fn apparent_primitive_kind(&self, key: &TypeKey) -> Option<IntrinsicKind> {
        match key {
            TypeKey::Intrinsic(kind) => match kind {
                IntrinsicKind::String
                | IntrinsicKind::Number
                | IntrinsicKind::Boolean
                | IntrinsicKind::Bigint
                | IntrinsicKind::Symbol => Some(*kind),
                _ => None,
            },
            TypeKey::Literal(literal) => match literal {
                LiteralValue::String(_) => Some(IntrinsicKind::String),
                LiteralValue::Number(_) => Some(IntrinsicKind::Number),
                LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
                LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
            },
            TypeKey::TemplateLiteral(_) => Some(IntrinsicKind::String),
            _ => None,
        }
    }

    fn apparent_primitive_shape(&mut self, kind: IntrinsicKind) -> ObjectShape {
        let members = apparent_primitive_members(self.interner, kind);
        let mut properties = Vec::with_capacity(members.len());

        for member in members {
            let name = self.interner.intern_string(member.name);
            match member.kind {
                ApparentMemberKind::Value(type_id) => properties.push(PropertyInfo {
                    name,
                    type_id,
                    write_type: type_id,
                    optional: false,
                    readonly: false,
                    is_method: false,
                }),
                ApparentMemberKind::Method(return_type) => properties.push(PropertyInfo {
                    name,
                    type_id: self.apparent_method_type(return_type),
                    write_type: self.apparent_method_type(return_type),
                    optional: false,
                    readonly: false,
                    is_method: true,
                }),
            }
        }

        let number_index = if kind == IntrinsicKind::String {
            Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::STRING,
                // Keep string index signature assignable to mutable targets for TS compat.
                readonly: false,
            })
        } else {
            None
        };

        ObjectShape {
            properties,
            string_index: None,
            number_index,
        }
    }

    fn apparent_method_type(&mut self, return_type: TypeId) -> TypeId {
        self.interner.function(FunctionShape {
            params: Vec::new(),
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    }

    /// Check literal to intrinsic subtyping
    fn check_literal_to_intrinsic(
        &self,
        literal: &LiteralValue,
        target: IntrinsicKind,
    ) -> SubtypeResult {
        let matches = match literal {
            LiteralValue::String(_) => target == IntrinsicKind::String,
            LiteralValue::Number(_) => target == IntrinsicKind::Number,
            LiteralValue::BigInt(_) => target == IntrinsicKind::Bigint,
            LiteralValue::Boolean(_) => target == IntrinsicKind::Boolean,
        };

        if matches {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Check if a literal string matches a template literal pattern
    /// Uses a backtracking algorithm to handle wildcards (string type holes)
    fn check_literal_matches_template_literal(
        &self,
        literal: Atom,
        template_spans: TemplateLiteralId,
    ) -> SubtypeResult {
        // Get the literal string value
        let literal_str = self.interner.resolve_atom(literal);

        // Get the template literal spans
        let spans = self.interner.template_list(template_spans);

        // Use backtracking to match the literal against the pattern
        if self.match_template_literal_recursive(literal_str.as_str(), &spans, 0) {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Recursively match a string against template literal spans using backtracking
    fn match_template_literal_recursive(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
    ) -> bool {
        // Base case: if we've processed all spans, check if we've consumed the entire string
        if span_idx >= spans.len() {
            return remaining.is_empty();
        }

        match &spans[span_idx] {
            TemplateSpan::Text(text) => {
                let text_str = self.interner.resolve_atom(*text);
                // Check if the remaining string starts with this text
                if remaining.starts_with(text_str.as_str()) {
                    // Continue matching with the rest of the string and spans
                    self.match_template_literal_recursive(
                        &remaining[text_str.len()..],
                        spans,
                        span_idx + 1,
                    )
                } else {
                    false
                }
            }
            TemplateSpan::Type(type_id) => {
                // Determine what kind of pattern this type represents
                match self.interner.lookup(*type_id) {
                    Some(TypeKey::Intrinsic(IntrinsicKind::String)) => {
                        // String type can match any substring (including empty)
                        // Try all possible lengths using backtracking
                        self.match_string_wildcard(remaining, spans, span_idx)
                    }
                    Some(TypeKey::Intrinsic(IntrinsicKind::Number)) => {
                        // Number type matches numeric strings
                        self.match_number_pattern(remaining, spans, span_idx)
                    }
                    Some(TypeKey::Intrinsic(IntrinsicKind::Boolean)) => {
                        // Boolean type matches "true" or "false"
                        self.match_boolean_pattern(remaining, spans, span_idx)
                    }
                    Some(TypeKey::Intrinsic(IntrinsicKind::Bigint)) => {
                        // Bigint type matches integer strings (potentially with n suffix in template context)
                        self.match_bigint_pattern(remaining, spans, span_idx)
                    }
                    Some(TypeKey::Literal(LiteralValue::String(pattern))) => {
                        let pattern_str = self.interner.resolve_atom(pattern);
                        // Literal string must match exactly at this position
                        if remaining.starts_with(pattern_str.as_str()) {
                            self.match_template_literal_recursive(
                                &remaining[pattern_str.len()..],
                                spans,
                                span_idx + 1,
                            )
                        } else {
                            false
                        }
                    }
                    Some(TypeKey::Literal(LiteralValue::Number(num))) => {
                        // Literal number - convert to string and match
                        let num_str = format_number_for_template(num.0);
                        if remaining.starts_with(&num_str) {
                            self.match_template_literal_recursive(
                                &remaining[num_str.len()..],
                                spans,
                                span_idx + 1,
                            )
                        } else {
                            false
                        }
                    }
                    Some(TypeKey::Literal(LiteralValue::Boolean(b))) => {
                        // Literal boolean - convert to string and match
                        let bool_str = if b { "true" } else { "false" };
                        if remaining.starts_with(bool_str) {
                            self.match_template_literal_recursive(
                                &remaining[bool_str.len()..],
                                spans,
                                span_idx + 1,
                            )
                        } else {
                            false
                        }
                    }
                    Some(TypeKey::Literal(LiteralValue::BigInt(n))) => {
                        // Literal bigint - convert to string and match
                        let bigint_str = self.interner.resolve_atom(n);
                        if remaining.starts_with(bigint_str.as_str()) {
                            self.match_template_literal_recursive(
                                &remaining[bigint_str.len()..],
                                spans,
                                span_idx + 1,
                            )
                        } else {
                            false
                        }
                    }
                    Some(TypeKey::Union(members)) => {
                        // For unions, try each member
                        self.match_union_pattern(remaining, spans, span_idx, members)
                    }
                    _ => {
                        // For other types, check if they're string-compatible
                        match self.apparent_primitive_kind_for_type(*type_id) {
                            Some(IntrinsicKind::String) => {
                                // Treat as string wildcard
                                self.match_string_wildcard(remaining, spans, span_idx)
                            }
                            Some(IntrinsicKind::Number) => {
                                self.match_number_pattern(remaining, spans, span_idx)
                            }
                            Some(IntrinsicKind::Boolean) => {
                                self.match_boolean_pattern(remaining, spans, span_idx)
                            }
                            _ => false,
                        }
                    }
                }
            }
        }
    }

    /// Match a string wildcard using backtracking
    /// Tries all possible lengths from 0 to remaining.len()
    fn match_string_wildcard(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
    ) -> bool {
        let is_last_span = span_idx == spans.len() - 1;

        // If this is the last span, any remaining string is valid
        if is_last_span {
            return true;
        }

        // Find the next text span to use as an anchor for optimization
        if let Some(next_text_pos) = self.find_next_text_span(spans, span_idx + 1) {
            if let TemplateSpan::Text(text) = &spans[next_text_pos] {
                let text_str = self.interner.resolve_atom(*text);
                // Optimization: only try positions where the next text could match
                for match_pos in remaining.match_indices(text_str.as_str()) {
                    // Try matching from this position
                    if self.match_template_literal_recursive(
                        &remaining[match_pos.0..],
                        spans,
                        span_idx + 1,
                    ) {
                        return true;
                    }
                }
                // Also try if the pattern can match with empty wildcard
                // (in case the next span is also a type that could consume the text)
                if self.match_template_literal_recursive(remaining, spans, span_idx + 1) {
                    return true;
                }
                return false;
            }
        }

        // No optimization available, try all possible lengths
        for len in 0..=remaining.len() {
            if self.match_template_literal_recursive(&remaining[len..], spans, span_idx + 1) {
                return true;
            }
        }
        false
    }

    /// Find the next text span after the given index
    fn find_next_text_span(&self, spans: &[TemplateSpan], start_idx: usize) -> Option<usize> {
        for i in start_idx..spans.len() {
            if matches!(spans[i], TemplateSpan::Text(_)) {
                return Some(i);
            }
        }
        None
    }

    /// Match a number pattern - matches valid numeric strings
    fn match_number_pattern(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
    ) -> bool {
        let is_last_span = span_idx == spans.len() - 1;

        // Find the longest valid number at the start of remaining
        let num_len = find_number_length(remaining);

        if num_len == 0 {
            // No valid number found, but empty match might be valid for last span
            if is_last_span {
                return remaining.is_empty();
            }
            return false;
        }

        // Try all valid number lengths from longest to shortest
        for len in (1..=num_len).rev() {
            // Verify this is a valid number
            if is_valid_number(&remaining[..len]) {
                if self.match_template_literal_recursive(&remaining[len..], spans, span_idx + 1) {
                    return true;
                }
            }
        }

        false
    }

    /// Match a boolean pattern - matches "true" or "false"
    fn match_boolean_pattern(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
    ) -> bool {
        // Try "true"
        if remaining.starts_with("true") {
            if self.match_template_literal_recursive(&remaining[4..], spans, span_idx + 1) {
                return true;
            }
        }
        // Try "false"
        if remaining.starts_with("false") {
            if self.match_template_literal_recursive(&remaining[5..], spans, span_idx + 1) {
                return true;
            }
        }
        false
    }

    /// Match a bigint pattern - matches integer strings
    fn match_bigint_pattern(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
    ) -> bool {
        let is_last_span = span_idx == spans.len() - 1;

        // Find the longest valid bigint at the start of remaining
        let int_len = find_integer_length(remaining);

        if int_len == 0 {
            if is_last_span {
                return remaining.is_empty();
            }
            return false;
        }

        // Try all valid integer lengths from longest to shortest
        for len in (1..=int_len).rev() {
            if self.match_template_literal_recursive(&remaining[len..], spans, span_idx + 1) {
                return true;
            }
        }

        false
    }

    /// Match a union pattern - try each member of the union
    fn match_union_pattern(
        &self,
        remaining: &str,
        spans: &[TemplateSpan],
        span_idx: usize,
        members: TypeListId,
    ) -> bool {
        let members = self.interner.type_list(members);

        for &member in members.iter() {
            match self.interner.lookup(member) {
                Some(TypeKey::Literal(LiteralValue::String(pattern))) => {
                    let pattern_str = self.interner.resolve_atom(pattern);
                    if remaining.starts_with(pattern_str.as_str()) {
                        if self.match_template_literal_recursive(
                            &remaining[pattern_str.len()..],
                            spans,
                            span_idx + 1,
                        ) {
                            return true;
                        }
                    }
                }
                Some(TypeKey::Literal(LiteralValue::Number(num))) => {
                    let num_str = format_number_for_template(num.0);
                    if remaining.starts_with(&num_str) {
                        if self.match_template_literal_recursive(
                            &remaining[num_str.len()..],
                            spans,
                            span_idx + 1,
                        ) {
                            return true;
                        }
                    }
                }
                Some(TypeKey::Literal(LiteralValue::BigInt(n))) => {
                    let bigint_str = self.interner.resolve_atom(n);
                    if remaining.starts_with(bigint_str.as_str()) {
                        if self.match_template_literal_recursive(
                            &remaining[bigint_str.len()..],
                            spans,
                            span_idx + 1,
                        ) {
                            return true;
                        }
                    }
                }
                Some(TypeKey::Literal(LiteralValue::Boolean(b))) => {
                    let bool_str = if b { "true" } else { "false" };
                    if remaining.starts_with(bool_str) {
                        if self.match_template_literal_recursive(
                            &remaining[bool_str.len()..],
                            spans,
                            span_idx + 1,
                        ) {
                            return true;
                        }
                    }
                }
                Some(TypeKey::Intrinsic(IntrinsicKind::String)) => {
                    // String in union acts as a wildcard
                    if self.match_string_wildcard(remaining, spans, span_idx) {
                        return true;
                    }
                }
                Some(TypeKey::Intrinsic(IntrinsicKind::Number)) => {
                    if self.match_number_pattern(remaining, spans, span_idx) {
                        return true;
                    }
                }
                Some(TypeKey::Intrinsic(IntrinsicKind::Boolean)) => {
                    if self.match_boolean_pattern(remaining, spans, span_idx) {
                        return true;
                    }
                }
                _ => {
                    // For other types, check primitive kind
                    match self.apparent_primitive_kind_for_type(member) {
                        Some(IntrinsicKind::String) => {
                            if self.match_string_wildcard(remaining, spans, span_idx) {
                                return true;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        false
    }

    /// Get the apparent primitive kind for a type (helper for template literal checking)
    fn apparent_primitive_kind_for_type(&self, type_id: TypeId) -> Option<IntrinsicKind> {
        let key = self.interner.lookup(type_id);
        match key {
            Some(TypeKey::Intrinsic(kind)) => Some(kind),
            Some(TypeKey::Literal(literal)) => match literal {
                LiteralValue::String(_) => Some(IntrinsicKind::String),
                LiteralValue::Number(_) => Some(IntrinsicKind::Number),
                LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
                LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
            },
            Some(TypeKey::TemplateLiteral(_)) => Some(IntrinsicKind::String),
            _ => None,
        }
    }

    /// Check tuple subtyping.
    ///
    /// Validates structural compatibility between tuple types, handling:
    /// - Required element count matching (source must have ≥ required elements than target)
    /// - Fixed element type compatibility (positional checking)
    /// - Rest element handling (variadic tuples, e.g., [...string[]])
    /// - Optional element compatibility
    /// - Closed tuple constraints (source can't exceed target's length)
    ///
    /// ## Tuple Subtyping Rules:
    /// 1. **Required elements**: Source must have at least as many required (non-optional) elements
    /// 2. **Rest elements**: When target has a rest element, source must match the expanded pattern
    /// 3. **Closed tuples**: If target has no rest, source can't have extra elements
    /// 4. **Type compatibility**: Each element type must be a subtype of the corresponding target
    ///
    /// ## Examples:
    /// - `[number, string]` ≤ `[number, string, boolean]` ✅
    /// - `[number, ...string[]]` ≤ `[number, ...string[]]` ✅
    /// - `[number, string]` ≤ `[number]` ❌ (extra element)
    /// - `[number]` ≤ `[number, string]` ❌ (missing element)
    fn check_tuple_subtype(
        &mut self,
        source: &[TupleElement],
        target: &[TupleElement],
    ) -> SubtypeResult {
        // Count required elements
        let source_required = source.iter().filter(|e| !e.optional && !e.rest).count();
        let target_required = target.iter().filter(|e| !e.optional && !e.rest).count();

        // Source must have at least as many required elements
        if source_required < target_required {
            return SubtypeResult::False;
        }

        // Check each element
        for (i, t_elem) in target.iter().enumerate() {
            if t_elem.rest {
                let expansion = self.expand_tuple_rest(t_elem.type_id);
                let outer_tail = &target[i + 1..];
                // Combined suffix = expansion.tail + outer_tail
                // We need to match these from the end of the source tuple
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
                            return SubtypeResult::False;
                        }
                        break;
                    }
                    let s_elem = &source[source_end - 1];
                    if s_elem.rest {
                        if !tail_elem.optional {
                            return SubtypeResult::False;
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
                        return SubtypeResult::False;
                    }
                    source_end -= 1;
                }

                let mut source_iter = source.iter().enumerate().take(source_end).skip(i);

                for t_fixed in &expansion.fixed {
                    match source_iter.next() {
                        Some((_, s_elem)) => {
                            if s_elem.rest {
                                return SubtypeResult::False;
                            }
                            if !self
                                .check_subtype(s_elem.type_id, t_fixed.type_id)
                                .is_true()
                            {
                                return SubtypeResult::False;
                            }
                        }
                        None => {
                            if !t_fixed.optional {
                                return SubtypeResult::False;
                            }
                        }
                    }
                }

                if let Some(variadic) = expansion.variadic {
                    let variadic_array = self.interner.array(variadic);
                    for (_, s_elem) in source_iter {
                        if s_elem.rest {
                            if !self.check_subtype(s_elem.type_id, variadic_array).is_true() {
                                return SubtypeResult::False;
                            }
                        } else if !self.check_subtype(s_elem.type_id, variadic).is_true() {
                            return SubtypeResult::False;
                        }
                    }
                    return SubtypeResult::True;
                }

                if source_iter.next().is_some() {
                    return SubtypeResult::False;
                }
                return SubtypeResult::True;
            }

            // Target is not rest
            if let Some(s_elem) = source.get(i) {
                if s_elem.rest {
                    // Source has rest but target expects fixed element -> Mismatch
                    // e.g. Target: [number, number], Source: [number, ...number[]]
                    return SubtypeResult::False;
                }

                if !self.check_subtype(s_elem.type_id, t_elem.type_id).is_true() {
                    return SubtypeResult::False;
                }
            } else if !t_elem.optional {
                // Missing required element
                return SubtypeResult::False;
            }
        }

        // If we reached here, target has NO rest element (it is closed).
        // Ensure source has no extra elements.

        // 1. Source length check: Source cannot have more elements than Target
        if source.len() > target.len() {
            return SubtypeResult::False;
        }

        // 2. Source open check: Source cannot have a rest element if Target is closed
        for s_elem in source {
            if s_elem.rest {
                return SubtypeResult::False;
            }
        }

        SubtypeResult::True
    }

    fn check_array_to_tuple_subtype(
        &mut self,
        source_elem: TypeId,
        target: &[TupleElement],
    ) -> SubtypeResult {
        // TypeScript semantics: Arrays (T[]) are generally NOT assignable to tuple types,
        // even variadic tuples like [...T[]], because tuples have specific structural
        // constraints that arrays don't satisfy.
        //
        // The ONLY exception is never[] which represents an empty array and can be
        // assigned to any tuple that allows empty (has no required elements).
        //
        // Cases:
        // - never[] -> [] : Yes (empty array to empty tuple)
        // - never[] -> [string?] : Yes (empty array to optional-only tuple)
        // - never[] -> [...string[]] : Yes (empty array to variadic tuple)
        // - never[] -> [string] : No (empty array cannot satisfy required element)
        // - string[] -> [...string[]] : No (arrays are not assignable to tuples)
        // - string[] -> [string?] : No (arrays are not assignable to tuples)

        // Only never[] can potentially be assigned to tuples
        if source_elem != TypeId::NEVER {
            return SubtypeResult::False;
        }

        // never[] can be assigned to a tuple if and only if the tuple allows empty
        if self.tuple_allows_empty(target) {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Check if a tuple type allows empty arrays.
    ///
    /// Determines whether `never[]` (empty array) can be assigned to a tuple type.
    /// A tuple allows empty if ALL of its elements are optional or it has a rest element
    /// with no required trailing elements.
    ///
    /// ## Examples:
    /// - `[]` ✅ - Empty tuple allows empty array
    /// - `[string?]` ✅ - Only optional element
    /// - `[string]` ❌ - Required element
    /// - `[...string[]]` ✅ - Rest element allows any number including zero
    /// - `[...string[], number]` ❌ - Required trailing element after rest
    ///
    /// ## Nested Tuple Spreads:
    /// When a rest element contains a nested tuple spread, we recursively check
    /// both the fixed elements and tail elements of the expansion.
    fn tuple_allows_empty(&self, target: &[TupleElement]) -> bool {
        for (index, elem) in target.iter().enumerate() {
            if elem.rest {
                // Check if there are any REQUIRED elements after the rest element
                // e.g., [...string[], number] has a required trailing element
                // but [...string[], number?] only has optional trailing elements
                let tail = &target[index + 1..];
                if tail.iter().any(|tail_elem| !tail_elem.optional) {
                    return false;
                }

                // Check the expanded rest element for required fixed elements
                let expansion = self.expand_tuple_rest(elem.type_id);
                if expansion.fixed.iter().any(|fixed| !fixed.optional) {
                    return false;
                }

                // Check tail elements from nested tuple spreads
                if expansion.tail.iter().any(|tail_elem| !tail_elem.optional) {
                    return false;
                }

                // Tuple with rest element allows empty if:
                // 1. No required trailing elements after the rest
                // 2. The rest expansion has no required fixed elements
                // 3. The expansion has no required tail elements
                return true;
            }

            if !elem.optional {
                return false;
            }
        }

        true
    }

    // Note: violates_weak_type, violates_weak_type_with_target_props, and has_common_property
    // were removed as dead code. Weak type checking is now handled exclusively by CompatChecker
    // (see compat.rs:167-170 and compat.rs:289-481) to avoid double-checking which caused
    // false positives (TS2322).

    fn lookup_property<'props>(
        &self,
        props: &'props [PropertyInfo],
        shape_id: Option<ObjectShapeId>,
        name: Atom,
    ) -> Option<&'props PropertyInfo> {
        if let Some(shape_id) = shape_id {
            match self.interner.object_property_index(shape_id, name) {
                PropertyLookup::Found(idx) => return props.get(idx),
                PropertyLookup::NotFound => return None,
                PropertyLookup::Uncached => {}
            }
        }
        props.iter().find(|p| p.name == name)
    }

    /// Check private brand compatibility for object subtyping.
    ///
    /// Private brands are used for nominal typing of classes with private fields.
    /// If both source and target have private brands, they must be the same.
    /// Returns false if brands don't match, true otherwise (including when neither has a brand).
    fn check_private_brand_compatibility(
        &self,
        source: &[PropertyInfo],
        target: &[PropertyInfo],
    ) -> bool {
        let source_brand = source.iter().find(|p| {
            let name = self.interner.resolve_atom(p.name);
            name.starts_with("__private_brand_")
        });
        let target_brand = target.iter().find(|p| {
            let name = self.interner.resolve_atom(p.name);
            name.starts_with("__private_brand_")
        });

        // If both have private brands (both are classes with private fields), check they match
        match (source_brand, target_brand) {
            (Some(s_brand), Some(t_brand)) => {
                let s_brand_name = self.interner.resolve_atom(s_brand.name);
                let t_brand_name = self.interner.resolve_atom(t_brand.name);
                s_brand_name == t_brand_name
            }
            _ => true, // If at least one doesn't have a brand, no conflict
        }
    }

    /// Check object subtyping (structural)
    fn check_object_subtype(
        &mut self,
        source: &[PropertyInfo],
        source_shape_id: Option<ObjectShapeId>,
        target: &[PropertyInfo],
    ) -> SubtypeResult {
        // Private brand checking for nominal typing of classes with private fields
        if !self.check_private_brand_compatibility(source, target) {
            return SubtypeResult::False;
        }

        // For each property in target, source must have a compatible property
        for t_prop in target {
            let s_prop = self.lookup_property(source, source_shape_id, t_prop.name);

            let result = match s_prop {
                Some(sp) => self.check_property_compatibility(sp, t_prop),
                None => {
                    // Property missing - only OK if target property is optional
                    if t_prop.optional {
                        SubtypeResult::True
                    } else {
                        SubtypeResult::False
                    }
                }
            };

            if !result.is_true() {
                return result;
            }
        }

        SubtypeResult::True
    }

    /// Check if a source property is compatible with a target property.
    ///
    /// This validates property compatibility for structural object subtyping:
    ///
    /// ## Rules:
    /// 1. **Optional compatibility**: Optional in source can't satisfy required in target
    ///    - `{ x?: number }` ≤ `{ x: number }` ❌
    ///    - `{ x: number }` ≤ `{ x?: number }` ✅
    ///
    /// 2. **Readonly compatibility**: Readonly in source can't satisfy mutable in target
    ///    - `{ readonly x: number }` ≤ `{ x: number }` ❌
    ///    - `{ x: number }` ≤ `{ readonly x: number }` ✅
    ///
    /// 3. **Type compatibility**: Source type must be subtype of target type
    ///    - Methods use bivariant checking (both directions)
    ///    - Properties use contravariant checking
    ///
    /// 4. **Write type compatibility**: For mutable properties with different write types,
    ///    target's write type must be subtype of source's (contravariance for writes)
    ///
    /// This validates:
    /// - Optional compatibility (optional source can't satisfy required target)
    /// - Readonly compatibility (readonly source can't satisfy mutable target)
    /// - Type compatibility (including write types for non-readonly properties)
    fn check_property_compatibility(
        &mut self,
        source: &PropertyInfo,
        target: &PropertyInfo,
    ) -> SubtypeResult {
        // Check optional compatibility
        // Optional in source can't satisfy required in target
        if source.optional && !target.optional {
            return SubtypeResult::False;
        }

        // Readonly in source can't satisfy mutable target
        if source.readonly && !target.readonly {
            return SubtypeResult::False;
        }

        // Property exists, check type compatibility
        let source_type = self.optional_property_type(source);
        let target_type = self.optional_property_type(target);
        let allow_bivariant = source.is_method || target.is_method;

        if !self
            .check_subtype_with_method_variance(source_type, target_type, allow_bivariant)
            .is_true()
        {
            return SubtypeResult::False;
        }

        // Check write type compatibility for non-readonly properties with different write types
        if !target.readonly
            && (source.write_type != source.type_id || target.write_type != target.type_id)
        {
            let source_write = self.optional_property_write_type(source);
            let target_write = self.optional_property_write_type(target);
            if !self
                .check_subtype_with_method_variance(target_write, source_write, allow_bivariant)
                .is_true()
            {
                return SubtypeResult::False;
            }
        }

        SubtypeResult::True
    }

    /// Check string index signature compatibility between source and target.
    ///
    /// Validates that string index signatures are compatible, handling:
    /// - Source has string index → must be subtype of target's index
    /// - Source lacks string index → all source properties must be compatible with target's index
    fn check_string_index_compatibility(
        &mut self,
        source: &ObjectShape,
        target: &ObjectShape,
    ) -> SubtypeResult {
        let Some(ref t_string_idx) = target.string_index else {
            return SubtypeResult::True; // Target has no string index constraint
        };

        match &source.string_index {
            Some(s_string_idx) => {
                // Source string index must be subtype of target
                if s_string_idx.readonly && !t_string_idx.readonly {
                    return SubtypeResult::False;
                }
                if !self
                    .check_subtype(s_string_idx.value_type, t_string_idx.value_type)
                    .is_true()
                {
                    return SubtypeResult::False;
                }
                SubtypeResult::True
            }
            None => {
                // Target has string index, source doesn't
                // All source properties must be compatible with target's string index
                for prop in &source.properties {
                    if !t_string_idx.readonly && prop.readonly {
                        return SubtypeResult::False;
                    }
                    let prop_type = self.optional_property_type(prop);
                    if !self
                        .check_subtype(prop_type, t_string_idx.value_type)
                        .is_true()
                    {
                        return SubtypeResult::False;
                    }
                }
                SubtypeResult::True
            }
        }
    }

    /// Check number index signature compatibility between source and target.
    ///
    /// Validates that number index signatures are compatible.
    /// Number indexing is optional, so it's OK if source lacks a number index.
    fn check_number_index_compatibility(
        &mut self,
        source: &ObjectShape,
        target: &ObjectShape,
    ) -> SubtypeResult {
        let Some(ref t_number_idx) = target.number_index else {
            return SubtypeResult::True; // Target has no number index constraint
        };

        match &source.number_index {
            Some(s_number_idx) => {
                // Source number index must be subtype of target
                if s_number_idx.readonly && !t_number_idx.readonly {
                    return SubtypeResult::False;
                }
                if !self
                    .check_subtype(s_number_idx.value_type, t_number_idx.value_type)
                    .is_true()
                {
                    return SubtypeResult::False;
                }
                SubtypeResult::True
            }
            None => {
                // Target has number index but source doesn't - this is OK
                // (number indexing is optional)
                SubtypeResult::True
            }
        }
    }

    /// Check object with index signature subtyping.
    ///
    /// Validates subtype compatibility between two objects that both have index signatures.
    /// This requires:
    /// 1. Named property compatibility (all target properties must exist in source)
    /// 2. String index signature compatibility (source index must satisfy target index)
    /// 3. Number index signature compatibility (source index must satisfy target index)
    /// 4. All source properties must be compatible with target index signatures
    /// 5. If source has both string and number indexes, they must be compatible
    ///
    /// ## Index Signature Rules:
    /// - Source index signature type must be subtype of target index signature type
    /// - Readonly in source cannot satisfy mutable in target
    /// - Number-indexed properties also checked against string index (number → string)
    ///
    /// ## Example:
    /// ```typescript
    /// interface Target {
    ///   [key: string]: number;
    ///   [index: number]: string;
    /// }
    /// interface Source {
    ///   [key: string]: number;  // Must satisfy Target's string index
    ///   [index: number]: string;  // Must satisfy Target's number index
    /// }
    /// ```
    fn check_object_with_index_subtype(
        &mut self,
        source: &ObjectShape,
        source_shape_id: Option<ObjectShapeId>,
        target: &ObjectShape,
    ) -> SubtypeResult {
        // First check named properties
        if !self
            .check_object_subtype(&source.properties, source_shape_id, &target.properties)
            .is_true()
        {
            return SubtypeResult::False;
        }

        // Check string index signature compatibility
        if !self
            .check_string_index_compatibility(source, target)
            .is_true()
        {
            return SubtypeResult::False;
        }

        // Check number index signature compatibility
        if !self
            .check_number_index_compatibility(source, target)
            .is_true()
        {
            return SubtypeResult::False;
        }

        if !self
            .check_properties_against_index_signatures(&source.properties, target)
            .is_true()
        {
            return SubtypeResult::False;
        }

        // If source has string index, all number-indexed properties must be compatible
        // (since number converts to string for property access)
        if let (Some(s_string_idx), Some(s_number_idx)) =
            (&source.string_index, &source.number_index)
            && !self
                .check_subtype(s_number_idx.value_type, s_string_idx.value_type)
                .is_true()
        {
            // This is a constraint violation in the source itself
            return SubtypeResult::False;
        }

        SubtypeResult::True
    }

    /// Check object with index signature to plain object subtyping.
    ///
    /// Validates that a source object with an index signature can be a subtype of
    /// a target object with only named properties. For each target property:
    /// 1. Look up the property by name in source (including via index signatures)
    /// 2. Check property compatibility (optional, readonly, type, write_type)
    /// 3. If property not found in source, check if index signature can satisfy it
    ///
    /// ## Key Difference:
    /// This is the reverse of `check_object_to_indexed` - the SOURCE has the index
    /// signature and the TARGET has only named properties.
    ///
    /// ## Example:
    /// ```typescript
    /// interface Source {
    ///   [key: string]: number;  // Index signature in source
    ///   x: string;
    /// }
    /// interface Target {
    ///   x: string;  // Named property in target
    ///   y: number;  // Must be satisfied by Source's index signature
    /// }
    /// // Source ≤ Target because Source's [key: string]: number can satisfy Target.y
    /// ```
    fn check_object_with_index_to_object(
        &mut self,
        source: &ObjectShape,
        source_shape_id: ObjectShapeId,
        target: &[PropertyInfo],
    ) -> SubtypeResult {
        for t_prop in target {
            if let Some(sp) =
                self.lookup_property(&source.properties, Some(source_shape_id), t_prop.name)
            {
                // Check optional compatibility
                if sp.optional && !t_prop.optional {
                    return SubtypeResult::False;
                }
                // Readonly in source can't satisfy mutable target
                if sp.readonly && !t_prop.readonly {
                    return SubtypeResult::False;
                }
                let source_type = self.optional_property_type(sp);
                let target_type = self.optional_property_type(t_prop);
                let allow_bivariant = sp.is_method || t_prop.is_method;
                if !self
                    .check_subtype_with_method_variance(source_type, target_type, allow_bivariant)
                    .is_true()
                {
                    return SubtypeResult::False;
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
                        return SubtypeResult::False;
                    }
                }
            } else if !self
                .check_missing_property_against_index_signatures(source, t_prop)
                .is_true()
            {
                return SubtypeResult::False;
            }
        }

        SubtypeResult::True
    }

    /// Check if a missing target property can be satisfied by source index signatures.
    ///
    /// When a target property doesn't exist in the source object, the source's index
    /// signatures can potentially satisfy it:
    /// - If property name is numeric, check against number index signature
    /// - Always check against string index signature (since numbers convert to strings)
    ///
    /// ## Rules:
    /// 1. Readonly index cannot satisfy mutable target property
    /// 2. Index type must be subtype of (or bivariant with) target property type
    /// 3. If no applicable index signature exists, property can only be satisfied if optional
    ///
    /// ## Example:
    /// ```typescript
    /// interface Source {
    ///   [key: string]: number;  // String index signature
    ///   [index: number]: string;  // Number index signature
    /// }
    /// interface Target {
    ///   x: number;  // Satisfied by [key: string]: number
    ///   [1]: string;  // Satisfied by [index: number]: string
    ///   y: boolean;  // NOT satisfied (no compatible index)
    /// }
    /// ```
    fn check_missing_property_against_index_signatures(
        &mut self,
        source: &ObjectShape,
        target_prop: &PropertyInfo,
    ) -> SubtypeResult {
        let mut checked = false;
        let target_type = self.optional_property_type(target_prop);

        if utils::is_numeric_property_name(self.interner, target_prop.name)
            && let Some(number_idx) = &source.number_index
        {
            checked = true;
            if number_idx.readonly && !target_prop.readonly {
                return SubtypeResult::False;
            }
            if !self
                .check_subtype_with_method_variance(
                    number_idx.value_type,
                    target_type,
                    target_prop.is_method,
                )
                .is_true()
            {
                return SubtypeResult::False;
            }
        }

        if let Some(string_idx) = &source.string_index {
            checked = true;
            if string_idx.readonly && !target_prop.readonly {
                return SubtypeResult::False;
            }
            if !self
                .check_subtype_with_method_variance(
                    string_idx.value_type,
                    target_type,
                    target_prop.is_method,
                )
                .is_true()
            {
                return SubtypeResult::False;
            }
        }

        if checked || target_prop.optional {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Check that source properties are compatible with target index signatures.
    ///
    /// When a target has an index signature, all source properties must satisfy it:
    /// - String index: All string-named properties must be compatible with index type
    /// - Number index: All numerically-named properties must be compatible with index type
    ///
    /// ## Additional Constraints:
    /// - Readonly property in source cannot satisfy mutable index in target
    /// - Methods use bivariant checking (both directions)
    /// - Regular properties use contravariant checking
    ///
    /// ## Example:
    /// ```typescript
    /// interface Target {
    ///   [key: string]: number;  // String index signature
    /// }
    /// interface Source {
    ///   x: number;   // Compatible with [key: string]: number
    ///   y: string;   // NOT compatible
    /// }
    /// ```
    fn check_properties_against_index_signatures(
        &mut self,
        source: &[PropertyInfo],
        target: &ObjectShape,
    ) -> SubtypeResult {
        let string_index = target.string_index.as_ref();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return SubtypeResult::True;
        }

        for prop in source {
            let prop_type = self.optional_property_type(prop);
            let allow_bivariant = prop.is_method;

            if let Some(number_idx) = number_index {
                let is_numeric = utils::is_numeric_property_name(self.interner, prop.name);
                if is_numeric
                    && !self
                        .check_subtype_with_method_variance(
                            prop_type,
                            number_idx.value_type,
                            allow_bivariant,
                        )
                        .is_true()
                {
                    return SubtypeResult::False;
                }
                if is_numeric && !number_idx.readonly && prop.readonly {
                    return SubtypeResult::False;
                }
            }

            if let Some(string_idx) = string_index {
                if !string_idx.readonly && prop.readonly {
                    return SubtypeResult::False;
                }
                if !self
                    .check_subtype_with_method_variance(
                        prop_type,
                        string_idx.value_type,
                        allow_bivariant,
                    )
                    .is_true()
                {
                    return SubtypeResult::False;
                }
            }
        }

        SubtypeResult::True
    }

    /// Check simple object to object with index signature.
    ///
    /// Validates that a source object with only named properties is a subtype of
    /// a target object with an index signature. This requires:
    /// 1. All target named properties must have compatible source properties
    /// 2. All source properties must be compatible with the index signature type
    ///
    /// ## Example:
    /// ```typescript
    /// interface Target {
    ///   [key: string]: number;
    ///   x: string;  // Named property
    /// }
    /// interface Source {
    ///   x: string;  // Must match Target.x
    ///   y: number;  // Must satisfy string index signature
    /// }
    /// // Source ≤ Target because:
    /// // - Source.x is compatible with Target.x
    /// // - Source.y (number) is compatible with [key: string]: number
    /// ```
    fn check_object_to_indexed(
        &mut self,
        source: &[PropertyInfo],
        source_shape_id: Option<ObjectShapeId>,
        target: &ObjectShape,
    ) -> SubtypeResult {
        // First check named properties match
        if !self
            .check_object_subtype(source, source_shape_id, &target.properties)
            .is_true()
        {
            return SubtypeResult::False;
        }

        self.check_properties_against_index_signatures(source, target)
    }

    /// Check if parameter types are compatible based on variance settings.
    ///
    /// In strict mode (contravariant): target_type <: source_type
    /// In legacy mode (bivariant): target_type <: source_type OR source_type <: target_type
    /// See https://github.com/microsoft/TypeScript/issues/18654.
    fn are_parameters_compatible(&mut self, source_type: TypeId, target_type: TypeId) -> bool {
        self.are_parameters_compatible_impl(source_type, target_type, false)
    }

    /// Check if type predicates in functions are compatible.
    ///
    /// Type predicates make functions more specific. A function with a type predicate
    /// can only be assigned to another function with a compatible predicate.
    ///
    /// Rules:
    /// - No predicate vs no predicate: compatible
    /// - Source has predicate, target doesn't: NOT compatible (source is more specific)
    /// - Target has predicate, source doesn't: compatible (target is more specific, accepts source)
    /// - Both have predicates: check if predicates are compatible
    ///
    /// For compatible predicates:
    /// - Same parameter target (e.g., both `x is T`)
    /// - Asserted types: source_predicate_type <: target_predicate_type
    fn are_type_predicates_compatible(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> bool {
        match (&source.type_predicate, &target.type_predicate) {
            // No predicates in either function - compatible
            (None, None) => true,

            // Source has predicate, target doesn't - allow assignment.
            // Type predicates are implemented as runtime boolean returns, so a function with
            // a predicate is still callable where a plain boolean-returning function is
            // expected (as in ReturnType<T>).
            (Some(_), None) => true,

            // Source has no predicate, target has one - still compatible.
            // This mirrors TypeScript's behavior: a less specific function (no predicate)
            // can be used where a more specific function (with a predicate) is expected,
            // because the predicate is an additional guarantee to the caller, not a stronger
            // requirement on the implementation.
            // Example: (x: string) => boolean is assignable to (x: string) => x is string.
            (None, Some(_)) => true,

            // Both have predicates - check compatibility
            (Some(source_pred), Some(target_pred)) => {
                // First, check if predicates target the same parameter
                // The targets must match (both assert on the same parameter)
                if source_pred.target != target_pred.target {
                    return false;
                }

                // Check asserts compatibility
                // Type guards (`x is T`) and assertions (`asserts x is T`) are NOT compatible
                // They serve different purposes and cannot be assigned to each other
                match (source_pred.asserts, target_pred.asserts) {
                    // Source is type guard, target is assertion - NOT compatible
                    // (x is T) cannot be assigned to (asserts x is U)
                    (false, true) => false,

                    // Source is assertion, target is type guard - NOT compatible
                    // (asserts x is T) cannot be assigned to (x is U)
                    (true, false) => false,
                    // Both are type guards - check type compatibility
                    // (x is T) assignable to (x is U) if T extends U
                    // Both are assertions - check type compatibility
                    // (asserts x is T) assignable to (asserts x is U) if T extends U
                    //
                    // For both cases, the logic is identical: check if the asserted types
                    // are compatible (source <: target).
                    (false, false) | (true, true) => {
                        match (source_pred.type_id, target_pred.type_id) {
                            (Some(source_type), Some(target_type)) => {
                                self.check_subtype(source_type, target_type).is_true()
                            }
                            (None, Some(_)) => false,
                            (Some(_), None) => true,
                            (None, None) => true,
                        }
                    }
                }
            }
        }
    }

    /// Check parameter compatibility with method bivariance support.
    /// Methods are bivariant even when strict_function_types is enabled.
    fn are_parameters_compatible_impl(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        is_method: bool,
    ) -> bool {
        let contains_this =
            self.type_contains_this_type(source_type) || self.type_contains_this_type(target_type);

        // Contravariant check: Target <: Source
        // Example: (x: Animal) => void <: (x: Cat) => void
        // Because Cat <: Animal (target <: source)
        let is_contravariant = self.check_subtype(target_type, source_type).is_true();

        // Methods are bivariant regardless of strict_function_types setting
        // UNLESS disable_method_bivariance is set
        // This matches TypeScript's behavior for method parameters
        let method_should_be_bivariant = is_method && !self.disable_method_bivariance;
        let use_bivariance = method_should_be_bivariant || !self.strict_function_types;

        if !use_bivariance {
            if contains_this {
                return self.check_subtype(source_type, target_type).is_true();
            }
            is_contravariant
        } else {
            // Bivariant: either direction works (Unsound, Legacy TS behavior)
            if is_contravariant {
                return true;
            }
            // Covariant check: Source <: Target
            self.check_subtype(source_type, target_type).is_true()
        }
    }

    fn type_contains_this_type(&self, type_id: TypeId) -> bool {
        let mut visited: HashSet<TypeId> = HashSet::new();
        self.type_contains_this_type_inner(type_id, &mut visited)
    }

    fn type_contains_this_type_inner(
        &self,
        type_id: TypeId,
        visited: &mut HashSet<TypeId>,
    ) -> bool {
        if !visited.insert(type_id) {
            return false;
        }

        let Some(key) = self.interner.lookup(type_id) else {
            return false;
        };

        match key {
            TypeKey::ThisType => true,
            TypeKey::Array(elem) => self.type_contains_this_type_inner(elem, visited),
            TypeKey::Tuple(list_id) => {
                let elements = self.interner.tuple_list(list_id);
                elements
                    .iter()
                    .any(|elem| self.type_contains_this_type_inner(elem.type_id, visited))
            }
            TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
                let members = self.interner.type_list(list_id);
                members
                    .iter()
                    .any(|&member| self.type_contains_this_type_inner(member, visited))
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape.properties.iter().any(|prop| {
                    self.type_contains_this_type_inner(prop.type_id, visited)
                        || self.type_contains_this_type_inner(prop.write_type, visited)
                })
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                if shape.properties.iter().any(|prop| {
                    self.type_contains_this_type_inner(prop.type_id, visited)
                        || self.type_contains_this_type_inner(prop.write_type, visited)
                }) {
                    return true;
                }
                if let Some(index) = &shape.string_index
                    && (self.type_contains_this_type_inner(index.key_type, visited)
                        || self.type_contains_this_type_inner(index.value_type, visited))
                {
                    return true;
                }
                if let Some(index) = &shape.number_index
                    && (self.type_contains_this_type_inner(index.key_type, visited)
                        || self.type_contains_this_type_inner(index.value_type, visited))
                {
                    return true;
                }
                false
            }
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                shape
                    .params
                    .iter()
                    .any(|param| self.type_contains_this_type_inner(param.type_id, visited))
                    || shape.this_type.is_some_and(|this_type| {
                        self.type_contains_this_type_inner(this_type, visited)
                    })
                    || self.type_contains_this_type_inner(shape.return_type, visited)
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                if shape.call_signatures.iter().any(|sig| {
                    sig.params
                        .iter()
                        .any(|param| self.type_contains_this_type_inner(param.type_id, visited))
                        || sig.this_type.is_some_and(|this_type| {
                            self.type_contains_this_type_inner(this_type, visited)
                        })
                        || self.type_contains_this_type_inner(sig.return_type, visited)
                }) {
                    return true;
                }
                if shape.construct_signatures.iter().any(|sig| {
                    sig.params
                        .iter()
                        .any(|param| self.type_contains_this_type_inner(param.type_id, visited))
                        || sig.this_type.is_some_and(|this_type| {
                            self.type_contains_this_type_inner(this_type, visited)
                        })
                        || self.type_contains_this_type_inner(sig.return_type, visited)
                }) {
                    return true;
                }
                shape.properties.iter().any(|prop| {
                    self.type_contains_this_type_inner(prop.type_id, visited)
                        || self.type_contains_this_type_inner(prop.write_type, visited)
                })
            }
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                info.constraint.is_some_and(|constraint| {
                    self.type_contains_this_type_inner(constraint, visited)
                }) || info
                    .default
                    .is_some_and(|default| self.type_contains_this_type_inner(default, visited))
            }
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_this_type_inner(app.base, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_this_type_inner(arg, visited))
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.type_contains_this_type_inner(cond.check_type, visited)
                    || self.type_contains_this_type_inner(cond.extends_type, visited)
                    || self.type_contains_this_type_inner(cond.true_type, visited)
                    || self.type_contains_this_type_inner(cond.false_type, visited)
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                mapped.type_param.constraint.is_some_and(|constraint| {
                    self.type_contains_this_type_inner(constraint, visited)
                }) || mapped
                    .type_param
                    .default
                    .is_some_and(|default| self.type_contains_this_type_inner(default, visited))
                    || self.type_contains_this_type_inner(mapped.constraint, visited)
                    || mapped.name_type.is_some_and(|name_type| {
                        self.type_contains_this_type_inner(name_type, visited)
                    })
                    || self.type_contains_this_type_inner(mapped.template, visited)
            }
            TypeKey::IndexAccess(obj, idx) => {
                self.type_contains_this_type_inner(obj, visited)
                    || self.type_contains_this_type_inner(idx, visited)
            }
            TypeKey::KeyOf(inner) | TypeKey::ReadonlyType(inner) => {
                self.type_contains_this_type_inner(inner, visited)
            }
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => {
                        self.type_contains_this_type_inner(*inner, visited)
                    }
                })
            }
            TypeKey::StringIntrinsic { type_arg, .. } => {
                self.type_contains_this_type_inner(type_arg, visited)
            }
            TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Ref(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::Error => false,
        }
    }

    fn are_this_parameters_compatible(
        &mut self,
        source_type: Option<TypeId>,
        target_type: Option<TypeId>,
    ) -> bool {
        if source_type.is_none() && target_type.is_none() {
            return true;
        }
        // Use Unknown instead of Any for stricter type checking
        // When this parameter type is not specified, we should not allow any value
        let source_type = source_type.unwrap_or(TypeId::UNKNOWN);
        let target_type = target_type.unwrap_or(TypeId::UNKNOWN);

        // this parameters follow the same variance rules as regular parameters:
        // - Strict mode: Contravariant (target <: source)
        // - Non-strict mode: Bivariant (both directions)
        // This behavior differs from an earlier implementation that used covariance.
        // The key insight is that `this` is a pseudo-parameter, so it follows
        // parameter variance rules, not return type variance rules.
        if self.strict_function_types {
            // Contravariant in strict mode
            self.check_subtype(target_type, source_type).is_true()
        } else {
            // Bivariant in non-strict mode
            self.check_subtype(source_type, target_type).is_true()
                || self.check_subtype(target_type, source_type).is_true()
        }
    }

    fn required_param_count(&self, params: &[ParamInfo]) -> usize {
        params
            .iter()
            .filter(|param| !param.optional && !param.rest)
            .count()
    }

    fn extra_required_accepts_undefined(
        &mut self,
        params: &[ParamInfo],
        from_index: usize,
        required_count: usize,
    ) -> bool {
        params
            .iter()
            .take(required_count)
            .skip(from_index)
            .all(|param| {
                self.check_subtype(TypeId::UNDEFINED, param.type_id)
                    .is_true()
            })
    }

    /// Check return type compatibility with void special-casing.
    fn check_return_compat(
        &mut self,
        source_return: TypeId,
        target_return: TypeId,
    ) -> SubtypeResult {
        if self.allow_void_return && target_return == TypeId::VOID {
            // `() => void` treats the return value as ignored. See https://github.com/microsoft/TypeScript/issues/25274.
            return SubtypeResult::True;
        }
        self.check_subtype(source_return, target_return)
    }

    /// Check if a union type is a subtype of a target type.
    ///
    /// Union source: all members must be subtypes of target.
    /// When target is an intersection, applies distributivity rules.
    fn check_union_source_subtype(
        &mut self,
        members: TypeListId,
        target: TypeId,
        target_key: &TypeKey,
    ) -> SubtypeResult {
        // Distributivity: (A | B) & C distributes to (A & C) | (B & C)
        if let TypeKey::Intersection(inter_members) = target_key {
            let inter_members = self.interner.type_list(*inter_members);
            let union_members = self.interner.type_list(members);

            // Check: (A | B) <: (C & D)
            for &union_member in union_members.iter() {
                let mut satisfies_all = true;
                for &inter_member in inter_members.iter() {
                    if !self.check_subtype(union_member, inter_member).is_true() {
                        satisfies_all = false;
                        break;
                    }
                }
                if !satisfies_all {
                    return SubtypeResult::False;
                }
            }
            return SubtypeResult::True;
        }

        let members = self.interner.type_list(members);
        for &member in members.iter() {
            // Don't accept `any` as universal subtype in union checks
            if member == TypeId::ANY && target != TypeId::ANY {
                return SubtypeResult::False;
            }
            if !self.check_subtype(member, target).is_true() {
                return SubtypeResult::False;
            }
        }
        SubtypeResult::True
    }

    /// Check if a source type is a subtype of a union type.
    ///
    /// When the target is a union, the source must be assignable to AT LEAST ONE
    /// union member. This is the "exists" quantifier - there exists some union member
    /// that the source is compatible with.
    ///
    /// ## Union Target Rule:
    /// `S <: (A | B | C)` if `S <: A` OR `S <: B` OR `S <: C`
    ///
    /// ## Keyof Special Case:
    /// If source is keyof T and the union includes all primitive types (string | number | symbol),
    /// then it's compatible (keyof can match any property key type).
    ///
    /// ## Examples:
    /// ```typescript
    /// // string <: (string | number) ✅
    /// // string <: (number | boolean) ❌
    /// // never <: (string | number) ✅ (never is subtype of everything)
    /// // keyof T <: (string | number | symbol) ✅ (if union is all primitives)
    /// ```
    fn check_union_target_subtype(
        &mut self,
        source: TypeId,
        source_key: &TypeKey,
        members: TypeListId,
    ) -> SubtypeResult {
        if matches!(source_key, TypeKey::KeyOf(_)) && self.union_includes_keyof_primitives(members)
        {
            return SubtypeResult::True;
        }
        let members = self.interner.type_list(members);
        for &member in members.iter() {
            if member == TypeId::ANY && source != TypeId::ANY {
                continue;
            }
            if self.check_subtype(source, member).is_true() {
                return SubtypeResult::True;
            }
        }
        SubtypeResult::False
    }

    /// Check if an intersection type is a subtype of a target type.
    ///
    /// Intersection source: source is subtype if any constituent is.
    /// Also handles type parameter constraint narrowing.
    fn check_intersection_source_subtype(
        &mut self,
        members: TypeListId,
        target: TypeId,
    ) -> SubtypeResult {
        let members = self.interner.type_list(members);

        // First, check if any member is directly a subtype
        for &member in members.iter() {
            if self.check_subtype(member, target).is_true() {
                return SubtypeResult::True;
            }
        }

        // For type parameters in intersections, try narrowing the constraint
        for &member in members.iter() {
            if let Some(TypeKey::TypeParameter(param_info)) | Some(TypeKey::Infer(param_info)) =
                self.interner.lookup(member)
                && let Some(constraint) = param_info.constraint
            {
                let other_members: Vec<TypeId> =
                    members.iter().filter(|&&m| m != member).copied().collect();

                if !other_members.is_empty() {
                    let mut all_members = vec![constraint];
                    all_members.extend(other_members);
                    let narrowed_constraint = self.interner.intersection(all_members);

                    if self.check_subtype(narrowed_constraint, target).is_true() {
                        return SubtypeResult::True;
                    }
                }
            }
        }

        SubtypeResult::False
    }

    /// Check if a source type is a subtype of an intersection type.
    ///
    /// Intersection target: all members must be satisfied.
    fn check_intersection_target_subtype(
        &mut self,
        source: TypeId,
        members: TypeListId,
    ) -> SubtypeResult {
        let members = self.interner.type_list(members);
        for &member in members.iter() {
            if !self.check_subtype(source, member).is_true() {
                return SubtypeResult::False;
            }
        }
        SubtypeResult::True
    }

    /// Check if a type parameter is a subtype of a target type.
    ///
    /// Handles both type parameter vs type parameter and type parameter vs concrete type.
    /// Implements TypeScript's soundness rules for type parameter compatibility.
    fn check_type_parameter_subtype(
        &mut self,
        s_info: &TypeParamInfo,
        target: TypeId,
        target_key: &TypeKey,
    ) -> SubtypeResult {
        // Type parameter vs type parameter
        if let TypeKey::TypeParameter(t_info) | TypeKey::Infer(t_info) = target_key {
            // Same type parameter by name - reflexive
            if s_info.name == t_info.name {
                return SubtypeResult::True;
            }

            // Different type parameters - check if source's constraint implies compatibility
            // TypeScript soundness: T <: U only if:
            // 1. Constraint(T) is exactly U (e.g., U extends T, checking U <: T)
            // 2. Constraint(T) extends U's constraint transitively
            if let Some(s_constraint) = s_info.constraint {
                // Check if source's constraint IS the target type parameter itself
                if s_constraint == target {
                    return SubtypeResult::True;
                }
                // Check if source's constraint is a subtype of the target type parameter
                if self.check_subtype(s_constraint, target).is_true() {
                    return SubtypeResult::True;
                }
            }
            // Two different type parameters with independent constraints are not interchangeable
            return SubtypeResult::False;
        }

        // Type parameter vs concrete type
        if let Some(constraint) = s_info.constraint {
            return self.check_subtype(constraint, target);
        }

        // Unconstrained type parameter acts like `unknown` (top type)
        // An unconstrained type param as source cannot be assigned to a concrete target
        SubtypeResult::False
    }

    /// Check subtype with optional method bivariance.
    ///
    /// When `allow_bivariant` is true, temporarily disables strict function types
    /// to allow bivariant parameter checking. This is used for method compatibility
    /// where TypeScript allows bivariance even in strict mode.
    ///
    /// ## Variance Modes:
    /// - **Contravariant (strict)**: `target <: source` - Function parameters in strict mode
    /// - **Bivariant (legacy)**: `target <: source OR source <: target` - Methods, legacy functions
    ///
    /// ## Example:
    /// ```typescript
    /// // Bivariant methods allow unsound but convenient assignments
    /// interface Animal { name: string; }
    /// interface Dog extends Animal { bark(): void; }
    /// class AnimalKeeper {
    ///   feed(animal: Animal) { ... }  // Contravariant parameter
    /// }
    /// class DogKeeper {
    ///   feed(dog: Dog) { ... }  // More specific
    /// }
    /// // DogKeeper.feed is assignable to AnimalKeeper.feed (bivariant)
    /// ```
    fn check_subtype_with_method_variance(
        &mut self,
        source: TypeId,
        target: TypeId,
        allow_bivariant: bool,
    ) -> SubtypeResult {
        if !allow_bivariant {
            return self.check_subtype(source, target);
        }
        let prev = self.strict_function_types;
        let prev_param_count = self.allow_bivariant_param_count;
        self.strict_function_types = false;
        self.allow_bivariant_param_count = true;
        let result = self.check_subtype(source, target);
        self.allow_bivariant_param_count = prev_param_count;
        self.strict_function_types = prev;
        result
    }

    fn explain_failure_with_method_variance(
        &mut self,
        source: TypeId,
        target: TypeId,
        allow_bivariant: bool,
    ) -> Option<SubtypeFailureReason> {
        if !allow_bivariant {
            return self.explain_failure(source, target);
        }
        let prev = self.strict_function_types;
        let prev_param_count = self.allow_bivariant_param_count;
        self.strict_function_types = false;
        self.allow_bivariant_param_count = true;
        let result = self.explain_failure(source, target);
        self.allow_bivariant_param_count = prev_param_count;
        self.strict_function_types = prev;
        result
    }

    /// Check if a tuple type is a subtype of an array type.
    ///
    /// Tuple is subtype of array if all tuple elements are subtypes of the array element type.
    /// Handles both regular elements and rest elements (with expansion).
    fn check_tuple_to_array_subtype(
        &mut self,
        elems: TupleListId,
        t_elem: TypeId,
    ) -> SubtypeResult {
        let elems = self.interner.tuple_list(elems);
        for elem in elems.iter() {
            if elem.rest {
                let expansion = self.expand_tuple_rest(elem.type_id);
                for fixed in expansion.fixed {
                    if !self.check_subtype(fixed.type_id, t_elem).is_true() {
                        return SubtypeResult::False;
                    }
                }
                if let Some(variadic) = expansion.variadic
                    && !self.check_subtype(variadic, t_elem).is_true()
                {
                    return SubtypeResult::False;
                }
                // Check tail elements from nested tuple spreads
                for tail_elem in expansion.tail {
                    if !self.check_subtype(tail_elem.type_id, t_elem).is_true() {
                        return SubtypeResult::False;
                    }
                }
            } else {
                // Regular element: T <: U
                if !self.check_subtype(elem.type_id, t_elem).is_true() {
                    return SubtypeResult::False;
                }
            }
        }
        SubtypeResult::True
    }

    /// Check if a function type is a subtype of a callable type.
    ///
    /// A single function can match a callable if it satisfies all target call signatures.
    fn check_function_to_callable_subtype(
        &mut self,
        s_fn_id: FunctionShapeId,
        t_callable_id: CallableShapeId,
    ) -> SubtypeResult {
        let s_fn = self.interner.function_shape(s_fn_id);
        let t_callable = self.interner.callable_shape(t_callable_id);
        for t_sig in &t_callable.call_signatures {
            if !self.check_call_signature_subtype_fn(&s_fn, t_sig).is_true() {
                return SubtypeResult::False;
            }
        }
        SubtypeResult::True
    }

    /// Check if a callable type is a subtype of a function type.
    ///
    /// At least one source signature must match the target function.
    fn check_callable_to_function_subtype(
        &mut self,
        s_callable_id: CallableShapeId,
        t_fn_id: FunctionShapeId,
    ) -> SubtypeResult {
        let s_callable = self.interner.callable_shape(s_callable_id);
        let t_fn = self.interner.function_shape(t_fn_id);

        if t_fn.is_constructor {
            for s_sig in &s_callable.construct_signatures {
                if self
                    .check_call_signature_subtype_to_fn(s_sig, &t_fn)
                    .is_true()
                {
                    return SubtypeResult::True;
                }
            }
            return SubtypeResult::False;
        }

        for s_sig in &s_callable.call_signatures {
            if self
                .check_call_signature_subtype_to_fn(s_sig, &t_fn)
                .is_true()
            {
                return SubtypeResult::True;
            }
        }
        SubtypeResult::False
    }

    /// Get the effective type of an optional property for reading.
    ///
    /// Optional properties in TypeScript can be undefined even if their type doesn't
    /// explicitly include undefined. This function adds undefined to the type unless
    /// exactOptionalPropertyTypes is enabled.
    ///
    /// ## Behavior:
    /// - If property is optional AND exact_optional_property_types is false:
    ///   - Returns `T | undefined` where T is the property type
    /// - Otherwise:
    ///   - Returns the property type as-is
    ///
    /// ## Example:
    /// ```typescript
    /// interface Example {
    ///   x?: number;  // Type is number, but effectively number | undefined
    /// }
    /// // exactOptionalPropertyTypes: false → x has type number | undefined
    /// // exactOptionalPropertyTypes: true  → x has type number
    /// ```
    fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional && !self.exact_optional_property_types {
            self.interner.union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        }
    }

    fn optional_property_write_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional && !self.exact_optional_property_types {
            self.interner.union2(prop.write_type, TypeId::UNDEFINED)
        } else {
            prop.write_type
        }
    }

    /// Check function subtyping
    fn check_function_subtype(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> SubtypeResult {
        // Constructor vs non-constructor
        if source.is_constructor != target.is_constructor {
            return SubtypeResult::False;
        }

        // Return type is covariant
        if !self
            .check_return_compat(source.return_type, target.return_type)
            .is_true()
        {
            return SubtypeResult::False;
        }

        if !self.are_this_parameters_compatible(source.this_type, target.this_type) {
            return SubtypeResult::False;
        }

        // Type predicates: a function with a type predicate is more specific
        // than one without or with a less specific predicate
        if !self.are_type_predicates_compatible(source, target) {
            return SubtypeResult::False;
        }
        // Method bivariance: if either source or target is a method, use bivariance for parameters
        let is_method = source.is_method || target.is_method;

        // Check if target has a rest parameter
        let target_has_rest = target.params.last().is_some_and(|p| p.rest);
        let source_has_rest = source.params.last().is_some_and(|p| p.rest);
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
        if !self.allow_bivariant_param_count
            && !rest_is_top
            && source_required > target_required
            && (!target_has_rest || !extra_required_ok)
        {
            return SubtypeResult::False;
        }

        // Count non-rest parameters
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

        // Compare fixed parameters
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source.params[i];
            let t_param = &target.params[i];

            // Check optional compatibility:
            // - Required param can substitute for optional param (if types match)
            // - Optional param CANNOT substitute for required param (unless type accepts undefined)
            if s_param.optional && !t_param.optional {
                // Source is optional, target is required
                // Optional param can only substitute for required if the type accepts undefined
                if !self
                    .check_subtype(TypeId::UNDEFINED, t_param.type_id)
                    .is_true()
                {
                    return SubtypeResult::False;
                }
            }

            // Check parameter compatibility (contravariant in strict mode, bivariant in legacy)
            // Methods use bivariance even in strict mode
            if !self.are_parameters_compatible_impl(s_param.type_id, t_param.type_id, is_method) {
                return SubtypeResult::False;
            }
        }

        // If target has rest parameter, check source's extra params against the rest type
        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return SubtypeResult::False; // Invalid rest parameter
            };
            if rest_is_top {
                return SubtypeResult::True;
            }

            // Check source params that exceed target's fixed count against rest type
            for i in target_fixed_count..source_fixed_count {
                let s_param = &source.params[i];
                // Check parameter compatibility against rest element type
                if !self.are_parameters_compatible_impl(s_param.type_id, rest_elem_type, is_method)
                {
                    return SubtypeResult::False;
                }
            }

            // If source also has a rest param, check it against target's rest
            if source_has_rest {
                let Some(s_rest_param) = source.params.last() else {
                    return SubtypeResult::False;
                };
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                // Check rest-to-rest parameter compatibility
                if !self.are_parameters_compatible_impl(s_rest_elem, rest_elem_type, is_method) {
                    return SubtypeResult::False;
                }
            }
        }

        if source_has_rest {
            let rest_param = if let Some(rest_param) = source.params.last() {
                rest_param
            } else {
                return SubtypeResult::False;
            };
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            let rest_is_top = self.allow_bivariant_rest
                && (rest_elem_type == TypeId::ANY || rest_elem_type == TypeId::UNKNOWN);

            if !rest_is_top {
                for i in source_fixed_count..target_fixed_count {
                    let t_param = &target.params[i];
                    if !self.are_parameters_compatible_impl(
                        rest_elem_type,
                        t_param.type_id,
                        is_method,
                    ) {
                        return SubtypeResult::False;
                    }
                }
            }
        }

        SubtypeResult::True
    }

    /// Get the element type of an array type, or return the type itself for any[]
    fn get_array_element_type(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::ANY {
            return TypeId::ANY;
        }
        match self.interner.lookup(type_id) {
            Some(TypeKey::Array(elem)) => elem,
            // For any[], the type itself is assignable from anything
            _ => type_id,
        }
    }

    /// Expand a tuple rest element into its constituent parts.
    ///
    /// Tuples can have rest elements like `[A, B, ...C[]]` which need to be expanded
    /// for subtype checking. This function recursively expands rest elements to produce:
    /// - `fixed`: Elements before the rest
    /// - `variadic`: The rest element's type (e.g., C for ...C[])
    /// - `tail`: Elements after the rest (rare, but valid in some TypeScript patterns)
    ///
    /// ## Examples:
    /// - `[number, string]` → fixed: [number, string], variadic: None, tail: []
    /// - `[number, ...string[]]` → fixed: [number], variadic: Some(string), tail: []
    /// - `[...T[], number]` → fixed: [], variadic: Some(T), tail: [number]
    ///
    /// ## Recursive Expansion:
    /// Nested rest elements are recursively expanded, so:
    /// - `[A, ...[...B[], C]]` → fixed: [A], variadic: Some(B), tail: [C]
    fn expand_tuple_rest(&self, type_id: TypeId) -> TupleRestExpansion {
        match self.interner.lookup(type_id) {
            Some(TypeKey::Array(elem)) => TupleRestExpansion {
                fixed: Vec::new(),
                variadic: Some(elem),
                tail: Vec::new(),
            },
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                let mut fixed = Vec::new();
                for (i, elem) in elements.iter().enumerate() {
                    if elem.rest {
                        let inner = self.expand_tuple_rest(elem.type_id);
                        fixed.extend(inner.fixed);
                        // Capture tail elements: inner.tail + elements after the rest
                        let mut tail = inner.tail;
                        tail.extend(elements[i + 1..].iter().cloned());
                        return TupleRestExpansion {
                            fixed,
                            variadic: inner.variadic,
                            tail,
                        };
                    }
                    fixed.push(elem.clone());
                }
                TupleRestExpansion {
                    fixed,
                    variadic: None,
                    tail: Vec::new(),
                }
            }
            _ => TupleRestExpansion {
                fixed: Vec::new(),
                variadic: Some(type_id),
                tail: Vec::new(),
            },
        }
    }

    /// Evaluate a meta-type (conditional, index access, mapped, etc.) to its concrete form.
    /// Uses TypeEvaluator to reduce types like `T extends U ? X : Y` to either X or Y.
    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        use crate::solver::evaluate::TypeEvaluator;
        let mut evaluator = TypeEvaluator::with_resolver(self.interner, self.resolver);
        evaluator.set_no_unchecked_indexed_access(self.no_unchecked_indexed_access);
        evaluator.evaluate(type_id)
    }

    /// Check callable subtyping with overloaded signatures.
    ///
    /// Validates that a source callable with multiple overloads is compatible with
    /// a target callable. This requires:
    /// 1. For each target call signature, at least one source signature must match
    /// 2. For each target construct signature, at least one source signature must match
    /// 3. All target properties must have compatible source properties (excluding private fields)
    ///
    /// ## Overload Matching:
    /// Overloaded callables use a "best match" algorithm where:
    /// - Source must have a signature compatible with EACH target signature
    /// - This is stricter than single function subtyping
    ///
    /// ## Example:
    /// ```typescript
    /// type Source = {
    ///   (x: number): void;
    ///   (x: string): void;
    /// };
    /// type Target = {
    ///   (x: number): void;
    /// };
    /// // Source ≤ Target because Source has a compatible signature for Target's signature
    /// ```
    fn check_callable_subtype(
        &mut self,
        source: &CallableShape,
        target: &CallableShape,
    ) -> SubtypeResult {
        // For each target call signature, at least one source signature must match
        for t_sig in &target.call_signatures {
            let mut found_match = false;
            for s_sig in &source.call_signatures {
                if self.check_call_signature_subtype(s_sig, t_sig).is_true() {
                    found_match = true;
                    break;
                }
            }
            if !found_match {
                return SubtypeResult::False;
            }
        }

        // For each target construct signature, at least one source signature must match
        for t_sig in &target.construct_signatures {
            let mut found_match = false;
            for s_sig in &source.construct_signatures {
                if self.check_call_signature_subtype(s_sig, t_sig).is_true() {
                    found_match = true;
                    break;
                }
            }
            if !found_match {
                return SubtypeResult::False;
            }
        }

        // Check properties (if any), excluding private fields (starting with #)
        // Private fields should not affect structural typing for constructor types
        let source_props: Vec<_> = source
            .properties
            .iter()
            .filter(|p| !self.interner.resolve_atom(p.name).starts_with('#'))
            .cloned()
            .collect();
        let target_props: Vec<_> = target
            .properties
            .iter()
            .filter(|p| !self.interner.resolve_atom(p.name).starts_with('#'))
            .cloned()
            .collect();
        if !self
            .check_object_subtype(&source_props, None, &target_props)
            .is_true()
        {
            return SubtypeResult::False;
        }

        SubtypeResult::True
    }

    /// Check call signature subtyping
    fn check_call_signature_subtype(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        // Return type is covariant
        if !self
            .check_return_compat(source.return_type, target.return_type)
            .is_true()
        {
            return SubtypeResult::False;
        }

        // Check if target has a rest parameter
        let target_has_rest = target.params.last().is_some_and(|p| p.rest);
        let source_has_rest = source.params.last().is_some_and(|p| p.rest);
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
        if !self.allow_bivariant_param_count
            && !rest_is_top
            && source_required > target_required
            && (!target_has_rest || !extra_required_ok)
        {
            return SubtypeResult::False;
        }

        // Count non-rest parameters
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

        // Compare fixed parameters
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source.params[i];
            let t_param = &target.params[i];
            // Check parameter compatibility (contravariant in strict mode, bivariant in legacy)
            if !self.are_parameters_compatible(s_param.type_id, t_param.type_id) {
                return SubtypeResult::False;
            }
        }

        // If target has rest parameter, check source's extra params against the rest type
        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return SubtypeResult::False; // Invalid rest parameter
            };
            if rest_is_top {
                return SubtypeResult::True;
            }

            for i in target_fixed_count..source_fixed_count {
                let s_param = &source.params[i];
                if !self.are_parameters_compatible(s_param.type_id, rest_elem_type) {
                    return SubtypeResult::False;
                }
            }

            if source_has_rest {
                let Some(s_rest_param) = source.params.last() else {
                    return SubtypeResult::False;
                };
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                // Check rest-to-rest parameter compatibility
                if !self.are_parameters_compatible(s_rest_elem, rest_elem_type) {
                    return SubtypeResult::False;
                }
            }
        }

        if source_has_rest {
            let rest_param = if let Some(rest_param) = source.params.last() {
                rest_param
            } else {
                return SubtypeResult::False;
            };
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            let rest_is_top = self.allow_bivariant_rest
                && (rest_elem_type == TypeId::ANY || rest_elem_type == TypeId::UNKNOWN);

            if !rest_is_top {
                for i in source_fixed_count..target_fixed_count {
                    let t_param = &target.params[i];
                    if !self.are_parameters_compatible(rest_elem_type, t_param.type_id) {
                        return SubtypeResult::False;
                    }
                }
            }
        }

        SubtypeResult::True
    }

    /// Check call signature subtype to function shape
    fn check_call_signature_subtype_to_fn(
        &mut self,
        source: &CallSignature,
        target: &FunctionShape,
    ) -> SubtypeResult {
        // Return type is covariant
        if !self
            .check_return_compat(source.return_type, target.return_type)
            .is_true()
        {
            return SubtypeResult::False;
        }

        if !self.are_this_parameters_compatible(source.this_type, target.this_type) {
            return SubtypeResult::False;
        }

        // Check if target has a rest parameter
        let target_has_rest = target.params.last().is_some_and(|p| p.rest);
        let source_has_rest = source.params.last().is_some_and(|p| p.rest);
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
        if !self.allow_bivariant_param_count
            && !rest_is_top
            && source_required > target_required
            && (!target_has_rest || !extra_required_ok)
        {
            return SubtypeResult::False;
        }

        // Count non-rest parameters
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

        // Compare fixed parameters
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source.params[i];
            let t_param = &target.params[i];
            // Check parameter compatibility (contravariant in strict mode, bivariant in legacy)
            if !self.are_parameters_compatible(s_param.type_id, t_param.type_id) {
                return SubtypeResult::False;
            }
        }

        // If target has rest parameter, check source's extra params against the rest type
        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return SubtypeResult::False; // Invalid rest parameter
            };
            if rest_is_top {
                return SubtypeResult::True;
            }

            for i in target_fixed_count..source_fixed_count {
                let s_param = &source.params[i];
                if !self.are_parameters_compatible(s_param.type_id, rest_elem_type) {
                    return SubtypeResult::False;
                }
            }

            if source_has_rest {
                let Some(s_rest_param) = source.params.last() else {
                    return SubtypeResult::False;
                };
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                // Check rest-to-rest parameter compatibility
                if !self.are_parameters_compatible(s_rest_elem, rest_elem_type) {
                    return SubtypeResult::False;
                }
            }
        }

        if source_has_rest {
            let rest_param = if let Some(rest_param) = source.params.last() {
                rest_param
            } else {
                return SubtypeResult::False;
            };
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            let rest_is_top = self.allow_bivariant_rest
                && (rest_elem_type == TypeId::ANY || rest_elem_type == TypeId::UNKNOWN);

            if !rest_is_top {
                for i in source_fixed_count..target_fixed_count {
                    let t_param = &target.params[i];
                    if !self.are_parameters_compatible(rest_elem_type, t_param.type_id) {
                        return SubtypeResult::False;
                    }
                }
            }
        }

        SubtypeResult::True
    }

    /// Check function shape subtype to call signature
    fn check_call_signature_subtype_fn(
        &mut self,
        source: &FunctionShape,
        target: &CallSignature,
    ) -> SubtypeResult {
        // Return type is covariant
        if !self
            .check_return_compat(source.return_type, target.return_type)
            .is_true()
        {
            return SubtypeResult::False;
        }

        if !self.are_this_parameters_compatible(source.this_type, target.this_type) {
            return SubtypeResult::False;
        }

        // Check if target has a rest parameter
        let target_has_rest = target.params.last().is_some_and(|p| p.rest);
        let source_has_rest = source.params.last().is_some_and(|p| p.rest);
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
        if !self.allow_bivariant_param_count
            && !rest_is_top
            && source_required > target_required
            && (!target_has_rest || !extra_required_ok)
        {
            return SubtypeResult::False;
        }

        // Count non-rest parameters
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

        // Compare fixed parameters
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source.params[i];
            let t_param = &target.params[i];
            // Check parameter compatibility (contravariant in strict mode, bivariant in legacy)
            if !self.are_parameters_compatible(s_param.type_id, t_param.type_id) {
                return SubtypeResult::False;
            }
        }

        // If target has rest parameter, check source's extra params against the rest type
        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return SubtypeResult::False; // Invalid rest parameter
            };
            if rest_is_top {
                return SubtypeResult::True;
            }

            for i in target_fixed_count..source_fixed_count {
                let s_param = &source.params[i];
                if !self.are_parameters_compatible(s_param.type_id, rest_elem_type) {
                    return SubtypeResult::False;
                }
            }

            if source_has_rest {
                let Some(s_rest_param) = source.params.last() else {
                    return SubtypeResult::False;
                };
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                // Check rest-to-rest parameter compatibility
                if !self.are_parameters_compatible(s_rest_elem, rest_elem_type) {
                    return SubtypeResult::False;
                }
            }
        }

        if source_has_rest {
            let rest_param = if let Some(rest_param) = source.params.last() {
                rest_param
            } else {
                return SubtypeResult::False;
            };
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            let rest_is_top = self.allow_bivariant_rest
                && (rest_elem_type == TypeId::ANY || rest_elem_type == TypeId::UNKNOWN);

            if !rest_is_top {
                for i in source_fixed_count..target_fixed_count {
                    let t_param = &target.params[i];
                    if !self.are_parameters_compatible(rest_elem_type, t_param.type_id) {
                        return SubtypeResult::False;
                    }
                }
            }
        }

        SubtypeResult::True
    }
}

// =============================================================================
// Error Explanation API
// =============================================================================

/// Reason why a subtype check failed.
/// Used by `explain_failure` to provide detailed error messages.
#[derive(Clone, Debug)]
pub enum SubtypeFailureReason {
    /// A required property is missing in the source type.
    MissingProperty {
        property_name: Atom,
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Property types are incompatible.
    PropertyTypeMismatch {
        property_name: Atom,
        source_property_type: TypeId,
        target_property_type: TypeId,
        nested_reason: Option<Box<SubtypeFailureReason>>,
    },
    /// Optional property cannot satisfy required property.
    OptionalPropertyRequired { property_name: Atom },
    /// Readonly property cannot satisfy mutable property.
    ReadonlyPropertyMismatch { property_name: Atom },
    /// Return types are incompatible.
    ReturnTypeMismatch {
        source_return: TypeId,
        target_return: TypeId,
        nested_reason: Option<Box<SubtypeFailureReason>>,
    },
    /// Parameter types are incompatible.
    ParameterTypeMismatch {
        param_index: usize,
        source_param: TypeId,
        target_param: TypeId,
    },
    /// Too many parameters in source.
    TooManyParameters {
        source_count: usize,
        target_count: usize,
    },
    /// Tuple element count mismatch.
    TupleElementMismatch {
        source_count: usize,
        target_count: usize,
    },
    /// Tuple element type mismatch.
    TupleElementTypeMismatch {
        index: usize,
        source_element: TypeId,
        target_element: TypeId,
    },
    /// Array element type mismatch.
    ArrayElementMismatch {
        source_element: TypeId,
        target_element: TypeId,
    },
    /// Index signature value type mismatch.
    IndexSignatureMismatch {
        index_kind: &'static str, // "string" or "number"
        source_value_type: TypeId,
        target_value_type: TypeId,
    },
    /// No union member matches.
    NoUnionMemberMatches {
        source_type: TypeId,
        target_union_members: Vec<TypeId>,
    },
    /// No overlapping properties for weak type target.
    NoCommonProperties {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Generic type mismatch (no more specific reason).
    TypeMismatch {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Intrinsic type mismatch (e.g., string vs number).
    IntrinsicTypeMismatch {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Literal type mismatch (e.g., "hello" vs "world" or "hello" vs 42).
    LiteralTypeMismatch {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Error type encountered - indicates unresolved type that should not be silently compatible.
    ErrorType {
        source_type: TypeId,
        target_type: TypeId,
    },
}

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
        // ERROR types should produce a failure reason, not be silently ignored.
        // This ensures that unresolved types (TS2304) still trigger downstream TS2322 errors.
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return Some(SubtypeFailureReason::ErrorType {
                source_type: source,
                target_type: target,
            });
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

/// Format a number for template literal string coercion
/// Follows JavaScript's number-to-string conversion rules
fn format_number_for_template(num: f64) -> String {
    if num.is_nan() {
        return "NaN".to_string();
    }
    if num.is_infinite() {
        return if num.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    // Use JavaScript-like formatting (no trailing .0 for integers)
    if num.fract() == 0.0 && num.abs() < 1e15 {
        format!("{:.0}", num)
    } else {
        // Use default Rust formatting which is close enough for most cases
        let s = format!("{}", num);
        // Remove unnecessary trailing zeros after decimal point
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Find the length of a valid number at the start of a string
fn find_number_length(s: &str) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    // Handle optional sign
    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
        i += 1;
    }

    // Check for special values
    if s.len() > i && s[i..].starts_with("Infinity") {
        return i + 8;
    }
    if s.len() > i && s[i..].starts_with("NaN") {
        return i + 3;
    }

    let start = i;
    let mut has_digits = false;
    let mut has_dot = false;
    let mut has_exponent = false;

    // Integer or decimal part
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            has_digits = true;
            i += 1;
        } else if chars[i] == '.' && !has_dot && !has_exponent {
            has_dot = true;
            i += 1;
        } else if (chars[i] == 'e' || chars[i] == 'E') && has_digits && !has_exponent {
            has_exponent = true;
            i += 1;
            // Optional sign after exponent
            if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                i += 1;
            }
            // Must have at least one digit after exponent
            if i >= chars.len() || !chars[i].is_ascii_digit() {
                // Invalid exponent, backtrack
                i = if has_dot { i - 2 } else { i - 1 };
                if i > 0 && (chars[i - 1] == '+' || chars[i - 1] == '-') {
                    i -= 1;
                }
                break;
            }
        } else {
            break;
        }
    }

    if !has_digits {
        return 0;
    }

    // Don't count trailing dot without digits
    if i > start && chars[i - 1] == '.' && (i == start + 1 || !chars[i - 2].is_ascii_digit()) {
        i -= 1;
    }

    i
}

/// Check if a string is a valid number representation
fn is_valid_number(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // Handle special values
    if s == "NaN" || s == "Infinity" || s == "-Infinity" || s == "+Infinity" {
        return true;
    }
    // Try parsing as f64
    s.parse::<f64>().is_ok()
}

/// Find the length of a valid integer at the start of a string
fn find_integer_length(s: &str) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    // Handle optional sign
    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
        i += 1;
    }

    let start = i;

    // Must have at least one digit
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }

    if i == start {
        return 0;
    }

    i
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
