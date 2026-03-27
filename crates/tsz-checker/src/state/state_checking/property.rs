//! Object literal excess property checking.
//!
//! Property access resolution lives in the sibling `property_access` module.
//! Readonly assignment checking lives in the sibling `readonly` module.

use crate::context::TypingRequest;
use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use std::collections::HashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn check_excess_property_initializer_implicit_any(
        &mut self,
        elem_idx: NodeIndex,
        target: TypeId,
    ) {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return;
        };

        match elem_node.kind {
            syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                    let contextual_type = self
                        .get_property_name_resolved(prop.name)
                        .and_then(|name| {
                            self.contextual_object_literal_property_type(target, &name)
                        })
                        .and_then(|ty| self.contextual_type_option_for_expression(Some(ty)));

                    if let Some(contextual_type) = contextual_type {
                        let request = TypingRequest::with_contextual_type(contextual_type);
                        self.invalidate_initializer_for_context_change(prop.initializer);
                        self.get_type_of_node_with_request(prop.initializer, &request);
                    } else {
                        self.check_for_nested_function_ts7006(prop.initializer);
                    }
                }
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
                    let contextual_type = self
                        .get_property_name_resolved(method.name)
                        .and_then(|name| {
                            self.contextual_object_literal_property_type(target, &name)
                        })
                        .and_then(|ty| self.contextual_type_option_for_expression(Some(ty)));

                    if let Some(contextual_type) = contextual_type {
                        let request = TypingRequest::with_contextual_type(contextual_type);
                        self.invalidate_function_like_for_contextual_retry(elem_idx);
                        self.get_type_of_function_with_request(elem_idx, &request);
                    } else {
                        for (pi, &param_idx) in method.parameters.nodes.iter().enumerate() {
                            if let Some(param_node) = self.ctx.arena.get(param_idx)
                                && let Some(param) = self.ctx.arena.get_parameter(param_node)
                            {
                                self.maybe_report_implicit_any_parameter(param, false, pi);
                            }
                        }
                        if method.body.is_some() {
                            self.check_for_nested_function_ts7006(method.body);
                        }
                    }
                }
            }
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.ctx.arena.get_accessor(elem_node) {
                    let contextual_type = self
                        .get_property_name_resolved(accessor.name)
                        .and_then(|name| {
                            self.contextual_object_literal_property_type(target, &name)
                        })
                        .and_then(|ty| self.contextual_type_option_for_expression(Some(ty)));

                    if let Some(contextual_type) = contextual_type {
                        let request = TypingRequest::with_contextual_type(contextual_type);
                        self.invalidate_function_like_for_contextual_retry(elem_idx);
                        self.get_type_of_function_with_request(elem_idx, &request);
                    } else {
                        for (pi, &param_idx) in accessor.parameters.nodes.iter().enumerate() {
                            if let Some(param_node) = self.ctx.arena.get(param_idx)
                                && let Some(param) = self.ctx.arena.get_parameter(param_node)
                            {
                                self.maybe_report_implicit_any_parameter(param, false, pi);
                            }
                        }
                        if accessor.body.is_some() {
                            self.check_for_nested_function_ts7006(accessor.body);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn nested_property_target_type(
        &mut self,
        owner_type: TypeId,
        prop_name: Atom,
        fallback: TypeId,
    ) -> TypeId {
        let prop_name_str = self.ctx.types.resolve_atom(prop_name);

        if let Some(type_id) =
            self.contextual_object_literal_property_type(owner_type, prop_name_str.as_ref())
        {
            return type_id;
        }

        if let Some(type_id) = self
            .ctx
            .types
            .contextual_property_type(owner_type, prop_name_str.as_ref())
        {
            return type_id;
        }

        let resolved_owner = self.resolve_type_for_property_access(owner_type);
        if resolved_owner != owner_type
            && let Some(type_id) = self
                .ctx
                .types
                .contextual_property_type(resolved_owner, prop_name_str.as_ref())
        {
            return type_id;
        }

        match self.resolve_property_access_with_env(owner_type, &prop_name_str) {
            tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. } => {
                type_id
            }
            _ => fallback,
        }
    }

    pub(crate) fn check_object_literal_excess_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        use crate::query_boundaries::common as freshness_query;

        // Ensure the target type is fully resolved before excess property
        // checking. Interface types are represented as Lazy(DefId) and need
        // their structural body materialized in the type environment for
        // object_shape queries to succeed.
        self.ensure_relation_input_ready(target);

        // Excess property checking against unresolved inference targets leaks
        // provisional diagnostics like `__infer_0 | PromiseLike<__infer_0>`.
        // tsc waits until the contextual target is concrete before deciding
        // whether a fresh object literal has excess properties.
        let evaluated_target = self.evaluate_type_with_env(target);

        // Excess property checks do not apply to type parameters (even with constraints).
        if query::is_type_parameter_like(self.ctx.types, target) {
            return;
        }

        // Only check excess properties for FRESH object literals
        let is_fresh_source = freshness_query::is_fresh_object_type(self.ctx.types, source);
        let explicit_property_names = if is_fresh_source {
            None
        } else {
            self.explicit_object_literal_property_names_for_spread(idx)
        };
        // Non-fresh object literals should be exempt from excess-property checks
        // unless they use spread, in which case we still check explicit properties.
        if !is_fresh_source && explicit_property_names.is_none() {
            return;
        }

        // Get the properties of source type using type_queries
        let Some(source_shape) = query::object_shape(self.ctx.types, source) else {
            return;
        };
        let source_props = source_shape.properties.as_slice();
        let effective_target = self.normalized_target_for_excess_properties(target);
        let resolved_target = self.prune_impossible_object_union_members_with_env(effective_target);

        // Handle union targets first using type_queries
        if let Some(members) = query::union_members(self.ctx.types, resolved_target) {
            let mut target_shapes = Vec::new();
            let mut any_member_has_number_index = false;
            let mut has_unresolved_member = false;

            for &member in &members {
                let resolved_member = self.resolve_type_for_property_access(member);
                if self.contextual_type_is_unresolved_for_argument_refresh(member)
                    || self.contextual_type_is_unresolved_for_argument_refresh(resolved_member)
                {
                    has_unresolved_member = true;
                }
                let Some(shape) = query::object_shape(self.ctx.types, resolved_member) else {
                    // If a union member is the `object` intrinsic, it conceptually
                    // accepts any properties, so excess property checking should not
                    // apply at all.
                    if resolved_member == TypeId::OBJECT {
                        return;
                    }
                    // TypeScript still applies excess property checking to the
                    // concrete members of unions like `T | { prop: boolean }`.
                    if query::is_type_parameter_like(self.ctx.types, resolved_member) {
                        continue;
                    }
                    continue;
                };

                // String index signatures accept ANY string-keyed property,
                // so all excess property checking can be skipped.
                if shape.string_index.is_some() {
                    return;
                }

                // Empty types (like `{}`) accept any non-primitive,
                // so skip excess property checking entirely.
                if shape.properties.is_empty() && shape.number_index.is_none() {
                    return;
                }

                // Track number index signatures: they accept numeric properties
                // but NOT arbitrary string properties like 'jj'.
                if shape.number_index.is_some() {
                    any_member_has_number_index = true;
                }

                // The global `Object` interface has properties (toString, valueOf,
                // constructor, etc.) but is "wide" enough that tsc skips excess
                // property checking when it appears in a union.  Detect it by
                // checking whether ALL property names are standard Object.prototype
                // methods.  Similarly, skip for `Function` (has bind, call, apply, etc.).
                if self.is_global_object_or_function_shape(&shape) {
                    return;
                }

                target_shapes.push(shape.clone());
            }

            if target_shapes.is_empty() {
                return;
            }

            if self.try_discriminated_union_excess_check(source, target, idx) {
                return;
            }

            // For union excess property checking, tsc uses two strategies:
            //
            // 1. Discriminant narrowing: if a source property has a unit literal
            //    value (e.g. kind: "sq") that narrows to a strict subset of union
            //    members, check excess against only those members. This matches
            //    tsc's behavior for discriminated unions.
            //
            // 2. Fallback: check if property exists in ANY member of the union.
            //    A property is only excess if it doesn't appear in any member.
            //    This differs from the old `matched_shapes` approach which
            //    incorrectly restricted property existence checks to only
            //    structurally-matched members, causing false TS2353 errors.
            let discriminant_shapes = self
                .discriminant_matching_union_member_indices(
                    idx,
                    source_props,
                    &target_shapes,
                    explicit_property_names.as_ref(),
                )
                .unwrap_or_default()
                .into_iter()
                .map(|i| target_shapes[i].clone())
                .collect::<Vec<_>>();
            let effective_shapes = if discriminant_shapes.is_empty() {
                if has_unresolved_member {
                    let matching_shapes = target_shapes
                        .iter()
                        .filter(|shape| {
                            shape.properties.iter().all(|target_prop| {
                                if target_prop.optional {
                                    return true;
                                }
                                source_props.iter().any(|source_prop| {
                                    source_prop.name == target_prop.name
                                        && self.is_assignable_to(
                                            source_prop.type_id,
                                            target_prop.type_id,
                                        )
                                })
                            })
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    if matching_shapes.is_empty() {
                        return;
                    }
                    matching_shapes
                } else {
                    target_shapes
                }
            } else {
                discriminant_shapes
            };

            for source_prop in source_props {
                if explicit_property_names.is_some()
                    && !explicit_property_names
                        .as_ref()
                        .is_some_and(|names| names.contains(&source_prop.name))
                {
                    continue;
                }

                // For unions, check if property exists in ANY member
                let target_prop_types: Vec<TypeId> = effective_shapes
                    .iter()
                    .filter_map(|shape| {
                        shape
                            .properties
                            .iter()
                            .find(|prop| prop.name == source_prop.name)
                            .map(|prop| prop.type_id)
                    })
                    .collect();

                if target_prop_types.is_empty() {
                    // A number index signature covers numeric property names
                    if any_member_has_number_index {
                        let name_str = self.ctx.types.resolve_atom(source_prop.name);
                        if tsz_solver::utils::is_numeric_literal_name(&name_str) {
                            continue;
                        }
                    }
                    let report_idx = self
                        .find_object_literal_property_element(idx, source_prop.name)
                        .unwrap_or(idx);
                    let prop_name = self.object_literal_property_display_name(
                        report_idx,
                        self.ctx.types.resolve_atom(source_prop.name).as_ref(),
                    );
                    self.error_excess_property_at(&prop_name, target, report_idx);
                    self.check_excess_property_initializer_implicit_any(report_idx, target);
                } else {
                    // =============================================================
                    // NESTED OBJECT LITERAL EXCESS PROPERTY CHECKING
                    // =============================================================
                    // For nested object literals, recursively check for excess properties
                    // Example: { x: { y: 1, z: 2 } } where target is { x: { y: number } }
                    // should error on 'z' in the nested object literal
                    //
                    // CRITICAL FIX: For union targets, we must union all property types
                    // from all members. Using only the first member causes false positives.
                    // Example: type T = { x: { a: number } } | { x: { b: number } }
                    // Assigning { x: { b: 1 } } should NOT error on 'b'.
                    // =============================================================
                    let nested_target = tsz_solver::utils::union_or_single(
                        self.ctx.types,
                        target_prop_types.clone(),
                    );
                    let nested_target = self.nested_property_target_type(
                        effective_target,
                        source_prop.name,
                        nested_target,
                    );

                    self.check_nested_object_literal_excess_properties(
                        source_prop.name,
                        Some(nested_target),
                        idx,
                    );
                }
            }
            return;
        }

        if self.contextual_type_is_unresolved_for_argument_refresh(target)
            || self.contextual_type_is_unresolved_for_argument_refresh(evaluated_target)
        {
            return;
        }

        // Handle intersection targets
        if let Some(members) = query::intersection_members(self.ctx.types, resolved_target) {
            let mut target_shapes = Vec::new();
            let mut dynamic_members = Vec::new();
            let mut has_index_signature = false;
            let mut index_value_types: Vec<TypeId> = Vec::new();
            let mut has_primitive_member = false;

            for &member in members.iter() {
                let resolved_member = self.resolve_type_for_property_access(member);
                let resolved_member = self.narrow_union_target_by_object_literal_discriminants(
                    resolved_member,
                    idx,
                    explicit_property_names.as_ref(),
                );
                if query::is_type_parameter_like(self.ctx.types, resolved_member) {
                    return;
                }
                if tsz_solver::is_primitive_type(self.ctx.types, resolved_member) {
                    has_primitive_member = true;
                    continue;
                }
                if let Some(shape) = query::object_shape(self.ctx.types, resolved_member) {
                    if let Some(ref idx_sig) = shape.string_index {
                        has_index_signature = true;
                        index_value_types.push(idx_sig.value_type);
                    }
                    if shape.number_index.is_some() {
                        has_index_signature = true;
                    }
                    target_shapes.push(shape.clone());
                } else {
                    // `object` is structurally equivalent to `{}` — it has no named
                    // properties or index signatures, but should NOT suppress excess
                    // property checking on other intersection members.
                    // In tsc, `object & { err: string }` still checks excess properties
                    // against `{ err: string }`.
                    if resolved_member == TypeId::OBJECT {
                        continue;
                    }
                    dynamic_members.push(resolved_member);
                }
            }

            if has_primitive_member {
                return;
            }

            if target_shapes.is_empty() && dynamic_members.is_empty() {
                return;
            }

            for source_prop in source_props {
                if explicit_property_names.is_some()
                    && !explicit_property_names
                        .as_ref()
                        .is_some_and(|names| names.contains(&source_prop.name))
                {
                    continue;
                }

                // For intersections, property exists if it's in ANY member's named
                // properties OR covered by an index signature.
                let mut found_in_named = false;
                let mut member_may_accept_unknown = false;
                let mut nested_target_types = Vec::new();

                for shape in &target_shapes {
                    if let Some(prop) = shape.properties.iter().find(|p| p.name == source_prop.name)
                    {
                        found_in_named = true;
                        nested_target_types.push(prop.type_id);
                    }
                }
                for &member in &dynamic_members {
                    match self.resolve_property_access_with_env(
                        member,
                        self.ctx.types.resolve_atom(source_prop.name).as_ref(),
                    ) {
                        tsz_solver::operations::property::PropertyAccessResult::Success {
                            type_id,
                            ..
                        } => {
                            found_in_named = true;
                            nested_target_types.push(type_id);
                        }
                        tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                            ..
                        } => {}
                        _ => {
                            member_may_accept_unknown = true;
                        }
                    }
                }

                let is_known = found_in_named || has_index_signature || member_may_accept_unknown;

                if !is_known {
                    let report_idx = self
                        .find_object_literal_property_element(idx, source_prop.name)
                        .unwrap_or(idx);
                    let prop_name = self.object_literal_property_display_name(
                        report_idx,
                        self.ctx.types.resolve_atom(source_prop.name).as_ref(),
                    );
                    self.error_excess_property_at(&prop_name, target, report_idx);
                    self.check_excess_property_initializer_implicit_any(report_idx, target);
                } else {
                    // Combine named property types with index signature value types
                    // for the nested excess check. This ensures that for intersections
                    // like `{ [k: string]: { a: 0 } } & { [k: string]: { b: 0 } }`,
                    // the nested target is `{ a: 0 } & { b: 0 }`.
                    let all_nested: Vec<TypeId> = nested_target_types
                        .into_iter()
                        .chain(index_value_types.iter().copied())
                        .collect();

                    if !all_nested.is_empty() {
                        let nested_target =
                            tsz_solver::utils::intersection_or_single(self.ctx.types, all_nested);
                        let nested_target = self.nested_property_target_type(
                            effective_target,
                            source_prop.name,
                            nested_target,
                        );
                        self.check_nested_object_literal_excess_properties(
                            source_prop.name,
                            Some(nested_target),
                            idx,
                        );
                    }
                }
            }
            return;
        }

        if crate::query_boundaries::common::is_mapped_type(self.ctx.types, effective_target) {
            for source_prop in source_props {
                if explicit_property_names.is_some()
                    && !explicit_property_names
                        .as_ref()
                        .is_some_and(|names| names.contains(&source_prop.name))
                {
                    continue;
                }

                let prop_name = self.ctx.types.resolve_atom(source_prop.name);
                match self.resolve_property_access_with_env(effective_target, prop_name.as_ref()) {
                    tsz_solver::operations::property::PropertyAccessResult::Success {
                        type_id,
                        ..
                    } => {
                        let nested_target = self.nested_property_target_type(
                            effective_target,
                            source_prop.name,
                            type_id,
                        );
                        self.check_nested_object_literal_excess_properties(
                            source_prop.name,
                            Some(nested_target),
                            idx,
                        );
                    }
                    tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                        ..
                    } => {
                        let report_idx = self
                            .find_object_literal_property_element(idx, source_prop.name)
                            .unwrap_or(idx);
                        let prop_name = self.object_literal_property_display_name(
                            report_idx,
                            self.ctx.types.resolve_atom(source_prop.name).as_ref(),
                        );
                        self.error_excess_property_at(&prop_name, target, report_idx);
                        self.check_excess_property_initializer_implicit_any(report_idx, target);
                    }
                    _ => return,
                }
            }
            return;
        }

        // Handle simple object targets via the canonical boundary classification.
        //
        // The boundary's `classify_object_properties` determines which source
        // properties are excess (WHAT), while this checker code handles WHERE to
        // anchor diagnostics and recursive nested-literal checking.
        if let Some(target_shape) = query::object_shape(self.ctx.types, resolved_target) {
            let target_props = target_shape.properties.as_slice();

            // When the target has a string index signature, outer property names are
            // all valid (any string key is accepted). But we still need to check
            // nested object literals against the index signature VALUE type for excess
            // properties. E.g., for target `{ [k: string]: { a: 0 } & { b: 0 } }`,
            // a nested `{ a: 0, b: 0, c: 0 }` should flag `c` as excess.
            if let Some(ref idx_sig) = target_shape.string_index {
                let idx_value_type = idx_sig.value_type;
                for source_prop in source_props {
                    if explicit_property_names.is_some()
                        && !explicit_property_names
                            .as_ref()
                            .is_some_and(|names| names.contains(&source_prop.name))
                    {
                        continue;
                    }
                    // Combine with any named property type (if the property also exists explicitly)
                    let mut nested_types = vec![idx_value_type];
                    if let Some(target_prop) =
                        target_props.iter().find(|p| p.name == source_prop.name)
                    {
                        nested_types.push(target_prop.type_id);
                    }
                    let nested_target =
                        tsz_solver::utils::intersection_or_single(self.ctx.types, nested_types);
                    let nested_target = self.nested_property_target_type(
                        effective_target,
                        source_prop.name,
                        nested_target,
                    );
                    self.check_nested_object_literal_excess_properties(
                        source_prop.name,
                        Some(nested_target),
                        idx,
                    );
                }
                return;
            }

            // Use the boundary classification to determine early-exit conditions and
            // which properties are excess, instead of re-implementing shape analysis.
            use crate::query_boundaries::assignability::classify_object_properties;
            let classification =
                classify_object_properties(self.ctx.types, source, resolved_target);

            if let Some(ref cls) = classification {
                // Boundary-driven early exits: empty object, index signatures,
                // global Object/Function, type parameters.
                if cls.target_is_empty_object
                    || cls.target_has_index_signature
                    || cls.target_is_global_object_or_function
                    || cls.target_is_type_parameter
                {
                    return;
                }
            }

            // Report excess properties identified by the boundary, then check nested.
            for source_prop in source_props {
                if explicit_property_names.is_some()
                    && !explicit_property_names
                        .as_ref()
                        .is_some_and(|names| names.contains(&source_prop.name))
                {
                    continue;
                }

                let prop_name = self.ctx.types.resolve_atom(source_prop.name);
                let dynamic_target_prop_type = target_props
                    .iter()
                    .all(|prop| prop.name != source_prop.name)
                    .then(|| {
                        self.contextual_object_literal_property_type(target, prop_name.as_ref())
                            .or_else(|| {
                                [
                                    target,
                                    self.evaluate_contextual_type(target),
                                    effective_target,
                                ]
                                .into_iter()
                                .find_map(|candidate| {
                                    match self.resolve_property_access_with_env(
                                        candidate,
                                        prop_name.as_ref(),
                                    ) {
                                        tsz_solver::operations::property::PropertyAccessResult::Success {
                                            type_id,
                                            from_index_signature: false,
                                            ..
                                        } if type_id != TypeId::ANY && type_id != TypeId::ERROR => Some(type_id),
                                        _ => None,
                                    }
                                })
                            })
                    })
                    .flatten()
                    .filter(|&type_id| type_id != TypeId::ANY);

                // Use boundary classification for the excess-property decision,
                // but honor property-resolution fallbacks for contextual targets
                // whose structural shape has not materialized the accessible keys yet.
                let boundary_marks_excess = classification
                    .as_ref()
                    .is_some_and(|cls| cls.excess_properties.contains(&source_prop.name));
                let boundary_excess_is_authoritative = classification
                    .as_ref()
                    .is_some_and(|cls| cls.trimmed_source_assignable);
                let is_excess = boundary_marks_excess
                    && (boundary_excess_is_authoritative || dynamic_target_prop_type.is_none());
                if is_excess {
                    let report_idx = self
                        .find_object_literal_property_element(idx, source_prop.name)
                        .unwrap_or(idx);
                    let prop_name = self.object_literal_property_display_name(
                        report_idx,
                        self.ctx.types.resolve_atom(source_prop.name).as_ref(),
                    );
                    self.error_excess_property_at(&prop_name, target, report_idx);
                    self.check_excess_property_initializer_implicit_any(report_idx, target);
                } else {
                    // Property exists in target — check nested object literals.
                    let target_prop_type = target_props
                        .iter()
                        .find(|p| p.name == source_prop.name)
                        .map(|prop| prop.type_id)
                        .or(dynamic_target_prop_type);
                    if let Some(target_prop_type) = target_prop_type {
                        let nested_target = self.nested_property_target_type(
                            effective_target,
                            source_prop.name,
                            target_prop_type,
                        );
                        self.check_nested_object_literal_excess_properties(
                            source_prop.name,
                            Some(nested_target),
                            idx,
                        );
                    }
                }
            }
        }
        // Note: Missing property checks are handled by solver's explain_failure
    }

    /// Boolean query: does a fresh source have any excess properties relative to
    /// the target?
    ///
    /// Delegates to the canonical `classify_object_properties` boundary function
    /// For fresh object literals assigned to discriminated union targets, detect
    /// the discriminant member and emit TS2353 for excess properties against that
    /// specific member. Returns `true` if excess properties were found and
    /// reported, meaning the caller should skip the regular assignability error.
    ///
    /// This matches tsc behavior: `{ kind: "sq", x: 12 }` assigned to
    /// `Square | Rectangle` where Square = { kind: "sq", size: number }
    /// reports "'x' does not exist in type 'Square'" (not a generic TS2322).
    pub(crate) fn try_discriminated_union_excess_check(
        &mut self,
        source: TypeId,
        target: TypeId,
        obj_literal_idx: NodeIndex,
    ) -> bool {
        use crate::query_boundaries::common as freshness_query;

        let is_fresh_source = freshness_query::is_fresh_object_type(self.ctx.types, source);
        let explicit_property_names = if is_fresh_source {
            None
        } else {
            self.explicit_object_literal_property_names_for_spread(obj_literal_idx)
        };

        if !is_fresh_source && explicit_property_names.is_none() {
            return false;
        }

        let Some(source_shape) = query::object_shape(self.ctx.types, source) else {
            return false;
        };

        let resolved_target = self.resolve_type_for_property_access(target);
        let Some(members) = query::union_members(self.ctx.types, resolved_target) else {
            return false;
        };

        // Try to get the original (possibly Lazy) union members for type name display.
        // If the target resolves through a type alias, the original members preserve
        // their Lazy wrappers and format as named types (e.g., "Square" instead of
        // "{ size: number; kind: \"sq\" }").
        let original_members = query::union_members(self.ctx.types, target);

        // Collect resolved shapes for each union member, along with the original
        // TypeId (for error message formatting) which preserves type alias names.
        let mut member_shapes: Vec<(TypeId, std::sync::Arc<tsz_solver::ObjectShape>)> = Vec::new();
        for (i, &member) in members.iter().enumerate() {
            let resolved = self.resolve_type_for_property_access(member);
            if let Some(shape) = query::object_shape(self.ctx.types, resolved) {
                // Prefer the original (Lazy) member for display, fall back to resolved
                let display_id = original_members
                    .as_ref()
                    .and_then(|orig| orig.get(i).copied())
                    .unwrap_or(member);
                member_shapes.push((display_id, shape));
            }
        }

        if member_shapes.is_empty() {
            return false;
        }

        // Find a source property with a unit type that matches exactly one member
        let source_props = source_shape.properties.as_slice();
        let union_shapes: Vec<_> = member_shapes
            .iter()
            .map(|(_, shape)| shape.clone())
            .collect();
        let matching_indices = self
            .discriminant_matching_union_member_indices(
                obj_literal_idx,
                source_props,
                &union_shapes,
                explicit_property_names.as_ref(),
            )
            .unwrap_or_default();

        let Some(idx) = matching_indices.first().copied() else {
            return false;
        };
        let narrowed_member_type = member_shapes[idx].0;
        let narrowed_shape = &member_shapes[idx].1;

        // Collect excess properties (not in narrowed member) with their AST positions.
        // tsc reports only the first excess property in source order.
        let mut excess_candidates: Vec<(tsz_common::interner::Atom, NodeIndex, u32)> = Vec::new();
        for source_prop in source_props {
            if explicit_property_names.is_some()
                && !explicit_property_names
                    .as_ref()
                    .is_some_and(|names| names.contains(&source_prop.name))
            {
                continue;
            }

            let exists_in_narrowed = narrowed_shape
                .properties
                .iter()
                .any(|p| p.name == source_prop.name);

            if !exists_in_narrowed {
                let report_idx = self
                    .find_object_literal_property_element(obj_literal_idx, source_prop.name)
                    .unwrap_or(obj_literal_idx);
                let pos = self
                    .ctx
                    .arena
                    .get(report_idx)
                    .map(|n| n.pos)
                    .unwrap_or(u32::MAX);
                excess_candidates.push((source_prop.name, report_idx, pos));
            }
        }

        // Report the first excess property by source position (earliest in file)
        if let Some(earliest) = excess_candidates.iter().min_by_key(|c| c.2) {
            let prop_name = self.object_literal_property_display_name(
                earliest.1,
                self.ctx.types.resolve_atom(earliest.0).as_ref(),
            );
            self.error_excess_property_at(&prop_name, narrowed_member_type, earliest.1);
            self.check_excess_property_initializer_implicit_any(earliest.1, narrowed_member_type);
            true
        } else {
            false
        }
    }

    fn discriminant_matching_union_member_indices(
        &mut self,
        obj_literal_idx: NodeIndex,
        source_props: &[tsz_solver::PropertyInfo],
        union_shapes: &[std::sync::Arc<tsz_solver::ObjectShape>],
        explicit_property_names: Option<&HashSet<Atom>>,
    ) -> Option<Vec<usize>> {
        let direct_discriminants =
            self.object_literal_direct_unit_discriminants(obj_literal_idx, explicit_property_names);

        for (prop_name, prop_type) in direct_discriminants {
            let source_prop = source_props.iter().find(|prop| prop.name == prop_name);
            let Some(source_prop) = source_prop else {
                continue;
            };

            let mut target_prop_types = Vec::with_capacity(union_shapes.len());
            for shape in union_shapes {
                let Some(target_prop) =
                    shape.properties.iter().find(|p| p.name == source_prop.name)
                else {
                    target_prop_types.clear();
                    break;
                };
                target_prop_types.push(target_prop.type_id);
            }

            if target_prop_types.len() != union_shapes.len() {
                continue;
            }

            // tsc's discriminant narrowing doesn't require ALL member property types
            // to be unit types. It checks which members the source unit value is
            // assignable to. E.g., for { a: null } | { a: string }, source `a: null`
            // narrows to the first member because null is assignable to null but not
            // to string (in strict mode).
            let matching_indices: Vec<usize> = target_prop_types
                .iter()
                .enumerate()
                .filter_map(|(i, &target_ty)| self.is_subtype_of(prop_type, target_ty).then_some(i))
                .collect();

            if !matching_indices.is_empty() && matching_indices.len() < union_shapes.len() {
                return Some(matching_indices);
            }
        }

        None
    }

    fn object_literal_direct_unit_discriminants(
        &mut self,
        obj_literal_idx: NodeIndex,
        explicit_property_names: Option<&HashSet<Atom>>,
    ) -> Vec<(Atom, TypeId)> {
        let Some(obj_node) = self.ctx.arena.get(obj_literal_idx) else {
            return Vec::new();
        };
        let Some(obj_lit) = self.ctx.arena.get_literal_expr(obj_node) else {
            return Vec::new();
        };

        let mut discriminants = Vec::new();
        for &elem_idx in &obj_lit.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                continue;
            };
            let Some(prop_name) = self.get_property_name_resolved(prop.name) else {
                continue;
            };
            let prop_atom = self.ctx.types.intern_string(&prop_name);
            if explicit_property_names.is_some_and(|names| !names.contains(&prop_atom)) {
                continue;
            }
            let Some(lit_type) = self.literal_type_from_initializer(prop.initializer) else {
                continue;
            };
            if !query::is_unit_type(self.ctx.types, lit_type) {
                continue;
            }
            discriminants.push((prop_atom, lit_type));
        }

        discriminants
    }

    fn narrow_union_target_by_object_literal_discriminants(
        &mut self,
        union_type: TypeId,
        obj_literal_idx: NodeIndex,
        explicit_property_names: Option<&HashSet<Atom>>,
    ) -> TypeId {
        let Some(members) = query::union_members(self.ctx.types, union_type) else {
            return union_type;
        };

        let direct_discriminants =
            self.object_literal_direct_unit_discriminants(obj_literal_idx, explicit_property_names);
        if direct_discriminants.is_empty() {
            return union_type;
        }

        for (prop_name, prop_type) in direct_discriminants {
            let mut matching_members = Vec::new();
            let mut fully_discriminated = true;

            for &member in &members {
                let resolved_member = self.resolve_type_for_property_access(member);
                let Some(shape) = query::object_shape(self.ctx.types, resolved_member) else {
                    fully_discriminated = false;
                    break;
                };
                let Some(prop) = shape.properties.iter().find(|prop| prop.name == prop_name) else {
                    fully_discriminated = false;
                    break;
                };
                if !query::is_unit_type(self.ctx.types, prop.type_id) {
                    fully_discriminated = false;
                    break;
                }
                if self.is_subtype_of(prop_type, prop.type_id) {
                    matching_members.push(member);
                }
            }

            if fully_discriminated
                && !matching_members.is_empty()
                && matching_members.len() < members.len()
            {
                return tsz_solver::utils::union_or_single(self.ctx.types, matching_members);
            }
        }

        union_type
    }

    /// Detect whether an object shape represents the global `Object` or `Function`
    /// interface (or similar built-in prototypes).
    ///
    /// Delegates to the canonical boundary function
    /// `query_boundaries::assignability::is_global_object_or_function_shape`.
    fn is_global_object_or_function_shape(&self, shape: &tsz_solver::ObjectShape) -> bool {
        crate::query_boundaries::assignability::is_global_object_or_function_shape_boundary(
            self.ctx.types,
            shape,
        )
    }

    fn explicit_object_literal_property_names_for_spread(
        &self,
        obj_literal_idx: NodeIndex,
    ) -> Option<HashSet<Atom>> {
        let obj_node = self.ctx.arena.get(obj_literal_idx)?;
        if obj_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let obj_lit = self.ctx.arena.get_literal_expr(obj_node)?;

        let has_spread = obj_lit.elements.nodes.iter().any(|&elem_idx| {
            self.ctx.arena.get(elem_idx).is_some_and(|elem_node| {
                elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
                    || elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
            })
        });
        if !has_spread {
            return None;
        }

        let mut explicit_names = HashSet::new();
        for &elem_idx in &obj_lit.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            let elem_prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(elem_node)
                    .and_then(|method| self.get_property_name(method.name)),
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                    .ctx
                    .arena
                    .get_accessor(elem_node)
                    .and_then(|accessor| self.get_property_name(accessor.name)),
                _ => None,
            };

            if let Some(name) = elem_prop_name {
                explicit_names.insert(self.ctx.types.intern_string(&name));
            }
        }

        Some(explicit_names)
    }

    /// Check nested object literal properties for excess properties.
    ///
    /// This implements recursive excess property checking for nested object literals.
    /// For example, in `const p: { x: { y: number } } = { x: { y: 1, z: 2 } }`,
    /// the nested object literal `{ y: 1, z: 2 }` should be checked for excess property `z`.
    fn check_nested_object_literal_excess_properties(
        &mut self,
        prop_name: tsz_common::interner::Atom,
        target_prop_type: Option<TypeId>,
        obj_literal_idx: NodeIndex,
    ) {
        // Get the AST node for the object literal
        let Some(obj_node) = self.ctx.arena.get(obj_literal_idx) else {
            return;
        };

        let Some(obj_lit) = self.ctx.arena.get_literal_expr(obj_node) else {
            return;
        };

        // =============================================================
        // CRITICAL FIX: Iterate in reverse to handle duplicate properties
        // =============================================================
        // JavaScript/TypeScript behavior is "last property wins".
        // Example: const o = { x: { a: 1 }, x: { b: 1 } }
        // The runtime value of o.x is { b: 1 }, so we must check the last assignment.
        // =============================================================
        for &elem_idx in obj_lit.elements.nodes.iter().rev() {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Get the property name from this element
            let elem_prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name))
                    .map(|name| self.ctx.types.intern_string(&name)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| {
                        self.get_property_name(prop.name)
                            .map(|name| self.ctx.types.intern_string(&name))
                    }),
                _ => None,
            };

            // Skip if this property doesn't match the one we're looking for
            if elem_prop_name != Some(prop_name) {
                continue;
            }

            // Get the value expression for this property
            let value_idx = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .map(|prop| prop.initializer),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    // For shorthand properties, the value expression is the same as the property name expression
                    self.ctx
                        .arena
                        .get_shorthand_property(elem_node)
                        .map(|prop| prop.name)
                }
                _ => None,
            };

            let Some(value_idx) = value_idx else {
                continue;
            };

            // =============================================================
            // CRITICAL FIX: Handle parenthesized expressions
            // =============================================================
            // TypeScript treats parenthesized object literals as fresh.
            // Example: x: ({ a: 1 }) should be checked for excess properties.
            // We need to unwrap parentheses before checking the kind.
            // =============================================================
            let effective_value_idx = self.ctx.arena.skip_parenthesized(value_idx);
            let Some(value_node) = self.ctx.arena.get(effective_value_idx) else {
                continue;
            };

            if value_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                // Check if we have a target type for this property
                if let Some(nested_target_type) = target_prop_type {
                    // Preserve the derived contextual target when retyping nested object
                    // literals for recursive excess-property checks.
                    let nested_request =
                        crate::context::TypingRequest::with_contextual_type(nested_target_type);
                    let nested_source_type =
                        self.get_type_of_node_with_request(effective_value_idx, &nested_request);

                    // Recursively check the nested object literal for excess properties
                    self.check_object_literal_excess_properties(
                        nested_source_type,
                        nested_target_type,
                        effective_value_idx,
                    );
                }

                return; // Found the property, stop searching
            }
        }
    }

    /// Find the property element node in an object literal by interned property name.
    fn find_object_literal_property_element(
        &self,
        obj_literal_idx: NodeIndex,
        prop_name: tsz_common::interner::Atom,
    ) -> Option<NodeIndex> {
        let obj_node = self.ctx.arena.get(obj_literal_idx)?;
        let obj_lit = self.ctx.arena.get_literal_expr(obj_node)?;
        for &elem_idx in &obj_lit.elements.nodes {
            let elem_node = self.ctx.arena.get(elem_idx)?;
            let elem_prop_atom = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name))
                    .map(|name| self.ctx.types.intern_string(&name)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| self.get_identifier_text_from_idx(prop.name))
                    .map(|name| self.ctx.types.intern_string(&name)),
                _ => None,
            };
            if elem_prop_atom == Some(prop_name) {
                return Some(elem_idx);
            }
        }
        None
    }

    fn object_literal_property_display_name(
        &self,
        elem_idx: NodeIndex,
        fallback_name: &str,
    ) -> String {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return fallback_name.to_string();
        };

        match elem_node.kind {
            syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                .ctx
                .arena
                .get_property_assignment(elem_node)
                .and_then(|prop| {
                    self.ctx.arena.get(prop.name).and_then(|name_node| {
                        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                            self.computed_property_display_name(prop.name)
                        } else {
                            self.get_property_name(prop.name)
                        }
                    })
                })
                .unwrap_or_else(|| fallback_name.to_string()),
            syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                .ctx
                .arena
                .get_shorthand_property(elem_node)
                .and_then(|prop| self.get_identifier_text_from_idx(prop.name))
                .unwrap_or_else(|| fallback_name.to_string()),
            _ => fallback_name.to_string(),
        }
    }

    /// TS2353 guard for object destructuring from object literals with computed keys.
    ///
    /// TypeScript reports excess-property errors for computed properties in object
    /// literal initializers when the binding pattern contains only explicit keys.
    pub(crate) fn check_destructuring_object_literal_computed_excess_properties(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
        target_type: TypeId,
        use_pattern_target_type: bool,
    ) {
        if initializer_idx.is_none() || target_type == TypeId::ERROR {
            return;
        }

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return;
        }
        let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        // Keep this narrow: if the pattern has rest, leave behavior to
        // the general relation path.
        for &element_idx in &pattern.elements.nodes {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            let Some(element) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };
            if element.dot_dot_dot_token {
                return;
            }
        }

        let effective_init = self.ctx.arena.skip_parenthesized(initializer_idx);
        let Some(init_node) = self.ctx.arena.get(effective_init) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        };
        let Some(init_lit) = self.ctx.arena.get_literal_expr(init_node) else {
            return;
        };

        // Use explicit pattern keys only when the pattern is the source of truth (no
        // explicit annotation). This avoids overriding annotated target types like
        // `{x: string}` with a synthetic `{x: any}` shape.
        let effective_target_type = if use_pattern_target_type {
            self.object_binding_pattern_target_type_for_excess_checks(pattern_idx)
                .unwrap_or(target_type)
        } else {
            target_type
        };

        if effective_target_type == TypeId::ANY {
            return;
        }

        // Get the properties of the target type
        let Some(target_shape) = query::object_shape(self.ctx.types, effective_target_type) else {
            return;
        };
        let target_props = target_shape.properties.as_slice();

        for &elem_idx in &init_lit.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Get the property name from this element
            let prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| {
                        if let Some(name_node) = self.ctx.arena.get(prop.name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        {
                            return self.computed_property_display_name(prop.name);
                        }

                        self.get_property_name(prop.name)
                    }),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                _ => None,
            };

            let prop_name = if let Some(pn) = prop_name {
                pn
            } else if elem_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                && let Some(prop) = self.ctx.arena.get_property_assignment(elem_node)
                && let Some(name_node) = self.ctx.arena.get(prop.name)
                && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            {
                self.computed_property_display_name(prop.name)
                    .unwrap_or("[computed property]".to_string())
            } else {
                continue;
            };

            let prop_atom = self.ctx.types.intern_string(&prop_name);

            // Check if the property exists in the target type
            let target_prop = target_props.iter().find(|p| p.name == prop_atom);
            if target_prop.is_none() {
                self.error_excess_property_at_no_suggestion(
                    &prop_name,
                    effective_target_type,
                    elem_idx,
                );
                self.check_excess_property_initializer_implicit_any(
                    elem_idx,
                    effective_target_type,
                );
            }
        }
    }

    fn object_binding_pattern_target_type_for_excess_checks(
        &mut self,
        pattern_idx: NodeIndex,
    ) -> Option<TypeId> {
        use tsz_common::interner::Atom;

        let pattern_node = self.ctx.arena.get(pattern_idx)?;
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return None;
        }
        let pattern = self.ctx.arena.get_binding_pattern(pattern_node)?;

        // Empty binding pattern `var {} = ...` — no properties are being destructured,
        // so no excess property check is needed. tsc treats `{}` as "ignore all properties".
        if pattern.elements.nodes.is_empty() {
            return None;
        }

        let mut prop_names: Vec<Atom> = Vec::new();
        for &element_idx in &pattern.elements.nodes {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            let Some(element) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };

            if element.dot_dot_dot_token {
                return None;
            }

            let property_names: Vec<Atom> = if element.property_name.is_some() {
                let Some(property_name_node) = self.ctx.arena.get(element.property_name) else {
                    continue;
                };
                if property_name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    let computed = self.ctx.arena.get_computed_property(property_name_node)?;
                    let key_type = self.get_type_of_node(computed.expression);
                    let (string_keys, number_keys) =
                        self.get_literal_key_union_from_type(key_type)?;
                    if string_keys.is_empty() && number_keys.is_empty() {
                        return None;
                    }

                    let mut names = Vec::with_capacity(string_keys.len() + number_keys.len());
                    names.extend(string_keys);
                    names.extend(number_keys.into_iter().map(|num| {
                        self.ctx.types.intern_string(
                            &tsz_solver::utils::canonicalize_numeric_name(&num.to_string())
                                .unwrap_or_else(|| num.to_string()),
                        )
                    }));
                    names
                } else {
                    vec![
                        self.ctx
                            .types
                            .intern_string(&self.get_property_name(element.property_name)?),
                    ]
                }
            } else {
                vec![
                    self.ctx
                        .types
                        .intern_string(&self.get_identifier_text_from_idx(element.name)?),
                ]
            };

            prop_names.extend(property_names);
        }

        if prop_names.is_empty() {
            return None;
        }

        let mut props = Vec::with_capacity(prop_names.len());
        for name in prop_names {
            if props
                .iter()
                .any(|prop: &tsz_solver::PropertyInfo| prop.name == name)
            {
                continue;
            }
            props.push(tsz_solver::PropertyInfo {
                name,
                type_id: TypeId::ANY,
                write_type: TypeId::ANY,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: tsz_common::Visibility::Public,
                parent_id: None,
                declaration_order: props.len() as u32,
            });
        }

        Some(self.ctx.types.factory().object(props))
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::check_source_diagnostics;

    #[test]
    fn ts2353_spread_object_literal_reports_explicit_excess_property_only() {
        let diags = check_source_diagnostics(
            "let x = { b: 1, extra: 2 };\nlet xx: { a, b } = { a: 1, ...x, z: 3 };",
        );

        let ts2353 = diags.iter().filter(|d| d.code == 2353).collect::<Vec<_>>();
        assert_eq!(
            ts2353.len(),
            1,
            "expected one TS2353 for z, got {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
        assert!(
            ts2353[0].message_text.contains("'z'"),
            "TS2353 should mention z, got: {}",
            ts2353[0].message_text
        );
    }

    #[test]
    fn ts2353_inferred_pattern_target_type_reports_computed_property_name() {
        let diags = check_source_diagnostics(
            "const k = 'extra';\nconst source = { x: 1, y: 2 };\nlet { x } = { x: 1, ...source, [k]: 3 };",
        );

        let ts2353 = diags.iter().filter(|d| d.code == 2353).collect::<Vec<_>>();
        assert_eq!(
            ts2353.len(),
            1,
            "expected one TS2353 for [k], got {:?}",
            diags.iter().map(|d| d.code).collect::<Vec<_>>()
        );
        assert!(
            ts2353[0].message_text.contains("'[k]'") || ts2353[0].message_text.contains("\"[k]\""),
            "TS2353 should mention [k], got: {}",
            ts2353[0].message_text
        );
    }

    #[test]
    fn excess_property_method_contextual_retry_keeps_parameter_types() {
        let diags = check_source_diagnostics(
            r#"
type Nested = { run: (value: string) => string };
declare function accept(value: { nested: Nested }): void;

accept({
    nested: {
        run(value) { return value; },
        extra: 1,
    },
});
"#,
        );

        let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
        assert_eq!(
            ts7006.len(),
            0,
            "Expected method contextual retry during excess-property checking to keep parameter context, got: {diags:?}"
        );
    }

    #[test]
    fn excess_property_accessor_contextual_retry_keeps_setter_parameter_types() {
        let diags = check_source_diagnostics(
            r#"
type Access = { get size(): number; set size(value: number); };
declare function accept(value: Access): void;

accept({
    get size() { return 1; },
    set size(value) { void value; },
    extra: 1,
});
"#,
        );

        let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
        assert_eq!(
            ts7006.len(),
            0,
            "Expected accessor contextual retry during excess-property checking to keep setter parameter context, got: {diags:?}"
        );
    }
}
