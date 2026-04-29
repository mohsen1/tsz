//! JSX component props inference: generic inference, contextual typing for
//! function-valued attributes, intrinsic props lookup, and component props
//! recovery.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::checkers_domain::jsx) fn normalize_jsx_contextual_callable_member(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let type_id =
            crate::query_boundaries::common::unwrap_readonly_or_noinfer(self.ctx.types, type_id)
                .unwrap_or(type_id);
        let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
        else {
            return type_id;
        };

        let normalized = tsz_solver::FunctionShape {
            type_params: shape.type_params.clone(),
            params: shape
                .params
                .iter()
                .map(|param| tsz_solver::ParamInfo {
                    name: param.name,
                    type_id: crate::query_boundaries::common::unwrap_readonly_or_noinfer(
                        self.ctx.types,
                        param.type_id,
                    )
                    .unwrap_or(param.type_id),
                    optional: param.optional,
                    rest: param.rest,
                })
                .collect(),
            this_type: shape.this_type.map(|this_type| {
                crate::query_boundaries::common::unwrap_readonly_or_noinfer(
                    self.ctx.types,
                    this_type,
                )
                .unwrap_or(this_type)
            }),
            return_type: crate::query_boundaries::common::unwrap_readonly_or_noinfer(
                self.ctx.types,
                shape.return_type,
            )
            .unwrap_or(shape.return_type),
            type_predicate: shape.type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        };
        self.ctx.types.factory().function(normalized)
    }

    pub(in crate::checkers_domain::jsx) fn refine_jsx_callable_contextual_type(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let resolved = self.resolve_type_for_property_access(type_id);
        let resolved = self.evaluate_type_with_env(resolved);
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, resolved)
        else {
            return self.normalize_jsx_contextual_callable_member(resolved);
        };

        let mut callable_members = Vec::new();
        for member in members {
            let member = self.resolve_type_for_property_access(member);
            let member = self.evaluate_type_with_env(member);
            let is_callable =
                crate::query_boundaries::common::function_shape_for_type(self.ctx.types, member)
                    .is_some_and(|shape| !shape.is_constructor)
                    || crate::query_boundaries::common::call_signatures_for_type(
                        self.ctx.types,
                        member,
                    )
                    .is_some_and(|sigs| !sigs.is_empty());
            if is_callable {
                callable_members.push(self.normalize_jsx_contextual_callable_member(member));
            }
        }

        match callable_members.len() {
            0 => type_id,
            1 => callable_members[0],
            _ => self.ctx.types.factory().union(callable_members),
        }
    }

    pub(in crate::checkers_domain::jsx) fn select_jsx_single_children_target_type(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let resolved = self.resolve_type_for_property_access(type_id);
        let resolved = self.evaluate_type_with_env(resolved);
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, resolved)
        else {
            return resolved;
        };

        let single_members: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|&member| !self.type_requires_multiple_children(member))
            .collect();
        match single_members.as_slice() {
            [] => resolved,
            [single_member] => *single_member,
            _ => self.ctx.types.factory().union(single_members),
        }
    }

    pub(in crate::checkers_domain::jsx) fn select_jsx_multiple_children_target_type(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let resolved = self.resolve_type_for_property_access(type_id);
        let resolved = self.evaluate_type_with_env(resolved);
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, resolved)
        else {
            return resolved;
        };

        let multiple_members: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|&member| self.type_allows_multiple_children(member))
            .collect();
        match multiple_members.as_slice() {
            [] => resolved,
            [multiple_member] => *multiple_member,
            _ => self.ctx.types.factory().union(multiple_members),
        }
    }

    pub(in crate::checkers_domain::jsx) fn jsx_multiple_children_element_type(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let resolved = self.select_jsx_multiple_children_target_type(type_id);
        let resolved = self.resolve_type_for_property_access(resolved);
        let resolved = self.evaluate_type_with_env(resolved);

        if let Some(element_type) =
            crate::query_boundaries::common::array_element_type(self.ctx.types, resolved)
        {
            return Some(self.refine_jsx_callable_contextual_type(element_type));
        }

        if let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, resolved)
        {
            let element_types: Vec<TypeId> = elements
                .iter()
                .map(|elem| self.refine_jsx_callable_contextual_type(elem.type_id))
                .collect();
            return match element_types.as_slice() {
                [] => None,
                [element_type] => Some(*element_type),
                _ => Some(self.ctx.types.factory().union(element_types)),
            };
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, resolved)
        {
            let mut element_types = Vec::new();
            for member in members {
                if let Some(element_type) = self.jsx_multiple_children_element_type(member) {
                    element_types.push(element_type);
                } else {
                    // Non-array union members (like `{}` in ReactFragment) should be
                    // included directly as valid target types for individual child
                    // assignability. `{}` accepts any non-nullish value including
                    // functions, so omitting it causes false TS2322 for function children.
                    element_types.push(member);
                }
            }
            return match element_types.as_slice() {
                [] => None,
                [element_type] => Some(*element_type),
                _ => Some(self.ctx.types.factory().union(element_types)),
            };
        }

        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, resolved)
            .and_then(|shape| shape.number_index.as_ref().map(|index| index.value_type))
            .map(|value_type| self.refine_jsx_callable_contextual_type(value_type))
    }

    pub(in crate::checkers_domain::jsx) fn jsx_children_contextual_type_for_body_shape(
        &mut self,
        children_type: TypeId,
        child_count: usize,
    ) -> TypeId {
        if child_count > 1 {
            return self
                .jsx_multiple_children_element_type(children_type)
                .unwrap_or(children_type);
        }

        let single_children_type = self.select_jsx_single_children_target_type(children_type);
        self.refine_jsx_callable_contextual_type(single_children_type)
    }

    pub(in crate::checkers_domain::jsx) const fn file_has_same_line_adjacent_jsx_recovery_pattern(
        &self,
    ) -> bool {
        // Previously this used text-based heuristics to detect adjacent JSX
        // recovery patterns (e.g., `/><` or `></`), but those patterns also
        // match normal JSX syntax (e.g., `</span><div>` or `</span>`).
        // Parser recovery situations are already detected by `has_parse_errors`
        // and `all_parse_error_positions`, so this heuristic is no longer needed.
        false
    }

    pub(in crate::checkers_domain::jsx) fn normalize_jsx_component_type_for_resolution(
        &mut self,
        component_type: TypeId,
    ) -> TypeId {
        let evaluated = self.evaluate_application_type(component_type);
        let evaluated = self.evaluate_type_with_env(evaluated);
        let resolved = self.resolve_type_for_property_access(evaluated);
        let resolved = self.resolve_lazy_type(resolved);
        let resolved = self.evaluate_application_type(resolved);
        self.evaluate_type_with_env(resolved)
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_component_metadata_type(
        &mut self,
        tag_name_idx: NodeIndex,
        fallback_type: TypeId,
    ) -> TypeId {
        let Some(sym_id) = self.resolve_identifier_symbol(tag_name_idx) else {
            return fallback_type;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return fallback_type;
        };

        if !symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
            && let Some(name) = self.get_identifier_text_from_idx(tag_name_idx)
        {
            let expando_props = self.collect_expando_properties_for_root(&name);
            let mut metadata_props = Vec::new();
            for prop_name in ["defaultProps", "propTypes"] {
                if expando_props.contains(prop_name) {
                    let atom = self.ctx.types.intern_string(prop_name);
                    let type_id =
                        self.declared_expando_property_type_for_root(sym_id, &name, prop_name);
                    metadata_props.push(tsz_solver::PropertyInfo::new(atom, type_id));
                }
            }
            if !metadata_props.is_empty() {
                return self.ctx.types.factory().object(metadata_props);
            }
        }

        for mut decl_idx in symbol.all_declarations() {
            let Some(mut decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(parent) = self.ctx.arena.parent_of(decl_idx)
                && parent.is_some()
            {
                decl_idx = parent;
                let Some(parent_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                decl_node = parent_node;
            }

            if decl_node.kind == syntax_kind_ext::CLASS_DECLARATION
                && let Some(class) = self.ctx.arena.get_class(decl_node)
            {
                return self.get_class_constructor_type(decl_idx, class);
            }
        }

        fallback_type
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_specific_string_literal_component_tag_name(
        &self,
        tag_name_idx: NodeIndex,
        component_type: TypeId,
    ) -> Option<String> {
        let tag_name_node = self.ctx.arena.get(tag_name_idx)?;
        if tag_name_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let ident = self
            .ctx
            .arena
            .get_identifier(tag_name_node)?
            .escaped_text
            .as_str();
        if ident
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
        {
            return None;
        }

        self.get_jsx_single_string_literal_tag_name(component_type)
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_single_string_literal_tag_name(
        &self,
        type_id: TypeId,
    ) -> Option<String> {
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, type_id)
            && let Some(constraint) =
                crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, type_id)
            && constraint != type_id
        {
            return self.get_jsx_single_string_literal_tag_name(constraint);
        }

        if let Some(name) =
            crate::query_boundaries::common::string_literal_value(self.ctx.types, type_id)
        {
            return Some(self.ctx.types.resolve_atom(name).as_str().to_string());
        }

        let members = crate::query_boundaries::common::union_members(self.ctx.types, type_id)?;
        let mut literal_name = None;
        for member in members {
            let name =
                crate::query_boundaries::common::string_literal_value(self.ctx.types, member)?;
            match literal_name {
                Some(existing) if existing != name => return None,
                Some(_) => {}
                None => literal_name = Some(name),
            }
        }

        literal_name.map(|name| self.ctx.types.resolve_atom(name).as_str().to_string())
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_library_managed_attributes_application(
        &mut self,
        component_type: TypeId,
        props_type: TypeId,
    ) -> Option<TypeId> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let lma_sym_id = exports.get("LibraryManagedAttributes")?;
        let lma_ref = self.resolve_symbol_as_lazy_type(lma_sym_id);
        Some(
            self.ctx
                .types
                .factory()
                .application(lma_ref, vec![component_type, props_type]),
        )
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_dynamic_intrinsic_display_props_type(
        &mut self,
        tag_name_idx: NodeIndex,
        component_type: TypeId,
        fallback_type: TypeId,
    ) -> TypeId {
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, component_type)
            && let Some(constraint) = crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                component_type,
            )
            && let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, constraint)
        {
            for member in members.into_iter().rev() {
                if let Some(tag_name) = self.get_jsx_single_string_literal_tag_name(member)
                    && let Some(props_type) =
                        self.get_jsx_intrinsic_props_for_tag(tag_name_idx, &tag_name, false)
                {
                    return props_type;
                }
            }
        }

        fallback_type
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_dynamic_intrinsic_props_for_component_type(
        &mut self,
        tag_name_idx: NodeIndex,
        component_type: TypeId,
    ) -> Option<(TypeId, bool, String)> {
        let intrinsic_elements_type = self.get_intrinsic_elements_type()?;
        let raw_props_type = self
            .ctx
            .types
            .factory()
            .index_access(intrinsic_elements_type, component_type);
        let normalized_props = self.normalize_jsx_required_props_target(raw_props_type);
        if matches!(
            normalized_props,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN
        ) {
            return None;
        }

        let semantic_props_type = self
            .get_jsx_library_managed_attributes_application(component_type, normalized_props)
            .unwrap_or(raw_props_type);
        let display_props_type = self.get_jsx_dynamic_intrinsic_display_props_type(
            tag_name_idx,
            component_type,
            normalized_props,
        );
        let display_props_type = self
            .get_jsx_library_managed_attributes_application(component_type, display_props_type)
            .unwrap_or(display_props_type);

        let raw_has_type_params = crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            raw_props_type,
        ) || crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            semantic_props_type,
        ) || crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            component_type,
        );
        let display_target = self.format_type(display_props_type);
        Some((semantic_props_type, raw_has_type_params, display_target))
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_identifier_declared_type(
        &mut self,
        tag_name_idx: NodeIndex,
        fallback_type: TypeId,
    ) -> TypeId {
        let Some(sym_id) = self.resolve_identifier_symbol(tag_name_idx) else {
            return fallback_type;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return fallback_type;
        };
        let Some(&decl_idx) = symbol.declarations.first() else {
            return fallback_type;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return fallback_type;
        };

        if let Some(param) = self.ctx.arena.get_parameter(decl_node)
            && param.type_annotation.is_some()
        {
            return self.get_type_from_type_node(param.type_annotation);
        }
        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
            && var_decl.type_annotation.is_some()
        {
            return self.get_type_from_type_node(var_decl.type_annotation);
        }

        fallback_type
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_intrinsic_props_from_template_literal_index_signatures(
        &mut self,
        tag: &str,
    ) -> Option<TypeId> {
        let intrinsic_elements_sym_id = self.get_intrinsic_elements_symbol_id()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(intrinsic_elements_sym_id, &lib_binders)?;
        let tag_literal =
            crate::query_boundaries::common::create_string_literal_type(self.ctx.types, tag);
        let mut candidates = Vec::new();

        for mut decl_idx in symbol.all_declarations() {
            let Some(mut decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(parent) = self.ctx.arena.parent_of(decl_idx)
                && parent.is_some()
            {
                decl_idx = parent;
                let Some(parent_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                decl_node = parent_node;
            }

            let members = match decl_node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    let Some(iface) = self.ctx.arena.get_interface(decl_node) else {
                        continue;
                    };
                    &iface.members.nodes
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    let Some(alias) = self.ctx.arena.get_type_alias(decl_node) else {
                        continue;
                    };
                    let Some(type_node) = self.ctx.arena.get(alias.type_node) else {
                        continue;
                    };
                    if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
                        continue;
                    }
                    let Some(type_lit) = self.ctx.arena.get_type_literal(type_node) else {
                        continue;
                    };
                    &type_lit.members.nodes
                }
                _ => continue,
            };

            for &member_idx in members {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                    continue;
                };
                let Some(param_idx) = index_sig.parameters.nodes.first().copied() else {
                    continue;
                };
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if param.type_annotation.is_none() {
                    continue;
                }

                let key_type = self.get_type_from_type_node(param.type_annotation);
                let key_type = self.evaluate_type_with_env(key_type);
                if !crate::query_boundaries::common::is_template_literal_type(
                    self.ctx.types,
                    key_type,
                ) || !self.is_assignable_to(tag_literal, key_type)
                {
                    continue;
                }

                let value_type = if index_sig.type_annotation.is_some() {
                    let value_type = self.get_type_from_type_node(index_sig.type_annotation);
                    self.evaluate_type_with_env(value_type)
                } else {
                    TypeId::ANY
                };
                candidates.push((key_type, value_type));
            }
        }

        let mut best_matches: Vec<(TypeId, TypeId)> = Vec::new();
        for (candidate_key, candidate_value) in candidates {
            let mut candidate_is_best = true;
            let mut i = 0;
            while i < best_matches.len() {
                let (best_key, _) = best_matches[i];
                let candidate_more_specific = self.is_assignable_to(candidate_key, best_key)
                    && !self.is_assignable_to(best_key, candidate_key);
                if candidate_more_specific {
                    best_matches.swap_remove(i);
                    continue;
                }

                let best_more_specific = self.is_assignable_to(best_key, candidate_key)
                    && !self.is_assignable_to(candidate_key, best_key);
                if best_more_specific {
                    candidate_is_best = false;
                    break;
                }
                i += 1;
            }

            if candidate_is_best {
                best_matches.push((candidate_key, candidate_value));
            }
        }

        match best_matches.len() {
            0 => None,
            1 => best_matches.first().map(|(_, value_type)| *value_type),
            _ => Some(
                self.ctx.types.factory().union(
                    best_matches
                        .into_iter()
                        .map(|(_, value_type)| value_type)
                        .collect(),
                ),
            ),
        }
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_intrinsic_props_for_tag(
        &mut self,
        element_idx: NodeIndex,
        tag: &str,
        report_missing: bool,
    ) -> Option<TypeId> {
        let intrinsic_elements_type = self.get_intrinsic_elements_type()?;
        let evaluated_ie = self.evaluate_type_with_env(intrinsic_elements_type);
        let tag_atom = self.ctx.types.intern_string(tag);
        let cache_key = (intrinsic_elements_type, tag_atom);

        if let Some(&cached) = self.ctx.jsx_intrinsic_props_cache.get(&cache_key)
            && (!report_missing || cached != TypeId::ERROR)
        {
            return Some(cached);
        }

        use crate::query_boundaries::common::PropertyAccessResult;
        let template_literal_props =
            self.get_jsx_intrinsic_props_from_template_literal_index_signatures(tag);
        let props = match self.resolve_property_access_with_env(evaluated_ie, tag) {
            PropertyAccessResult::Success {
                type_id,
                from_index_signature,
                ..
            } => {
                if from_index_signature {
                    template_literal_props.unwrap_or(type_id)
                } else {
                    type_id
                }
            }
            PropertyAccessResult::PropertyNotFound { .. } => {
                if let Some(props) = template_literal_props {
                    props
                } else {
                    if report_missing {
                        use tsz_common::diagnostics::diagnostic_codes;
                        let message = format!(
                            "Property '{tag}' does not exist on type 'JSX.IntrinsicElements'."
                        );
                        self.error_at_node(
                            element_idx,
                            &message,
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        );
                        self.check_missing_intrinsic_tag_against_jsx_element_type(element_idx, tag);
                        self.ctx
                            .jsx_intrinsic_props_cache
                            .insert(cache_key, TypeId::ERROR);
                    }
                    TypeId::ERROR
                }
            }
            _ => template_literal_props.unwrap_or(TypeId::ANY),
        };

        if props != TypeId::ERROR || report_missing {
            self.ctx.jsx_intrinsic_props_cache.insert(cache_key, props);
        }

        Some(props)
    }

    fn check_missing_intrinsic_tag_against_jsx_element_type(
        &mut self,
        element_idx: NodeIndex,
        tag: &str,
    ) {
        let Some(tag_name_idx) = self.jsx_tag_name_idx_for_element_like(element_idx) else {
            return;
        };
        let Some(element_type_sym_id) = self.get_jsx_namespace_export_symbol_id("ElementType")
        else {
            return;
        };
        let element_type = self.type_reference_symbol_type(element_type_sym_id);
        if matches!(
            element_type,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::NEVER
        ) {
            return;
        }

        let tag_type = self.ctx.types.literal_string(tag);
        if self.is_assignable_to(tag_type, element_type) {
            return;
        }

        use tsz_common::diagnostics::diagnostic_codes;
        self.error_at_node_msg(
            tag_name_idx,
            diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT,
            &[tag],
        );
    }

    fn jsx_tag_name_idx_for_element_like(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(idx)?;
        if let Some(opening) = self.ctx.arena.get_jsx_opening(node) {
            return Some(opening.tag_name);
        }
        if let Some(closing) = self.ctx.arena.get_jsx_closing(node) {
            return Some(closing.tag_name);
        }
        Some(idx)
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_intrinsic_tag_name(
        &self,
        tag_name_idx: NodeIndex,
    ) -> Option<String> {
        let tag_name_node = self.ctx.arena.get(tag_name_idx)?;
        if tag_name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(tag_name_node)
                .map(|id| id.escaped_text.as_str().to_string());
        }

        if tag_name_node.kind == syntax_kind_ext::JSX_NAMESPACED_NAME {
            return self
                .ctx
                .arena
                .get_jsx_namespaced_name(tag_name_node)
                .and_then(|ns| {
                    let ns_id = self.ctx.arena.get(ns.namespace)?;
                    let ns_text = self.ctx.arena.get_identifier(ns_id)?.escaped_text.as_str();
                    let name_id = self.ctx.arena.get(ns.name)?;
                    let name_text = self
                        .ctx
                        .arena
                        .get_identifier(name_id)?
                        .escaped_text
                        .as_str();
                    Some(format!("{ns_text}:{name_text}"))
                });
        }

        None
    }

    pub(in crate::checkers_domain::jsx) fn instantiate_jsx_function_shape_with_substitution(
        &self,
        func: &tsz_solver::FunctionShape,
        substitution: &crate::query_boundaries::common::TypeSubstitution,
    ) -> tsz_solver::FunctionShape {
        let mut full_substitution = substitution.clone();
        for type_param in &func.type_params {
            if full_substitution.get(type_param.name).is_none() {
                let preserved_type_param = self.ctx.types.factory().type_param(*type_param);
                full_substitution.insert(type_param.name, preserved_type_param);
            }
        }
        tsz_solver::FunctionShape {
            params: func
                .params
                .iter()
                .map(|param| tsz_solver::ParamInfo {
                    name: param.name,
                    type_id: crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        param.type_id,
                        &full_substitution,
                    ),
                    optional: param.optional,
                    rest: param.rest,
                })
                .collect(),
            return_type: crate::query_boundaries::common::instantiate_type(
                self.ctx.types,
                func.return_type,
                &full_substitution,
            ),
            this_type: func.this_type.map(|this_type| {
                crate::query_boundaries::common::instantiate_type(
                    self.ctx.types,
                    this_type,
                    &full_substitution,
                )
            }),
            type_params: vec![],
            type_predicate: func.type_predicate.as_ref().map(|predicate| {
                tsz_solver::TypePredicate {
                    asserts: predicate.asserts,
                    target: predicate.target,
                    type_id: predicate.type_id.map(|tid| {
                        crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            tid,
                            &full_substitution,
                        )
                    }),
                    parameter_index: predicate.parameter_index,
                }
            }),
            is_constructor: func.is_constructor,
            is_method: func.is_method,
        }
    }

    pub(in crate::checkers_domain::jsx) fn infer_jsx_generic_component_props_type(
        &mut self,
        attributes_idx: NodeIndex,
        component_type: TypeId,
        request: &crate::context::TypingRequest,
    ) -> Option<TypeId> {
        use crate::computation::call_inference::should_preserve_contextual_application_shape;

        let opening_idx = self.ctx.arena.get_extended(attributes_idx)?.parent;
        let function_shape = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            component_type,
        )
        .filter(|shape| !shape.type_params.is_empty() && !shape.params.is_empty())
        .or_else(|| {
            self.infer_jsx_generic_class_component_signature(opening_idx, component_type)
        })?;
        if function_shape.type_params.is_empty() || function_shape.params.is_empty() {
            return None;
        }

        let children_prop_name = self.get_jsx_children_prop_name();
        let provided_attrs = self.collect_jsx_union_resolution_attrs(attributes_idx)?;

        // Two-pass inference for JSX generics, mirroring the regular call path
        // in call.rs.  Function-valued attrs (callbacks with untyped params)
        // return None from collect_jsx_union_resolution_attrs; they need
        // contextual typing from inferred type params.
        let mut concrete_attrs: Vec<(String, TypeId)> = Vec::new();
        let mut has_function_valued_attrs = false;
        for (name, ty) in &provided_attrs {
            match ty {
                Some(ty) => {
                    if name != &children_prop_name {
                        concrete_attrs.push((name.clone(), *ty));
                    }
                }
                None => {
                    if name != &children_prop_name {
                        has_function_valued_attrs = true;
                    }
                }
            }
        }

        // Function-valued children should be checked against the inferred children
        // context in a staged pass, not treated as concrete attrs during Round 1.
        let has_function_valued_children = provided_attrs
            .iter()
            .any(|(name, ty)| name == &children_prop_name && ty.is_none())
            || self
                .get_jsx_body_child_nodes(attributes_idx)
                .is_some_and(|children| {
                    children.iter().copied().any(|child_idx| {
                        self.ctx.arena.get(child_idx).is_some_and(|child| {
                            child.kind == syntax_kind_ext::JSX_EXPRESSION
                                && self
                                    .ctx
                                    .arena
                                    .get_jsx_expression(child)
                                    .and_then(|expr| expr.expression.into_option())
                                    .and_then(|expr_idx| self.ctx.arena.get(expr_idx))
                                    .is_some_and(|expr| {
                                        matches!(
                                            expr.kind,
                                            syntax_kind_ext::ARROW_FUNCTION
                                                | syntax_kind_ext::FUNCTION_EXPRESSION
                                        )
                                    })
                        })
                    })
                });

        // === Round 1: Infer type params from concrete attrs only ===
        let mut substitution = if concrete_attrs.is_empty() {
            crate::query_boundaries::common::TypeSubstitution::new()
        } else {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&concrete_attrs);
            let env = self.ctx.type_env.borrow();
            crate::query_boundaries::checkers::call::compute_contextual_types_with_context(
                self.ctx.types,
                &self.ctx,
                &env,
                &function_shape,
                &[attrs_type],
                None,
            )
        };

        // === Round 2: Contextually type function-valued attrs ===
        // Use the Round 1 substitution to provide contextual types for
        // callback attrs. Their return types can then refine inference.
        //
        // Children callbacks run in a staged pass after non-children callbacks
        // (for example selector props) have had a chance to refine generic
        // parameters. Otherwise defaulted type params can freeze the children
        // context too early and produce the wrong TS2322/TS7006 pair.
        //
        // When substitution is empty (all attrs are function-valued, no
        // defaults/constraints), we still enter this block to bootstrap
        // inference from function-valued attrs whose contextual parameter
        // types are concrete (don't depend on type params being inferred).
        // This enables intra-expression inference in JSX, e.g.:
        //   <Foo a={() => 10} b={(arg) => arg.toString()} />
        // where `a: (x: string) => T` has concrete param types and can
        // be typed first to infer T, then `b: (arg: T) => void` is typed
        // with the inferred T.
        if has_function_valued_attrs || has_function_valued_children {
            let mut all_attrs = concrete_attrs;

            if has_function_valued_attrs {
                let r1_instantiated = self.instantiate_jsx_function_shape_with_substitution(
                    &function_shape,
                    &substitution,
                );
                if let Some(r1_props_param) = r1_instantiated.params.first() {
                    let r1_props_type = r1_props_param.type_id;
                    let r1_props_type = self.resolve_type_for_property_access(r1_props_type);
                    let r1_props_type = self.evaluate_type_with_env(r1_props_type);
                    let typed_attr_start = all_attrs.len();
                    let unresolved_type_params: rustc_hash::FxHashSet<_> = function_shape
                        .type_params
                        .iter()
                        .filter(|tp| substitution.get(tp.name).is_none())
                        .map(|tp| tp.name)
                        .collect();

                    self.collect_function_valued_jsx_attr_types(
                        attributes_idx,
                        r1_props_type,
                        &children_prop_name,
                        request,
                        Some(&unresolved_type_params),
                        &mut all_attrs,
                    );
                    self.refine_jsx_generic_substitution_from_typed_attrs(
                        r1_props_type,
                        &function_shape,
                        &all_attrs[typed_attr_start..],
                        &mut substitution,
                    );
                }
            }

            if !all_attrs.is_empty() {
                let full_attrs_type = self.build_jsx_provided_attrs_object_type(&all_attrs);
                let round2_sub = {
                    let env = self.ctx.type_env.borrow();
                    crate::query_boundaries::checkers::call::compute_contextual_types_with_context(
                        self.ctx.types,
                        &self.ctx,
                        &env,
                        &function_shape,
                        &[full_attrs_type],
                        None,
                    )
                };
                for (&name, &ty) in round2_sub.map() {
                    substitution.insert(name, ty);
                }
            }

            if has_function_valued_attrs {
                let r2_instantiated = self.instantiate_jsx_function_shape_with_substitution(
                    &function_shape,
                    &substitution,
                );
                if let Some(r2_props_param) = r2_instantiated.params.first() {
                    let r2_props_type = r2_props_param.type_id;
                    let r2_props_type = self.resolve_type_for_property_access(r2_props_type);
                    let r2_props_type = self.evaluate_type_with_env(r2_props_type);
                    let typed_attr_start = all_attrs.len();
                    let unresolved_type_params: rustc_hash::FxHashSet<_> = function_shape
                        .type_params
                        .iter()
                        .filter(|tp| substitution.get(tp.name).is_none())
                        .map(|tp| tp.name)
                        .collect();

                    self.collect_function_valued_jsx_attr_types(
                        attributes_idx,
                        r2_props_type,
                        &children_prop_name,
                        request,
                        (!unresolved_type_params.is_empty()).then_some(&unresolved_type_params),
                        &mut all_attrs,
                    );
                    if all_attrs.len() > typed_attr_start {
                        self.refine_jsx_generic_substitution_from_typed_attrs(
                            r2_props_type,
                            &function_shape,
                            &all_attrs[typed_attr_start..],
                            &mut substitution,
                        );

                        let full_attrs_type = self.build_jsx_provided_attrs_object_type(&all_attrs);
                        let round3_sub = {
                            let env = self.ctx.type_env.borrow();
                            crate::query_boundaries::checkers::call::compute_contextual_types_with_context(
                                self.ctx.types,
                                &self.ctx,
                                &env,
                                &function_shape,
                                &[full_attrs_type],
                                None,
                            )
                        };
                        for (&name, &ty) in round3_sub.map() {
                            substitution.insert(name, ty);
                        }
                    }
                }
            }

            for tp in &function_shape.type_params {
                if substitution.get(tp.name).is_none()
                    && let Some(replacement) = tp.default.or(tp.constraint)
                {
                    substitution.insert(tp.name, replacement);
                }
            }

            if has_function_valued_children {
                let r2_instantiated = self.instantiate_jsx_function_shape_with_substitution(
                    &function_shape,
                    &substitution,
                );
                if let Some(r2_props_param) = r2_instantiated.params.first() {
                    let r2_props_type = r2_props_param.type_id;
                    let r2_props_type = self.resolve_type_for_property_access(r2_props_type);
                    let r2_props_type = self.evaluate_type_with_env(r2_props_type);
                    let typed_attr_start = all_attrs.len();

                    self.collect_function_valued_jsx_children_types(
                        attributes_idx,
                        r2_props_type,
                        &children_prop_name,
                        request,
                        &mut all_attrs,
                    );
                    self.refine_jsx_generic_substitution_from_typed_attrs(
                        r2_props_type,
                        &function_shape,
                        &all_attrs[typed_attr_start..],
                        &mut substitution,
                    );

                    if !all_attrs.is_empty() {
                        let full_attrs_type = self.build_jsx_provided_attrs_object_type(&all_attrs);
                        let round3_sub = {
                            let env = self.ctx.type_env.borrow();
                            crate::query_boundaries::checkers::call::compute_contextual_types_with_context(
                                self.ctx.types,
                                &self.ctx,
                                &env,
                                &function_shape,
                                &[full_attrs_type],
                                None,
                            )
                        };
                        for (&name, &ty) in round3_sub.map() {
                            substitution.insert(name, ty);
                        }
                    }
                }
            }
        }

        for tp in &function_shape.type_params {
            if substitution.get(tp.name).is_none()
                && let Some(replacement) = tp.default.or(tp.constraint)
            {
                substitution.insert(tp.name, replacement);
            }
        }

        if substitution.is_empty() {
            return None;
        }

        let instantiated =
            self.instantiate_jsx_function_shape_with_substitution(&function_shape, &substitution);
        let props_type = instantiated.params.first()?.type_id;
        let props_type =
            if crate::query_boundaries::common::is_union_type(self.ctx.types, props_type)
                || should_preserve_contextual_application_shape(self.ctx.types, props_type)
            {
                props_type
            } else {
                let props_type = self.resolve_type_for_property_access(props_type);
                self.evaluate_type_with_env(props_type)
            };
        let props_type = self.apply_jsx_library_managed_attributes(component_type, props_type);
        let props_type = self.narrow_jsx_props_union_from_attributes(attributes_idx, props_type);
        if props_type == TypeId::ANY || props_type == TypeId::UNKNOWN || props_type == TypeId::ERROR
        {
            return None;
        }
        Some(props_type)
    }

    pub(in crate::checkers_domain::jsx) fn refine_jsx_generic_substitution_from_typed_attrs(
        &mut self,
        props_type: TypeId,
        function_shape: &tsz_solver::FunctionShape,
        typed_attrs: &[(String, TypeId)],
        substitution: &mut crate::query_boundaries::common::TypeSubstitution,
    ) {
        use crate::query_boundaries::common::PropertyAccessResult;
        use rustc_hash::FxHashSet;

        let tracked_type_params: FxHashSet<_> = function_shape
            .type_params
            .iter()
            .map(|tp| tp.name)
            .collect();

        for (attr_name, attr_type) in typed_attrs {
            let expected_type = match self.resolve_property_access_with_env(props_type, attr_name) {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                _ => continue,
            };
            let expected_type = self.refine_jsx_callable_contextual_type(expected_type);
            let synthetic_shape = tsz_solver::FunctionShape {
                type_params: function_shape.type_params.clone(),
                params: vec![tsz_solver::ParamInfo {
                    name: Some(self.ctx.types.intern_string(attr_name)),
                    type_id: expected_type,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            };
            let attr_sub = {
                let env = self.ctx.type_env.borrow();
                crate::query_boundaries::checkers::call::compute_contextual_types_with_context(
                    self.ctx.types,
                    &self.ctx,
                    &env,
                    &synthetic_shape,
                    &[*attr_type],
                    None,
                )
            };
            for (&name, &ty) in attr_sub.map() {
                substitution.insert(name, ty);
            }

            if let (Some(source_sig), Some(target_sig)) = (
                crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    *attr_type,
                ),
                crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    expected_type,
                ),
            ) {
                let mut visited = FxHashSet::default();
                self.collect_return_context_substitution(
                    source_sig.return_type,
                    target_sig.return_type,
                    &tracked_type_params,
                    substitution,
                    &mut visited,
                );
            }
        }
    }

    /// Contextually type function-valued JSX attributes using the expected
    /// props type from Round 1 inference.
    pub(in crate::checkers_domain::jsx) fn collect_function_valued_jsx_attr_types(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        children_prop_name: &str,
        request: &crate::context::TypingRequest,
        unresolved_type_params: Option<&rustc_hash::FxHashSet<tsz_common::interner::Atom>>,
        out: &mut Vec<(String, TypeId)>,
    ) {
        use crate::query_boundaries::common::PropertyAccessResult;

        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };
            if attr_node.kind != syntax_kind_ext::JSX_ATTRIBUTE {
                continue;
            }
            let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                continue;
            };
            let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                continue;
            };
            if attr_name == "key" || attr_name == "ref" || attr_name == children_prop_name {
                continue;
            }
            if out.iter().any(|(name, _)| name == &attr_name) {
                continue;
            }

            // Only process function-valued attributes.
            let Some(init_node) = self.ctx.arena.get(attr_data.initializer) else {
                continue;
            };
            let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                self.ctx
                    .arena
                    .get_jsx_expression(init_node)
                    .map(|expr| expr.expression)
                    .unwrap_or(attr_data.initializer)
            } else {
                attr_data.initializer
            };
            let Some(value_node) = self.ctx.arena.get(value_idx) else {
                continue;
            };
            if value_node.kind != syntax_kind_ext::ARROW_FUNCTION
                && value_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            {
                continue;
            }

            // Look up the expected type for this property in the Round 1 props.
            let expected_type = match self.resolve_property_access_with_env(props_type, &attr_name)
            {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                _ => continue,
            };

            let contextual_type = self.refine_jsx_callable_contextual_type(expected_type);
            if let Some(names) = unresolved_type_params.filter(|names| !names.is_empty()) {
                let should_defer =
                    crate::query_boundaries::checkers::call::get_contextual_signature(
                        self.ctx.types,
                        contextual_type,
                    )
                    .is_some_and(|signature| {
                        signature.params.iter().any(|param| {
                            crate::query_boundaries::common::references_any_type_param_named(
                                self.ctx.types,
                                param.type_id,
                                names,
                            )
                        })
                    });
                if should_defer {
                    // The callback references unresolved type parameters. Skip typing it
                    // now - it will be properly typed in a later round once the type
                    // parameters are resolved. Typing it here with unresolved params
                    // would emit false diagnostics like "Property X does not exist on type T".
                    continue;
                }
            }
            // Invalidate cached symbol types before re-typing with a new contextual type.
            // This ensures parameters get the updated inferred type (e.g., `arg: number`)
            // instead of the stale type from an earlier pass (e.g., `arg: T`).
            self.invalidate_function_like_for_contextual_retry(value_idx);
            let typed = self.compute_type_of_node_with_request(
                value_idx,
                &(*request).contextual(contextual_type),
            );
            out.push((attr_name, typed));
        }
    }

    pub(in crate::checkers_domain::jsx) fn collect_function_valued_jsx_children_types(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        children_prop_name: &str,
        request: &crate::context::TypingRequest,
        out: &mut Vec<(String, TypeId)>,
    ) {
        use crate::query_boundaries::common::PropertyAccessResult;

        let expected_children_type =
            match self.resolve_property_access_with_env(props_type, children_prop_name) {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                _ => return,
            };
        let contextual_type = self.refine_jsx_callable_contextual_type(expected_children_type);
        let child_request = (*request).contextual(contextual_type);

        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };
            if attr_node.kind != syntax_kind_ext::JSX_ATTRIBUTE {
                continue;
            }
            let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                continue;
            };
            let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                continue;
            };
            if attr_name != children_prop_name {
                continue;
            }

            let Some(init_node) = self.ctx.arena.get(attr_data.initializer) else {
                return;
            };
            let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                self.ctx
                    .arena
                    .get_jsx_expression(init_node)
                    .map(|expr| expr.expression)
                    .unwrap_or(attr_data.initializer)
            } else {
                attr_data.initializer
            };
            let typed = self.compute_type_of_node_with_request(value_idx, &child_request);
            out.push((children_prop_name.to_string(), typed));
            return;
        }

        let Some(child_nodes) = self.get_jsx_body_child_nodes(attributes_idx) else {
            return;
        };

        let mut child_types = Vec::new();
        let mut has_spread_child = false;
        for child_idx in child_nodes {
            let Some(child_node) = self.ctx.arena.get(child_idx) else {
                continue;
            };
            let child_type = if child_node.kind == syntax_kind_ext::JSX_EXPRESSION
                && let Some(expr_data) = self.ctx.arena.get_jsx_expression(child_node)
                && expr_data.dot_dot_dot_token
            {
                has_spread_child = true;
                let spread_type =
                    self.get_type_of_node_with_request(expr_data.expression, &child_request);
                self.normalize_jsx_spread_child_type(child_idx, spread_type)
            } else if child_node.kind == syntax_kind_ext::JSX_EXPRESSION
                && let Some(expr_data) = self.ctx.arena.get_jsx_expression(child_node)
                && expr_data.expression.is_some()
                && self
                    .ctx
                    .arena
                    .get(expr_data.expression)
                    .is_some_and(|expr| {
                        matches!(
                            expr.kind,
                            syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
                        )
                    })
            {
                self.ctx
                    .implicit_any_contextual_closures
                    .insert(expr_data.expression);
                self.ctx
                    .implicit_any_checked_closures
                    .insert(expr_data.expression);
                self.invalidate_function_like_for_contextual_retry(expr_data.expression);
                self.get_type_of_node_with_request(expr_data.expression, &child_request)
            } else {
                self.get_type_of_node_with_request(child_idx, &child_request)
            };
            child_types.push(child_type);
        }

        if child_types.is_empty() {
            return;
        }

        let synthesized_type = if child_types.len() == 1 && !has_spread_child {
            child_types[0]
        } else {
            let element_type = self.ctx.types.factory().union(child_types);
            self.ctx.types.factory().array(element_type)
        };
        out.push((children_prop_name.to_string(), synthesized_type));
    }

    pub(in crate::checkers_domain::jsx) fn recover_jsx_component_props_type(
        &mut self,
        attributes_idx: NodeIndex,
        component_type: TypeId,
        element_idx: Option<NodeIndex>,
        request: &crate::context::TypingRequest,
    ) -> Option<(TypeId, bool)> {
        let normalized_component_type =
            self.normalize_jsx_component_type_for_resolution(component_type);
        // Only pass element_idx (which authorizes TS2607 emission) when the
        // JSX usage actually supplies attributes that would need a props type.
        // `<Foo />` with no attributes shouldn't trip "missing 'props' property"
        // even if the class doesn't expose one, since nothing is being checked.
        let attributes_have_content = self
            .ctx
            .arena
            .get(attributes_idx)
            .and_then(|n| self.ctx.arena.get_jsx_attributes(n))
            .is_some_and(|a| !a.properties.nodes.is_empty());
        let element_idx_for_emit = if attributes_have_content {
            element_idx
        } else {
            None
        };
        if let Some((props_type, raw_has_type_params)) =
            self.get_jsx_props_type_for_component(component_type, element_idx_for_emit)
        {
            if raw_has_type_params
                && let Some(inferred_props) = self
                    .infer_jsx_generic_component_props_type(
                        attributes_idx,
                        normalized_component_type,
                        request,
                    )
                    .or_else(|| {
                        self.get_default_instantiated_generic_class_props_type(
                            normalized_component_type,
                        )
                    })
                    .or_else(|| {
                        self.get_default_instantiated_generic_sfc_props_type(
                            normalized_component_type,
                        )
                    })
            {
                return Some((inferred_props, true));
            }

            return Some((props_type, raw_has_type_params));
        }

        let has_function_valued_jsx_attrs = self
            .collect_jsx_union_resolution_attrs(attributes_idx)
            .is_some_and(|attrs| {
                let children_prop_name = self.get_jsx_children_prop_name();
                attrs
                    .into_iter()
                    .any(|(name, ty)| name != children_prop_name && ty.is_none())
            });
        let is_class_like_component = self
            .ctx
            .arena
            .get_extended(attributes_idx)
            .map(|ext| ext.parent)
            .and_then(|opening_idx| {
                self.infer_jsx_generic_class_component_signature(
                    opening_idx,
                    normalized_component_type,
                )
            })
            .is_some();

        let fallback_props = if is_class_like_component && !has_function_valued_jsx_attrs {
            self.get_default_instantiated_generic_class_props_type(normalized_component_type)
                .or_else(|| {
                    self.get_default_instantiated_generic_sfc_props_type(normalized_component_type)
                })
        } else {
            self.infer_jsx_generic_component_props_type(
                attributes_idx,
                normalized_component_type,
                request,
            )
            .or_else(|| {
                self.get_default_instantiated_generic_class_props_type(normalized_component_type)
            })
            .or_else(|| {
                self.get_default_instantiated_generic_sfc_props_type(normalized_component_type)
            })
        };

        fallback_props.map(|props_type| (props_type, false))
    }
}
