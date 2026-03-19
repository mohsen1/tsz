//! Object literal excess property and property access checking.
//!
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
                        self.clear_type_cache_recursive(prop.initializer);
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
                        self.clear_type_cache_recursive(elem_idx);
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
                        self.clear_type_cache_recursive(elem_idx);
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
        use tsz_solver::relations::freshness;

        // Excess property checks do not apply to type parameters (even with constraints).
        if query::is_type_parameter_like(self.ctx.types, target) {
            return;
        }

        // Only check excess properties for FRESH object literals
        // This is the key TypeScript behavior:
        // - const p: Point = {x: 1, y: 2, z: 3}  // ERROR: 'z' is excess (fresh)
        // - const obj = {x: 1, y: 2, z: 3}; p = obj;  // OK: obj loses freshness
        // - const p: Point = { ...source, z: 3 }  // ERROR: only explicit property `z` is checked
        //
        // IMPORTANT: Freshness is tracked on the TypeId itself.
        // This fixes the "Zombie Freshness" bug by keeping fresh vs non-fresh
        // object types distinct at the interner level.
        let is_fresh_source = freshness::is_fresh_object_type(self.ctx.types, source);
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

            for &member in &members {
                let resolved_member = self.resolve_type_for_property_access(member);
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

                if shape.properties.is_empty()
                    || shape.string_index.is_some()
                    || shape.number_index.is_some()
                {
                    return;
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
                target_shapes
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

        // Handle object targets using type_queries
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

            // Empty object {} accepts any properties - no excess property check needed.
            // This is a key TypeScript behavior: {} means "any non-nullish value".
            // See https://github.com/microsoft/TypeScript/issues/60582
            if target_props.is_empty() {
                return;
            }

            if target_shape.number_index.is_some() {
                return;
            }

            // The global `Object` and `Function` interfaces from lib.d.ts accept
            // any object — skip excess property checking when they are the target.
            if self.is_global_object_or_function_shape(&target_shape) {
                return;
            }
            // This is the "freshness" or "strict object literal" check
            for source_prop in source_props {
                if explicit_property_names.is_some()
                    && !explicit_property_names
                        .as_ref()
                        .is_some_and(|names| names.contains(&source_prop.name))
                {
                    continue;
                }

                let target_prop = target_props.iter().find(|p| p.name == source_prop.name);
                if target_prop.is_none() {
                    let report_idx = self
                        .find_object_literal_property_element(idx, source_prop.name)
                        .unwrap_or(idx);
                    let prop_name = self.object_literal_property_display_name(
                        report_idx,
                        self.ctx.types.resolve_atom(source_prop.name).as_ref(),
                    );
                    self.error_excess_property_at(&prop_name, target, report_idx);
                    self.check_excess_property_initializer_implicit_any(report_idx, target);
                } else if let Some(target_prop) = target_prop {
                    // =============================================================
                    // NESTED OBJECT LITERAL EXCESS PROPERTY CHECKING
                    // =============================================================
                    // For nested object literals, recursively check for excess properties
                    let nested_target = self.nested_property_target_type(
                        effective_target,
                        source_prop.name,
                        target_prop.type_id,
                    );
                    self.check_nested_object_literal_excess_properties(
                        source_prop.name,
                        Some(nested_target),
                        idx,
                    );
                }
            }
        }
        // Note: Missing property checks are handled by solver's explain_failure
    }

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
        use tsz_solver::relations::freshness;

        let is_fresh_source = freshness::is_fresh_object_type(self.ctx.types, source);
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

            if !target_prop_types
                .iter()
                .all(|&target_ty| query::is_unit_type(self.ctx.types, target_ty))
            {
                continue;
            }

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
    /// interface (or similar built-in prototypes).  These types have only inherited
    /// method properties (toString, valueOf, constructor, bind, call, apply, …)
    /// and should suppress excess property checking when they appear as union members.
    fn is_global_object_or_function_shape(&self, shape: &tsz_solver::ObjectShape) -> bool {
        // Object.prototype methods:
        static OBJECT_PROTO: &[&str] = &[
            "constructor",
            "toString",
            "toLocaleString",
            "valueOf",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
        ];
        // Function.prototype methods (superset of Object):
        static FUNCTION_PROTO: &[&str] = &[
            "apply",
            "call",
            "bind",
            "toString",
            "length",
            "arguments",
            "caller",
            "prototype",
            "constructor",
            "toLocaleString",
            "valueOf",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
            // Symbol-keyed members are ignored by name check
        ];

        if shape.properties.is_empty() {
            return false;
        }

        shape.properties.iter().all(|prop| {
            let name = self.ctx.types.resolve_atom_ref(prop.name);
            OBJECT_PROTO.contains(&name.as_ref()) || FUNCTION_PROTO.contains(&name.as_ref())
        })
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

        // Keep this narrow: if the pattern has rest or computed names, leave behavior to
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
            if element.property_name.is_some()
                && let Some(prop_name_node) = self.ctx.arena.get(element.property_name)
                && prop_name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            {
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
                self.error_excess_property_at(&prop_name, effective_target_type, elem_idx);
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

            let property_name = if element.property_name.is_some() {
                let Some(property_name_node) = self.ctx.arena.get(element.property_name) else {
                    continue;
                };
                if property_name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    return None;
                }
                self.get_property_name(element.property_name)
            } else {
                self.get_identifier_text_from_idx(element.name)
            };

            property_name.as_ref()?;
        }

        let synthetic_type = self.infer_type_from_binding_pattern(pattern_idx, TypeId::ANY);
        if synthetic_type == TypeId::ANY || synthetic_type == TypeId::ERROR {
            None
        } else {
            Some(synthetic_type)
        }
    }

    pub(crate) fn computed_property_display_name(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.ctx.arena.get_computed_property(name_node)?;
        if let Some(ident_name) = self.get_identifier_text_from_idx(computed.expression) {
            return Some(format!("[{ident_name}]"));
        }

        let expr_node = self.ctx.arena.get(computed.expression)?;
        if expr_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16 {
            let literal = self.ctx.arena.get_literal(expr_node)?;
            return Some(format!("[\"{}\"]", literal.text));
        }

        if expr_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16 {
            let literal = self.ctx.arena.get_literal(expr_node)?;
            return Some(format!(
                "[{}]",
                tsz_solver::utils::canonicalize_numeric_name(&literal.text)
                    .unwrap_or_else(|| literal.text.clone())
            ));
        }

        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(expr_node)?;
            let obj_node = self.ctx.arena.get(access.expression)?;
            let obj_ident = self.ctx.arena.get_identifier(obj_node)?;
            if obj_ident.escaped_text.as_str() == "Symbol" {
                let prop_node = self.ctx.arena.get(access.name_or_argument)?;
                let prop_ident = self.ctx.arena.get_identifier(prop_node)?;
                return Some(format!("[Symbol.{}]", prop_ident.escaped_text));
            }
        }

        None
    }

    /// Resolve property access using `TypeEnvironment` (includes lib.d.ts types).
    ///
    /// This method creates a `PropertyAccessEvaluator` with the `TypeEnvironment` as the resolver,
    /// allowing primitive property access to use lib.d.ts definitions instead of just hardcoded lists.
    ///
    /// For example, "foo".length will look up the String interface from lib.d.ts.
    pub(crate) fn resolve_property_access_with_env(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> tsz_solver::operations::property::PropertyAccessResult {
        // Resolve TypeQuery types (typeof X) before property access.
        // The solver-internal evaluator has no TypeResolver, so TypeQuery types
        // can't be resolved there. Resolve them here using the checker's environment.
        let object_type = self.resolve_type_query_type(object_type);

        // Ensure preconditions are ready in the environment for non-trivial
        // property-access inputs. Already-resolved/function-like inputs don't
        // need relation preconditioning here.
        let resolution_kind =
            crate::query_boundaries::state::type_environment::classify_for_property_access_resolution(
                self.ctx.types,
                object_type,
            );
        if !matches!(
            resolution_kind,
            crate::query_boundaries::state::type_environment::PropertyAccessResolutionKind::Resolved
                | crate::query_boundaries::state::type_environment::PropertyAccessResolutionKind::FunctionLike
        ) {
            self.ensure_relation_input_ready(object_type);
        }

        // Route through QueryDatabase so repeated property lookups hit QueryCache.
        // This is especially important for hot paths like repeated `string[].push`
        // checks in class-heavy files.
        let result = self.ctx.types.resolve_property_access_with_options(
            object_type,
            prop_name,
            self.ctx.compiler_options.no_unchecked_indexed_access,
        );

        self.resolve_property_access_with_env_post_query(object_type, prop_name, result)
    }

    /// Continue environment-aware property access resolution from an already
    /// computed initial solver result.
    ///
    /// This avoids duplicate first-pass lookups in hot paths that already
    /// queried `resolve_property_access_with_options` and only need mapped/
    /// application fallback behavior.
    pub(crate) fn resolve_property_access_with_env_post_query(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
        result: tsz_solver::operations::property::PropertyAccessResult,
    ) -> tsz_solver::operations::property::PropertyAccessResult {
        let mut result = result;
        let mut resolved_object_type = object_type;
        let mut mapped_candidate_type = object_type;

        // If the receiver is an Application (e.g. Promise<number> or Pick<T, K>),
        // the QueryCache's noop TypeResolver can't expand it. Evaluate the
        // Application to its structural form so mapped-type revalidation can use
        // the real object shape. Only retry the initial lookup when it already
        // failed; otherwise preserve the original first-pass result and use the
        // expanded type only for mapped-property validation below.
        if tsz_solver::is_generic_application(self.ctx.types, object_type) {
            let expanded = self.evaluate_application_type(object_type);
            if expanded != object_type && expanded != TypeId::ANY && expanded != TypeId::ERROR {
                mapped_candidate_type = expanded;
                resolved_object_type = expanded;
                result = self.ctx.types.resolve_property_access_with_options(
                    expanded,
                    prop_name,
                    self.ctx.compiler_options.no_unchecked_indexed_access,
                );
            }
        }

        let pruned_object_type = self.prune_impossible_object_union_members_with_env(resolved_object_type);
        if pruned_object_type != resolved_object_type {
            resolved_object_type = pruned_object_type;
            mapped_candidate_type = pruned_object_type;
            result = self.ctx.types.resolve_property_access_with_options(
                pruned_object_type,
                prop_name,
                self.ctx.compiler_options.no_unchecked_indexed_access,
            );
        }

        // If the solver returned PropertyNotFound for a TypeParameter whose
        // constraint is an Application (e.g. `P extends Partial<Foo>`), the
        // solver's NoopResolver couldn't expand the Application body.  Evaluate
        // the constraint through the checker's TypeEnvironment and retry.
        // TODO: Move this resolution into the solver's PropertyAccessEvaluator
        // once it gains full TypeEnvironment/TypeResolver awareness.
        if matches!(
            result,
            tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound { .. }
        ) && let Some(constraint) =
            crate::query_boundaries::state::checking::type_parameter_constraint(
                self.ctx.types,
                resolved_object_type,
            )
        {
            let evaluated = self.evaluate_type_with_env(constraint);
            if evaluated != constraint && evaluated != TypeId::ANY && evaluated != TypeId::ERROR {
                let retry_result = self.ctx.types.resolve_property_access_with_options(
                    evaluated,
                    prop_name,
                    self.ctx.compiler_options.no_unchecked_indexed_access,
                );
                if matches!(
                    retry_result,
                    tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                ) {
                    result = retry_result;
                    resolved_object_type = evaluated;
                }
            }
        }

        if query::is_mapped_type(self.ctx.types, mapped_candidate_type)
            && let Some(mapped_property) =
                self.resolve_mapped_property_with_env(mapped_candidate_type, prop_name)
        {
            return mapped_property;
        }

        if matches!(
            result,
            tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound { .. }
        ) && let Some(members) =
            query::intersection_members(self.ctx.types, resolved_object_type)
        {
            let prop_atom = self.ctx.types.intern_string(prop_name);
            let mut member_results = Vec::new();
            let mut any_from_index = false;
            let mut saw_deferred_any_fallback = false;

            for member in members {
                match self.resolve_property_access_with_env(member, prop_name) {
                    tsz_solver::operations::property::PropertyAccessResult::Success {
                        type_id,
                        from_index_signature,
                        ..
                    } => {
                        if type_id == TypeId::ANY
                            && !from_index_signature
                            && query::needs_env_eval(self.ctx.types, member)
                        {
                            saw_deferred_any_fallback = true;
                            continue;
                        }
                        member_results.push(type_id);
                        any_from_index |= from_index_signature;
                    }
                    tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                        ..
                    } => {}
                    other => return other,
                }
            }

            if !member_results.is_empty() {
                let type_id = match member_results.len() {
                    1 => member_results[0],
                    _ => self.ctx.types.factory().intersection(member_results),
                };
                return tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id,
                    write_type: None,
                    from_index_signature: any_from_index,
                };
            }

            if saw_deferred_any_fallback {
                return tsz_solver::operations::property::PropertyAccessResult::simple(TypeId::ANY);
            }

            result = tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                type_id: resolved_object_type,
                property_name: prop_atom,
            };
        }

        // If property not found and the type is a Mapped type (e.g. { [P in Keys]: T }),
        // the solver's NoopResolver can't resolve Lazy(DefId) constraints (type alias refs).
        // Expand the mapped type using the checker's type environment and retry.
        if matches!(
            result,
            tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound { .. }
        ) && query::is_mapped_type(self.ctx.types, resolved_object_type)
        {
            let expanded = self.evaluate_mapped_type_with_resolution(resolved_object_type);
            if expanded != resolved_object_type
                && expanded != TypeId::ANY
                && expanded != TypeId::ERROR
            {
                return self.ctx.types.resolve_property_access_with_options(
                    expanded,
                    prop_name,
                    self.ctx.compiler_options.no_unchecked_indexed_access,
                );
            }
        }

        result
    }

    /// Resolve a single mapped-type property with environment-aware key/template
    /// evaluation, without expanding the whole mapped object.
    ///
    /// Returns `None` when we cannot safely decide (e.g. complex key space),
    /// allowing the caller to fall back to full mapped expansion.
    fn resolve_mapped_property_with_env(
        &mut self,
        mapped_type: TypeId,
        prop_name: &str,
    ) -> Option<tsz_solver::operations::property::PropertyAccessResult> {
        let mapped_id = tsz_solver::mapped_type_id(self.ctx.types, mapped_type)?;
        let mapped = self.ctx.types.mapped_type(mapped_id);

        let prop_atom = self.ctx.types.intern_string(prop_name);
        let cache_key = (mapped_type, prop_atom);

        if let Some(cached) = self
            .ctx
            .narrowing_cache
            .property_cache
            .borrow()
            .get(&cache_key)
            .copied()
        {
            return Some(match cached {
                Some(type_id) => tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id,
                    write_type: None,
                    from_index_signature: false,
                },
                None => tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                    type_id: mapped_type,
                    property_name: prop_atom,
                },
            });
        }

        let constraint = self.evaluate_mapped_constraint_with_resolution(mapped.constraint);
        if let Some(property_type) =
            crate::query_boundaries::state::checking::get_finite_mapped_property_type(
                self.ctx.types,
                mapped_id,
                prop_name,
            )
        {
            self.ctx
                .narrowing_cache
                .property_cache
                .borrow_mut()
                .insert(cache_key, Some(property_type));
            return Some(
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id: property_type,
                    write_type: None,
                    from_index_signature: false,
                },
            );
        }

        if let Some(names) =
            crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
                self.ctx.types,
                mapped_id,
            )
        {
            if !names.contains(&prop_atom) {
                self.ctx
                    .narrowing_cache
                    .property_cache
                    .borrow_mut()
                    .insert(cache_key, None);
            }
            if !names.contains(&prop_atom) {
                return Some(
                    tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                        type_id: mapped_type,
                        property_name: prop_atom,
                    },
                );
            }
        }

        if mapped.name_type.is_some() {
            return None;
        }

        let mut matching_source_keys = Vec::new();

        // If the constraint is an explicit literal key set, reject unknown keys early.
        // For non-literal/complex constraints, fall back to full expansion.
        if !query::is_string_type(self.ctx.types, constraint) {
            let keys = query::extract_string_literal_keys(self.ctx.types, constraint);
            if !keys.is_empty() && keys.contains(&prop_atom) {
                matching_source_keys.push(prop_atom);
            }
            if !keys.is_empty() && matching_source_keys.is_empty() {
                self.ctx
                    .narrowing_cache
                    .property_cache
                    .borrow_mut()
                    .insert(cache_key, None);
                return Some(
                    tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                        type_id: mapped_type,
                        property_name: prop_atom,
                    },
                );
            }
            if keys.is_empty() {
                if let Some(keyof_target) = query::keyof_target(self.ctx.types, mapped.constraint)
                    .or_else(|| query::keyof_target(self.ctx.types, constraint))
                {
                    if matches!(
                        self.resolve_property_access_with_env(keyof_target, prop_name),
                        tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                    ) {
                        // `keyof T`-driven mapped types like Readonly<T> preserve
                        // the property surface of T, even when the key set isn't
                        // reducible to string literals. Keep going and instantiate
                        // the template for the requested property.
                    } else {
                        self.ctx
                            .narrowing_cache
                            .property_cache
                            .borrow_mut()
                            .insert(cache_key, None);
                        return Some(
                            tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                                type_id: mapped_type,
                                property_name: prop_atom,
                            },
                        );
                    }
                } else {
                    self.ctx
                        .narrowing_cache
                        .property_cache
                        .borrow_mut()
                        .insert(cache_key, None);
                    return Some(
                        tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                            type_id: mapped_type,
                            property_name: prop_atom,
                        },
                    );
                }
            }
        }

        if matching_source_keys.is_empty() {
            matching_source_keys.push(prop_atom);
        }

        let mut property_types = Vec::new();
        for source_key_atom in matching_source_keys {
            let key_literal = self.ctx.types.literal_string_atom(source_key_atom);
            let instantiated =
                crate::query_boundaries::state::checking::instantiate_mapped_template_for_property(
                    self.ctx.types,
                    mapped.template,
                    mapped.type_param.name,
                    key_literal,
                );
            let property_type = self.evaluate_type_with_env(instantiated);
            let property_type = match mapped.optional_modifier {
                Some(tsz_solver::MappedModifier::Add) => self
                    .ctx
                    .types
                    .factory()
                    .union(vec![property_type, TypeId::UNDEFINED]),
                Some(tsz_solver::MappedModifier::Remove) | None => property_type,
            };
            property_types.push(property_type);
        }

        let property_type = match property_types.len() {
            0 => return None,
            1 => property_types[0],
            _ => self.ctx.types.factory().union(property_types),
        };

        self.ctx
            .narrowing_cache
            .property_cache
            .borrow_mut()
            .insert(cache_key, Some(property_type));

        Some(
            tsz_solver::operations::property::PropertyAccessResult::Success {
                type_id: property_type,
                write_type: None,
                from_index_signature: false,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::check_source_diagnostics;
    use crate::{
        context::CheckerOptions,
        query_boundaries::type_construction::TypeInterner,
        state::CheckerState,
    };
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

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

    /// Mapped type template with name collision: `MyReadonly`<P> where P is a
    /// user type parameter with the same name as the mapped key param.
    /// Name-based substitution must be bypassed to avoid incorrectly
    /// replacing the outer P with the key literal.
    #[test]
    fn mapped_type_name_collision_readonly_of_type_param() {
        let diags = check_source_diagnostics(
            "interface Foo { foo(): void }
type MyPartial<T> = { [P in keyof T]?: T[P] };
type MyReadonly<T> = { readonly [P in keyof T]: T[P] };
class A<P extends MyPartial<Foo>> {
    constructor(public props: MyReadonly<P>) {}
    doSomething() {
        this.props.foo && this.props.foo()
    }
}",
        );
        let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
        assert!(
            relevant.is_empty(),
            "expected zero errors for MyReadonly<P> property access with && guard, got: {:?}",
            relevant
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }

    /// Property access on a type parameter with a mapped-type constraint
    /// should resolve through the constraint.
    #[test]
    fn type_param_property_access_with_mapped_constraint() {
        let diags = check_source_diagnostics(
            "interface Foo { foo(): void }
type MyPartial<T> = { [P in keyof T]?: T[P] };
function f<P extends MyPartial<Foo>>(p: P) {
    p.foo;
}",
        );
        let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
        assert!(
            relevant.is_empty(),
            "expected zero errors for type param property access via constraint, got: {:?}",
            relevant
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }

    fn build_checker(source: &str) -> (ParserState, tsz_parser::parser::NodeIndex, BinderState, TypeInterner) {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        (parser, root, binder, types)
    }

    #[test]
    fn mapped_enum_discriminant_application_exposes_member_property() {
        let source = r#"
enum ABC { A, B }

type Gen<T extends ABC> = { v: T } & (
  { v: ABC.A, a: string } |
  { v: ABC.B, b: string }
);

type Gen2<T extends ABC> = {
  [Property in keyof Gen<T>]: string;
};

type ProbeGen = Gen<ABC.A>;
type Probe = Gen2<ABC.A>;
"#;

        let (parser, root, binder, types) = build_checker(source);
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        checker.ctx.set_lib_contexts(Vec::new());
        checker.check_source_file(root);

        let probe_sym = checker
            .ctx
            .binder
            .file_locals
            .get("Probe")
            .expect("Probe symbol");
        let probe_gen_sym = checker
            .ctx
            .binder
            .file_locals
            .get("ProbeGen")
            .expect("ProbeGen symbol");
        let probe_gen_type = checker.type_reference_symbol_type(probe_gen_sym);
        let probe_type = checker.type_reference_symbol_type(probe_sym);
        let gen_a_result = checker.resolve_property_access_with_env(probe_gen_type, "a");
        let a_result = checker.resolve_property_access_with_env(probe_type, "a");

        assert!(
            matches!(
                gen_a_result,
                tsz_solver::operations::property::PropertyAccessResult::Success { .. }
            ),
            "expected ProbeGen.a to resolve, got {gen_a_result:?} for type {:?}",
            checker.ctx.types.lookup(probe_gen_type),
        );

        assert!(
            matches!(
                a_result,
                tsz_solver::operations::property::PropertyAccessResult::Success { .. }
            ),
            "expected Probe.a to resolve, got {a_result:?} for type {:?}",
            checker.ctx.types.lookup(probe_type),
        );
    }
}
