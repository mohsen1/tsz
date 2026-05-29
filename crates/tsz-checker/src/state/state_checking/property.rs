use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use std::collections::HashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

mod union_index_signature_diagnostics;

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

    pub(crate) fn resolve_index_signature_key_type_via_env(&self, key_type: TypeId) -> TypeId {
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

    fn union_member_has_type_parameter_for_excess_display(&self, member: TypeId) -> bool {
        query::is_type_parameter_like(self.ctx.types, member)
            || crate::query_boundaries::common::contains_generic_type_parameters(
                self.ctx.types,
                member,
            )
    }

    fn type_is_recursive_operation_application(&self, type_id: TypeId) -> bool {
        if crate::query_boundaries::type_predicates::is_recursive_operation_application(
            self.ctx.types,
            &self.ctx.definition_store,
            type_id,
        ) {
            return true;
        }

        if let Some(alias) = self.ctx.types.get_display_alias(type_id)
            && crate::query_boundaries::type_predicates::is_recursive_operation_application(
                self.ctx.types,
                &self.ctx.definition_store,
                alias,
            )
        {
            return true;
        }

        false
    }

    fn type_contains_recursive_operation_application(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::type_predicates::contains_recursive_operation_application(
            self.ctx.types,
            &self.ctx.definition_store,
            type_id,
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
        self.ensure_relation_input_ready(effective_target);
        self.ensure_relation_input_ready(resolved_target);

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
                    if self.index_value_type_is_deferred(string_index.value_type)
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
                                        && self
                                            .assign_relation_outcome(
                                                source_prop.type_id,
                                                target_prop.type_id,
                                            )
                                            .related
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
                    let nested_target = self.widen_nested_target_for_property(
                        nested_target,
                        target,
                        effective_target,
                    );

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
            let materialized_recursive_target =
                [effective_target, resolved_target, evaluated_target]
                    .into_iter()
                    .any(|candidate| query::object_shape(self.ctx.types, candidate).is_some())
                    && [
                        target,
                        effective_target,
                        resolved_target,
                        evaluated_target,
                        union_check_target,
                    ]
                    .into_iter()
                    .any(|candidate| {
                        self.type_is_recursive_operation_application(candidate)
                            || self.type_contains_recursive_operation_application(candidate)
                            || self.target_is_or_displays_type_application(candidate)
                    });
            if !materialized_recursive_target {
                return;
            }
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
                        // When the resolved property type is a direct recursive
                        // self-reference to one of the intersection members (e.g.
                        // `User.parent: User` in `User & { admin: boolean }`), widen it
                        // to the full outer intersection so nested literals are checked
                        // against all members, not just the recursive member alone.
                        let nested_target =
                            self.widen_nested_target_if_recursive(nested_target, resolved_target);
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
                        // Check this property but continue iterating — tsc reports all
                        // mismatching properties, not just the first one found.
                        self.check_object_literal_named_property_value(
                            idx,
                            source_prop.name,
                            source_prop.type_id,
                            effective_target,
                            type_id,
                        );
                        let nested_target = self.nested_property_target_type(
                            effective_target,
                            source_prop.name,
                            type_id,
                        );
                        let nested_target = self.widen_nested_target_for_property(
                            nested_target,
                            target,
                            effective_target,
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
                        // Continue iterating after a mismatch — tsc reports all mismatching
                        // properties, not just the first one.
                        self.check_object_literal_named_property_value(
                            idx,
                            source_prop.name,
                            source_prop.type_id,
                            effective_target,
                            target_prop.type_id,
                        );
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
                    let nested_target = self.widen_nested_target_for_property(
                        nested_target,
                        target,
                        effective_target,
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
                        // Check each property value independently: do not return early when
                        // a mismatch is found. tsc reports all mismatching properties, so we
                        // must continue iterating after the first error.
                        if should_check_named_values {
                            self.check_object_literal_named_property_value(
                                idx,
                                source_prop.name,
                                source_prop.type_id,
                                effective_target,
                                target_prop_type,
                            );
                        }
                        let nested_target = self.nested_property_target_type(
                            effective_target,
                            source_prop.name,
                            target_prop_type,
                        );
                        let nested_target = self.widen_nested_target_for_property(
                            nested_target,
                            target,
                            effective_target,
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
        }
        // Note: Missing property checks are handled by solver's explain_failure
    }
}

include!("property/excess_property_tail.rs");

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

    // -----------------------------------------------------------------------
    // Recursive intersection excess-property tests (issue #8687)
    //
    // Structural rule: when `interface A { p?: A }` is a member of `A & B`,
    // a nested literal for `p` must be checked against `A & B`, not just `A`.
    // -----------------------------------------------------------------------

    #[test]
    fn ts2353_no_false_positive_for_recursive_intersection_nested_literal() {
        // `parent` is `User | undefined` in User, but the target is `UserGroup`
        // (= User & { admin: boolean }).  A nested literal that includes `admin`
        // must NOT trigger TS2353.
        let diags = check_source_diagnostics(
            r#"
interface User { name: string; parent?: User; }
type UserGroup = User & { admin: boolean; }
const u: UserGroup = { name: "Alice", admin: true, parent: { name: "Bob", admin: false } };
"#,
        );
        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert!(
            ts2353.is_empty(),
            "expected no TS2353 for valid nested intersection literal, got: {ts2353:?}"
        );
    }

    #[test]
    fn ts2353_no_false_positive_recursive_intersection_renamed_type_param() {
        // Variant with differently-named interface to prove the fix is not
        // keyed on the name "User".
        let diags = check_source_diagnostics(
            r#"
interface Node { value: number; child?: Node; }
type AnnotatedNode = Node & { label: string; }
const n: AnnotatedNode = { value: 1, label: "root", child: { value: 2, label: "leaf" } };
"#,
        );
        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert!(
            ts2353.is_empty(),
            "expected no TS2353 for valid recursive annotated node, got: {ts2353:?}"
        );
    }

    #[test]
    fn ts2353_still_reports_genuinely_excess_property_on_recursive_intersection() {
        // Even with a recursive intersection target, a truly extra property
        // (one that's in neither member) must still cause TS2353.
        let diags = check_source_diagnostics(
            r#"
interface User { name: string; parent?: User; }
type UserGroup = User & { admin: boolean; }
const u: UserGroup = { name: "Alice", admin: true, parent: { name: "Bob", admin: false, extra: 99 } };
"#,
        );
        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert_eq!(
            ts2353.len(),
            1,
            "expected exactly one TS2353 for 'extra', got: {ts2353:?}"
        );
        assert!(
            ts2353[0].message_text.contains("'extra'"),
            "TS2353 should mention 'extra', got: {}",
            ts2353[0].message_text
        );
    }

    #[test]
    fn ts2353_no_false_positive_recursive_intersection_via_type_alias() {
        // Same structural pattern through an explicit type alias rather than a
        // direct interface reference, to confirm alias indirection is handled.
        let diags = check_source_diagnostics(
            r#"
interface Category { name: string; parent?: Category; }
type TaggedCategory = Category & { tag: string; }
const c: TaggedCategory = { name: "root", tag: "top", parent: { name: "child", tag: "mid" } };
"#,
        );
        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert!(
            ts2353.is_empty(),
            "expected no TS2353 for valid tagged recursive category, got: {ts2353:?}"
        );
    }

    #[test]
    fn ts2353_debug_structural_type_alias_recursive_intersection() {
        // DEBUG: structural type alias (not interface) recursive intersection
        let diags = check_source_diagnostics(
            r#"
type Chain = { data: string; rest?: Chain; };
type MarkedChain = Chain & { marker: number; }
const c: MarkedChain = { data: "a", marker: 1, rest: { data: "b", marker: 2 } };
"#,
        );
        let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
        assert!(
            ts2353.is_empty(),
            "expected no TS2353 for structural type alias recursive intersection, got: {ts2353:?}"
        );
    }
}
