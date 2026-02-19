//! Visitor-based dispatch for structural subtype checking.
//!
//! Contains `SubtypeVisitor` which implements the `TypeVisitor` trait,
//! dispatching to appropriate `SubtypeChecker` methods based on the
//! source type structure.

use crate::def::DefId;
use crate::diagnostics::SubtypeFailureReason;
use crate::subtype::{SubtypeChecker, SubtypeResult};
use crate::type_resolver::TypeResolver;
use crate::types::{
    CallableShapeId, ConditionalTypeId, FunctionShapeId, IntrinsicKind, LiteralValue, MappedTypeId,
    ObjectFlags, ObjectShape, ObjectShapeId, StringIntrinsicKind, SymbolRef, TupleListId,
    TypeApplicationId, TypeData, TypeId, TypeListId, TypeParamInfo,
};
use crate::visitor::{
    TypeVisitor, array_element_type, callable_shape_id, enum_components, function_shape_id,
    intrinsic_kind, literal_value, object_shape_id, object_with_index_shape_id,
    readonly_inner_type, tuple_list_id,
};

// =============================================================================
// Task #48: SubtypeVisitor - Visitor Pattern for Subtype Checking
// =============================================================================

/// Visitor for structural subtype checking.
///
/// This visitor implements the North Star Rule 2 (Visitor Pattern for type operations).
/// It wraps a mutable reference to `SubtypeChecker` and the target type, dispatching
/// to the appropriate checker methods based on the source type's structure.
///
/// ## Design
///
/// - **Binary Relation**: Subtyping is binary (A <: B), but visitor is unary (visits A).
///   The target type B is stored as a field.
/// - **Double Dispatch**: Many visitor methods must inspect both source and target kinds
///   to determine which checker method to call (e.g., tuple-to-tuple vs tuple-to-array).
/// - **Coinduction**: All recursive checks MUST go through `self.checker.check_subtype()`
///   to ensure cycle detection works correctly.
/// - **Pre-checks**: Special cases (apparent shapes, target-is-union) remain in
///   `check_subtype_inner` before dispatching to the visitor.
pub struct SubtypeVisitor<'a, 'b, R: TypeResolver> {
    /// Reference to the parent checker (for recursive checks and state).
    pub checker: &'a mut SubtypeChecker<'b, R>,
    /// The source type being visited (the "A" in "A <: B").
    /// Stored because some delegation methods need the full `TypeId`, not just unpacked data.
    pub source: TypeId,
    /// The target type we're checking against (the "B" in "A <: B").
    pub target: TypeId,
}

impl<'a, 'b, R: TypeResolver> TypeVisitor for SubtypeVisitor<'a, 'b, R> {
    type Output = SubtypeResult;

    // Default: return False for unimplemented variants
    fn default_output() -> Self::Output {
        SubtypeResult::False
    }

    // Core intrinsics - delegate to checker
    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output {
        if let Some(t_kind) = intrinsic_kind(self.checker.interner, self.target) {
            return self.checker.check_intrinsic_subtype(kind, t_kind);
        }
        if self.checker.is_boxed_primitive_subtype(kind, self.target) {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output {
        if let Some(t_kind) = intrinsic_kind(self.checker.interner, self.target) {
            return self.checker.check_literal_to_intrinsic(value, t_kind);
        }
        if let Some(t_lit) = literal_value(self.checker.interner, self.target) {
            return if value == &t_lit {
                SubtypeResult::True
            } else {
                SubtypeResult::False
            };
        }
        // Trace: Literal doesn't match target
        if let Some(tracer) = &mut self.checker.tracer
            && !tracer.on_mismatch_dyn(SubtypeFailureReason::LiteralTypeMismatch {
                source_type: self.source,
                target_type: self.target,
            })
        {
            return SubtypeResult::False;
        }
        SubtypeResult::False
    }

    fn visit_array(&mut self, element_type: TypeId) -> Self::Output {
        if let Some(t_elem) = array_element_type(self.checker.interner, self.target) {
            self.checker.check_subtype(element_type, t_elem)
        } else {
            // Target is not an array type. Try to resolve Array<element_type> via the
            // Array<T> interface and check structurally.
            // This handles cases like: number[] <: Iterable<number>, number[] <: { length: number; toString(): string }
            if let Some(result) = self
                .checker
                .check_array_interface_subtype(element_type, self.target)
            {
                return result;
            }
            // Trace: Array source doesn't match non-array target
            if let Some(tracer) = &mut self.checker.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: self.source,
                    target_type: self.target,
                })
            {
                return SubtypeResult::False;
            }
            SubtypeResult::False
        }
    }

    fn visit_tuple(&mut self, list_id: u32) -> Self::Output {
        // Double dispatch: check target type to determine which helper to call
        // Tuple <: Tuple, Tuple <: Array, Array <: Tuple
        let s_tuple_id = TupleListId(list_id);

        if let Some(t_list) = tuple_list_id(self.checker.interner, self.target) {
            // Tuple <: Tuple
            let s_elems = self.checker.interner.tuple_list(s_tuple_id);
            let t_elems = self.checker.interner.tuple_list(t_list);
            self.checker.check_tuple_subtype(&s_elems, &t_elems)
        } else if let Some(t_elem) = array_element_type(self.checker.interner, self.target) {
            // Tuple <: Array
            self.checker
                .check_tuple_to_array_subtype(s_tuple_id, t_elem)
        } else {
            // Trace: Tuple source doesn't match non-tuple/non-array target
            if let Some(tracer) = &mut self.checker.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: self.source,
                    target_type: self.target,
                })
            {
                return SubtypeResult::False;
            }
            SubtypeResult::False
        }
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        // Union <: Target requires ALL members to be subtypes
        let member_list = self.checker.interner.type_list(TypeListId(list_id));
        for &member in member_list.iter() {
            if !self.checker.check_subtype(member, self.target).is_true() {
                // Trace: No union member matches target
                if let Some(tracer) = &mut self.checker.tracer
                    && !tracer.on_mismatch_dyn(SubtypeFailureReason::NoUnionMemberMatches {
                        source_type: self.source,
                        target_union_members: vec![self.target],
                    })
                {
                    return SubtypeResult::False;
                }
                return SubtypeResult::False;
            }
        }
        SubtypeResult::True
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        // Special case: T & SomeType <: T
        // If target is a type parameter and it appears as a member of the intersection,
        // the intersection is a more specific version (T with null/undefined excluded)
        // and is assignable to the type parameter.
        // This handles the common pattern: T & {} to exclude null/undefined from T.
        // NOTE: This code path is rarely reached because check_subtype_inner has an
        // earlier check when target is a type parameter (line 2575). This code exists
        // for cases where the intersection check happens via other paths.

        // Intersection <: Target requires AT LEAST ONE member to be subtype
        let member_list = self.checker.interner.type_list(TypeListId(list_id));
        for &member in member_list.iter() {
            if self.checker.check_subtype(member, self.target).is_true() {
                return SubtypeResult::True;
            }
        }

        // Special case: If target is an object type, check if MERGED properties satisfy it
        // This handles cases like: { a: string } & { b: number } <: { a: string; b: number }
        if object_shape_id(self.checker.interner, self.target).is_some()
            || object_with_index_shape_id(self.checker.interner, self.target).is_some()
        {
            use crate::objects::{PropertyCollectionResult, collect_properties};

            match collect_properties(self.source, self.checker.interner, self.checker.resolver) {
                PropertyCollectionResult::Any => {
                    // any & T = any, so check if any is subtype of target
                    return self.checker.check_subtype(TypeId::ANY, self.target);
                }
                PropertyCollectionResult::NonObject => {
                    // No object properties to check
                }
                PropertyCollectionResult::Properties {
                    properties,
                    string_index,
                    number_index,
                } => {
                    if !properties.is_empty() || string_index.is_some() || number_index.is_some() {
                        let merged_type = if string_index.is_some() || number_index.is_some() {
                            self.checker.interner.object_with_index(ObjectShape {
                                flags: ObjectFlags::empty(),
                                properties,
                                string_index,
                                number_index,
                                symbol: None,
                            })
                        } else {
                            self.checker.interner.object(properties)
                        };
                        if self
                            .checker
                            .check_subtype(merged_type, self.target)
                            .is_true()
                        {
                            return SubtypeResult::True;
                        }
                    }
                }
            }
        }

        // Trace: No intersection member matches target
        if let Some(tracer) = &mut self.checker.tracer
            && !tracer.on_mismatch_dyn(SubtypeFailureReason::NoIntersectionMemberMatches {
                source_type: self.source,
                target_type: self.target,
            })
        {
            return SubtypeResult::False;
        }
        SubtypeResult::False
    }

    fn visit_type_parameter(&mut self, param_info: &TypeParamInfo) -> Self::Output {
        self.checker
            .check_type_parameter_subtype(param_info, self.target)
    }

    fn visit_recursive(&mut self, _de_bruijn_index: u32) -> Self::Output {
        // Recursive references are valid in coinductive semantics
        SubtypeResult::True
    }

    fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
        // Resolve the Lazy(DefId) type using the resolver
        let resolved = self
            .checker
            .resolver
            .resolve_lazy(DefId(def_id), self.checker.interner)
            .unwrap_or(self.source);

        // If resolution succeeded and changed the type, restart the check
        // This is critical for coinductive cycle detection to work correctly
        if resolved != self.source {
            self.checker.check_subtype(resolved, self.target)
        } else {
            // Resolution failed or returned the same type - fall through
            SubtypeResult::False
        }
    }

    fn visit_ref(&mut self, symbol_ref: u32) -> Self::Output {
        let resolved = self
            .checker
            .resolver
            .resolve_symbol_ref(SymbolRef(symbol_ref), self.checker.interner)
            .unwrap_or(self.source);

        // If resolution succeeded and changed the type, restart the check
        // This is critical for coinductive cycle detection to work correctly
        if resolved != self.source {
            self.checker.check_subtype(resolved, self.target)
        } else {
            // Resolution failed or returned the same type - fall through
            SubtypeResult::False
        }
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        // Readonly types have specific subtyping rules:
        // - Readonly<T> <: Readonly<U> if T <: U
        // - Readonly<T> is NOT assignable to mutable T (safety)
        // - T <: Readonly<T> is allowed (can add readonly) - handled by target peeling in check_subtype_inner

        // Case: Readonly<S> <: Readonly<T>
        // If target is also Readonly, compare inner types
        if let Some(t_inner) = readonly_inner_type(self.checker.interner, self.target) {
            return self.checker.check_subtype(inner_type, t_inner);
        }

        // Case: Readonly<S> <: Mutable<T>
        // Readonly source cannot be assigned to mutable target for safety reasons.
        // Exception: target is any/unknown (handled by fast paths in check_subtype_inner).
        SubtypeResult::False
    }

    fn visit_string_intrinsic(
        &mut self,
        _kind: StringIntrinsicKind,
        _type_arg: TypeId,
    ) -> Self::Output {
        // String intrinsics are handled by evaluation
        SubtypeResult::False
    }

    fn visit_enum(&mut self, def_id: u32, member_type: TypeId) -> Self::Output {
        // Enums are nominal types - nominal identity matters for enum-to-enum
        if let Some((t_def, _t_members)) = enum_components(self.checker.interner, self.target) {
            // Enum to Enum: Nominal check - DefIds must match
            return if DefId(def_id) == t_def {
                SubtypeResult::True
            } else {
                SubtypeResult::False
            };
        }

        // Enum to non-Enum: Structural check on member type
        // e.g., Enum(1, 2, 3) <: number
        self.checker.check_subtype(member_type, self.target)
    }

    // Double dispatch implementations for structural types
    // These check the target type to determine which helper method to call

    fn visit_object(&mut self, shape_id: u32) -> Self::Output {
        // Double dispatch: check target type to determine which helper to call
        let s_shape = self.checker.interner.object_shape(ObjectShapeId(shape_id));

        if let Some(t_shape_id) = object_shape_id(self.checker.interner, self.target) {
            // Object <: Object
            let t_shape = self.checker.interner.object_shape(t_shape_id);
            self.checker
                .check_object_subtype(&s_shape, Some(ObjectShapeId(shape_id)), &t_shape)
        } else if let Some(t_shape_id) =
            object_with_index_shape_id(self.checker.interner, self.target)
        {
            // Object <: ObjectWithIndex
            let t_shape = self.checker.interner.object_shape(t_shape_id);
            self.checker.check_object_to_indexed(
                &s_shape.properties,
                Some(ObjectShapeId(shape_id)),
                &t_shape,
            )
        } else {
            // Trace: Object source doesn't match non-object target
            if let Some(tracer) = &mut self.checker.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: self.source,
                    target_type: self.target,
                })
            {
                return SubtypeResult::False;
            }
            SubtypeResult::False
        }
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        // Double dispatch: check target type to determine which helper to call
        let s_shape = self.checker.interner.object_shape(ObjectShapeId(shape_id));

        if let Some(t_shape_id) = object_with_index_shape_id(self.checker.interner, self.target) {
            // ObjectWithIndex <: ObjectWithIndex
            let t_shape = self.checker.interner.object_shape(t_shape_id);
            self.checker.check_object_with_index_subtype(
                &s_shape,
                Some(ObjectShapeId(shape_id)),
                &t_shape,
            )
        } else if let Some(t_shape_id) = object_shape_id(self.checker.interner, self.target) {
            // ObjectWithIndex <: Object
            let t_shape = self.checker.interner.object_shape(t_shape_id);
            self.checker.check_object_with_index_to_object(
                &s_shape,
                ObjectShapeId(shape_id),
                &t_shape.properties,
            )
        } else {
            // Trace: ObjectWithIndex source doesn't match non-object target
            if let Some(tracer) = &mut self.checker.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: self.source,
                    target_type: self.target,
                })
            {
                return SubtypeResult::False;
            }
            SubtypeResult::False
        }
    }
    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        // Double dispatch: check target type to determine which helper to call
        if let Some(t_fn_id) = function_shape_id(self.checker.interner, self.target) {
            // Function <: Function
            let s_fn = self
                .checker
                .interner
                .function_shape(FunctionShapeId(shape_id));
            let t_fn = self.checker.interner.function_shape(t_fn_id);
            self.checker.check_function_subtype(&s_fn, &t_fn)
        } else if let Some(t_callable_id) = callable_shape_id(self.checker.interner, self.target) {
            // Function <: Callable
            self.checker
                .check_function_to_callable_subtype(FunctionShapeId(shape_id), t_callable_id)
        } else {
            // Trace: Function source doesn't match non-function target
            if let Some(tracer) = &mut self.checker.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: self.source,
                    target_type: self.target,
                })
            {
                return SubtypeResult::False;
            }
            SubtypeResult::False
        }
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        // Double dispatch: check target type to determine which helper to call
        if let Some(t_callable_id) = callable_shape_id(self.checker.interner, self.target) {
            // Callable <: Callable
            let s_callable = self
                .checker
                .interner
                .callable_shape(CallableShapeId(shape_id));
            let t_callable = self.checker.interner.callable_shape(t_callable_id);
            self.checker
                .check_callable_subtype(&s_callable, &t_callable)
        } else if let Some(t_fn_id) = function_shape_id(self.checker.interner, self.target) {
            // Callable <: Function
            self.checker
                .check_callable_to_function_subtype(CallableShapeId(shape_id), t_fn_id)
        } else {
            // Trace: Callable source doesn't match non-callable target
            if let Some(tracer) = &mut self.checker.tracer
                && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                    source_type: self.source,
                    target_type: self.target,
                })
            {
                return SubtypeResult::False;
            }
            SubtypeResult::False
        }
    }
    fn visit_bound_parameter(&mut self, _de_bruijn_index: u32) -> Self::Output {
        SubtypeResult::False
    }
    fn visit_application(&mut self, app_id: u32) -> Self::Output {
        // Application types require the original source TypeId for proper expansion
        self.checker.check_application_expansion_target(
            self.source,
            self.target,
            TypeApplicationId(app_id),
        )
    }
    fn visit_conditional(&mut self, cond_id: u32) -> Self::Output {
        // Conditional types require special handling
        self.checker.conditional_branches_subtype(
            self.checker
                .interner
                .conditional_type(ConditionalTypeId(cond_id))
                .as_ref(),
            self.target,
        )
    }

    fn visit_mapped(&mut self, mapped_id: u32) -> Self::Output {
        // Mapped types require the original source TypeId for proper expansion
        self.checker.check_mapped_expansion_target(
            self.source,
            self.target,
            MappedTypeId(mapped_id),
        )
    }
    fn visit_index_access(&mut self, object_type: TypeId, key_type: TypeId) -> Self::Output {
        use crate::visitor::index_access_parts;

        // S[I] <: T[J]  <=>  S <: T  AND  I <: J
        // This handles deferred index access types (usually involving type parameters).
        if let Some((t_obj, t_idx)) = index_access_parts(self.checker.interner, self.target) {
            // Coinductive check: delegate back to check_subtype for both parts
            if self.checker.check_subtype(object_type, t_obj).is_true()
                && self.checker.check_subtype(key_type, t_idx).is_true()
            {
                return SubtypeResult::True;
            }
        }

        // If target is not an IndexAccess, we cannot prove subtyping.
        // Note: If S[I] could have been simplified to a concrete type that matches the target,
        // evaluate_type() in the caller (check_subtype) would have already handled it.
        if let Some(tracer) = &mut self.checker.tracer
            && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                source_type: self.source,
                target_type: self.target,
            })
        {
            return SubtypeResult::False;
        }
        SubtypeResult::False
    }
    fn visit_template_literal(&mut self, template_id: u32) -> Self::Output {
        use crate::types::IntrinsicKind;
        use crate::types::TemplateLiteralId;

        use crate::visitor::{intrinsic_kind, template_literal_id};

        // Template literal <: string is always true
        if intrinsic_kind(self.checker.interner, self.target) == Some(IntrinsicKind::String) {
            return SubtypeResult::True;
        }

        // Template literal <: Template literal
        // Use generalized pattern matching that handles different span structures
        if let Some(t_template_id) = template_literal_id(self.checker.interner, self.target) {
            let s_id = TemplateLiteralId(template_id);
            return self
                .checker
                .check_template_assignable_to_template(s_id, t_template_id);
        }

        // Trace: Template literal doesn't match target
        if let Some(tracer) = &mut self.checker.tracer
            && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                source_type: self.source,
                target_type: self.target,
            })
        {
            return SubtypeResult::False;
        }
        SubtypeResult::False
    }
    fn visit_type_query(&mut self, symbol_ref: u32) -> Self::Output {
        use crate::types::SymbolRef;

        // TypeQuery (typeof X) is a reference to a value symbol.
        // We need to resolve it to its structural type before comparing.
        let sym = SymbolRef(symbol_ref);

        // Attempt to resolve the symbol to its structural type.
        // Prioritize DefId-based resolution (Lazy) over legacy SymbolRef (Ref).
        let resolved = self
            .checker
            .resolver
            .resolve_symbol_ref(sym, self.checker.interner)
            .unwrap_or(self.source);

        // If resolution succeeded and gave us a different type, restart the check.
        // This recursion is critical for coinductive cycle detection.
        if resolved != self.source {
            self.checker.check_subtype(resolved, self.target)
        } else {
            // If resolution failed or returned the same ID, we cannot prove subtyping.
            SubtypeResult::False
        }
    }
    fn visit_keyof(&mut self, inner_type: TypeId) -> Self::Output {
        use crate::types::IntrinsicKind;
        use crate::visitor::{keyof_inner_type, union_list_id};

        // keyof S <: keyof T  <=>  T <: S (Contravariant)
        // If target is also a keyof type, check inner types in reverse
        if let Some(t_inner) = keyof_inner_type(self.checker.interner, self.target) {
            return self.checker.check_subtype(t_inner, inner_type);
        }

        // If inner_type is a TypeParameter, keyof T is NOT a subtype of primitives
        // (deferred keyof - we don't know what keys T has)
        if matches!(
            self.checker.interner.lookup(inner_type),
            Some(TypeData::TypeParameter(_))
        ) {
            return SubtypeResult::False;
        }

        // keyof T is always a subtype of string | number | symbol
        // Check if target is a union that matches this pattern
        if let Some(union_id) = union_list_id(self.checker.interner, self.target) {
            let members = self.checker.interner.type_list(union_id);
            // Check if all members are string, number, or symbol
            let all_primitive = members.iter().all(|&m| {
                matches!(
                    self.checker.interner.lookup(m),
                    Some(TypeData::Intrinsic(
                        IntrinsicKind::String | IntrinsicKind::Number | IntrinsicKind::Symbol
                    ))
                )
            });
            if all_primitive && !members.is_empty() {
                return SubtypeResult::True;
            }
        }

        // keyof is also subtype of the specific primitive if it matches
        if let Some(TypeData::Intrinsic(
            IntrinsicKind::String | IntrinsicKind::Number | IntrinsicKind::Symbol,
        )) = self.checker.interner.lookup(self.target)
        {
            return SubtypeResult::True;
        }

        // Trace: keyof doesn't match target
        if let Some(tracer) = &mut self.checker.tracer
            && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                source_type: self.source,
                target_type: self.target,
            })
        {
            return SubtypeResult::False;
        }
        SubtypeResult::False
    }
    fn visit_this_type(&mut self) -> Self::Output {
        use crate::visitor::is_this_type;

        // If target is also a 'this' type, they are compatible.
        // This handles cases like comparing two uninstantiated generic methods.
        if is_this_type(self.checker.interner, self.target) {
            return SubtypeResult::True;
        }

        // If we reach here, 'this' is being compared against a non-this type.
        // In most cases, check_subtype_inner's apparent_primitive_shape_for_type
        // would have resolved 'this' to its containing class/interface.
        // If that didn't happen or didn't result in 'True', we return False.
        if let Some(tracer) = &mut self.checker.tracer
            && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                source_type: self.source,
                target_type: self.target,
            })
        {
            return SubtypeResult::False;
        }
        SubtypeResult::False
    }
    fn visit_infer(&mut self, param_info: &TypeParamInfo) -> Self::Output {
        // 'infer R' behaves like a type parameter during structural subtyping.
        // It is a subtype of the target if its constraint satisfies the target.
        self.checker
            .check_type_parameter_subtype(param_info, self.target)
    }
    fn visit_unique_symbol(&mut self, symbol_ref: u32) -> Self::Output {
        use crate::visitor::unique_symbol_ref;

        // unique symbol has nominal identity - same symbol ref is subtype
        if let Some(t_symbol_ref) = unique_symbol_ref(self.checker.interner, self.target) {
            return if symbol_ref == t_symbol_ref.0 {
                SubtypeResult::True
            } else {
                SubtypeResult::False
            };
        }

        // unique symbol is always a subtype of symbol
        if let Some(TypeData::Intrinsic(IntrinsicKind::Symbol)) =
            self.checker.interner.lookup(self.target)
        {
            return SubtypeResult::True;
        }

        // Trace: unique symbol doesn't match target
        if let Some(tracer) = &mut self.checker.tracer
            && !tracer.on_mismatch_dyn(SubtypeFailureReason::TypeMismatch {
                source_type: self.source,
                target_type: self.target,
            })
        {
            return SubtypeResult::False;
        }
        SubtypeResult::False
    }
    fn visit_module_namespace(&mut self, _symbol_ref: u32) -> Self::Output {
        SubtypeResult::False
    }
    fn visit_error(&mut self) -> Self::Output {
        SubtypeResult::False
    }
}
