//! Error Explanation API for subtype checking.
//!
//! This module implements the "slow path" for generating structured failure reasons
//! when a subtype check fails. It re-runs subtype logic with tracing to produce
//! detailed error diagnostics (TS2322, TS2739, TS2740, TS2741, etc.).

use crate::def::resolver::TypeResolver;
use crate::diagnostics::SubtypeFailureReason;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::SubtypeChecker;
use crate::type_queries::data::get_object_symbol;
use crate::types::{
    FunctionShape, IntrinsicKind, LiteralValue, ObjectShape, ObjectShapeId, PropertyInfo,
    TupleElement, TypeId, Visibility,
};
use crate::utils;
use crate::visitor::is_type_parameter;
use crate::visitor::{
    array_element_type, callable_shape_id, function_shape_id, intrinsic_kind, literal_value,
    object_shape_id, object_with_index_shape_id, readonly_inner_type, tuple_list_id, union_list_id,
};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Collect source properties including those from intersection members.
    /// This ensures merged types (e.g., `{ a: string } & { b: number }`) have
    /// all properties available for missing property checks.
    fn collect_source_properties(&self, source: TypeId) -> Vec<PropertyInfo> {
        use crate::type_queries::data::get_intersection_members;

        let mut props = Vec::new();

        // Get base shape properties
        if let Some(shape_id) = object_shape_id(self.interner, source) {
            let shape = self.interner.object_shape(shape_id);
            props.extend(shape.properties.iter().cloned());
        }

        // Add properties from intersection members
        if let Some(members) = get_intersection_members(self.interner, source) {
            for member in members {
                if let Some(shape_id) = object_shape_id(self.interner, member) {
                    let shape = self.interner.object_shape(shape_id);
                    for prop in shape.properties.iter() {
                        if !props.iter().any(|p| p.name == prop.name) {
                            props.push(prop.clone());
                        }
                    }
                }
            }
        }

        props
    }

    fn is_late_bound_symbol_property_name(&self, name: tsz_common::interner::Atom) -> bool {
        let name = self.interner.resolve_atom_ref(name);
        name.starts_with("[Symbol.") || name.starts_with("__@")
    }

    /// Returns `true` if `type_id` is function-like — i.e. has at least one
    /// call or construct signature. Used by TS2739/TS2741 explain code to skip
    /// `prototype` from the missing-property list (tsc treats `prototype` as
    /// implicit on any callable value).
    fn type_has_callable_signature(&self, type_id: TypeId) -> bool {
        use crate::type_queries::has_call_signatures;
        if has_call_signatures(self.interner, type_id) {
            return true;
        }
        if let Some(cid) = callable_shape_id(self.interner, type_id) {
            let shape = self.interner.callable_shape(cid);
            return !shape.call_signatures.is_empty() || !shape.construct_signatures.is_empty();
        }
        if function_shape_id(self.interner, type_id).is_some() {
            return true;
        }
        false
    }

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
        let pair = (source, target);
        match self.guard.enter(pair) {
            crate::recursion::RecursionResult::Entered => {}
            crate::recursion::RecursionResult::Cycle
            | crate::recursion::RecursionResult::DepthExceeded
            | crate::recursion::RecursionResult::IterationExceeded => {
                return Some(SubtypeFailureReason::TypeMismatch {
                    source_type: source,
                    target_type: target,
                });
            }
        }
        let result = self.explain_failure_guarded(source, target);
        self.guard.leave(pair);
        result
    }

    fn explain_failure_guarded(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        // Fast path: if types are equal, no failure
        if source == target {
            return None;
        }

        if !self.strict_null_checks && source.is_nullish() {
            let null_to_void = source == TypeId::NULL && target == TypeId::VOID;
            if !null_to_void {
                return None;
            }
        }

        // Check for any/unknown/never special cases
        if source.is_any() || target.is_any_or_unknown() {
            return None;
        }
        if source.is_never() {
            return None;
        }
        // ERROR types should produce ErrorType failure reason
        if source.is_error() || target.is_error() {
            return Some(SubtypeFailureReason::ErrorType {
                source_type: source,
                target_type: target,
            });
        }

        // Note: Weak type checking is handled by CompatChecker (compat.rs:167-170).
        // Removed redundant check here to avoid double-checking which caused false positives.

        self.explain_failure_inner(source, target)
    }

    /// Resolve a `TypeQuery(SymbolRef)` type to its structural form for explain.
    ///
    /// Delegates to `resolve_type_query_symbol` (defined in generics.rs) which
    /// resolves via `resolve_ref` (value-space / constructor type) first, then
    /// falls back to `resolve_lazy` for non-class symbols (e.g., namespaces).
    fn resolve_type_query_for_explain(&self, type_id: TypeId) -> TypeId {
        if let Some(sym_ref) =
            crate::type_queries::get_type_query_symbol_ref(self.interner, type_id)
        {
            self.resolve_type_query_symbol(sym_ref)
                .map(|resolved| self.resolve_lazy_type(resolved))
                .unwrap_or(type_id)
        } else {
            type_id
        }
    }

    fn explain_failure_inner(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        // Resolve lazy types (interfaces, type aliases) to their structural forms.
        // Without this, interface types (TypeData::Lazy) won't match the object_shape_id
        // check below, causing TS2322 instead of TS2741/TS2739/TS2740.
        let mut resolved_source = self.resolve_lazy_type(source);
        let mut resolved_target = self.resolve_lazy_type(target);

        // Resolve TypeQuery types (typeof X) to their value-space structural forms.
        // Without this, `typeof Namespace` types remain as TypeQuery(SymbolRef) and
        // skip property comparison, preventing TS2741 from being emitted.
        resolved_source = self.resolve_type_query_for_explain(resolved_source);
        resolved_target = self.resolve_type_query_for_explain(resolved_target);

        // Expand applications (like Array<number>, MyGeneric<string>) to structural forms
        if let Some(app_id) = crate::visitor::application_id(self.interner, resolved_source)
            && let Some(expanded) = self.try_expand_application(app_id)
        {
            resolved_source = self.resolve_lazy_type(expanded);
        }
        if let Some(app_id) = crate::visitor::application_id(self.interner, resolved_target)
            && let Some(expanded) = self.try_expand_application(app_id)
        {
            resolved_target = self.resolve_lazy_type(expanded);
        }

        // TSC emits TS4104 when a readonly array/tuple is assigned to a mutable
        // array/tuple target. This check must happen before structural analysis —
        // readonly-to-mutable is the primary failure reason and short-circuits further
        // elaboration. When the target is a type parameter (not a concrete
        // array/tuple), the short-circuit depends on the source: readonly plain
        // arrays may still produce TS4104 via the existing constraint heuristic,
        // but a readonly source whose inner is a *tuple* (e.g. `readonly [...T]`)
        // must fall through to structural analysis so the tsc-parity TS2322 path
        // can report it — see variadicTuples1.ts:160 where `t: T` (target is a
        // type parameter) gets TS2322, while `m: [...T]` on line 162 gets TS4104.
        if let Some(readonly_source_inner) = readonly_inner_type(self.interner, resolved_source)
            && readonly_inner_type(self.interner, resolved_target).is_none()
        {
            let is_mutable_array_or_tuple = array_element_type(self.interner, resolved_target)
                .is_some()
                || tuple_list_id(self.interner, resolved_target).is_some();
            let source_inner_is_tuple =
                tuple_list_id(self.interner, readonly_source_inner).is_some();
            let is_type_param_with_array_constraint = !is_mutable_array_or_tuple
                && !source_inner_is_tuple
                && is_type_parameter(self.interner, resolved_target)
                && crate::visitor::type_param_info(self.interner, resolved_target)
                    .and_then(|info| info.constraint)
                    .is_some_and(|constraint| {
                        let resolved_constraint = self.resolve_lazy_type(constraint);
                        array_element_type(self.interner, resolved_constraint).is_some()
                            || tuple_list_id(self.interner, resolved_constraint).is_some()
                    });
            if is_mutable_array_or_tuple || is_type_param_with_array_constraint {
                return Some(SubtypeFailureReason::ReadonlyToMutableAssignment {
                    source_type: source,
                    target_type: target,
                });
            }
        }

        // TSC emits TS2322 (generic "not assignable") instead of TS2741/TS2739
        // when the target type is an intersection. Intersection types combine
        // constraints from multiple sources, so drilling into individual member
        // properties is misleading. Return TypeMismatch so the checker emits TS2322.
        // Check BEFORE evaluate_type, which may merge intersection members into
        // a single object, losing the intersection information.
        if crate::visitor::intersection_list_id(self.interner, resolved_target).is_some() {
            return Some(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        // Evaluate meta-types (Mapped, Conditional, KeyOf, etc.) to structural forms.
        // Application expansion may produce a Mapped type (e.g., Required<Foo> →
        // { [K in keyof Foo]-?: Foo[K] }) which needs further evaluation to a concrete
        // object type so property enumeration can generate TS2739/TS2741 diagnostics.
        let eval_source = self.evaluate_type(resolved_source);
        if eval_source != resolved_source {
            resolved_source = eval_source;
        }
        let eval_target = self.evaluate_type(resolved_target);
        if eval_target != resolved_target {
            resolved_target = eval_target;
        }

        if let Some(shape) = self.apparent_primitive_shape_for_type(resolved_source) {
            if let Some(t_shape_id) = object_shape_id(self.interner, resolved_target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                return self.explain_object_failure(
                    source,
                    target,
                    &shape.properties,
                    None,
                    &t_shape.properties,
                );
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, resolved_target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                let source_kind = self.apparent_primitive_kind(resolved_source);
                let has_string_index = t_shape.string_index.is_some();
                let has_number_index = t_shape.number_index.is_some();
                let allow_indexed_structural = !has_string_index
                    && (!has_number_index || source_kind == Some(IntrinsicKind::String));
                if !allow_indexed_structural {
                    return Some(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    });
                }
                return self.explain_indexed_object_failure(source, target, &shape, None, &t_shape);
            }
        }

        // Handle `object` intrinsic (non-primitive type) as source when target is an object.
        // `object` has no own properties, so all required target properties are "missing".
        // This produces TS2741/TS2739 instead of generic TS2322.
        if resolved_source == TypeId::OBJECT {
            if let Some(t_shape_id) = object_shape_id(self.interner, resolved_target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                return self.explain_object_failure(source, target, &[], None, &t_shape.properties);
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, resolved_target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                return self.explain_indexed_object_failure(
                    source,
                    target,
                    &ObjectShape::default(),
                    None,
                    &t_shape,
                );
            }
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, resolved_source),
            object_shape_id(self.interner, resolved_target),
        ) {
            let s_props = self.collect_source_properties(resolved_source);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.explain_object_failure(
                source,
                target,
                &s_props,
                Some(s_shape_id),
                &t_shape.properties,
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, resolved_source),
            object_with_index_shape_id(self.interner, resolved_target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.explain_indexed_object_failure(
                source,
                target,
                &s_shape,
                Some(s_shape_id),
                &t_shape,
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, resolved_source),
            object_shape_id(self.interner, resolved_target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.explain_object_with_index_to_object_failure(
                source,
                target,
                &s_shape,
                s_shape_id,
                &t_shape.properties,
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, resolved_source),
            object_with_index_shape_id(self.interner, resolved_target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.explain_indexed_object_failure(
                source,
                target,
                &s_shape,
                Some(s_shape_id),
                &t_shape,
            );
        }

        // Intersection source vs object target: collect merged properties from all
        // object-like members of the intersection, then check for missing properties.
        // This produces TS2739/TS2741 for branded/intersection types like
        // `number & { __brand: T }` assigned to an object type.
        if crate::visitor::intersection_list_id(self.interner, resolved_source).is_some() {
            let t_shape_id = object_shape_id(self.interner, resolved_target)
                .or_else(|| object_with_index_shape_id(self.interner, resolved_target));
            if let Some(t_sid) = t_shape_id {
                let collected = crate::objects::collect_properties(
                    resolved_source,
                    self.interner,
                    self.resolver,
                );
                if let crate::objects::PropertyCollectionResult::Properties { properties, .. } =
                    collected
                {
                    let t_shape = self.interner.object_shape(t_sid);
                    return self.explain_object_failure(
                        source,
                        target,
                        &properties,
                        None,
                        &t_shape.properties,
                    );
                }
            }
        }

        // Object source vs array target: resolve Array<T> to its interface properties
        // and find missing members. TSC emits TS2740 here (missing properties from array).
        if let Some(t_elem) = array_element_type(self.interner, resolved_target) {
            let s_shape_id = object_shape_id(self.interner, resolved_source)
                .or_else(|| object_with_index_shape_id(self.interner, resolved_source));
            if let Some(s_sid) = s_shape_id
                && let Some(array_base) = self.resolver.get_array_base_type()
            {
                let params = self.resolver.get_array_base_type_params();
                let instantiated = if params.is_empty() {
                    array_base
                } else {
                    let subst = TypeSubstitution::from_args(self.interner, params, &[t_elem]);
                    instantiate_type(self.interner, array_base, &subst)
                };
                let resolved_inst = self.resolve_lazy_type(instantiated);
                // The Array interface may resolve to an object shape or a callable shape
                // (with properties like length, push, concat, etc.)
                let s_shape = self.interner.object_shape(s_sid);
                if let Some(t_obj_sid) = object_shape_id(self.interner, resolved_inst)
                    .or_else(|| object_with_index_shape_id(self.interner, resolved_inst))
                {
                    let t_shape = self.interner.object_shape(t_obj_sid);
                    return self.explain_object_failure(
                        source,
                        target,
                        &s_shape.properties,
                        Some(s_sid),
                        &t_shape.properties,
                    );
                }
                // Array interface resolved to a callable shape — use its properties
                if let Some(callable_sid) = callable_shape_id(self.interner, resolved_inst) {
                    let callable = self.interner.callable_shape(callable_sid);
                    if !callable.properties.is_empty() {
                        return self.explain_object_failure(
                            source,
                            target,
                            &s_shape.properties,
                            Some(s_sid),
                            &callable.properties,
                        );
                    }
                }
            }
        }

        // Array source vs Object target: resolve Array<T> to its interface properties
        // and find missing members. TSC emits TS2739/TS2741 here.
        if let Some(s_elem) = array_element_type(self.interner, resolved_source) {
            let t_shape_id = object_shape_id(self.interner, resolved_target)
                .or_else(|| object_with_index_shape_id(self.interner, resolved_target));
            if let Some(t_sid) = t_shape_id
                && let Some(array_base) = self.resolver.get_array_base_type()
            {
                let params = self.resolver.get_array_base_type_params();
                let instantiated = if params.is_empty() {
                    array_base
                } else {
                    let subst = TypeSubstitution::from_args(self.interner, params, &[s_elem]);
                    instantiate_type(self.interner, array_base, &subst)
                };
                let resolved_inst = self.resolve_lazy_type(instantiated);
                // The Array interface may resolve to an object shape or a callable shape
                let t_shape = self.interner.object_shape(t_sid);
                if let Some(s_obj_sid) = object_shape_id(self.interner, resolved_inst)
                    .or_else(|| object_with_index_shape_id(self.interner, resolved_inst))
                {
                    let s_shape = self.interner.object_shape(s_obj_sid);
                    return self.explain_object_failure(
                        source,
                        target,
                        &s_shape.properties,
                        Some(s_obj_sid),
                        &t_shape.properties,
                    );
                }
                if let Some(callable_sid) = callable_shape_id(self.interner, resolved_inst) {
                    let callable = self.interner.callable_shape(callable_sid);
                    if !callable.properties.is_empty() {
                        return self.explain_object_failure(
                            source,
                            target,
                            &callable.properties,
                            None,
                            &t_shape.properties,
                        );
                    }
                }
            }
        }

        if let (Some(s_fn_id), Some(t_fn_id)) = (
            function_shape_id(self.interner, source),
            function_shape_id(self.interner, target),
        ) {
            let s_fn = self.interner.function_shape(s_fn_id);
            let t_fn = self.interner.function_shape(t_fn_id);
            return self.explain_function_failure(&s_fn, &t_fn);
        }

        // Callable target with properties: when assigning to a callable type that has
        // additional properties (e.g., `{ (): string; prop: number }`), check for missing
        // properties from the source. This produces TS2741/TS2739 instead of generic TS2322.
        if let Some(t_callable_id) = callable_shape_id(self.interner, resolved_target) {
            let t_callable = self.interner.callable_shape(t_callable_id);
            if !t_callable.properties.is_empty() {
                let source_props: Vec<PropertyInfo> = if let Some(s_callable_id) =
                    callable_shape_id(self.interner, resolved_source)
                {
                    self.interner
                        .callable_shape(s_callable_id)
                        .properties
                        .clone()
                } else if let Some(s_shape_id) = object_shape_id(self.interner, resolved_source) {
                    self.interner.object_shape(s_shape_id).properties.clone()
                } else {
                    vec![]
                };
                return self.explain_object_failure(
                    source,
                    target,
                    &source_props,
                    None,
                    &t_callable.properties,
                );
            }
        }

        // Callable source vs Object target: when a callable type is assigned to an
        // object type, check for missing properties to produce TS2741/TS2739 instead
        // of generic TS2322.
        //
        // This applies to all callable types (functions, methods, constructors).
        // When a function is assigned to an object type, we should report which
        // properties are missing (TS2741/TS2739) rather than just saying it's not
        // assignable (TS2322).
        if let Some(s_callable_id) = callable_shape_id(self.interner, resolved_source) {
            let s_callable = self.interner.callable_shape(s_callable_id);
            if let Some(t_shape_id) = object_shape_id(self.interner, resolved_target)
                .or_else(|| object_with_index_shape_id(self.interner, resolved_target))
            {
                let t_shape = self.interner.object_shape(t_shape_id);
                return self.explain_object_failure(
                    source,
                    target,
                    &s_callable.properties,
                    None,
                    &t_shape.properties,
                );
            }
        }

        if let (Some(s_elem), Some(t_elem)) = (
            array_element_type(self.interner, source),
            array_element_type(self.interner, target),
        ) {
            if !self.check_subtype(s_elem, t_elem).is_true() {
                return Some(SubtypeFailureReason::ArrayElementMismatch {
                    source_element: s_elem,
                    target_element: t_elem,
                });
            }
            return None;
        }

        // Object-with-index source vs Tuple target: check for missing numeric properties.
        // When an array-like object type (e.g., interface StrNum extends Array { 0: string; ... })
        // is assigned to a tuple type (e.g., [number, number, number]), detect missing
        // required numeric index properties and produce TS2741 instead of generic TS2322.
        // Only applies to types with index signatures (array-like); plain object types without
        // index signatures fall through to the generic TypeMismatch path, matching tsc behavior.
        if let Some(t_tuple_id) = tuple_list_id(self.interner, resolved_target)
            && let Some(s_sid) = object_with_index_shape_id(self.interner, resolved_source)
        {
            let t_elems = self.interner.tuple_list(t_tuple_id);
            let s_shape = self.interner.object_shape(s_sid);
            let mut missing_props: Vec<tsz_common::interner::Atom> = Vec::new();
            for (i, t_elem) in t_elems.iter().enumerate() {
                if t_elem.is_required() {
                    let prop_name = self.interner.intern_string(&i.to_string());
                    let has_prop = s_shape.properties.iter().any(|p| p.name == prop_name);
                    if !has_prop {
                        missing_props.push(prop_name);
                    }
                }
            }
            if missing_props.len() > 1 {
                return Some(SubtypeFailureReason::MissingProperties {
                    property_names: missing_props,
                    source_type: source,
                    target_type: target,
                });
            }
            if missing_props.len() == 1 {
                return Some(SubtypeFailureReason::MissingProperty {
                    property_name: missing_props[0],
                    source_type: source,
                    target_type: target,
                });
            }
        }

        if let (Some(s_elems), Some(t_elems)) = (
            tuple_list_id(self.interner, source),
            tuple_list_id(self.interner, target),
        ) {
            let s_elems = self.interner.tuple_list(s_elems);
            let t_elems = self.interner.tuple_list(t_elems);
            return self.explain_tuple_failure(&s_elems, &t_elems);
        }

        if let Some(members) = union_list_id(self.interner, resolved_target) {
            let members = self.interner.type_list(members);
            return Some(SubtypeFailureReason::NoUnionMemberMatches {
                source_type: source,
                target_union_members: members.as_ref().to_vec(),
            });
        }

        if let (Some(s_kind), Some(t_kind)) = (
            intrinsic_kind(self.interner, source),
            intrinsic_kind(self.interner, target),
        ) {
            if s_kind != t_kind {
                return Some(SubtypeFailureReason::IntrinsicTypeMismatch {
                    source_type: source,
                    target_type: target,
                });
            }
            return None;
        }

        if literal_value(self.interner, source).is_some()
            && literal_value(self.interner, target).is_some()
        {
            return Some(SubtypeFailureReason::LiteralTypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        if let (Some(lit), Some(t_kind)) = (
            literal_value(self.interner, source),
            intrinsic_kind(self.interner, target),
        ) {
            let compatible = match lit {
                LiteralValue::String(_) => t_kind == IntrinsicKind::String,
                LiteralValue::Number(_) => t_kind == IntrinsicKind::Number,
                LiteralValue::BigInt(_) => t_kind == IntrinsicKind::Bigint,
                LiteralValue::Boolean(_) => t_kind == IntrinsicKind::Boolean,
            };
            if !compatible {
                return Some(SubtypeFailureReason::LiteralTypeMismatch {
                    source_type: source,
                    target_type: target,
                });
            }
            return None;
        }

        if intrinsic_kind(self.interner, source).is_some()
            && literal_value(self.interner, target).is_some()
        {
            return Some(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        Some(SubtypeFailureReason::TypeMismatch {
            source_type: source,
            target_type: target,
        })
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
        // First pass: collect all missing required property names.
        // tsc emits TS2739 (multiple missing) or TS2741 (single missing) before
        // checking property type compatibility.
        // Collect with declaration_order so we can sort by source order (tsc lists
        // missing properties in declaration order, not Atom/hash order).
        // For class inheritance, we need to show own properties first, then inherited.
        let target_symbol = get_object_symbol(self.interner, target);
        let mut missing_with_order: Vec<(
            tsz_common::interner::Atom,
            u32,
            Option<tsz_binder::SymbolId>,
        )> = Vec::new();
        let mut seen_names: rustc_hash::FxHashSet<tsz_common::interner::Atom> =
            rustc_hash::FxHashSet::default();
        for t_prop in target_props {
            if !t_prop.optional {
                let s_prop = self.lookup_property(source_props, source_shape_id, t_prop.name);
                if s_prop.is_none() && seen_names.insert(t_prop.name) {
                    missing_with_order.push((
                        t_prop.name,
                        t_prop.declaration_order,
                        t_prop.parent_id,
                    ));
                }
            }
        }
        missing_with_order.sort_by(
            |(left_name, left_order, left_parent), (right_name, right_order, right_parent)| {
                let name_order = || {
                    self.interner
                        .resolve_atom_ref(*left_name)
                        .cmp(&self.interner.resolve_atom_ref(*right_name))
                };
                // For class types, own properties (where parent_id matches the target symbol)
                // should come before inherited properties
                let left_is_own = target_symbol.is_some() && *left_parent == target_symbol;
                let right_is_own = target_symbol.is_some() && *right_parent == target_symbol;

                match (left_is_own, right_is_own) {
                    (true, false) => return std::cmp::Ordering::Less,
                    (false, true) => return std::cmp::Ordering::Greater,
                    (true, true) => {
                        // When both are own properties of the target, tsc lists
                        // them in source-declaration order. Genuine source
                        // members carry non-zero `declaration_order`, so sort
                        // by it. Synthesized members (e.g. class `prototype`)
                        // carry `declaration_order == 0` and stay first via
                        // the (false, true) tie-break below; stable `sort_by`
                        // preserves their relative order.
                        match (*left_order > 0, *right_order > 0) {
                            (true, true) => {
                                return left_order.cmp(right_order).then_with(name_order);
                            }
                            (false, true) => return std::cmp::Ordering::Less,
                            (true, false) => return std::cmp::Ordering::Greater,
                            (false, false) => return std::cmp::Ordering::Equal,
                        }
                    }
                    (false, false) => {}
                }

                // Inherited-on-both-sides path: fall through to declaration_order
                // (1-based) comparison, with alphabetic tie-break for synthesized
                // properties. This preserves the prior interface-merge ordering.
                match (*left_order > 0, *right_order > 0) {
                    (true, true) => left_order.cmp(right_order).then_with(name_order),
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    (false, false) => name_order(),
                }
            },
        );
        let has_non_symbol_missing = missing_with_order
            .iter()
            .any(|(name, _, _)| !self.is_late_bound_symbol_property_name(*name));
        if !has_non_symbol_missing {
            // All missing properties are late-bound symbols (e.g. [Symbol.iterator]).
            // tsc does not list symbol-only missing properties in TS2739/TS2741 messages;
            // clear so we fall through to property type checking or TypeMismatch.
            missing_with_order.clear();
        } else if matches!(
            crate::type_queries::extended::classify_array_like(self.interner, target),
            crate::type_queries::extended::ArrayLikeKind::Array(_)
                | crate::type_queries::extended::ArrayLikeKind::Tuple
                | crate::type_queries::extended::ArrayLikeKind::Readonly(_)
        ) {
            // For array-like targets, tsc treats `[Symbol.iterator]` /
            // `[Symbol.unscopables]` as implicitly satisfied by any object
            // source (via the iteration protocol fallback), and omits them
            // from the TS2739/TS2740 missing list. Keep this behavior so that
            // e.g. `Type 'I1' is missing the following properties from type
            // 'any[]': length, pop, push, concat, and 25 more` — not 27.
            missing_with_order
                .retain(|(name, _, _)| !self.is_late_bound_symbol_property_name(*name));
        }
        // For non-array targets (e.g. `ArrayConstructor`), tsc lists both named
        // and symbol-keyed properties in TS2739/TS2741 (e.g. `isArray, from,
        // of, [Symbol.species]`). Keep the full list in that case.

        // tsc treats `prototype` as implicit on callable sources (any function
        // or class value has a `.prototype` in JS), so it never lists it as a
        // missing property — even when comparing a plain function type against
        // an interface like `ArrayConstructor` that declares `prototype`.
        // Strip it here if the source has call or construct signatures.
        if !missing_with_order.is_empty() && self.type_has_callable_signature(source) {
            let prototype_atom = self.interner.intern_string("prototype");
            missing_with_order.retain(|(name, _, _)| *name != prototype_atom);
        }
        let missing_props: Vec<tsz_common::interner::Atom> = missing_with_order
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();

        if missing_props.len() > 1 {
            return Some(SubtypeFailureReason::MissingProperties {
                property_names: missing_props,
                source_type: source,
                target_type: target,
            });
        }
        if missing_props.len() == 1 {
            return Some(SubtypeFailureReason::MissingProperty {
                property_name: missing_props[0],
                source_type: source,
                target_type: target,
            });
        }

        // Second pass: check property type compatibility
        for t_prop in target_props {
            let s_prop = self.lookup_property(source_props, source_shape_id, t_prop.name);

            if let Some(sp) = s_prop {
                // Check nominal identity for private/protected properties
                if t_prop.visibility != Visibility::Public {
                    if sp.parent_id != t_prop.parent_id {
                        return Some(SubtypeFailureReason::PropertyNominalMismatch {
                            property_name: t_prop.name,
                        });
                    }
                }
                // Cannot assign private/protected source to public target
                else if sp.visibility != Visibility::Public {
                    return Some(SubtypeFailureReason::PropertyVisibilityMismatch {
                        property_name: t_prop.name,
                        source_visibility: sp.visibility,
                        target_visibility: t_prop.visibility,
                    });
                }

                // Check optional/required mismatch
                if sp.optional && !t_prop.optional {
                    return Some(SubtypeFailureReason::OptionalPropertyRequired {
                        property_name: t_prop.name,
                    });
                }

                // Check property type compatibility
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
                    && !sp.readonly
                    && (sp.write_type != TypeId::NONE && sp.write_type != sp.type_id
                        || t_prop.write_type != TypeId::NONE && t_prop.write_type != t_prop.type_id)
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
                        // Check if the failure is due to missing properties
                        if let Some(reason) =
                            self.explain_failure(s_string_idx.value_type, t_string_idx.value_type)
                            && matches!(
                                reason,
                                SubtypeFailureReason::MissingProperty { .. }
                                    | SubtypeFailureReason::MissingProperties { .. }
                            )
                        {
                            return Some(reason);
                        }
                        return Some(SubtypeFailureReason::IndexSignatureMismatch {
                            index_kind: "string",
                            source_value_type: s_string_idx.value_type,
                            target_value_type: t_string_idx.value_type,
                        });
                    }
                }
                None => {
                    // Class/interface types must have an explicit string index
                    // signature — a number index alone is not enough (see
                    // check_string_index_compatibility for the full rationale).
                    if self.requires_explicit_declared_index_signature(source_shape) {
                        return Some(SubtypeFailureReason::MissingIndexSignature {
                            index_kind: "string",
                        });
                    }

                    for prop in &source_shape.properties {
                        // Strip `undefined` from optional property types when checking
                        // against index signatures, matching tsc behavior.
                        let prop_type = if prop.optional {
                            crate::narrowing::utils::remove_undefined(self.interner, prop.type_id)
                        } else {
                            prop.type_id
                        };
                        if !self
                            .check_subtype(prop_type, t_string_idx.value_type)
                            .is_true()
                        {
                            // Check if the failure is due to missing properties
                            if let Some(reason) =
                                self.explain_failure(prop_type, t_string_idx.value_type)
                                && matches!(
                                    reason,
                                    SubtypeFailureReason::MissingProperty { .. }
                                        | SubtypeFailureReason::MissingProperties { .. }
                                )
                            {
                                return Some(reason);
                            }
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
        if let Some(ref t_number_idx) = target_shape.number_index {
            if let Some(ref s_number_idx) = source_shape.number_index {
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
                    // Check if the failure is due to missing properties
                    if let Some(reason) =
                        self.explain_failure(s_number_idx.value_type, t_number_idx.value_type)
                        && matches!(
                            reason,
                            SubtypeFailureReason::MissingProperty { .. }
                                | SubtypeFailureReason::MissingProperties { .. }
                        )
                    {
                        return Some(reason);
                    }
                    return Some(SubtypeFailureReason::IndexSignatureMismatch {
                        index_kind: "number",
                        source_value_type: s_number_idx.value_type,
                        target_value_type: t_number_idx.value_type,
                    });
                }
            } else if let Some(ref s_string_idx) = source_shape.string_index {
                if s_string_idx.readonly && !t_number_idx.readonly {
                    return Some(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    });
                }
                if !self
                    .check_subtype(s_string_idx.value_type, t_number_idx.value_type)
                    .is_true()
                {
                    // Check if the failure is due to missing properties
                    if let Some(reason) =
                        self.explain_failure(s_string_idx.value_type, t_number_idx.value_type)
                        && matches!(
                            reason,
                            SubtypeFailureReason::MissingProperty { .. }
                                | SubtypeFailureReason::MissingProperties { .. }
                        )
                    {
                        return Some(reason);
                    }
                    return Some(SubtypeFailureReason::IndexSignatureMismatch {
                        index_kind: "number",
                        source_value_type: s_string_idx.value_type,
                        target_value_type: t_number_idx.value_type,
                    });
                }
            } else if source_shape.symbol.is_some() {
                return Some(SubtypeFailureReason::MissingIndexSignature {
                    index_kind: "number",
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
                // Check nominal identity for private/protected properties
                // Private and protected members are nominally typed - they must
                // originate from the same declaration (same parent_id)
                if t_prop.visibility != Visibility::Public {
                    if sp.parent_id != t_prop.parent_id {
                        return Some(SubtypeFailureReason::PropertyNominalMismatch {
                            property_name: t_prop.name,
                        });
                    }
                }
                // Cannot assign private/protected source to public target
                else if sp.visibility != Visibility::Public {
                    return Some(SubtypeFailureReason::PropertyVisibilityMismatch {
                        property_name: t_prop.name,
                        source_visibility: sp.visibility,
                        target_visibility: t_prop.visibility,
                    });
                }

                if sp.optional && !t_prop.optional {
                    return Some(SubtypeFailureReason::OptionalPropertyRequired {
                        property_name: t_prop.name,
                    });
                }
                // NOTE: TypeScript allows readonly source to satisfy mutable target
                // (readonly is a constraint on the reference, not structural compatibility)

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
                    && !sp.readonly
                    && (sp.write_type != TypeId::NONE && sp.write_type != sp.type_id
                        || t_prop.write_type != TypeId::NONE && t_prop.write_type != t_prop.type_id)
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
            // Strip `undefined` from optional property types when checking against
            // index signatures, matching tsc behavior.
            let prop_type = if prop.optional {
                crate::narrowing::utils::remove_undefined(self.interner, prop.type_id)
            } else {
                prop.type_id
            };
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
                        // Check if the failure is due to missing properties
                        if let Some(reason) = self.explain_failure(prop_type, number_idx.value_type)
                            && matches!(
                                reason,
                                SubtypeFailureReason::MissingProperty { .. }
                                    | SubtypeFailureReason::MissingProperties { .. }
                            )
                        {
                            return Some(reason);
                        }
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
                    // Check if the failure is due to missing properties
                    if let Some(reason) = self.explain_failure(prop_type, string_idx.value_type)
                        && matches!(
                            reason,
                            SubtypeFailureReason::MissingProperty { .. }
                                | SubtypeFailureReason::MissingProperties { .. }
                        )
                    {
                        return Some(reason);
                    }
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
        let target_fixed_count = if target_has_rest {
            target.params.len().saturating_sub(1)
        } else {
            target.params.len()
        };
        // When the target has a rest parameter (e.g., ...args: number[]),
        // it can absorb unlimited arguments — skip the too-many check entirely
        // so we fall through to per-parameter type checking.
        if !rest_is_top && !target_has_rest && source_required > target_fixed_count {
            return Some(SubtypeFailureReason::TooManyParameters {
                source_count: source_required,
                target_count: target_fixed_count,
            });
        }

        // Check parameter types
        let source_has_rest = source.params.last().is_some_and(|p| p.rest);
        let source_fixed_count = if source_has_rest {
            source.params.len().saturating_sub(1)
        } else {
            source.params.len()
        };
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        // Constructor and method signatures are bivariant even with strictFunctionTypes
        let is_method_or_ctor =
            source.is_method || target.is_method || source.is_constructor || target.is_constructor;
        for i in 0..fixed_compare_count {
            let s_param = &source.params[i];
            let t_param = &target.params[i];
            // Compare declared parameter types, matching the subtype rules.
            // When both params are optional, strip `undefined` so
            // `(x?: T)` and `(x?: T | undefined)` compare as equivalent.
            let (s_effective, t_effective) = self.effective_param_type_pair(s_param, t_param);
            // Check parameter compatibility (contravariant in strict mode, bivariant in legacy)
            if !self.are_parameters_compatible_impl(s_effective, t_effective, is_method_or_ctor) {
                return Some(SubtypeFailureReason::ParameterTypeMismatch {
                    param_index: i,
                    source_param: s_effective,
                    target_param: t_effective,
                });
            }
        }

        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return None; // Invalid rest parameter
            };
            if rest_is_top {
                if let Some((param_index, source_param)) =
                    self.first_top_rest_unassignable_source_param(&source.params)
                {
                    return Some(SubtypeFailureReason::ParameterTypeMismatch {
                        param_index,
                        source_param,
                        target_param: rest_elem_type,
                    });
                }
                return None;
            }

            for i in target_fixed_count..source_fixed_count {
                let s_param = &source.params[i];
                if !self.are_parameters_compatible_impl(
                    s_param.type_id,
                    rest_elem_type,
                    is_method_or_ctor,
                ) {
                    return Some(SubtypeFailureReason::ParameterTypeMismatch {
                        param_index: i,
                        source_param: s_param.type_id,
                        target_param: rest_elem_type,
                    });
                }
            }

            if source_has_rest {
                let s_rest_param = source.params.last()?;
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                if !self.are_parameters_compatible_impl(
                    s_rest_elem,
                    rest_elem_type,
                    is_method_or_ctor,
                ) {
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
            let rest_is_top = self.allow_bivariant_rest && rest_elem_type.is_any_or_unknown();

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

        None
    }

    /// Explain why a tuple type assignment failed.
    fn explain_tuple_failure(
        &mut self,
        source: &[TupleElement],
        target: &[TupleElement],
    ) -> Option<SubtypeFailureReason> {
        let source_required = crate::utils::required_element_count(source);
        let target_required = crate::utils::required_element_count(target);

        if source_required < target_required {
            return Some(SubtypeFailureReason::TupleElementMismatch {
                source_count: source.len(),
                target_count: target.len(),
            });
        }

        // When both source and target are closed tuples (no rest elements) and
        // source has more elements than target allows, prefer the arity-mismatch
        // reason over an element-level type mismatch. This matches tsc, which
        // reports the outer "Source has N element(s) but target allows only M"
        // diagnostic instead of drilling into a specific element when the
        // length already disqualifies the relation.
        let target_has_rest = target.iter().any(|e| e.rest);
        let source_has_rest = source.iter().any(|e| e.rest);
        if !target_has_rest && !source_has_rest && source.len() > target.len() {
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
                    // Type parameter rest spread requires matching rest in source
                    if tail_elem.rest && is_type_parameter(self.interner, tail_elem.type_id) {
                        let s_elem = &source[source_end - 1];
                        if s_elem.rest {
                            let tp_array = self.interner.array(tail_elem.type_id);
                            if !self.check_subtype(s_elem.type_id, tp_array).is_true() {
                                return Some(SubtypeFailureReason::TupleElementTypeMismatch {
                                    index: source_end - 1,
                                    source_element: s_elem.type_id,
                                    target_element: tail_elem.type_id,
                                });
                            }
                            source_end -= 1;
                            continue;
                        }
                        return Some(SubtypeFailureReason::TypeMismatch {
                            source_type: source.first().map(|e| e.type_id).unwrap_or(TypeId::NEVER),
                            target_type: tail_elem.type_id,
                        });
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
                    let variadic_is_type_param = is_type_parameter(self.interner, variadic);
                    let variadic_array = self.interner.array(variadic);
                    for (j, s_elem) in source_iter {
                        if s_elem.rest {
                            if !self.check_subtype(s_elem.type_id, variadic_array).is_true() {
                                return Some(SubtypeFailureReason::TupleElementTypeMismatch {
                                    index: j,
                                    source_element: s_elem.type_id,
                                    target_element: variadic_array,
                                });
                            }
                        } else if variadic_is_type_param {
                            return Some(SubtypeFailureReason::TypeMismatch {
                                source_type: s_elem.type_id,
                                target_type: variadic,
                            });
                        } else if !self.check_subtype(s_elem.type_id, variadic).is_true() {
                            return Some(SubtypeFailureReason::TupleElementTypeMismatch {
                                index: j,
                                source_element: s_elem.type_id,
                                target_element: variadic,
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
                    // Drill into the nested failure: if the element mismatch is due to a
                    // missing property (e.g., {} vs {a: string}), return MissingProperty
                    // to produce TS2741 instead of generic TS2322. This matches tsc behavior
                    // for tuple literals where elements have missing properties.
                    if let Some(nested) = self.explain_failure(s_elem.type_id, t_elem.type_id)
                        && matches!(
                            nested,
                            SubtypeFailureReason::MissingProperty { .. }
                                | SubtypeFailureReason::MissingProperties { .. }
                        )
                    {
                        return Some(nested);
                    }
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
