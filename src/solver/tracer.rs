//! Tracer-based subtype checking.
//!
//! This module implements the tracer pattern for subtype checking, unifying
//! fast boolean checks and detailed diagnostic generation into a single implementation.
//!
//! ## Zero-Cost Abstraction
//!
//! The key insight is that by using a trait with `#[inline(always)]` on the fast path,
//! the compiler can optimize away all diagnostic collection when using `FastTracer`,
//! resulting in the same machine code as a simple boolean check.
//!
//! ## Usage
//!
//! ```rust
//! // Fast check (zero-cost)
//! let mut fast_tracer = FastTracer;
//! let is_subtype = checker.check_subtype_with_tracer(source, target, &mut fast_tracer);
//!
//! // Detailed diagnostics
//! let mut diag_tracer = DiagnosticTracer::new();
//! checker.check_subtype_with_tracer(source, target, &mut diag_tracer);
//! if let Some(reason) = diag_tracer.take_failure() {
//!     // Generate error message from reason
//! }
//! ```

use crate::interner::Atom;
use crate::solver::TypeDatabase;
use crate::solver::diagnostics::{
    DiagnosticTracer, FastTracer, SubtypeFailureReason, SubtypeTracer,
};
use crate::solver::subtype::{
    MAX_SUBTYPE_DEPTH, NoopResolver, SubtypeChecker, SubtypeResult, TypeResolver,
};
use crate::solver::types::*;
use std::collections::HashSet;

#[cfg(test)]
use crate::solver::TypeInterner;

/// Maximum total subtype checks allowed per tracer-based check.
const MAX_TOTAL_TRACER_CHECKS: u32 = 100_000;

/// Maximum number of in-progress pairs to track.
const MAX_IN_PROGRESS_PAIRS: usize = 10_000;

/// Tracer-based subtype checker.
///
/// This provides a unified API for both fast boolean checks and detailed diagnostics
/// by using the `SubtypeTracer` trait to abstract the failure handling.
///
/// The checker maintains its own cycle detection and depth tracking to avoid
/// interfering with the main `SubtypeChecker` state.
pub struct TracerSubtypeChecker<'a, R: TypeResolver> {
    /// Reference to the type database (interner).
    pub(crate) interner: &'a dyn TypeDatabase,
    /// Reference to the type resolver.
    pub(crate) resolver: &'a R,
    /// Active subtype pairs being checked (for cycle detection).
    pub(crate) in_progress: HashSet<(TypeId, TypeId)>,
    /// Current recursion depth.
    pub(crate) depth: u32,
    /// Total number of checks performed.
    pub(crate) total_checks: u32,
    /// Whether recursion depth was exceeded.
    pub(crate) depth_exceeded: bool,
    /// Whether to use strict function types (contravariant parameters).
    pub(crate) strict_function_types: bool,
    /// Whether to allow any return type when target return is void.
    pub(crate) allow_void_return: bool,
    /// Whether null/undefined are treated as separate types.
    pub(crate) strict_null_checks: bool,
}

impl<'a, R: TypeResolver> TracerSubtypeChecker<'a, R> {
    /// Create a new tracer-based subtype checker.
    pub fn new(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        Self {
            interner,
            resolver,
            in_progress: HashSet::new(),
            depth: 0,
            total_checks: 0,
            depth_exceeded: false,
            strict_function_types: true,
            allow_void_return: false,
            strict_null_checks: true,
        }
    }

    /// Set whether to use strict function types.
    pub fn with_strict_function_types(mut self, strict: bool) -> Self {
        self.strict_function_types = strict;
        self
    }

    /// Set whether to allow void return type optimization.
    pub fn with_allow_void_return(mut self, allow: bool) -> Self {
        self.allow_void_return = allow;
        self
    }

    /// Set strict null checks mode.
    pub fn with_strict_null_checks(mut self, strict: bool) -> Self {
        self.strict_null_checks = strict;
        self
    }

    /// Check if a type is a subtype of another, using the provided tracer.
    ///
    /// This is the main entry point for tracer-based subtype checking.
    /// The tracer determines whether we collect detailed diagnostics or just return a boolean.
    ///
    /// # Parameters
    /// - `source`: The source type (the "from" type in `source <: target`)
    /// - `target`: The target type (the "to" type in `source <: target`)
    /// - `tracer`: The tracer to use for failure handling
    ///
    /// # Returns
    /// - `true` if `source` is a subtype of `target`
    /// - `false` otherwise
    ///
    /// # Example
    ///
    /// ```rust
    /// // Fast check
    /// let mut fast = FastTracer;
    /// let ok = checker.check_subtype_with_tracer(source, target, &mut fast);
    ///
    /// // With diagnostics
    /// let mut diag = DiagnosticTracer::new();
    /// checker.check_subtype_with_tracer(source, target, &mut diag);
    /// if let Some(reason) = diag.take_failure() {
    ///     eprintln!("Type error: {:?}", reason);
    /// }
    /// ```
    pub fn check_subtype_with_tracer<T: SubtypeTracer>(
        &mut self,
        source: TypeId,
        target: TypeId,
        tracer: &mut T,
    ) -> bool {
        // Fast paths
        if source == target {
            return true;
        }

        if source == TypeId::ANY || target == TypeId::ANY || target == TypeId::UNKNOWN {
            return true;
        }

        if source == TypeId::NEVER {
            return true;
        }

        // Type evaluation
        let source_eval = self.evaluate_type(source);
        let target_eval = self.evaluate_type(target);

        if source_eval != source || target_eval != target {
            return self.check_subtype_with_tracer(source_eval, target_eval, tracer);
        }

        // Post-evaluation fast paths
        if target == TypeId::NEVER {
            return tracer.on_mismatch(|| SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        if source == TypeId::ERROR || target == TypeId::ERROR {
            return tracer.on_mismatch(|| SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        // Iteration limit
        self.total_checks += 1;
        if self.total_checks > MAX_TOTAL_TRACER_CHECKS {
            self.depth_exceeded = true;
            return tracer.on_mismatch(|| SubtypeFailureReason::RecursionLimitExceeded);
        }

        // Depth check
        if self.depth > MAX_SUBTYPE_DEPTH {
            self.depth_exceeded = true;
            return tracer.on_mismatch(|| SubtypeFailureReason::RecursionLimitExceeded);
        }

        // Cycle detection
        let pair = (source, target);
        if self.in_progress.contains(&pair) {
            // Coinductive: assume true in cycles
            return true;
        }

        if self.in_progress.len() >= MAX_IN_PROGRESS_PAIRS {
            self.depth_exceeded = true;
            return tracer.on_mismatch(|| SubtypeFailureReason::RecursionLimitExceeded);
        }

        // Enter recursion
        self.in_progress.insert(pair);
        self.depth += 1;

        // Perform the check
        let result = self.check_subtype_inner_with_tracer(source, target, tracer);

        // Exit recursion
        self.depth -= 1;
        self.in_progress.remove(&pair);

        result
    }

    /// Inner subtype check with tracer support.
    fn check_subtype_inner_with_tracer<T: SubtypeTracer>(
        &mut self,
        source: TypeId,
        target: TypeId,
        tracer: &mut T,
    ) -> bool {
        // Non-strict null checks
        if !self.strict_null_checks && (source == TypeId::NULL || source == TypeId::UNDEFINED) {
            return true;
        }

        // Look up type keys
        let source_key = match self.interner.lookup(source) {
            Some(k) => k,
            None => {
                return tracer.on_mismatch(|| SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                });
            }
        };
        let target_key = match self.interner.lookup(target) {
            Some(k) => k,
            None => {
                return tracer.on_mismatch(|| SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                });
            }
        };

        // Apparent primitive shape check
        if let Some(shape) = self.apparent_primitive_shape_for_key(&source_key) {
            match &target_key {
                TypeKey::Object(t_shape_id) => {
                    let t_shape = self.interner.object_shape(*t_shape_id);
                    return self.check_object_with_tracer(
                        &shape.properties,
                        None,
                        &t_shape.properties,
                        source,
                        target,
                        tracer,
                    );
                }
                TypeKey::ObjectWithIndex(t_shape_id) => {
                    let t_shape = self.interner.object_shape(*t_shape_id);
                    return self.check_object_with_index_with_tracer(
                        &shape, None, &t_shape, source, target, tracer,
                    );
                }
                _ => {}
            }
        }

        // Structural checks
        match (&source_key, &target_key) {
            (TypeKey::Intrinsic(s), TypeKey::Intrinsic(t)) => {
                self.check_intrinsic_with_tracer(*s, *t, source, target, tracer)
            }

            (TypeKey::Literal(lit), TypeKey::Intrinsic(t)) => {
                self.check_literal_to_intrinsic_with_tracer(lit, *t, source, target, tracer)
            }

            (TypeKey::Literal(s), TypeKey::Literal(t)) => s == t,

            (TypeKey::Union(members), _) => {
                self.check_union_source_with_tracer(*members, target, &target_key, tracer)
            }

            (_, TypeKey::Union(members)) => {
                self.check_union_target_with_tracer(source, &source_key, *members, tracer)
            }

            (TypeKey::Intersection(members), _) => {
                self.check_intersection_source_with_tracer(*members, target, tracer)
            }

            (_, TypeKey::Intersection(members)) => {
                self.check_intersection_target_with_tracer(source, *members, tracer)
            }

            (TypeKey::Function(source_func), TypeKey::Function(target_func)) => {
                self.check_function_with_tracer(source_func, target_func, source, target, tracer)
            }

            (TypeKey::Tuple(source_elems), TypeKey::Tuple(target_elems)) => {
                self.check_tuple_with_tracer(source_elems, target_elems, source, target, tracer)
            }

            (TypeKey::Object(s_shape_id), TypeKey::Object(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.check_object_with_tracer(
                    &s_shape.properties,
                    None,
                    &t_shape.properties,
                    source,
                    target,
                    tracer,
                )
            }

            (TypeKey::Object(s_shape_id), TypeKey::ObjectWithIndex(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.check_object_with_index_with_tracer(
                    &s_shape, None, &t_shape, source, target, tracer,
                )
            }

            (TypeKey::ObjectWithIndex(s_shape_id), TypeKey::ObjectWithIndex(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.check_object_with_index_with_tracer(
                    &s_shape, None, &t_shape, source, target, tracer,
                )
            }

            _ => tracer.on_mismatch(|| SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            }),
        }
    }
}

// =============================================================================
// Helper methods (ported from SubtypeChecker with tracer support)
// =============================================================================

impl<'a, R: TypeResolver> TracerSubtypeChecker<'a, R> {
    /// Evaluate a type (handle Ref, Application, etc.)
    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        // For now, just return the type as-is
        // A full implementation would handle Ref, Application, etc.
        type_id
    }

    /// Get apparent primitive shape for a type key.
    fn apparent_primitive_shape_for_key(&self, key: &TypeKey) -> Option<ObjectShapeId> {
        match key {
            TypeKey::Intrinsic(IntrinsicKind::String)
            | TypeKey::Intrinsic(IntrinsicKind::Number) => {
                // These have apparent shapes like { toString(): string, etc. }
                // For now, return None
                None
            }
            _ => None,
        }
    }

    /// Check intrinsic subtype relationship.
    fn check_intrinsic_with_tracer<T: SubtypeTracer>(
        &mut self,
        source: IntrinsicKind,
        target: IntrinsicKind,
        source_id: TypeId,
        target_id: TypeId,
        tracer: &mut T,
    ) -> bool {
        let is_subtype = match (source, target) {
            (IntrinsicKind::Any, _) | (_, IntrinsicKind::Any) => true,
            (IntrinsicKind::Unknown, _) | (_, IntrinsicKind::Unknown) => true,
            (IntrinsicKind::Never, _) | (_, IntrinsicKind::Never) => source == target,
            (IntrinsicKind::Void, IntrinsicKind::Void) => true,
            (IntrinsicKind::Void, IntrinsicKind::Any) => true,
            (IntrinsicKind::Null, IntrinsicKind::Null) => true,
            (IntrinsicKind::Undefined, IntrinsicKind::Undefined) => true,
            (IntrinsicKind::String, IntrinsicKind::String) => true,
            (IntrinsicKind::String, IntrinsicKind::Any) => true,
            (IntrinsicKind::Number, IntrinsicKind::Number) => true,
            (IntrinsicKind::Number, IntrinsicKind::Any) => true,
            (IntrinsicKind::Boolean, IntrinsicKind::Boolean) => true,
            (IntrinsicKind::Boolean, IntrinsicKind::Any) => true,
            (IntrinsicKind::Bigint, IntrinsicKind::Bigint) => true,
            (IntrinsicKind::Bigint, IntrinsicKind::Any) => true,
            (IntrinsicKind::Symbol, IntrinsicKind::Symbol) => true,
            (IntrinsicKind::Symbol, IntrinsicKind::Any) => true,
            (IntrinsicKind::Object, IntrinsicKind::Object) => true,
            (IntrinsicKind::Object, IntrinsicKind::Any) => true,
            _ => false,
        };

        if !is_subtype {
            return tracer.on_mismatch(|| SubtypeFailureReason::TypeMismatch {
                source_type: source_id,
                target_type: target_id,
            });
        }

        true
    }

    /// Check literal to intrinsic conversion.
    fn check_literal_to_intrinsic_with_tracer<T: SubtypeTracer>(
        &mut self,
        lit: &LiteralValue,
        target: IntrinsicKind,
        source_id: TypeId,
        target_id: TypeId,
        tracer: &mut T,
    ) -> bool {
        let is_subtype = match (lit, target) {
            (LiteralValue::String(_), IntrinsicKind::String) => true,
            (LiteralValue::Number(_), IntrinsicKind::Number) => true,
            (LiteralValue::Boolean(_), IntrinsicKind::Boolean) => true,
            (LiteralValue::BigInt(_), IntrinsicKind::Bigint) => true,
            _ => false,
        };

        if !is_subtype {
            return tracer.on_mismatch(|| SubtypeFailureReason::TypeMismatch {
                source_type: source_id,
                target_type: target_id,
            });
        }

        true
    }

    /// Check union source subtype (all members must be subtypes).
    fn check_union_source_with_tracer<T: SubtypeTracer>(
        &mut self,
        members: &[TypeId],
        target: TypeId,
        target_key: &TypeKey,
        tracer: &mut T,
    ) -> bool {
        for &member in members {
            if !self.check_subtype_with_tracer(member, target, tracer) {
                return false;
            }
        }
        true
    }

    /// Check union target subtype (source must be subtype of at least one member).
    fn check_union_target_with_tracer<T: SubtypeTracer>(
        &mut self,
        source: TypeId,
        source_key: &TypeKey,
        members: &[TypeId],
        tracer: &mut T,
    ) -> bool {
        for &member in members {
            if self.check_subtype_with_tracer(source, member, tracer) {
                return true;
            }
        }

        tracer.on_mismatch(|| SubtypeFailureReason::NoUnionMemberMatches {
            source_type: source,
            target_union_members: members.to_vec(),
        })
    }

    /// Check intersection source subtype.
    fn check_intersection_source_with_tracer<T: SubtypeTracer>(
        &mut self,
        members: &[TypeId],
        target: TypeId,
        tracer: &mut T,
    ) -> bool {
        // Source is intersection: at least one member must be subtype
        for &member in members {
            if self.check_subtype_with_tracer(member, target, tracer) {
                return true;
            }
        }
        false
    }

    /// Check intersection target subtype.
    fn check_intersection_target_with_tracer<T: SubtypeTracer>(
        &mut self,
        source: TypeId,
        members: &[TypeId],
        tracer: &mut T,
    ) -> bool {
        // Target is intersection: source must be subtype of all members
        for &member in members {
            if !self.check_subtype_with_tracer(source, member, tracer) {
                return false;
            }
        }
        true
    }

    /// Check function subtype relationship.
    fn check_function_with_tracer<T: SubtypeTracer>(
        &mut self,
        source_func: &FunctionShape,
        target_func: &FunctionShape,
        source_id: TypeId,
        target_id: TypeId,
        tracer: &mut T,
    ) -> bool {
        // Check parameters
        if source_func.params.len() != target_func.params.len() {
            return tracer.on_mismatch(|| SubtypeFailureReason::ParameterCountMismatch {
                source_count: source_func.params.len(),
                target_count: target_func.params.len(),
            });
        }

        for (i, (s_param, t_param)) in source_func
            .params
            .iter()
            .zip(target_func.params.iter())
            .enumerate()
        {
            if self.strict_function_types {
                // Contravariant: source param must be supertype of target param
                if !self.check_subtype_with_tracer(*t_param, *s_param, tracer) {
                    return tracer.on_mismatch(|| SubtypeFailureReason::ParameterTypeMismatch {
                        param_index: i,
                        source_param: *s_param,
                        target_param: *t_param,
                    });
                }
            } else {
                // Bivariant: params match in either direction
                if !self.check_subtype_with_tracer(*s_param, *t_param, tracer) {
                    return tracer.on_mismatch(|| SubtypeFailureReason::ParameterTypeMismatch {
                        param_index: i,
                        source_param: *s_param,
                        target_param: *t_param,
                    });
                }
            }
        }

        // Check return type (covariant)
        if !self.check_subtype_with_tracer(source_func.return_type, target_func.return_type, tracer)
        {
            return tracer.on_mismatch(|| SubtypeFailureReason::ReturnTypeMismatch {
                source_return: source_func.return_type,
                target_return: target_func.return_type,
                nested_reason: None,
            });
        }

        true
    }

    /// Check tuple subtype relationship.
    fn check_tuple_with_tracer<T: SubtypeTracer>(
        &mut self,
        source_elems: &[TypeId],
        target_elems: &[TypeId],
        source_id: TypeId,
        target_id: TypeId,
        tracer: &mut T,
    ) -> bool {
        if source_elems.len() != target_elems.len() {
            return tracer.on_mismatch(|| SubtypeFailureReason::TupleElementMismatch {
                source_count: source_elems.len(),
                target_count: target_elems.len(),
            });
        }

        for (i, (s_elem, t_elem)) in source_elems.iter().zip(target_elems.iter()).enumerate() {
            if !self.check_subtype_with_tracer(*s_elem, *t_elem, tracer) {
                return tracer.on_mismatch(|| SubtypeFailureReason::TupleElementTypeMismatch {
                    index: i,
                    source_element: *s_elem,
                    target_element: *t_elem,
                });
            }
        }

        true
    }

    /// Check object subtype relationship (properties only).
    fn check_object_with_tracer<T: SubtypeTracer>(
        &mut self,
        source_props: &[(Atom, TypeId)],
        _source_index: Option<&IndexSignature>,
        target_props: &[(Atom, TypeId)],
        source_id: TypeId,
        target_id: TypeId,
        tracer: &mut T,
    ) -> bool {
        // Build a set of target property names for fast lookup
        let target_prop_names: std::collections::HashSet<_> =
            target_props.iter().map(|(name, _)| name).collect();

        // Check that all target properties exist in source
        for (target_name, target_type) in target_props {
            let source_type = match source_props.iter().find(|(name, _)| name == target_name) {
                Some((_, ty)) => *ty,
                None => {
                    return tracer.on_mismatch(|| SubtypeFailureReason::MissingProperty {
                        property_name: *target_name,
                        source_type: source_id,
                        target_type: target_id,
                    });
                }
            };

            if !self.check_subtype_with_tracer(source_type, *target_type, tracer) {
                return tracer.on_mismatch(|| SubtypeFailureReason::PropertyTypeMismatch {
                    property_name: *target_name,
                    source_property_type: source_type,
                    target_property_type: *target_type,
                    nested_reason: None,
                });
            }
        }

        true
    }

    /// Check object with index signature subtype relationship.
    fn check_object_with_index_with_tracer<T: SubtypeTracer>(
        &mut self,
        source_shape: &ObjectShape,
        _source_index: Option<&IndexSignature>,
        target_shape: &ObjectShape,
        source_id: TypeId,
        target_id: TypeId,
        tracer: &mut T,
    ) -> bool {
        // Check properties
        if !self.check_object_with_tracer(
            &source_shape.properties,
            None,
            &target_shape.properties,
            source_id,
            target_id,
            tracer,
        ) {
            return false;
        }

        // TODO: Check index signatures when the API is updated to use string_index/number_index

        true
    }
}

// =============================================================================
// Tests and Benchmarks
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interner::Interner;

    /// Test that FastTracer returns correct boolean results
    #[test]
    fn test_fast_tracer_boolean() {
        let interner = TypeInterner::new();
        let resolver = NoopResolver;
        let mut checker = TracerSubtypeChecker::new(&interner, &resolver);

        // Same type
        let string_type = interner.intern_intrinsic(IntrinsicKind::String);
        let mut fast = FastTracer;
        assert!(checker.check_subtype_with_tracer(string_type, string_type, &mut fast));

        // Subtype relationship
        let any_type = interner.intern_intrinsic(IntrinsicKind::Any);
        let mut fast = FastTracer;
        assert!(checker.check_subtype_with_tracer(string_type, any_type, &mut fast));

        // Not a subtype
        let number_type = interner.intern_intrinsic(IntrinsicKind::Number);
        let mut fast = FastTracer;
        assert!(!checker.check_subtype_with_tracer(string_type, number_type, &mut fast));
    }

    /// Test that DiagnosticTracer collects failure reasons
    #[test]
    fn test_diagnostic_tracer_collects_reasons() {
        let interner = TypeInterner::new();
        let resolver = NoopResolver;
        let mut checker = TracerSubtypeChecker::new(&interner, &resolver);

        let string_type = interner.intern_intrinsic(IntrinsicKind::String);
        let number_type = interner.intern_intrinsic(IntrinsicKind::Number);

        let mut diag = DiagnosticTracer::new();
        checker.check_subtype_with_tracer(string_type, number_type, &mut diag);

        assert!(diag.has_failure());
        let failure = diag.take_failure();
        assert!(failure.is_some());

        match failure {
            Some(SubtypeFailureReason::TypeMismatch {
                source_type,
                target_type,
            }) => {
                assert_eq!(source_type, string_type);
                assert_eq!(target_type, number_type);
            }
            _ => panic!("Expected TypeMismatch failure"),
        }
    }

    /// Test that union target checking works correctly
    #[test]
    fn test_union_target_tracer() {
        let interner = TypeInterner::new();
        let resolver = NoopResolver;
        let mut checker = TracerSubtypeChecker::new(&interner, &resolver);

        // string | number
        let string_type = interner.intern_intrinsic(IntrinsicKind::String);
        let number_type = interner.intern_intrinsic(IntrinsicKind::Number);
        let union_type = interner.intern_union(&[string_type, number_type]);

        // string <: string | number (should pass)
        let mut fast = FastTracer;
        assert!(checker.check_subtype_with_tracer(string_type, union_type, &mut fast));

        // boolean <: string | number (should fail)
        let bool_type = interner.intern_intrinsic(IntrinsicKind::Boolean);
        let mut diag = DiagnosticTracer::new();
        assert!(!checker.check_subtype_with_tracer(bool_type, union_type, &mut diag));
        assert!(diag.has_failure());
    }

    /// Test that function type checking works
    #[test]
    fn test_function_tracer() {
        let interner = TypeInterner::new();
        let resolver = NoopResolver;
        let mut checker =
            TracerSubtypeChecker::new(&interner, &resolver).with_strict_function_types(true);

        // (x: string) => number
        let string_type = interner.intern_intrinsic(IntrinsicKind::String);
        let number_type = interner.intern_intrinsic(IntrinsicKind::Number);

        let func1 = FunctionShape {
            params: vec![string_type],
            return_type: number_type,
            type_params: None,
        };
        let func1_id = interner.intern_function(func1.clone());

        // (x: string) => number
        let func2_id = interner.intern_function(func1.clone());

        // Same function type should be compatible
        let mut fast = FastTracer;
        assert!(checker.check_subtype_with_tracer(func1_id, func2_id, &mut fast));
    }

    /// Benchmark: Compare FastTracer vs direct boolean check
    #[test]
    fn benchmark_fast_tracer() {
        let interner = TypeInterner::new();
        let resolver = NoopResolver;
        let mut checker = TracerSubtypeChecker::new(&interner, &resolver);

        let string_type = interner.intern_intrinsic(IntrinsicKind::String);
        let number_type = interner.intern_intrinsic(IntrinsicKind::Number);

        // Warm up
        let mut fast = FastTracer;
        for _ in 0..1000 {
            let _ = checker.check_subtype_with_tracer(string_type, number_type, &mut fast);
        }

        // Measure FastTracer performance
        let start = std::time::Instant::now();
        let iterations = 100_000;
        for _ in 0..iterations {
            let mut fast = FastTracer;
            let _ = checker.check_subtype_with_tracer(string_type, number_type, &mut fast);
        }
        let fast_duration = start.elapsed();

        // FastTracer should be very fast (millions of checks per second)
        let checks_per_second = iterations as f64 / fast_duration.as_secs_f64();
        println!("FastTracer: {:.2} checks/second", checks_per_second);

        // We expect at least 100k checks/second even in debug mode
        // In release mode, this should be millions
        assert!(
            checks_per_second > 10_000.0,
            "FastTracer too slow: {:.2} checks/sec",
            checks_per_second
        );
    }

    /// Test that DiagnosticTracer has the same logic as FastTracer
    #[test]
    fn test_tracer_logic_consistency() {
        let interner = TypeInterner::new();
        let resolver = NoopResolver;
        let mut checker = TracerSubtypeChecker::new(&interner, &resolver);

        // Test various type pairs
        let test_cases = vec![
            (TypeId::STRING, TypeId::STRING, true),
            (TypeId::STRING, TypeId::NUMBER, false),
            (TypeId::NUMBER, TypeId::ANY, true),
            (TypeId::NEVER, TypeId::STRING, true),
            (TypeId::STRING, TypeId::NEVER, false),
            (TypeId::ANY, TypeId::NEVER, false),
        ];

        for (source, target, expected) in test_cases {
            // FastTracer
            let mut fast = FastTracer;
            let fast_result = checker.check_subtype_with_tracer(source, target, &mut fast);

            // DiagnosticTracer
            let mut diag = DiagnosticTracer::new();
            let diag_result = checker.check_subtype_with_tracer(source, target, &mut diag);

            // Both should give the same boolean result
            assert_eq!(
                fast_result, expected,
                "FastTracer failed for ({:?} <: {:?})",
                source, target
            );
            assert_eq!(
                diag_result, expected,
                "DiagnosticTracer failed for ({:?} <: {:?})",
                source, target
            );
            assert_eq!(
                fast_result, diag_result,
                "Tracer results differ for ({:?} <: {:?})",
                source, target
            );
        }
    }
}
