use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use std::collections::HashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn target_index_signature_accepts_source_property_with_env(
        &self,
        target: TypeId,
        source_prop: &tsz_solver::PropertyInfo,
    ) -> bool {
        if crate::query_boundaries::assignability::target_index_signature_accepts_source_property(
            self.ctx.types,
            target,
            source_prop,
        ) {
            return true;
        }
        if !source_prop.is_symbol_named {
            return false;
        }

        if let Some(shape) = crate::query_boundaries::common::get_merged_object_shape_for_type(
            self.ctx.types,
            target,
        ) {
            return shape.string_index.as_ref().is_some_and(|idx| {
                let key_type = self.resolve_index_signature_key_type_via_env(idx.key_type);
                key_type != idx.key_type
                    && crate::query_boundaries::assignability::index_signature_key_type_accepts_symbol(
                        self.ctx.types,
                        key_type,
                    )
            });
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, target)
        {
            return members.iter().any(|&member| {
                self.target_index_signature_accepts_source_property_with_env(member, source_prop)
            });
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, target)
        {
            return members.iter().any(|&member| {
                self.target_index_signature_accepts_source_property_with_env(member, source_prop)
            });
        }

        false
    }

    fn resolve_index_signature_key_type_via_env(&self, key_type: TypeId) -> TypeId {
        let mut current = key_type;
        for _ in 0..8 {
            let Some(def_id) =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, current)
            else {
                break;
            };
            let resolved = self.ctx.type_env.borrow().get_def(def_id);
            match resolved {
                Some(next) if next != current => current = next,
                _ => break,
            }
        }
        current
    }

    fn report_excess_function_like_implicit_any(&mut self, func_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            && node.kind != syntax_kind_ext::ARROW_FUNCTION
            && node.kind != syntax_kind_ext::METHOD_DECLARATION
            && node.kind != syntax_kind_ext::GET_ACCESSOR
            && node.kind != syntax_kind_ext::SET_ACCESSOR
        {
            return false;
        }
        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };
        for &param_idx in &func.parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            if param.type_annotation.is_some() {
                continue;
            }
            let param_name = self.parameter_name_for_error(param.name);
            if param_name.is_empty() {
                continue;
            }
            let Some((start, end)) = self.get_node_span(param.name) else {
                continue;
            };
            let template = tsz_common::diagnostics::get_message_template(
                crate::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
            )
            .unwrap_or("");
            let message = crate::diagnostics::format_message(template, &[&param_name, "any"]);
            self.ctx
                .deferred_excess_property_implicit_any_diagnostics
                .push(tsz_common::diagnostics::Diagnostic::error(
                    self.ctx.file_name.clone(),
                    start,
                    end.saturating_sub(start),
                    message,
                    crate::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
                ));
        }
        if func.body.is_some() {
            self.check_for_nested_function_ts7006(func.body);
        }
        true
    }

    fn check_excess_property_initializer_implicit_any(
        &mut self,
        elem_idx: NodeIndex,
        _target: TypeId,
    ) {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return;
        };

        match elem_node.kind {
            syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                    self.invalidate_initializer_for_context_change(prop.initializer);
                    if !self.report_excess_function_like_implicit_any(prop.initializer) {
                        self.check_for_nested_function_ts7006(prop.initializer);
                    }
                }
            }
            syntax_kind_ext::METHOD_DECLARATION
                if self.ctx.arena.get_method_decl(elem_node).is_some() =>
            {
                self.invalidate_function_like_for_contextual_retry(elem_idx);
                self.report_excess_function_like_implicit_any(elem_idx);
            }
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR
                if self.ctx.arena.get_accessor(elem_node).is_some() =>
            {
                self.invalidate_function_like_for_contextual_retry(elem_idx);
                self.report_excess_function_like_implicit_any(elem_idx);
            }
            _ => {}
        }
    }

    fn track_earliest_excess(
        &self,
        current: &mut Option<(Atom, NodeIndex, u32)>,
        name: Atom,
        report_idx: NodeIndex,
    ) {
        let pos = self.ctx.arena.get(report_idx).map_or(u32::MAX, |n| n.pos);
        if current.is_none_or(|(_, _, best)| pos < best) {
            *current = Some((name, report_idx, pos));
        }
    }

    fn emit_tracked_excess_property(
        &mut self,
        tracked: Option<(Atom, NodeIndex, u32)>,
        target: TypeId,
    ) {
        if let Some((prop_atom, report_idx, _)) = tracked {
            let prop_name = self.object_literal_property_display_name(
                report_idx,
                self.ctx.types.resolve_atom(prop_atom).as_ref(),
            );
            self.error_excess_property_at(&prop_name, target, report_idx);
            self.check_excess_property_initializer_implicit_any(report_idx, target);
        }
    }

    fn track_earliest_excess_with_target(
        &self,
        current: &mut Option<(Atom, NodeIndex, u32, TypeId)>,
        name: Atom,
        report_idx: NodeIndex,
        target: TypeId,
    ) {
        let pos = self.ctx.arena.get(report_idx).map_or(u32::MAX, |n| n.pos);
        if current.is_none_or(|(_, _, best, _)| pos < best) {
            *current = Some((name, report_idx, pos, target));
        }
    }

    fn emit_tracked_excess_property_with_target(
        &mut self,
        tracked: Option<(Atom, NodeIndex, u32, TypeId)>,
    ) {
        if let Some((prop_atom, report_idx, _, target)) = tracked {
            let prop_name = self.object_literal_property_display_name(
                report_idx,
                self.ctx.types.resolve_atom(prop_atom).as_ref(),
            );
            self.error_excess_property_at(&prop_name, target, report_idx);
            self.check_excess_property_initializer_implicit_any(report_idx, target);
        }
    }

    fn string_index_key_accepts_property_name(
        &mut self,
        key_type: TypeId,
        prop_name: &str,
        is_symbol_named: bool,
    ) -> bool {
        if key_type == TypeId::SYMBOL {
            return is_symbol_named
                || prop_name.starts_with("[Symbol.")
                || prop_name.starts_with("__unique_")
                || prop_name.starts_with("__@");
        }

        if key_type == TypeId::STRING {
            return true;
        }

        if is_symbol_named {
            return false;
        }

        let prop_literal =
            crate::query_boundaries::common::create_string_literal_type(self.ctx.types, prop_name);
        self.is_assignable_to(prop_literal, key_type)
    }

    fn union_member_has_type_parameter_for_excess_display(&self, member: TypeId) -> bool {
        query::is_type_parameter_like(self.ctx.types, member)
            || crate::query_boundaries::common::contains_generic_type_parameters(
                self.ctx.types,
                member,
            )
    }

    pub(crate) fn check_object_literal_excess_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        use crate::query_boundaries::common as freshness_query;

        self.ensure_relation_input_ready(target);

        let const_assertion_object_literal = self.const_assertion_object_literal_expression(idx);
        let object_literal_idx = const_assertion_object_literal.unwrap_or(idx);
        let evaluated_target = self.evaluate_type_with_env(target);

        if crate::query_boundaries::common::type_is_conditional_type_result_with_unresolved_inference(
            self.ctx.types,
            target,
        ) || crate::query_boundaries::common::type_is_conditional_type_result_with_unresolved_inference(
            self.ctx.types,
            evaluated_target,
        ) {
            return;
        }

        // Run named-property value checks before excess-property reporting. When
        // a known property is already invalid, tsc reports that assignability
        // error instead of additionally reporting an excess property from the
        // same object literal.
        let emitted_named_property_value_error = self
            .check_object_literal_named_property_values_against_target(object_literal_idx, target);
        if emitted_named_property_value_error {
            return;
        }

        // Excess property checks do not apply to type parameters (even with constraints).
        if query::is_type_parameter_like(self.ctx.types, target) {
            return;
        }

        // Only check excess properties for FRESH object literals
        let is_fresh_source = freshness_query::is_fresh_object_type(self.ctx.types, source);
        let explicit_property_names = if is_fresh_source {
            None
        } else if const_assertion_object_literal.is_some() {
            self.explicit_object_literal_property_names(object_literal_idx)
        } else {
            self.explicit_object_literal_property_names_for_spread(object_literal_idx)
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
        let union_check_target = self.excess_property_union_target(target, resolved_target);

        if [target, effective_target, resolved_target, evaluated_target]
            .into_iter()
            .filter_map(|candidate| {
                crate::query_boundaries::common::mapped_type_info(self.ctx.types, candidate)
            })
            .any(|mapped| {
                !crate::query_boundaries::common::is_valid_mapped_type_key_type(
                    self.ctx.types,
                    mapped.constraint,
                )
            })
        {
            return;
        }

        let mut generic_mapped_excess: Option<(tsz_common::interner::Atom, NodeIndex, u32)> = None;
        for source_prop in source_props {
            if explicit_property_names.is_some()
                && !explicit_property_names
                    .as_ref()
                    .is_some_and(|names| names.contains(&source_prop.name))
            {
                continue;
            }

            let prop_name = self.ctx.types.resolve_atom(source_prop.name);
            // Only mark a property as excess when EVERY normalized view of the
            // target reports it as missing. The original `target` may still be
            // an alias-displayed Application/Mapped whose constraint isn't
            // fully evaluated (e.g. `Omit<P, "x">`'s body uses `Exclude<keyof P,
            // "x">` which `extract_string_literal_keys` can't reduce to literals
            // without full evaluation). When `effective_target` or
            // `resolved_target` has been reduced to a concrete object whose
            // shape contains the property, we must trust those verdicts —
            // otherwise we emit TS2353 false positives for valid Pick/Omit
            // assignments like `const p: Omit<Person, "email"> = { name, age }`.
            // Structural rule: a generic mapped receiver "lacks property X"
            // only when no normalized form can locate X.
            if [target, effective_target, resolved_target]
                .into_iter()
                .all(|candidate| {
                    self.generic_mapped_receiver_lacks_explicit_property(
                        candidate,
                        prop_name.as_ref(),
                    )
                })
            {
                let report_idx = self
                    .find_object_literal_property_element(object_literal_idx, source_prop.name)
                    .unwrap_or(object_literal_idx);
                self.track_earliest_excess(
                    &mut generic_mapped_excess,
                    source_prop.name,
                    report_idx,
                );
            }
        }
        if generic_mapped_excess.is_some() {
            self.emit_tracked_excess_property(generic_mapped_excess, target);
            return;
        }

        if let Some(members) = query::intersection_members(self.ctx.types, union_check_target) {
            let mut first_excess: Option<(Atom, NodeIndex, u32)> = None;
            for source_prop in source_props {
                if explicit_property_names.is_some()
                    && !explicit_property_names
                        .as_ref()
                        .is_some_and(|names| names.contains(&source_prop.name))
                {
                    continue;
                }

                let prop_name = self.ctx.types.resolve_atom(source_prop.name);
                let mut accepted = false;
                let mut checked_any_member = false;
                let mut uncertain = false;

                for &member in &members {
                    let resolved_member = self.resolve_type_for_property_access(member);
                    if self.target_index_signature_accepts_source_property_with_env(
                        resolved_member,
                        source_prop,
                    ) {
                        accepted = true;
                        break;
                    }
                    if self
                        .generic_mapped_receiver_lacks_explicit_property(member, prop_name.as_ref())
                        || self.generic_mapped_receiver_lacks_explicit_property(
                            resolved_member,
                            prop_name.as_ref(),
                        )
                    {
                        checked_any_member = true;
                        continue;
                    }

                    match self.resolve_property_access_with_env(resolved_member, prop_name.as_ref())
                    {
                        tsz_solver::operations::property::PropertyAccessResult::Success {
                            type_id,
                            ..
                        }
                        | tsz_solver::operations::property::PropertyAccessResult::PossiblyNullOrUndefined {
                            property_type: Some(type_id),
                            ..
                        } if type_id != TypeId::ERROR => {
                            accepted = true;
                            break;
                        }
                        tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                            ..
                        } => {
                            checked_any_member = true;
                        }
                        _ => {
                            if let Some(shape) = query::object_shape(self.ctx.types, resolved_member)
                            {
                                if shape.string_index.is_some() {
                                    accepted = true;
                                    break;
                                }
                                checked_any_member = true;
                            } else if resolved_member == TypeId::OBJECT
                                || query::is_type_parameter_like(self.ctx.types, resolved_member)
                            {
                                uncertain = true;
                            } else {
                                checked_any_member = true;
                            }
                        }
                    }
                }

                if !accepted && checked_any_member && !uncertain {
                    let report_idx = self
                        .find_object_literal_property_element(object_literal_idx, source_prop.name)
                        .unwrap_or(object_literal_idx);
                    self.track_earliest_excess(&mut first_excess, source_prop.name, report_idx);
                }
            }
            if first_excess.is_some() {
                self.emit_tracked_excess_property(first_excess, target);
                return;
            }
        }

        // Handle union targets first using type_queries. For named type aliases,
        // EPC needs the alias body before property-access resolution collapses
        // redundant union members (for example `Common | Common & A`).
        if let Some(members) = query::union_members(self.ctx.types, union_check_target) {
            let mut target_members = Vec::new();
            let mut any_member_has_string_index = false;
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
                    // Unresolved generic union arms can collapse to `any` during
                    // property-access resolution; keep checking concrete arms, but
                    // let the diagnostic display use the concrete arm if it is the
                    // only EPC-relevant member.
                    if resolved_member == TypeId::ANY
                        && !crate::query_boundaries::assignability::contains_any_type(
                            self.ctx.types,
                            target,
                        )
                    {
                        has_unresolved_member = true;
                        continue;
                    }
                    // TypeScript still applies excess property checking to the
                    // concrete members of unions like `T | { prop: boolean }`.
                    if self.union_member_has_type_parameter_for_excess_display(resolved_member) {
                        has_unresolved_member = true;
                        continue;
                    }
                    continue;
                };

                if let Some(string_index) = &shape.string_index {
                    any_member_has_string_index = true;
                    if Self::index_value_type_is_deferred(self.ctx.types, string_index.value_type)
                        || crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            resolved_target,
                        )
                    {
                        return;
                    }
                }

                // Empty types (like `{}`) accept any non-primitive,
                // so skip excess property checking entirely.
                if shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
                {
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

                target_members.push((resolved_member, shape.clone()));
            }

            if target_members.is_empty() {
                return;
            }

            let target_shapes = target_members
                .iter()
                .map(|(_, shape)| shape.clone())
                .collect::<Vec<_>>();

            if self.try_union_index_signature_value_check(
                source_props,
                idx,
                &target_shapes,
                explicit_property_names.as_ref(),
            ) {
                return;
            }

            // String index signatures accept arbitrary string-keyed property
            // names, so fallback TS2353 excess-property checking can be skipped
            // once index-signature value compatibility has had a chance to run.
            if any_member_has_string_index {
                return;
            }

            if self.try_discriminated_union_excess_check(source, union_check_target, idx) {
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
                .map(|i| target_members[i].clone())
                .collect::<Vec<_>>();
            let had_discriminant_narrowing = !discriminant_shapes.is_empty();
            let effective_members = if !had_discriminant_narrowing {
                if has_unresolved_member {
                    let matching_members = target_members
                        .iter()
                        .filter(|(_, shape)| {
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
                    if matching_members.is_empty() {
                        return;
                    }
                    matching_members
                } else {
                    target_members
                }
            } else {
                discriminant_shapes
            };

            let effective_shapes = effective_members
                .iter()
                .map(|(_, shape)| shape.clone())
                .collect::<Vec<_>>();

            // First excess by source order (see `track_earliest_excess`).
            let mut first_excess: Option<(Atom, NodeIndex, u32, TypeId)> = None;
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
                        .find_object_literal_property_element(object_literal_idx, source_prop.name)
                        .unwrap_or(object_literal_idx);
                    let concrete_diagnostic_members = effective_members
                        .iter()
                        .filter(|(member, _)| {
                            !self.union_member_has_type_parameter_for_excess_display(*member)
                        })
                        .collect::<Vec<_>>();
                    let diagnostic_target = if concrete_diagnostic_members.len() == 1
                        && (has_unresolved_member
                            || concrete_diagnostic_members.len() != effective_members.len()
                            || crate::query_boundaries::common::contains_generic_type_parameters(
                                self.ctx.types,
                                target,
                            )) {
                        concrete_diagnostic_members[0].0
                    } else {
                        target
                    };
                    self.track_earliest_excess_with_target(
                        &mut first_excess,
                        source_prop.name,
                        report_idx,
                        diagnostic_target,
                    );
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
                    let nested_target = if had_discriminant_narrowing {
                        nested_target
                    } else {
                        self.nested_property_target_type(
                            effective_target,
                            source_prop.name,
                            nested_target,
                        )
                    };

                    if had_discriminant_narrowing
                        && self.try_emit_nested_discriminated_union_assignability_error(
                            source,
                            target,
                            idx,
                            source_prop.name,
                            nested_target,
                        )
                    {
                        return;
                    }

                    if self.check_nested_object_literal_excess_properties(
                        source_prop.name,
                        Some(nested_target),
                        idx,
                    ) {
                        return;
                    }
                }
            }
            self.emit_tracked_excess_property_with_target(first_excess);
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
                if crate::query_boundaries::common::is_primitive_type(
                    self.ctx.types,
                    resolved_member,
                ) {
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

            // First excess by source order (see `track_earliest_excess`).
            let mut first_excess: Option<(Atom, NodeIndex, u32)> = None;
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
                        .find_object_literal_property_element(object_literal_idx, source_prop.name)
                        .unwrap_or(object_literal_idx);
                    self.track_earliest_excess(&mut first_excess, source_prop.name, report_idx);
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
                        if self.check_nested_object_literal_excess_properties(
                            source_prop.name,
                            Some(nested_target),
                            idx,
                        ) {
                            return;
                        }
                    }
                }
            }
            self.emit_tracked_excess_property(first_excess, target);
            return;
        }

        if crate::query_boundaries::common::is_mapped_type(self.ctx.types, effective_target) {
            if let Some(mapped) =
                crate::query_boundaries::common::mapped_type_info(self.ctx.types, effective_target)
                && !crate::query_boundaries::common::is_valid_mapped_type_key_type(
                    self.ctx.types,
                    mapped.constraint,
                )
            {
                return;
            }

            // First excess by source order (see `track_earliest_excess`).
            let mut first_excess: Option<(Atom, NodeIndex, u32)> = None;
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
                        if self.check_object_literal_named_property_value(
                            idx,
                            source_prop.name,
                            source_prop.type_id,
                            effective_target,
                            type_id,
                        ) {
                            return;
                        }
                        let nested_target = self.nested_property_target_type(
                            effective_target,
                            source_prop.name,
                            type_id,
                        );
                        if self.check_nested_object_literal_excess_properties(
                            source_prop.name,
                            Some(nested_target),
                            idx,
                        ) {
                            return;
                        }
                    }
                    tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                        ..
                    } => {
                        let report_idx = self
                            .find_object_literal_property_element(
                                object_literal_idx,
                                source_prop.name,
                            )
                            .unwrap_or(object_literal_idx);
                        self.track_earliest_excess(&mut first_excess, source_prop.name, report_idx);
                    }
                    _ => return,
                }
            }
            self.emit_tracked_excess_property(first_excess, target);
            return;
        }

        // Handle simple object targets via the canonical boundary classification.
        //
        // The boundary's `classify_object_properties` determines which source
        // properties are excess (WHAT), while this checker code handles WHERE to
        // anchor diagnostics and recursive nested-literal checking.
        if let Some(target_shape) = query::object_shape(self.ctx.types, resolved_target) {
            let target_props = target_shape.properties.as_slice();
            let should_check_named_values = [target, effective_target, resolved_target]
                .into_iter()
                .any(|candidate| {
                    self.target_is_mapped_or_mapped_application(candidate)
                        || self.target_is_or_displays_type_application(candidate)
                });

            // When the target has a string index signature, outer property names are
            // valid only when they are accepted by the index key type. Broad
            // `[k: string]` accepts every string key, while template-literal
            // index keys such as `` `data-${string}` `` only accept matching
            // property names. We still check nested object literals against the
            // applicable index signature VALUE type for excess properties.
            if let Some(ref idx_sig) = target_shape.string_index {
                if self.try_union_index_signature_value_check(
                    source_props,
                    idx,
                    std::slice::from_ref(&target_shape),
                    explicit_property_names.as_ref(),
                ) {
                    return;
                }

                let idx_value_type = idx_sig.value_type;
                let idx_key_type = idx_sig.key_type;
                let mut first_excess: Option<(Atom, NodeIndex, u32)> = None;
                for source_prop in source_props {
                    if explicit_property_names.is_some()
                        && !explicit_property_names
                            .as_ref()
                            .is_some_and(|names| names.contains(&source_prop.name))
                    {
                        continue;
                    }

                    let prop_name = self.ctx.types.resolve_atom(source_prop.name);
                    let matches_index = self.string_index_key_accepts_property_name(
                        idx_key_type,
                        prop_name.as_ref(),
                        source_prop.is_symbol_named,
                    );
                    let target_prop = target_props.iter().find(|p| p.name == source_prop.name);

                    if !matches_index && target_prop.is_none() {
                        let report_idx = self
                            .find_object_literal_property_element(
                                object_literal_idx,
                                source_prop.name,
                            )
                            .unwrap_or(object_literal_idx);
                        self.track_earliest_excess(&mut first_excess, source_prop.name, report_idx);
                        continue;
                    }

                    let mut nested_types = Vec::new();
                    if matches_index {
                        nested_types.push(idx_value_type);
                    }
                    if let Some(target_prop) = target_prop {
                        if self.check_object_literal_named_property_value(
                            idx,
                            source_prop.name,
                            source_prop.type_id,
                            effective_target,
                            target_prop.type_id,
                        ) {
                            return;
                        }
                        nested_types.push(target_prop.type_id);
                    }
                    if nested_types.is_empty() {
                        continue;
                    }
                    let nested_target =
                        tsz_solver::utils::intersection_or_single(self.ctx.types, nested_types);
                    let nested_target = self.nested_property_target_type(
                        effective_target,
                        source_prop.name,
                        nested_target,
                    );
                    if self.check_nested_object_literal_excess_properties(
                        source_prop.name,
                        Some(nested_target),
                        idx,
                    ) {
                        return;
                    }
                }
                self.emit_tracked_excess_property(first_excess, target);
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
                    || cls.target_is_global_object_or_function
                    || cls.target_is_type_parameter
                {
                    return;
                }
                if cls.target_has_index_signature && cls.excess_properties.is_empty() {
                    return;
                }
            }

            // First excess by source order (see `track_earliest_excess`).
            let mut first_excess: Option<(Atom, NodeIndex, u32)> = None;
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
                if boundary_marks_excess
                    && self.target_index_signature_accepts_source_property_with_env(
                        resolved_target,
                        source_prop,
                    )
                {
                    continue;
                }
                let boundary_excess_is_authoritative = classification
                    .as_ref()
                    .is_some_and(|cls| cls.trimmed_source_assignable);
                let is_excess = boundary_marks_excess
                    && (boundary_excess_is_authoritative || dynamic_target_prop_type.is_none());
                if is_excess {
                    let report_idx = self
                        .find_object_literal_property_element(object_literal_idx, source_prop.name)
                        .unwrap_or(object_literal_idx);
                    self.track_earliest_excess(&mut first_excess, source_prop.name, report_idx);
                } else {
                    // Property exists in target — check nested object literals.
                    let target_prop_type = target_props
                        .iter()
                        .find(|p| p.name == source_prop.name)
                        .map(|prop| prop.type_id)
                        .or(dynamic_target_prop_type);
                    if let Some(target_prop_type) = target_prop_type {
                        if should_check_named_values
                            && self.check_object_literal_named_property_value(
                                idx,
                                source_prop.name,
                                source_prop.type_id,
                                effective_target,
                                target_prop_type,
                            )
                        {
                            return;
                        }
                        let nested_target = self.nested_property_target_type(
                            effective_target,
                            source_prop.name,
                            target_prop_type,
                        );
                        if self.check_nested_object_literal_excess_properties(
                            source_prop.name,
                            Some(nested_target),
                            idx,
                        ) {
                            return;
                        }
                    }
                }
            }
            if let Some((prop_name_atom, report_idx, _)) = first_excess {
                let prop_name = self.object_literal_property_display_name(
                    report_idx,
                    self.ctx.types.resolve_atom(prop_name_atom).as_ref(),
                );
                self.error_excess_property_at(&prop_name, target, report_idx);
                self.check_excess_property_initializer_implicit_any(report_idx, target);
            }
        }
        // Note: Missing property checks are handled by solver's explain_failure
    }

    fn excess_property_union_target(&self, target: TypeId, resolved_target: TypeId) -> TypeId {
        if query::union_members(self.ctx.types, resolved_target).is_some() {
            return resolved_target;
        }

        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, target)
        else {
            return resolved_target;
        };
        let Some(def) = self.ctx.definition_store.get(def_id) else {
            return resolved_target;
        };
        if def.kind != tsz_solver::def::DefKind::TypeAlias || !def.type_params.is_empty() {
            return resolved_target;
        }
        let Some(body) = def.body else {
            return resolved_target;
        };
        if query::union_members(self.ctx.types, body).is_some() {
            body
        } else {
            resolved_target
        }
    }

    pub(crate) fn excess_property_target_from_type_annotation(
        &mut self,
        type_node: NodeIndex,
    ) -> Option<TypeId> {
        let mut visited = HashSet::new();
        self.excess_property_annotation_union_type(type_node, &mut visited)
            .filter(|&ty| query::union_members(self.ctx.types, ty).is_some())
    }

    fn excess_property_annotation_union_type(
        &mut self,
        type_node: NodeIndex,
        visited: &mut HashSet<NodeIndex>,
    ) -> Option<TypeId> {
        if !visited.insert(type_node) {
            return None;
        }

        let node = self.ctx.arena.get(type_node)?;
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            let wrapped = self.ctx.arena.get_wrapped_type(node)?;
            return self.excess_property_annotation_union_type(wrapped.type_node, visited);
        }

        if node.kind == syntax_kind_ext::UNION_TYPE {
            let composite = self.ctx.arena.get_composite_type(node)?;
            let contains_intersection_member = composite
                .types
                .nodes
                .iter()
                .any(|&member| self.annotation_node_contains_intersection(member));
            if !contains_intersection_member {
                return None;
            }
            let member_types = composite
                .types
                .nodes
                .iter()
                .map(|&member| self.excess_property_annotation_component_type(member, visited))
                .collect::<Vec<_>>();
            return Some(tsz_solver::utils::union_or_single_literal_reduce(
                self.ctx.types,
                member_types,
            ));
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let alias_body = self.local_non_generic_type_alias_body_for_reference(type_node)?;
            return self.excess_property_annotation_union_type(alias_body, visited);
        }

        None
    }

    fn excess_property_annotation_component_type(
        &mut self,
        type_node: NodeIndex,
        visited: &mut HashSet<NodeIndex>,
    ) -> TypeId {
        let Some(node) = self.ctx.arena.get(type_node) else {
            return TypeId::ERROR;
        };

        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.excess_property_annotation_component_type(wrapped.type_node, visited);
        }

        if node.kind == syntax_kind_ext::INTERSECTION_TYPE
            && let Some(composite) = self.ctx.arena.get_composite_type(node)
        {
            let member_types = composite
                .types
                .nodes
                .iter()
                .map(|&member| self.excess_property_annotation_component_type(member, visited))
                .collect::<Vec<_>>();
            return self.raw_intersection_or_single(member_types);
        }

        if let Some(union_type) = self.excess_property_annotation_union_type(type_node, visited) {
            return union_type;
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(lazy_type) = self.named_type_reference_lazy_type(type_node)
        {
            return lazy_type;
        }

        self.get_type_from_type_node(type_node)
    }

    fn annotation_node_contains_intersection(&self, type_node: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_node) else {
            return false;
        };
        if node.kind == syntax_kind_ext::INTERSECTION_TYPE {
            return true;
        }
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.annotation_node_contains_intersection(wrapped.type_node);
        }
        false
    }

    fn raw_intersection_or_single(&self, members: Vec<TypeId>) -> TypeId {
        let mut iter = members.into_iter();
        let Some(mut result) = iter.next() else {
            return TypeId::UNKNOWN;
        };
        for member in iter {
            result = self.ctx.types.intersect_types_raw2(result, member);
        }
        result
    }

    fn named_type_reference_lazy_type(&mut self, type_node: NodeIndex) -> Option<TypeId> {
        let node = self.ctx.arena.get(type_node)?;
        let type_ref = self.ctx.arena.get_type_ref(node)?;
        if type_ref.type_arguments.is_some() {
            return None;
        }
        let TypeSymbolResolution::Type(sym_id) =
            self.resolve_identifier_symbol_in_type_position_without_tracking(type_ref.type_name)
        else {
            return None;
        };
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        let _ = self.get_type_from_type_node(type_node);
        Some(self.ctx.types.lazy(def_id))
    }

    fn local_non_generic_type_alias_body_for_reference(
        &self,
        type_node: NodeIndex,
    ) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(type_node)?;
        let type_ref = self.ctx.arena.get_type_ref(node)?;
        if type_ref.type_arguments.is_some() {
            return None;
        }
        let TypeSymbolResolution::Type(sym_id) =
            self.resolve_identifier_symbol_in_type_position_without_tracking(type_ref.type_name)
        else {
            return None;
        };
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
            return None;
        }
        symbol.declarations.iter().copied().find_map(|decl_idx| {
            let decl_node = self.ctx.arena.get(decl_idx)?;
            if decl_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                return None;
            }
            let type_alias = self.ctx.arena.get_type_alias(decl_node)?;
            if type_alias.type_parameters.is_some() {
                return None;
            }
            Some(type_alias.type_node)
        })
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
        let resolved_target = self.excess_property_union_target(target, resolved_target);
        let Some(members) = query::union_members(self.ctx.types, resolved_target) else {
            return false;
        };

        // Collect resolved shapes for each union member, along with the original
        // TypeId (for error message formatting) which preserves type alias names.
        //
        // IMPORTANT: Use `member` directly as display_id, not `original_members[i]`.
        // Resolution can change the order of union members (e.g., during normalization),
        // so index-based lookup into original_members would cause misalignment between
        // display names and shapes. Using `member` preserves the Lazy reference if the
        // union member is a type alias, giving proper display names like "Int" vs "{ type: ... }".
        //
        // When a union member is an intersection of object types
        // (e.g. `Common & A` or `BaseAttribute<string> & { type: 'string' }`),
        // `query::object_shape` returns None because intersections are not
        // backed by a single shape. Fall back to a solver-flattened apparent
        // shape so the discriminant narrowing and excess-property check can
        // see the merged property set, matching tsc's apparent-type behavior.
        let mut member_shapes: Vec<(TypeId, std::sync::Arc<tsz_solver::ObjectShape>)> = Vec::new();
        for &member in members.iter() {
            let resolved = self.resolve_type_for_property_access(member);
            if let Some(shape) = query::object_shape(self.ctx.types, resolved) {
                // Use member directly - it preserves Lazy wrappers for named types
                member_shapes.push((member, shape));
            } else if let Some(shape) = self.apparent_intersection_object_shape(resolved) {
                member_shapes.push((member, shape));
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

        if matching_indices.is_empty() {
            return false;
        }

        // Display the union of *all* members the discriminator narrowed to (tsc
        // shows e.g. `'StringAttribute | OneToOneAttribute'`, not just the first
        // matching member). The excess-property existence check still considers
        // every narrowed member so we don't false-positive on properties that
        // belong to one of the matches.
        let narrowed_member_types: Vec<TypeId> = matching_indices
            .iter()
            .map(|&i| member_shapes[i].0)
            .collect();
        let display_target = if narrowed_member_types.len() == 1 {
            narrowed_member_types[0]
        } else {
            tsz_solver::utils::union_or_single_literal_reduce(
                self.ctx.types,
                narrowed_member_types.clone(),
            )
        };
        let narrowed_shapes: Vec<&std::sync::Arc<tsz_solver::ObjectShape>> = matching_indices
            .iter()
            .map(|&i| &member_shapes[i].1)
            .collect();

        // Collect excess properties (not in any narrowed member) with their AST
        // positions. tsc reports only the first excess property in source order.
        let mut excess_candidates: Vec<(tsz_common::interner::Atom, NodeIndex, u32)> = Vec::new();
        for source_prop in source_props {
            if explicit_property_names.is_some()
                && !explicit_property_names
                    .as_ref()
                    .is_some_and(|names| names.contains(&source_prop.name))
            {
                continue;
            }

            let exists_in_narrowed = narrowed_shapes
                .iter()
                .any(|shape| shape.properties.iter().any(|p| p.name == source_prop.name));

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
            // Use the multi-member union only for the diagnostic message text
            // (display_target). For implicit-any initializer checking we still
            // pass the first-matching member to keep contextual typing on the
            // same path as the single-narrowed case.
            self.error_excess_property_at(&prop_name, display_target, earliest.1);
            self.check_excess_property_initializer_implicit_any(
                earliest.1,
                narrowed_member_types[0],
            );
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

        // Apply all discriminants sequentially to progressively narrow the set of
        // matching union members. This matches tsc's behavior for objects with
        // multiple discriminant properties like `{ p1: 'left', p2: false }` against
        // `{ p1: 'left'; p2: true; p3: number } | { p1: 'right'; p2: false; p4: string } | { p1: 'left'; p2: boolean }`:
        // - `p1: 'left'` narrows to members [0, 2]
        // - `p2: false` further narrows [0, 2] to [2] (member 0 has p2: true, not assignable)
        // Result: only member 2 is applicable, and excess property check uses that
        // narrowed set for the error message.
        //
        // A source property is treated as a discriminator when the *target union*
        // exposes the property as a tsc-style discriminant property
        // (`CheckFlags.Discriminant` = `HasLiteralType | HasNonUniformType`):
        //   - it must occur in at least one member,
        //   - the collected property types must contain at least one unit type, and
        //   - the collected property types must differ across members.
        // Once that holds, members that lack the property are filtered out — this
        // mirrors tsc's `discriminateTypeByDiscriminableItems` where
        // `getTypeOfPropertyOfType` returns undefined for missing properties and
        // the member is dropped from the candidate set.
        let mut active_indices: Vec<usize> = (0..union_shapes.len()).collect();
        let mut did_narrow = false;

        for (prop_name, prop_type) in direct_discriminants {
            if source_props.iter().all(|prop| prop.name != prop_name) {
                continue;
            }

            // Collect (member_index, target_prop_type) for the FULL union, so the
            // discriminator decision is based on the union shape rather than the
            // currently-narrowed set (which can shrink to a single member during
            // iteration but should still treat the original union as the reference
            // for "is this a discriminant property?").
            let mut full_members_with_prop: Vec<(usize, TypeId)> = Vec::new();
            for (i, shape) in union_shapes.iter().enumerate() {
                if let Some(target_prop) = shape.properties.iter().find(|p| p.name == prop_name) {
                    full_members_with_prop.push((i, target_prop.type_id));
                }
            }

            if full_members_with_prop.is_empty() {
                continue;
            }

            // tsc's discriminant requires at least one unit/literal type and
            // non-uniform types across the occurrences. Without those, narrowing
            // by the property would risk over-eliminating union members whose
            // shape only happened to differ in non-discriminator slots (for
            // example `{ a: 1, first: string } | { a: 2, second: string }` where
            // `first` is not a discriminant, only `a` is).
            // Evaluate target property types so intersections like
            // `(string | undefined) & 'string'` simplify to the unit literal
            // before the discriminator decision. Without this, a target type
            // like `BaseAttribute<string> & { type: 'string' }` keeps the raw
            // intersection shape on its `type` property and `is_unit_type`
            // returns false, even though the property is in fact a unit
            // literal — matching tsc's apparent-type behavior on intersections.
            let evaluated_full_members_with_prop: Vec<(usize, TypeId)> = full_members_with_prop
                .iter()
                .map(|&(i, ty)| (i, self.evaluate_type_with_env(ty)))
                .collect();
            let any_unit = evaluated_full_members_with_prop
                .iter()
                .any(|(_, ty)| query::is_unit_type(self.ctx.types, *ty));
            if !any_unit {
                continue;
            }
            let first_ty = evaluated_full_members_with_prop[0].1;
            let non_uniform = evaluated_full_members_with_prop
                .iter()
                .any(|(_, ty)| *ty != first_ty);
            if !non_uniform {
                continue;
            }

            // Filter active members: drop members whose target type does not
            // accept the source unit value. Members that *lack* the property
            // are kept as candidates — tsc's `discriminateTypeByDiscriminableItems`
            // treats missing properties on union members as compatible at the
            // discriminator step (the missing property becomes an excess
            // candidate downstream). Previously this filter dropped lacking
            // members too, which caused TS2353 to be missed for object literals
            // like `{ subkind: 1, kind: "b" }` against
            // `{ kind: "a"; subkind: 0|1; … } | { kind: "b" }` — kind narrows
            // to member 2, but tsz also dropped member 2 on the `subkind`
            // pass because it lacks subkind, leaving no narrowed member and
            // skipping excess emission entirely. See `compiler/missingDiscriminants*.ts`.
            let candidate: Vec<usize> = active_indices
                .iter()
                .copied()
                .filter(|&i| {
                    evaluated_full_members_with_prop
                        .iter()
                        .find(|(idx, _)| *idx == i)
                        .is_none_or(|(_, target_ty)| self.is_subtype_of(prop_type, *target_ty))
                })
                .collect();

            // Mirror tsc's `if (!candidate.length) break;` in
            // `discriminateTypeByDiscriminableItems`: when this discriminator
            // produces an empty candidate set, abandon discriminator-based
            // narrowing entirely. The previous `continue` (keeping the
            // pre-filter set and trying the next discriminator) lets a later
            // discriminator narrow over the *unfiltered* `active_indices` and
            // over-narrow the result. Example: source `{a:3, c:10, ...}`
            // against `{a:1,c:10,...}|{a:1,c:20,...}|{a:2,c:30,...}` —
            // `a=3` produces an empty candidate set, then `c=10` would
            // falsely pin the first member alone (TS2353 false positive).
            if candidate.is_empty() {
                return None;
            }

            if candidate.len() < active_indices.len() {
                did_narrow = true;
            }
            active_indices = candidate;
        }

        if did_narrow && active_indices.len() < union_shapes.len() {
            Some(active_indices)
        } else {
            None
        }
    }

    /// Build a flat object shape for an intersection of object types by
    /// collecting all member properties via the solver's intersection
    /// property collector. Returns `None` for non-intersection types or
    /// when no object-like properties can be collected.
    ///
    /// This is the boundary that lets discriminated-union excess-property
    /// checking see intersection union members (e.g. `Common & A`,
    /// `BaseAttribute<string> & { type: 'string' }`) as if they were
    /// flat object shapes, matching tsc's apparent-type behavior.
    fn apparent_intersection_object_shape(
        &self,
        type_id: TypeId,
    ) -> Option<std::sync::Arc<tsz_solver::ObjectShape>> {
        // Only fall back for intersections; other shapes are handled by the
        // direct `query::object_shape` path on the call site.
        query::intersection_members(self.ctx.types, type_id)?;
        match tsz_solver::objects::collect_properties(type_id, self.ctx.types, &self.ctx) {
            tsz_solver::objects::PropertyCollectionResult::Properties {
                properties,
                string_index,
                number_index,
            } => Some(std::sync::Arc::new(tsz_solver::ObjectShape {
                properties,
                string_index,
                number_index,
                ..tsz_solver::ObjectShape::default()
            })),
            _ => None,
        }
    }

    fn try_union_index_signature_value_check(
        &mut self,
        source_props: &[tsz_solver::PropertyInfo],
        obj_literal_idx: NodeIndex,
        union_shapes: &[std::sync::Arc<tsz_solver::ObjectShape>],
        explicit_property_names: Option<&HashSet<Atom>>,
    ) -> bool {
        let diag_count_before = self.ctx.diagnostics.len();

        for source_prop in source_props {
            if explicit_property_names.is_some()
                && !explicit_property_names
                    .as_ref()
                    .is_some_and(|names| names.contains(&source_prop.name))
            {
                continue;
            }

            // Named properties have their own union-member compatibility paths.
            // Keep this check scoped to properties whose only plausible union
            // acceptance is through an index signature.
            if union_shapes.iter().any(|shape| {
                shape
                    .properties
                    .iter()
                    .any(|target_prop| target_prop.name == source_prop.name)
            }) {
                continue;
            }

            let prop_name = self.ctx.types.resolve_atom(source_prop.name);
            let is_numeric_name = tsz_solver::utils::is_numeric_literal_name(&prop_name);
            let mut applicable_index_value_types = Vec::new();
            let mut accepted_by_index = false;
            let mut has_deferred_index_value_type = false;

            for shape in union_shapes {
                if let Some(string_index) = &shape.string_index {
                    if !self.string_index_key_accepts_property_name(
                        string_index.key_type,
                        prop_name.as_ref(),
                        source_prop.is_symbol_named,
                    ) {
                        continue;
                    }
                    if Self::index_value_type_is_deferred(self.ctx.types, string_index.value_type) {
                        has_deferred_index_value_type = true;
                        continue;
                    }
                    applicable_index_value_types.push(string_index.value_type);
                    if self.is_assignable_to(source_prop.type_id, string_index.value_type) {
                        accepted_by_index = true;
                        break;
                    }
                }

                if is_numeric_name && let Some(number_index) = &shape.number_index {
                    if Self::index_value_type_is_deferred(self.ctx.types, number_index.value_type) {
                        has_deferred_index_value_type = true;
                        continue;
                    }
                    applicable_index_value_types.push(number_index.value_type);
                    if self.is_assignable_to(source_prop.type_id, number_index.value_type) {
                        accepted_by_index = true;
                        break;
                    }
                }
            }

            if accepted_by_index
                || applicable_index_value_types.is_empty()
                || has_deferred_index_value_type
            {
                continue;
            }

            let target_value_type =
                tsz_solver::utils::union_or_single(self.ctx.types, applicable_index_value_types);
            let evaluated_target_value_type = self.evaluate_type_with_env(target_value_type);
            if crate::query_boundaries::common::type_is_conditional_type_result_with_unresolved_inference(
                self.ctx.types,
                target_value_type,
            ) || crate::query_boundaries::common::type_is_conditional_type_result_with_unresolved_inference(
                self.ctx.types,
                evaluated_target_value_type,
            ) {
                continue;
            }
            if self.is_assignable_to(source_prop.type_id, target_value_type) {
                continue;
            }

            let report_idx = self
                .find_object_literal_property_element(obj_literal_idx, source_prop.name)
                .unwrap_or(obj_literal_idx);
            if let Some(nested_idx) = self.object_literal_property_initializer(report_idx) {
                let nested_idx = self.ctx.arena.skip_parenthesized(nested_idx);
                if self
                    .ctx
                    .arena
                    .get(nested_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
                {
                    let nested_request =
                        crate::context::TypingRequest::with_contextual_type(target_value_type);
                    let nested_source =
                        self.get_type_of_node_with_request(nested_idx, &nested_request);
                    let before_nested = self.ctx.diagnostics.len();
                    self.check_object_literal_excess_properties(
                        nested_source,
                        target_value_type,
                        nested_idx,
                    );
                    if self.ctx.diagnostics.len() > before_nested {
                        continue;
                    }
                }
            }
            let computed_property = self
                .ctx
                .arena
                .get(report_idx)
                .and_then(|node| self.ctx.arena.get_property_assignment(node))
                .map(|prop| (prop.name, prop.initializer))
                .or_else(|| {
                    self.object_literal_property_name_and_value(obj_literal_idx, source_prop.name)
                })
                .or_else(|| {
                    let obj_node = self.ctx.arena.get(obj_literal_idx)?;
                    let obj_lit = self.ctx.arena.get_literal_expr(obj_node)?;
                    obj_lit.elements.nodes.iter().rev().find_map(|&elem_idx| {
                        let elem_node = self.ctx.arena.get(elem_idx)?;
                        let prop = self.ctx.arena.get_property_assignment(elem_node)?;
                        let resolved = self.get_property_name_resolved(prop.name)?;
                        (self.ctx.types.intern_string(&resolved) == source_prop.name)
                            .then_some((prop.name, prop.initializer))
                    })
                });
            if let Some((prop_name_idx, prop_value_idx)) = computed_property
                && self
                    .ctx
                    .arena
                    .get(prop_name_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

                let source_type = self
                    .literal_type_from_initializer(prop_value_idx)
                    .unwrap_or(source_prop.type_id);
                let source_str = self.format_type_for_assignability_message(source_type);
                let target_str = self.format_type_for_assignability_message(target_value_type);
                let message = format_message(
                    diagnostic_messages::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                self.error_at_node(
                    prop_name_idx,
                    &message,
                    diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
                continue;
            }
            self.error_type_not_assignable_at_with_anchor(
                source_prop.type_id,
                target_value_type,
                report_idx,
            );
        }

        self.ctx.diagnostics.len() > diag_count_before
    }

    fn index_value_type_is_deferred(types: &dyn tsz_solver::TypeDatabase, type_id: TypeId) -> bool {
        crate::query_boundaries::common::is_index_access_type(types, type_id)
            || crate::query_boundaries::common::contains_type_parameters(types, type_id)
    }

    fn try_emit_nested_discriminated_union_assignability_error(
        &mut self,
        outer_source: TypeId,
        outer_target: TypeId,
        obj_literal_idx: NodeIndex,
        prop_name: Atom,
        nested_target: TypeId,
    ) -> bool {
        if !self.is_object_like_nested_target(nested_target) {
            return false;
        }

        let literal_idx = if self
            .ctx
            .arena
            .get(obj_literal_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
        {
            obj_literal_idx
        } else {
            let Some(literal_idx) = self.find_rhs_object_literal(obj_literal_idx) else {
                return false;
            };
            literal_idx
        };

        let Some((report_idx, value_idx)) =
            self.object_literal_property_name_and_value(literal_idx, prop_name)
        else {
            return false;
        };
        let effective_value_idx = self.ctx.arena.skip_parenthesized(value_idx);
        let Some(value_node) = self.ctx.arena.get(effective_value_idx) else {
            return false;
        };
        if value_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        let Some(rejected_property) =
            self.nested_literal_rejected_fresh_property(effective_value_idx, nested_target)
        else {
            return false;
        };

        let source_str = self.format_type(outer_source);
        let target_str = self.format_type(outer_target);
        let message = crate::diagnostics::format_message(
            crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        if let Some((start, length)) =
            self.find_excess_property_anchor(effective_value_idx, rejected_property)
        {
            self.error(
                start,
                length,
                message,
                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        } else {
            self.error_at_anchor(
                report_idx,
                crate::error_reporter::DiagnosticAnchorKind::PropertyToken,
                &message,
                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }
        true
    }

    fn is_object_like_nested_target(&mut self, nested_target: TypeId) -> bool {
        let nested_target = self.evaluate_type_with_env(nested_target);
        let nested_target = self.resolve_type_for_property_access(nested_target);

        if query::object_shape(self.ctx.types, nested_target).is_some() {
            return true;
        }

        let resolved_target = self.prune_impossible_object_union_members_with_env(nested_target);
        query::union_members(self.ctx.types, resolved_target).is_some_and(|members| {
            members.iter().any(|member| {
                let resolved_member = self.resolve_type_for_property_access(*member);
                query::object_shape(self.ctx.types, resolved_member).is_some()
            })
        })
    }

    fn nested_literal_rejected_fresh_property(
        &mut self,
        nested_literal_idx: NodeIndex,
        nested_target: TypeId,
    ) -> Option<Atom> {
        let nested_target = self.evaluate_type_with_env(nested_target);
        let nested_target = self.resolve_type_for_property_access(nested_target);
        let resolved_target = self.prune_impossible_object_union_members_with_env(nested_target);
        let members = query::union_members(self.ctx.types, resolved_target)?;

        let mut target_shapes = Vec::new();
        for &member in &members {
            let resolved_member = self.resolve_type_for_property_access(member);
            let shape = query::object_shape(self.ctx.types, resolved_member)?;
            target_shapes.push(shape);
        }
        if target_shapes.is_empty() {
            return None;
        }

        let nested_node = self.ctx.arena.get(nested_literal_idx)?;
        let nested_literal = self.ctx.arena.get_literal_expr(nested_node)?;
        for &elem_idx in &nested_literal.elements.nodes {
            let elem_node = self.ctx.arena.get(elem_idx)?;
            let prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_property_assignment(elem_node)?;
                    self.get_property_name(prop.name)
                }
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_shorthand_property(elem_node)?;
                    self.get_property_name(prop.name)
                }
                _ => None,
            };
            let Some(prop_name) = prop_name.map(|name| self.ctx.types.intern_string(&name)) else {
                continue;
            };
            let accepted_by_target = target_shapes.iter().any(|shape| {
                shape
                    .properties
                    .iter()
                    .any(|target_prop| target_prop.name == prop_name)
                    || shape.string_index.is_some()
                    || (shape.number_index.is_some()
                        && tsz_solver::utils::is_numeric_literal_name(
                            &self.ctx.types.resolve_atom(prop_name),
                        ))
            });
            if !accepted_by_target {
                return Some(prop_name);
            }
        }

        None
    }

    pub(super) fn object_literal_property_name_and_value(
        &self,
        obj_literal_idx: NodeIndex,
        prop_name: Atom,
    ) -> Option<(NodeIndex, NodeIndex)> {
        let obj_node = self.ctx.arena.get(obj_literal_idx)?;
        let obj_lit = self.ctx.arena.get_literal_expr(obj_node)?;

        for &elem_idx in obj_lit.elements.nodes.iter().rev() {
            let elem_node = self.ctx.arena.get(elem_idx)?;
            match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_property_assignment(elem_node)?;
                    let elem_prop_name = self
                        .get_property_name(prop.name)
                        .map(|name| self.ctx.types.intern_string(&name));
                    if elem_prop_name == Some(prop_name) {
                        return Some((prop.name, prop.initializer));
                    }
                }
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_shorthand_property(elem_node)?;
                    let elem_prop_name = self
                        .get_property_name(prop.name)
                        .map(|name| self.ctx.types.intern_string(&name));
                    if elem_prop_name == Some(prop_name) {
                        return Some((prop.name, prop.name));
                    }
                }
                _ => {}
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

        self.explicit_object_literal_property_names(obj_literal_idx)
    }

    fn explicit_object_literal_property_names(
        &self,
        obj_literal_idx: NodeIndex,
    ) -> Option<HashSet<Atom>> {
        let obj_node = self.ctx.arena.get(obj_literal_idx)?;
        if obj_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let obj_lit = self.ctx.arena.get_literal_expr(obj_node)?;

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

    fn const_assertion_object_literal_expression(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let idx = self.ctx.arena.skip_parenthesized(idx);
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::AS_EXPRESSION
            && node.kind != syntax_kind_ext::TYPE_ASSERTION
        {
            return None;
        }
        let assertion = self.ctx.arena.get_type_assertion(node)?;
        if !self.is_const_assertion_type_node(assertion.type_node) {
            return None;
        }
        let expression_idx = self.ctx.arena.skip_parenthesized(assertion.expression);
        let expression = self.ctx.arena.get(expression_idx)?;
        if expression.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return Some(expression_idx);
        }
        None
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
    ) -> bool {
        let diagnostics_before = self.ctx.diagnostics.len();
        // Get the AST node for the object literal
        let Some(obj_node) = self.ctx.arena.get(obj_literal_idx) else {
            return false;
        };

        let Some(obj_lit) = self.ctx.arena.get_literal_expr(obj_node) else {
            return false;
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

                return self.ctx.diagnostics.len() > diagnostics_before;
            }
        }
        false
    }

    /// Find the property element node in an object literal by interned property name.
    pub(crate) fn find_object_literal_property_element(
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

    fn object_literal_property_initializer(&self, prop_idx: NodeIndex) -> Option<NodeIndex> {
        let prop_node = self.ctx.arena.get(prop_idx)?;
        match prop_node.kind {
            syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                .ctx
                .arena
                .get_property_assignment(prop_node)
                .map(|prop| prop.initializer),
            syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                .ctx
                .arena
                .get_shorthand_property(prop_node)
                .map(|prop| prop.name),
            _ => None,
        }
    }

    pub(crate) fn object_literal_property_display_name(
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
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
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

    /// Regression test: when a discriminated-union target has members whose
    /// discriminant property is an unsimplified intersection (e.g. the merged
    /// shape of `BaseAttribute<string> & { type: 'string' }` exposes
    /// `type: (string | undefined) & 'string'`), tsz must evaluate that
    /// property type before applying the `is_unit_type` discriminant test.
    /// Without evaluation, `is_unit_type(intersection)` returns false and the
    /// excess-property check silently bails, missing the TS2353 that tsc
    /// emits.
    #[test]
    fn ts2353_discriminated_union_with_intersected_member_property_types() {
        let diags = check_source_diagnostics(
            r#"
type BaseAttribute<T> = {
    type?: string | undefined;
    required?: boolean | undefined;
    defaultsTo?: T | undefined;
};
type StringAttribute = BaseAttribute<string> & { type: 'string'; };
type NumberAttribute = BaseAttribute<number> & {
    type: 'number';
    autoIncrement?: boolean | undefined;
};
type Attribute = string | StringAttribute | NumberAttribute;

const a: Attribute = {
    type: 'string',
    autoIncrement: true,
    required: true,
};
"#,
        );

        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert_eq!(
            ts2353.len(),
            1,
            "expected one TS2353 for 'autoIncrement' against StringAttribute, got: {diags:?}"
        );
        assert!(
            ts2353[0].message_text.contains("'autoIncrement'"),
            "TS2353 should mention 'autoIncrement', got: {}",
            ts2353[0].message_text
        );
    }
}
