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
//! ```rust,ignore
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

use crate::TypeDatabase;
use crate::diagnostics::{SubtypeFailureReason, SubtypeTracer};
use crate::subtype::{MAX_SUBTYPE_DEPTH, TypeResolver};
use crate::types::*;
use tsz_common::limits;

#[cfg(test)]
use crate::TypeInterner;
#[cfg(test)]
use crate::diagnostics::{DiagnosticTracer, FastTracer};
#[cfg(test)]
use crate::subtype::NoopResolver;

/// Maximum total subtype checks allowed per tracer-based check.
const MAX_TOTAL_TRACER_CHECKS: u32 = limits::MAX_TOTAL_TRACER_CHECKS;

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
    /// Reference to the type resolver (for resolving Ref types).
    #[allow(dead_code)]
    pub(crate) resolver: &'a R,
    /// Unified recursion guard for cycle detection, depth, and iteration limits.
    pub(crate) guard: crate::recursion::RecursionGuard<(TypeId, TypeId)>,
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
            guard: crate::recursion::RecursionGuard::new(
                MAX_SUBTYPE_DEPTH,
                MAX_TOTAL_TRACER_CHECKS,
            ),
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
        // Fast paths - identity
        if source == target {
            return true;
        }

        // never is subtype of everything (bottom type)
        if source == TypeId::NEVER {
            return true;
        }

        // Only never is subtype of never
        if target == TypeId::NEVER {
            return tracer.on_mismatch(|| SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        // Everything is subtype of any and unknown
        if target == TypeId::ANY || target == TypeId::UNKNOWN {
            return true;
        }

        // Type evaluation
        let source_eval = self.evaluate_type(source);
        let target_eval = self.evaluate_type(target);

        if source_eval != source || target_eval != target {
            return self.check_subtype_with_tracer(source_eval, target_eval, tracer);
        }

        if source == TypeId::ERROR || target == TypeId::ERROR {
            // Error types ARE compatible to suppress cascading errors
            return true;
        }

        // Unified enter: checks iterations, depth, cycle detection, and visiting set size
        let pair = (source, target);
        match self.guard.enter(pair) {
            crate::recursion::RecursionResult::Entered => {}
            crate::recursion::RecursionResult::Cycle => {
                // Coinductive: assume true in cycles
                return true;
            }
            crate::recursion::RecursionResult::DepthExceeded
            | crate::recursion::RecursionResult::IterationExceeded => {
                return tracer.on_mismatch(|| SubtypeFailureReason::RecursionLimitExceeded);
            }
        }

        // Perform the check
        let result = self.check_subtype_inner_with_tracer(source, target, tracer);

        // Exit recursion
        self.guard.leave(pair);

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

        // Apparent primitive shape check (for checking primitives against object interfaces)
        if let Some(ref shape) = self.apparent_primitive_shape_for_key(&source_key) {
            match &target_key {
                TypeKey::Object(t_shape_id) => {
                    let t_shape = self.interner.object_shape(*t_shape_id);
                    return self.check_object_with_tracer(
                        &shape.properties,
                        &t_shape.properties,
                        source,
                        target,
                        tracer,
                    );
                }
                TypeKey::ObjectWithIndex(t_shape_id) => {
                    let t_shape = self.interner.object_shape(*t_shape_id);
                    return self.check_object_with_index_with_tracer(
                        shape, &t_shape, source, target, tracer,
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

            (TypeKey::Union(members_id), _) => {
                self.check_union_source_with_tracer(*members_id, target, &target_key, tracer)
            }

            (_, TypeKey::Union(members_id)) => {
                self.check_union_target_with_tracer(source, &source_key, *members_id, tracer)
            }

            (TypeKey::Intersection(members_id), _) => {
                self.check_intersection_source_with_tracer(*members_id, target, tracer)
            }

            (_, TypeKey::Intersection(members_id)) => {
                self.check_intersection_target_with_tracer(source, *members_id, tracer)
            }

            (TypeKey::Function(source_func_id), TypeKey::Function(target_func_id)) => self
                .check_function_with_tracer(
                    *source_func_id,
                    *target_func_id,
                    source,
                    target,
                    tracer,
                ),

            (TypeKey::Tuple(source_list_id), TypeKey::Tuple(target_list_id)) => self
                .check_tuple_with_tracer(*source_list_id, *target_list_id, source, target, tracer),

            (TypeKey::Object(s_shape_id), TypeKey::Object(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.check_object_with_tracer(
                    &s_shape.properties,
                    &t_shape.properties,
                    source,
                    target,
                    tracer,
                )
            }

            (TypeKey::Object(s_shape_id), TypeKey::ObjectWithIndex(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.check_object_with_index_with_tracer(&s_shape, &t_shape, source, target, tracer)
            }

            (TypeKey::ObjectWithIndex(s_shape_id), TypeKey::ObjectWithIndex(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.check_object_with_index_with_tracer(&s_shape, &t_shape, source, target, tracer)
            }

            (TypeKey::ObjectWithIndex(s_shape_id), TypeKey::Object(t_shape_id)) => {
                let s_shape = self.interner.object_shape(*s_shape_id);
                let t_shape = self.interner.object_shape(*t_shape_id);
                self.check_object_with_index_with_tracer(&s_shape, &t_shape, source, target, tracer)
            }

            (TypeKey::Array(source_elem), TypeKey::Array(target_elem)) => {
                // Arrays are covariant
                self.check_subtype_with_tracer(*source_elem, *target_elem, tracer)
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
    /// Returns an ObjectShape for primitives that have apparent methods (e.g., String.prototype methods).
    /// Currently returns None as apparent shapes are not yet implemented.
    fn apparent_primitive_shape_for_key(&self, _key: &TypeKey) -> Option<ObjectShape> {
        // TODO: Return apparent shapes for primitives (String, Number, etc.)
        // For example, string has { toString(): string, valueOf(): string, ... }
        None
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
        // Note: many cases are handled in fast paths before this function is called,
        // but we still need to handle them here for completeness.
        let is_subtype = match (source, target) {
            // Same type
            (s, t) if s == t => true,
            // never is subtype of everything (bottom type) - but this should be caught earlier
            (IntrinsicKind::Never, _) => true,
            // Only never is subtype of never
            (_, IntrinsicKind::Never) => false,
            // any is subtype of everything except never (already handled above)
            (IntrinsicKind::Any, _) => true,
            // Everything is subtype of any
            (_, IntrinsicKind::Any) => true,
            // Everything is subtype of unknown
            (_, IntrinsicKind::Unknown) => true,
            // unknown is only subtype of unknown/any (already handled)
            (IntrinsicKind::Unknown, _) => false,
            // Function is subtype of object
            (IntrinsicKind::Function, IntrinsicKind::Object) => true,
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
        members_id: TypeListId,
        target: TypeId,
        _target_key: &TypeKey,
        tracer: &mut T,
    ) -> bool {
        let members = self.interner.type_list(members_id);
        for &member in members.iter() {
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
        _source_key: &TypeKey,
        members_id: TypeListId,
        tracer: &mut T,
    ) -> bool {
        let members = self.interner.type_list(members_id);
        for &member in members.iter() {
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
        members_id: TypeListId,
        target: TypeId,
        tracer: &mut T,
    ) -> bool {
        // Source is intersection: at least one member must be subtype
        let members = self.interner.type_list(members_id);
        for &member in members.iter() {
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
        members_id: TypeListId,
        tracer: &mut T,
    ) -> bool {
        // Target is intersection: source must be subtype of all members
        let members = self.interner.type_list(members_id);
        for &member in members.iter() {
            if !self.check_subtype_with_tracer(source, member, tracer) {
                return false;
            }
        }
        true
    }

    /// Check function subtype relationship.
    fn check_function_with_tracer<T: SubtypeTracer>(
        &mut self,
        source_shape_id: FunctionShapeId,
        target_shape_id: FunctionShapeId,
        _source_id: TypeId,
        _target_id: TypeId,
        tracer: &mut T,
    ) -> bool {
        let source_func = self.interner.function_shape(source_shape_id);
        let target_func = self.interner.function_shape(target_shape_id);

        // Check parameters - target can have more required params than source
        // (source is assignable if it accepts at least as many args as target requires)
        let source_params = &source_func.params;
        let target_params = &target_func.params;

        // Count required params
        let source_required = source_params
            .iter()
            .filter(|p| !p.optional && !p.rest)
            .count();
        let target_required = target_params
            .iter()
            .filter(|p| !p.optional && !p.rest)
            .count();

        // Source must accept at least as many required params as target
        if source_required > target_required {
            return tracer.on_mismatch(|| SubtypeFailureReason::ParameterCountMismatch {
                source_count: source_required,
                target_count: target_required,
            });
        }

        // Check parameter types
        let param_count = source_params.len().min(target_params.len());
        for i in 0..param_count {
            let s_param = &source_params[i];
            let t_param = &target_params[i];

            if self.strict_function_types && !source_func.is_method {
                // Contravariant: target param must be subtype of source param
                if !self.check_subtype_with_tracer(t_param.type_id, s_param.type_id, tracer) {
                    return tracer.on_mismatch(|| SubtypeFailureReason::ParameterTypeMismatch {
                        param_index: i,
                        source_param: s_param.type_id,
                        target_param: t_param.type_id,
                    });
                }
            } else {
                // Bivariant: params match in either direction
                let forward =
                    self.check_subtype_with_tracer(s_param.type_id, t_param.type_id, tracer);
                let backward =
                    self.check_subtype_with_tracer(t_param.type_id, s_param.type_id, tracer);
                if !forward && !backward {
                    return tracer.on_mismatch(|| SubtypeFailureReason::ParameterTypeMismatch {
                        param_index: i,
                        source_param: s_param.type_id,
                        target_param: t_param.type_id,
                    });
                }
            }
        }

        // Check return type (covariant)
        // Special case: void return type accepts any return
        if target_func.return_type != TypeId::VOID {
            if !self.check_subtype_with_tracer(
                source_func.return_type,
                target_func.return_type,
                tracer,
            ) {
                return tracer.on_mismatch(|| SubtypeFailureReason::ReturnTypeMismatch {
                    source_return: source_func.return_type,
                    target_return: target_func.return_type,
                    nested_reason: None,
                });
            }
        }

        true
    }

    /// Check tuple subtype relationship.
    fn check_tuple_with_tracer<T: SubtypeTracer>(
        &mut self,
        source_list_id: TupleListId,
        target_list_id: TupleListId,
        _source_id: TypeId,
        _target_id: TypeId,
        tracer: &mut T,
    ) -> bool {
        let source_elems = self.interner.tuple_list(source_list_id);
        let target_elems = self.interner.tuple_list(target_list_id);

        // Count required elements (non-optional, non-rest)
        let source_required = source_elems
            .iter()
            .filter(|e| !e.optional && !e.rest)
            .count();
        let target_required = target_elems
            .iter()
            .filter(|e| !e.optional && !e.rest)
            .count();

        // Source must have at least as many required elements as target
        if source_required < target_required {
            return tracer.on_mismatch(|| SubtypeFailureReason::TupleElementMismatch {
                source_count: source_elems.len(),
                target_count: target_elems.len(),
            });
        }

        // Check element types for the overlapping portion
        let check_count = source_elems.len().min(target_elems.len());
        for i in 0..check_count {
            let s_elem = &source_elems[i];
            let t_elem = &target_elems[i];

            // If target element is optional but source isn't, that's still OK
            // If source element is optional but target requires it, check below

            if !self.check_subtype_with_tracer(s_elem.type_id, t_elem.type_id, tracer) {
                return tracer.on_mismatch(|| SubtypeFailureReason::TupleElementTypeMismatch {
                    index: i,
                    source_element: s_elem.type_id,
                    target_element: t_elem.type_id,
                });
            }
        }

        true
    }

    /// Check object subtype relationship (properties only).
    fn check_object_with_tracer<T: SubtypeTracer>(
        &mut self,
        source_props: &[PropertyInfo],
        target_props: &[PropertyInfo],
        source_id: TypeId,
        target_id: TypeId,
        tracer: &mut T,
    ) -> bool {
        // Check that all target properties exist in source with compatible types
        for target_prop in target_props {
            let source_prop = source_props.iter().find(|p| p.name == target_prop.name);

            match source_prop {
                Some(src_prop) => {
                    // Check property type compatibility (covariant for read)
                    if !self.check_subtype_with_tracer(
                        src_prop.type_id,
                        target_prop.type_id,
                        tracer,
                    ) {
                        return tracer.on_mismatch(|| SubtypeFailureReason::PropertyTypeMismatch {
                            property_name: target_prop.name,
                            source_property_type: src_prop.type_id,
                            target_property_type: target_prop.type_id,
                            nested_reason: None,
                        });
                    }

                    // Check optional compatibility: source optional cannot satisfy required target
                    if src_prop.optional && !target_prop.optional {
                        return tracer.on_mismatch(|| {
                            SubtypeFailureReason::OptionalPropertyRequired {
                                property_name: target_prop.name,
                            }
                        });
                    }
                }
                None => {
                    // Property missing - error unless target property is optional
                    if !target_prop.optional {
                        return tracer.on_mismatch(|| SubtypeFailureReason::MissingProperty {
                            property_name: target_prop.name,
                            source_type: source_id,
                            target_type: target_id,
                        });
                    }
                }
            }
        }

        true
    }

    /// Check object with index signature subtype relationship.
    fn check_object_with_index_with_tracer<T: SubtypeTracer>(
        &mut self,
        source_shape: &ObjectShape,
        target_shape: &ObjectShape,
        source_id: TypeId,
        target_id: TypeId,
        tracer: &mut T,
    ) -> bool {
        // Check properties
        if !self.check_object_with_tracer(
            &source_shape.properties,
            &target_shape.properties,
            source_id,
            target_id,
            tracer,
        ) {
            return false;
        }

        // Check string index signature compatibility
        if let Some(ref target_idx) = target_shape.string_index {
            match &source_shape.string_index {
                Some(source_idx) => {
                    if !self.check_subtype_with_tracer(
                        source_idx.value_type,
                        target_idx.value_type,
                        tracer,
                    ) {
                        return tracer.on_mismatch(|| {
                            SubtypeFailureReason::IndexSignatureMismatch {
                                index_kind: "string",
                                source_value_type: source_idx.value_type,
                                target_value_type: target_idx.value_type,
                            }
                        });
                    }
                }
                None => {
                    // Source doesn't have string index but target does
                    // All source properties must be compatible with target's index signature
                    for prop in &source_shape.properties {
                        if !self.check_subtype_with_tracer(
                            prop.type_id,
                            target_idx.value_type,
                            tracer,
                        ) {
                            return tracer.on_mismatch(|| {
                                SubtypeFailureReason::IndexSignatureMismatch {
                                    index_kind: "string",
                                    source_value_type: prop.type_id,
                                    target_value_type: target_idx.value_type,
                                }
                            });
                        }
                    }
                }
            }
        }

        // Check number index signature compatibility
        if let Some(ref target_idx) = target_shape.number_index {
            if let Some(ref source_idx) = source_shape.number_index {
                if !self.check_subtype_with_tracer(
                    source_idx.value_type,
                    target_idx.value_type,
                    tracer,
                ) {
                    return tracer.on_mismatch(|| SubtypeFailureReason::IndexSignatureMismatch {
                        index_kind: "number",
                        source_value_type: source_idx.value_type,
                        target_value_type: target_idx.value_type,
                    });
                }
            }
        }

        true
    }
}

// =============================================================================
// Tests and Benchmarks
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that FastTracer returns correct boolean results
    #[test]
    fn test_fast_tracer_boolean() {
        let interner = TypeInterner::new();
        let resolver = NoopResolver;
        let mut checker = TracerSubtypeChecker::new(&interner, &resolver);

        // Same type - use built-in constants
        let string_type = TypeId::STRING;
        let mut fast = FastTracer;
        assert!(checker.check_subtype_with_tracer(string_type, string_type, &mut fast));

        // Subtype relationship
        let any_type = TypeId::ANY;
        let mut fast = FastTracer;
        assert!(checker.check_subtype_with_tracer(string_type, any_type, &mut fast));

        // Not a subtype
        let number_type = TypeId::NUMBER;
        let mut fast = FastTracer;
        assert!(!checker.check_subtype_with_tracer(string_type, number_type, &mut fast));
    }

    /// Test that DiagnosticTracer collects failure reasons
    #[test]
    fn test_diagnostic_tracer_collects_reasons() {
        let interner = TypeInterner::new();
        let resolver = NoopResolver;
        let mut checker = TracerSubtypeChecker::new(&interner, &resolver);

        let string_type = TypeId::STRING;
        let number_type = TypeId::NUMBER;

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
        let string_type = TypeId::STRING;
        let number_type = TypeId::NUMBER;
        let union_type = interner.union(vec![string_type, number_type]);

        // string <: string | number (should pass)
        let mut fast = FastTracer;
        assert!(checker.check_subtype_with_tracer(string_type, union_type, &mut fast));

        // boolean <: string | number (should fail)
        let bool_type = TypeId::BOOLEAN;
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
        let string_type = TypeId::STRING;
        let number_type = TypeId::NUMBER;

        let func1 = FunctionShape {
            params: vec![ParamInfo {
                name: None,
                type_id: string_type,
                optional: false,
                rest: false,
            }],
            return_type: number_type,
            type_params: Vec::new(),
            this_type: None,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };
        let func1_id = interner.function(func1.clone());

        // (x: string) => number (same type)
        let func2_id = interner.function(func1);

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

        let string_type = TypeId::STRING;
        let number_type = TypeId::NUMBER;

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

        // Test various type pairs using built-in constants
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
