//! JSX orchestration: main entry points for JSX element type resolution,
//! namespace/intrinsic lookups, children contextual typing, and attribute
//! name extraction.

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn normalize_jsx_contextual_callable_member(&mut self, type_id: TypeId) -> TypeId {
        let type_id =
            crate::query_boundaries::common::unwrap_readonly_or_noinfer(self.ctx.types, type_id)
                .unwrap_or(type_id);
        let Some(shape) = tsz_solver::type_queries::get_function_shape(self.ctx.types, type_id)
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

    pub(super) fn refine_jsx_callable_contextual_type(&mut self, type_id: TypeId) -> TypeId {
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
            let is_callable = tsz_solver::type_queries::get_function_shape(self.ctx.types, member)
                .is_some_and(|shape| !shape.is_constructor)
                || tsz_solver::type_queries::get_call_signatures(self.ctx.types, member)
                    .is_some_and(|sigs| !sigs.is_empty());
            if is_callable {
                callable_members.push(self.normalize_jsx_contextual_callable_member(member));
            }
        }

        match callable_members.len() {
            0 => resolved,
            1 => callable_members[0],
            _ => self.ctx.types.factory().union(callable_members),
        }
    }

    pub(super) fn select_jsx_single_children_target_type(&mut self, type_id: TypeId) -> TypeId {
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

    pub(super) fn select_jsx_multiple_children_target_type(&mut self, type_id: TypeId) -> TypeId {
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

    pub(super) fn jsx_multiple_children_element_type(&mut self, type_id: TypeId) -> Option<TypeId> {
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
                }
            }
            return match element_types.as_slice() {
                [] => None,
                [element_type] => Some(*element_type),
                _ => Some(self.ctx.types.factory().union(element_types)),
            };
        }

        tsz_solver::type_queries::get_object_shape(self.ctx.types, resolved)
            .and_then(|shape| shape.number_index.as_ref().map(|index| index.value_type))
            .map(|value_type| self.refine_jsx_callable_contextual_type(value_type))
    }

    fn jsx_children_contextual_type_for_body_shape(
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

    fn file_has_same_line_adjacent_jsx_recovery_pattern(&self) -> bool {
        // Previously this used text-based heuristics to detect adjacent JSX
        // recovery patterns (e.g., `/><` or `></`), but those patterns also
        // match normal JSX syntax (e.g., `</span><div>` or `</span>`).
        // Parser recovery situations are already detected by `has_parse_errors`
        // and `all_parse_error_positions`, so this heuristic is no longer needed.
        false
    }

    pub(super) fn normalize_jsx_component_type_for_resolution(
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

    fn get_jsx_component_metadata_type(
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

        if (symbol.flags & tsz_binder::symbol_flags::CLASS) == 0
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

        let mut decls = Vec::new();
        if symbol.value_declaration.is_some() {
            decls.push(symbol.value_declaration);
        }
        decls.extend(symbol.declarations.iter().copied());

        for mut decl_idx in decls {
            let Some(mut decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(parent) = self.ctx.arena.get_extended(decl_idx).map(|ext| ext.parent)
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

    pub(super) fn get_jsx_specific_string_literal_component_tag_name(
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

    pub(super) fn get_jsx_single_string_literal_tag_name(&self, type_id: TypeId) -> Option<String> {
        if let Some(name) =
            tsz_solver::type_queries::get_string_literal_value(self.ctx.types, type_id)
        {
            return Some(self.ctx.types.resolve_atom(name).as_str().to_string());
        }

        let members = crate::query_boundaries::common::union_members(self.ctx.types, type_id)?;
        let mut literal_name = None;
        for member in members {
            let name = tsz_solver::type_queries::get_string_literal_value(self.ctx.types, member)?;
            match literal_name {
                Some(existing) if existing != name => return None,
                Some(_) => {}
                None => literal_name = Some(name),
            }
        }

        literal_name.map(|name| self.ctx.types.resolve_atom(name).as_str().to_string())
    }

    fn get_jsx_intrinsic_props_from_template_literal_index_signatures(
        &mut self,
        tag: &str,
    ) -> Option<TypeId> {
        let intrinsic_elements_sym_id = self.get_intrinsic_elements_symbol_id()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(intrinsic_elements_sym_id, &lib_binders)?;
        let mut declarations = Vec::new();
        if symbol.value_declaration.is_some() {
            declarations.push(symbol.value_declaration);
        }
        declarations.extend(symbol.declarations.iter().copied());

        let tag_literal = tsz_solver::type_queries::create_string_literal_type(self.ctx.types, tag);
        let mut candidates = Vec::new();

        for mut decl_idx in declarations {
            let Some(mut decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(parent) = self.ctx.arena.get_extended(decl_idx).map(|ext| ext.parent)
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
                if !tsz_solver::visitor::is_template_literal_type(self.ctx.types, key_type)
                    || !self.is_assignable_to(tag_literal, key_type)
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

    pub(super) fn get_jsx_intrinsic_props_for_tag(
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

    fn get_jsx_intrinsic_tag_name(&self, tag_name_idx: NodeIndex) -> Option<String> {
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

    pub(super) fn instantiate_jsx_function_shape_with_substitution(
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

    fn infer_jsx_generic_component_props_type(
        &mut self,
        attributes_idx: NodeIndex,
        component_type: TypeId,
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

        // Function-valued body children should be checked against the inferred children context,
        // not feed back into type-parameter inference.
        let has_function_valued_children = false;

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

        // Fill unresolved type params with defaults/constraints.
        for tp in &function_shape.type_params {
            if substitution.get(tp.name).is_none()
                && let Some(replacement) = tp.default.or(tp.constraint)
            {
                substitution.insert(tp.name, replacement);
            }
        }

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
                for tp in &function_shape.type_params {
                    if substitution.get(tp.name).is_none()
                        && let Some(replacement) = tp.default.or(tp.constraint)
                    {
                        substitution.insert(tp.name, replacement);
                    }
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

        if substitution.is_empty() {
            return None;
        }

        let instantiated =
            self.instantiate_jsx_function_shape_with_substitution(&function_shape, &substitution);
        let props_type = instantiated.params.first()?.type_id;
        let props_type = if tsz_solver::is_union_type(self.ctx.types, props_type)
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

    fn refine_jsx_generic_substitution_from_typed_attrs(
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
    fn collect_function_valued_jsx_attr_types(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        children_prop_name: &str,
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
                    continue;
                }
            }
            let typed = self.compute_type_of_node_with_request(
                value_idx,
                &crate::context::TypingRequest::NONE
                    .read()
                    .assertion()
                    .contextual(contextual_type),
            );
            out.push((attr_name, typed));
        }
    }

    fn collect_function_valued_jsx_children_types(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        children_prop_name: &str,
        out: &mut Vec<(String, TypeId)>,
    ) {
        use crate::query_boundaries::common::PropertyAccessResult;

        let expected_children_type =
            match self.resolve_property_access_with_env(props_type, children_prop_name) {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                _ => return,
            };
        let contextual_type = self.refine_jsx_callable_contextual_type(expected_children_type);
        let child_request = crate::context::TypingRequest::NONE
            .read()
            .assertion()
            .contextual(contextual_type);

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

    fn recover_jsx_component_props_type(
        &mut self,
        attributes_idx: NodeIndex,
        component_type: TypeId,
        element_idx: Option<NodeIndex>,
    ) -> Option<(TypeId, bool)> {
        let normalized_component_type =
            self.normalize_jsx_component_type_for_resolution(component_type);
        if let Some((props_type, raw_has_type_params)) =
            self.get_jsx_props_type_for_component(component_type, element_idx)
        {
            if raw_has_type_params
                && tsz_solver::type_queries::get_construct_signatures(
                    self.ctx.types,
                    normalized_component_type,
                )
                .is_none_or(|sigs| sigs.is_empty())
                && let Some(inferred_props) = self
                    .infer_jsx_generic_component_props_type(
                        attributes_idx,
                        normalized_component_type,
                    )
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
            self.get_default_instantiated_generic_sfc_props_type(normalized_component_type)
        } else {
            self.infer_jsx_generic_component_props_type(attributes_idx, normalized_component_type)
                .or_else(|| {
                    self.get_default_instantiated_generic_sfc_props_type(normalized_component_type)
                })
        };

        fallback_props.map(|props_type| (props_type, false))
    }

    pub(super) fn infer_jsx_generic_class_component_signature(
        &mut self,
        _element_idx: NodeIndex,
        component_type: TypeId,
    ) -> Option<tsz_solver::FunctionShape> {
        let call_sig =
            tsz_solver::type_queries::get_construct_signatures(self.ctx.types, component_type)?
                .first()?
                .clone();
        let mut function_shape = tsz_solver::FunctionShape {
            type_params: call_sig.type_params,
            params: call_sig.params,
            this_type: call_sig.this_type,
            return_type: call_sig.return_type,
            type_predicate: call_sig.type_predicate,
            is_constructor: true,
            is_method: call_sig.is_method,
        };
        if function_shape.type_params.is_empty() {
            return None;
        }

        if function_shape.params.is_empty() {
            use crate::query_boundaries::common::PropertyAccessResult;

            let evaluated_return_type = self.evaluate_type_with_env(function_shape.return_type);
            let synthesized_param_type = match self
                .get_element_attributes_property_name_with_check(None)
            {
                None => {
                    match self.resolve_property_access_with_env(function_shape.return_type, "props")
                    {
                        PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                        _ => match self
                            .resolve_property_access_with_env(evaluated_return_type, "props")
                        {
                            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                            _ => None,
                        },
                    }
                }
                Some(name) if name.is_empty() => Some(function_shape.return_type),
                Some(name) => {
                    match self.resolve_property_access_with_env(function_shape.return_type, &name) {
                        PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                        _ => match self
                            .resolve_property_access_with_env(evaluated_return_type, &name)
                        {
                            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                            _ => None,
                        },
                    }
                }
            }
            .filter(|type_id| !matches!(*type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN));

            if let Some(type_id) = synthesized_param_type {
                let props_name = self.ctx.types.intern_string("props");
                function_shape
                    .params
                    .push(tsz_solver::ParamInfo::required(props_name, type_id));
            }
        }

        if function_shape.params.is_empty() {
            return None;
        }

        Some(function_shape)
    }

    /// Get the type of a JSX opening element (Rule #36: case-sensitive tag lookup).
    #[allow(dead_code)]
    pub(crate) fn get_type_of_jsx_opening_element(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_jsx_opening_element_with_children(idx, &TypingRequest::NONE, None)
    }

    pub(crate) fn get_type_of_jsx_opening_element_with_children(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
        children_ctx: Option<crate::checkers_domain::JsxChildrenContext>,
    ) -> TypeId {
        self.check_jsx_factory_in_scope(idx);
        self.check_jsx_import_source(idx);

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ANY;
        };
        let Some(jsx_opening) = self.ctx.arena.get_jsx_opening(node) else {
            return TypeId::ANY;
        };
        let tag_name_idx = jsx_opening.tag_name;
        let Some(tag_name_node) = self.ctx.arena.get(tag_name_idx) else {
            return TypeId::ANY;
        };
        // Namespaced tags (e.g., `svg:path`) are always intrinsic.
        let (tag_name, namespaced_tag_owned, is_intrinsic) = if tag_name_node.kind
            == tsz_scanner::SyntaxKind::Identifier as u16
        {
            let name = self
                .ctx
                .arena
                .get_identifier(tag_name_node)
                .map(|id| id.escaped_text.as_str());
            let intrinsic = name
                .as_ref()
                .is_some_and(|n| n.chars().next().is_some_and(|c| c.is_ascii_lowercase()));
            (name, None::<String>, intrinsic)
        } else if tag_name_node.kind == syntax_kind_ext::JSX_NAMESPACED_NAME {
            // Namespaced tags like `svg:path` → always intrinsic.
            // Build "namespace:name" string for IntrinsicElements lookup.
            // If the namespace part starts with uppercase (e.g., `<A:foo>`),
            // emit TS2639: React components cannot include JSX namespace names.
            let ns_str = self
                    .ctx
                    .arena
                    .get_jsx_namespaced_name(tag_name_node)
                    .and_then(|ns| {
                        let ns_id = self.ctx.arena.get(ns.namespace)?;
                        let ns_text = self.ctx.arena.get_identifier(ns_id)?.escaped_text.as_str();
                        // TS2639: React components (uppercase first char) cannot use
                        // namespace names. Only in React-based JSX modes.
                        if ns_text
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_uppercase())
                        {
                            use tsz_common::checker_options::JsxMode;
                            let jsx_mode = self.ctx.compiler_options.jsx_mode;
                            if matches!(jsx_mode, JsxMode::React | JsxMode::ReactJsx | JsxMode::ReactJsxDev) {
                                self.error_at_node(
                                    tag_name_idx,
                                    crate::diagnostics::diagnostic_messages::REACT_COMPONENTS_CANNOT_INCLUDE_JSX_NAMESPACE_NAMES,
                                    crate::diagnostics::diagnostic_codes::REACT_COMPONENTS_CANNOT_INCLUDE_JSX_NAMESPACE_NAMES,
                                );
                            }
                        }
                        let name_id = self.ctx.arena.get(ns.name)?;
                        let name_text = self
                            .ctx
                            .arena
                            .get_identifier(name_id)?
                            .escaped_text
                            .as_str();
                        Some(format!("{ns_text}:{name_text}"))
                    });
            (None, ns_str, true)
        } else {
            // Property access expression (e.g., React.Component)
            (None, None, false)
        };
        // Unify: for namespaced tags, use the owned string; for simple tags, use the borrowed &str.
        let effective_tag: Option<&str> = tag_name.or(namespaced_tag_owned.as_deref());

        if is_intrinsic {
            let ie_type = self.get_intrinsic_elements_type();
            // Intrinsic elements: look up JSX.IntrinsicElements[tagName]
            if let Some(tag) = effective_tag
                && ie_type.is_some()
            {
                let evaluated_props = self
                    .get_jsx_intrinsic_props_for_tag(idx, tag, true)
                    .unwrap_or(TypeId::ANY);

                // Check JSX attributes against the resolved props type.
                // For intrinsic elements, the display target is just the props type
                // (tsc doesn't wrap intrinsic element props in IntrinsicAttributes).
                let display_target = self.build_jsx_display_target(evaluated_props, None);
                self.check_jsx_attributes_against_props(
                    jsx_opening.attributes,
                    evaluated_props,
                    jsx_opening.tag_name,
                    None,
                    None,
                    false, // intrinsic elements never have raw type params
                    display_target,
                    None,
                    request,
                    children_ctx,
                );

                // tsc types ALL JSX expressions (both intrinsic and component) as
                // JSX.Element. Returning IntrinsicElements["tag"] causes false TS2322
                // when the JSX expression is used in a context expecting JSX.Element
                // (e.g., as a return value or assigned to a variable of type JSX.Element).
                if let Some(jsx_sym_id) = self.get_jsx_namespace_type() {
                    let lib_binders = self.get_lib_binders();
                    if let Some(symbol) = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(jsx_sym_id, &lib_binders)
                        && let Some(exports) = symbol.exports.as_ref()
                        && let Some(element_sym_id) = exports.get("Element")
                    {
                        return self.type_reference_symbol_type(element_sym_id);
                    }
                }
                return TypeId::ANY;
            }
            // TS7026: JSX element implicitly has type 'any' because no interface 'JSX.IntrinsicElements' exists.
            // tsc emits this unconditionally (regardless of noImplicitAny) when JSX.IntrinsicElements is absent.
            // The word "implicitly" in the message refers to the missing JSX infrastructure, not the noImplicitAny flag.
            //
            // Suppression rules (matching tsc behaviour):
            // 1. ReactJsx/ReactJsxDev modes use jsxImportSource for element types; they do not rely on
            //    the global JSX.IntrinsicElements, so TS7026 must not fire.
            // 2. When @jsxImportSource pragma or jsxImportSource config is set, the JSX namespace
            //    comes from the import source module, not the global scope. TS7026 must not fire.
            // 3. When the file has parser-level errors (e.g. malformed JSX attributes → TS1145),
            //    tsc suppresses TS7026 to avoid double-reporting in error-recovery situations.
            use tsz_common::checker_options::JsxMode;
            let jsx_mode = self.ctx.compiler_options.jsx_mode;
            // Suppress TS7026 when the JSX runtime uses an import source:
            // - react-jsx/react-jsxdev modes always use import source
            // - jsxImportSource config option also indicates import source usage
            // Note: a @jsxImportSource pragma alone (without the config option) does NOT
            // suppress TS7026 in preserve mode — the pragma is only effective in react-jsx modes.
            let uses_import_source = jsx_mode == JsxMode::ReactJsx
                || jsx_mode == JsxMode::ReactJsxDev
                || !self.ctx.compiler_options.jsx_import_source.is_empty();
            let file_has_any_parse_diag =
                self.ctx.has_parse_errors || !self.ctx.all_parse_error_positions.is_empty();
            let recovered_adjacent_sibling =
                self.file_has_same_line_adjacent_jsx_recovery_pattern();
            if !uses_import_source && !file_has_any_parse_diag && !recovered_adjacent_sibling {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    idx,
                    diagnostic_codes::JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_NO_INTERFACE_JSX_EXISTS,
                    &["IntrinsicElements"],
                );
            }
            // Grammar check: TS17000 for empty expressions in JSX attributes.
            self.check_grammar_jsx_element(jsx_opening.attributes);

            // Even when IntrinsicElements is missing, evaluate attribute expressions
            // to trigger definite-assignment checks (TS2454) and other diagnostics.
            // tsc evaluates these expressions regardless of JSX infrastructure availability.
            if let Some(attrs_node) = self.ctx.arena.get(jsx_opening.attributes)
                && let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node)
            {
                for &attr_idx in &attrs.properties.nodes {
                    if let Some(attr_node) = self.ctx.arena.get(attr_idx) {
                        if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                            if let Some(spread_data) =
                                self.ctx.arena.get_jsx_spread_attribute(attr_node)
                            {
                                self.compute_type_of_node(spread_data.expression);
                            }
                        } else if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE
                            && let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node)
                            && !attr_data.initializer.is_none()
                        {
                            self.compute_type_of_node(attr_data.initializer);
                        }
                    }
                }
            }
            TypeId::ANY
        } else {
            // Component: resolve as variable expression
            // The tag name is a reference to a component (function or class)
            let component_type = self.compute_type_of_node(tag_name_idx);

            // If the JSX element has explicit type arguments (e.g., <Component<T>>),
            // create an Application type to properly instantiate the generic component.
            // This ensures that when we check overloaded SFCs, the signatures are
            // instantiated with the provided type arguments rather than constraints/defaults.
            let component_type = if let Some(ref type_args_nodes) = jsx_opening.type_arguments {
                let type_args: Vec<TypeId> = type_args_nodes
                    .nodes
                    .iter()
                    .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                    .collect();
                if !type_args.is_empty() {
                    self.ctx.types.application(component_type, type_args)
                } else {
                    component_type
                }
            } else {
                component_type
            };

            let component_metadata_type =
                self.get_jsx_component_metadata_type(tag_name_idx, component_type);
            let resolved_component_type =
                self.normalize_jsx_component_type_for_resolution(component_type);
            let specific_intrinsic_tag = self.get_jsx_specific_string_literal_component_tag_name(
                tag_name_idx,
                resolved_component_type,
            );
            let tried_specific_intrinsic_lookup =
                specific_intrinsic_tag.is_some() && self.get_intrinsic_elements_type().is_some();

            if let Some(tag) = specific_intrinsic_tag.as_deref()
                && let Some(props_type) = self.get_jsx_intrinsic_props_for_tag(idx, tag, true)
                && props_type != TypeId::ERROR
            {
                let display_target = self.build_jsx_display_target(props_type, None);
                self.check_jsx_attributes_against_props(
                    jsx_opening.attributes,
                    props_type,
                    jsx_opening.tag_name,
                    None,
                    None,
                    false,
                    display_target,
                    None,
                    request,
                    children_ctx,
                );
                if let Some(jsx_sym_id) = self.get_jsx_namespace_type() {
                    let lib_binders = self.get_lib_binders();
                    if let Some(symbol) = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(jsx_sym_id, &lib_binders)
                        && let Some(exports) = symbol.exports.as_ref()
                        && let Some(element_sym_id) = exports.get("Element")
                    {
                        return self.type_reference_symbol_type(element_sym_id);
                    }
                }
                return TypeId::ANY;
            }

            // If the resolved type is string-like or a keyof type (e.g., `keyof ReactHTML`),
            // treat it as an intrinsic element. tsc allows `<Tag>` where Tag has a string
            // type without emitting TS2604.
            if (self.is_jsx_string_tag_type(resolved_component_type)
                || tsz_solver::type_queries::is_keyof_type(self.ctx.types, resolved_component_type))
                && !tried_specific_intrinsic_lookup
            {
                self.check_grammar_jsx_element(jsx_opening.attributes);
                if let Some(jsx_sym_id) = self.get_jsx_namespace_type() {
                    let lib_binders = self.get_lib_binders();
                    if let Some(symbol) = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(jsx_sym_id, &lib_binders)
                        && let Some(exports) = symbol.exports.as_ref()
                        && let Some(element_sym_id) = exports.get("Element")
                    {
                        return self.type_reference_symbol_type(element_sym_id);
                    }
                }
                return TypeId::ANY;
            }

            let jsx_element_expr_type = self.get_jsx_element_type_for_check();
            let reported_factory_arity =
                self.check_jsx_sfc_factory_arity(resolved_component_type, tag_name_idx);
            let recovered_props = if reported_factory_arity {
                None
            } else {
                self.recover_jsx_component_props_type(
                    jsx_opening.attributes,
                    component_metadata_type,
                    Some(idx),
                )
            };
            let uses_jsx_overload_resolution = recovered_props.is_none()
                && (self.is_overloaded_sfc(resolved_component_type)
                    || self.has_multi_signature_overloads(resolved_component_type)
                    || self.has_multi_construct_overloads(resolved_component_type));

            // Extract props type from the component and check attributes.
            // TS2607/TS2608 are emitted within props extraction when applicable.
            // Build display target with IntrinsicAttributes intersection for TS2322 messages.
            if let Some((props_type, raw_has_type_params)) = recovered_props {
                // TS2786: component return type must be valid JSX element
                self.check_jsx_component_return_type(resolved_component_type, tag_name_idx);
                let props_type =
                    self.narrow_jsx_props_union_from_attributes(jsx_opening.attributes, props_type);
                let preferred_props_display =
                    self.get_jsx_component_props_display_text(tag_name_idx);
                let display_target = self.build_jsx_display_target_with_preferred_props(
                    props_type,
                    Some(resolved_component_type),
                    preferred_props_display.as_deref(),
                );
                self.check_jsx_attributes_against_props(
                    jsx_opening.attributes,
                    props_type,
                    jsx_opening.tag_name,
                    Some(component_metadata_type),
                    Some(component_type),
                    raw_has_type_params,
                    display_target,
                    preferred_props_display.as_deref(),
                    request,
                    children_ctx,
                );
            } else if uses_jsx_overload_resolution {
                // JSX overload resolution: try each call signature (including generic
                // ones) against the provided attributes. If no overload matches, emit
                // TS2769. The `has_multi_signature_overloads` fallback covers cases
                // where all overloads are generic and props extraction returned None.
                self.check_jsx_overloaded_sfc(
                    resolved_component_type,
                    jsx_opening.attributes,
                    jsx_opening.tag_name,
                    children_ctx,
                );
            } else {
                // TS2786: component return type must be valid JSX element
                self.check_jsx_component_return_type(resolved_component_type, tag_name_idx);

                // Grammar check: TS17000 for empty expressions in JSX attributes.
                self.check_grammar_jsx_element(jsx_opening.attributes);

                // TS2604: JSX element type does not have any construct or call signatures.
                // Emit when the component type is concrete but lacks call/construct signatures.
                self.check_jsx_element_has_signatures(resolved_component_type, tag_name_idx);

                // Even when we can't extract component props (e.g., no ElementAttributesProperty),
                // check IntrinsicAttributes / IntrinsicClassAttributes<T> for required
                // properties (e.g., required `key`/`ref`). tsc checks these independently
                // of component props extraction.
                self.check_jsx_intrinsic_attributes_only(
                    resolved_component_type,
                    jsx_opening.attributes,
                    jsx_opening.tag_name,
                );

                // For generic SFCs (e.g., `Component<T>(props: T)`), we can't infer
                // type arguments, but we CAN check that spread attributes satisfy
                // IntrinsicAttributes. tsc checks spreads against
                // `IntrinsicAttributes & inferred_props` and emits TS2322 when an
                // unconstrained type parameter doesn't satisfy IntrinsicAttributes.
                self.check_generic_sfc_spread_intrinsic_attrs(
                    resolved_component_type,
                    jsx_opening.attributes,
                    jsx_opening.tag_name,
                );

                // Evaluate attribute values to trigger nested JSX processing and
                // definite-assignment checks, even when props type is unknown.
                // For generic components, set ANY contextual type to prevent
                // false TS7006 on callback parameters in JSX attributes when we
                // cannot recover a concrete props shape.
                let gen_ctx = self.is_generic_jsx_component(resolved_component_type);
                let inferred_generic_props = if gen_ctx {
                    self.infer_jsx_generic_component_props_type(
                        jsx_opening.attributes,
                        resolved_component_type,
                    )
                    .or_else(|| {
                        self.get_default_instantiated_generic_sfc_props_type(
                            resolved_component_type,
                        )
                    })
                } else {
                    None
                };
                let generic_attr_fallback = if gen_ctx {
                    request.read().normal_origin().contextual(TypeId::ANY)
                } else {
                    request.read().normal_origin().contextual_opt(None)
                };
                if let Some(attrs_node) = self.ctx.arena.get(jsx_opening.attributes)
                    && let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node)
                {
                    for &attr_idx in &attrs.properties.nodes {
                        if let Some(attr_node) = self.ctx.arena.get(attr_idx) {
                            if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                                if let Some(spread_data) =
                                    self.ctx.arena.get_jsx_spread_attribute(attr_node)
                                {
                                    let spread_request = inferred_generic_props
                                        .map(|props| {
                                            request.read().normal_origin().contextual(props)
                                        })
                                        .unwrap_or(generic_attr_fallback);
                                    self.compute_type_of_node_with_request(
                                        spread_data.expression,
                                        &spread_request,
                                    );
                                }
                            } else if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE
                                && let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node)
                                && !attr_data.initializer.is_none()
                            {
                                let attr_value_idx = if let Some(init_node) =
                                    self.ctx.arena.get(attr_data.initializer)
                                {
                                    if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                                        self.ctx
                                            .arena
                                            .get_jsx_expression(init_node)
                                            .map(|expr| expr.expression)
                                            .unwrap_or(attr_data.initializer)
                                    } else {
                                        attr_data.initializer
                                    }
                                } else {
                                    continue;
                                };
                                if gen_ctx && inferred_generic_props.is_none() {
                                    if let Some(value_node) = self.ctx.arena.get(attr_value_idx)
                                        && matches!(
                                            value_node.kind,
                                            syntax_kind_ext::ARROW_FUNCTION
                                                | syntax_kind_ext::FUNCTION_EXPRESSION
                                        )
                                    {
                                        continue;
                                    }
                                }
                                let attr_request = if let Some(props_type) = inferred_generic_props
                                {
                                    let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                                        continue;
                                    };
                                    let Some(attr_name) = self.get_jsx_attribute_name(name_node)
                                    else {
                                        continue;
                                    };
                                    let props_for_access =
                                        self.normalize_jsx_required_props_target(props_type);
                                    match self.resolve_property_access_with_env(
                                        props_for_access,
                                        &attr_name,
                                    ) {
                                        crate::query_boundaries::common::PropertyAccessResult::Success { type_id, .. } => {
                                            request
                                                .read()
                                                .normal_origin()
                                                .contextual(tsz_solver::remove_undefined(self.ctx.types, type_id))
                                        }
                                        _ => {
                                            if attr_name != "as"
                                                && let Some(as_tag) = self
                                                    .collect_jsx_union_resolution_attrs(
                                                        jsx_opening.attributes,
                                                    )
                                                    .and_then(|attrs| {
                                                        attrs.into_iter().find_map(|(name, ty)| {
                                                            if name == "as" {
                                                                ty.and_then(|ty| {
                                                                    self.get_jsx_single_string_literal_tag_name(ty)
                                                                })
                                                            } else {
                                                                None
                                                            }
                                                        })
                                                    })
                                                && let Some(intrinsic_props) = self
                                                    .get_jsx_intrinsic_props_for_tag(idx, &as_tag, false)
                                            {
                                                let intrinsic_props =
                                                    self.normalize_jsx_required_props_target(
                                                        intrinsic_props,
                                                    );
                                                if let crate::query_boundaries::common::PropertyAccessResult::Success { type_id, .. } =
                                                    self.resolve_property_access_with_env(
                                                        intrinsic_props,
                                                        &attr_name,
                                                    )
                                                {
                                                    request
                                                        .read()
                                                        .normal_origin()
                                                        .contextual(tsz_solver::remove_undefined(
                                                            self.ctx.types,
                                                            type_id,
                                                        ))
                                                } else {
                                                    generic_attr_fallback
                                                }
                                            } else {
                                                generic_attr_fallback
                                            }
                                        }
                                    }
                                } else {
                                    generic_attr_fallback
                                };
                                if gen_ctx {
                                    if let Some(value_node) = self.ctx.arena.get(attr_value_idx)
                                        && matches!(
                                            value_node.kind,
                                            syntax_kind_ext::ARROW_FUNCTION
                                                | syntax_kind_ext::FUNCTION_EXPRESSION
                                        )
                                    {
                                        let has_function_context = attr_request
                                            .contextual_type
                                            .is_some_and(|ctx_type| {
                                                let ctx_type = self
                                                    .resolve_type_for_property_access(ctx_type);
                                                tsz_solver::type_queries::get_function_shape(
                                                    self.ctx.types,
                                                    ctx_type,
                                                )
                                                .is_some()
                                                    || tsz_solver::type_queries::get_call_signatures(
                                                        self.ctx.types,
                                                        ctx_type,
                                                    )
                                                    .is_some_and(|sigs| !sigs.is_empty())
                                            });
                                        if !has_function_context {
                                            continue;
                                        }
                                    }
                                }
                                self.compute_type_of_node_with_request(
                                    attr_value_idx,
                                    &attr_request,
                                );
                            }
                        }
                    }
                }
            }

            // The type of a JSX component element expression is always JSX.Element
            // (i.e. React.ReactElement<any>), not the component constructor/function
            // type. Returning the component type causes false TS2322 errors when the
            // JSX expression is used in a position that expects JSX.Element (e.g. as
            // the return value of `render(): JSX.Element`).
            // We look up JSX.Element directly here instead of calling get_jsx_element_type()
            // to avoid re-running the factory-in-scope diagnostics that were already
            // emitted at the top of get_type_of_jsx_opening_element.
            if let Some(element_type) = jsx_element_expr_type {
                return element_type;
            }
            // Fallback: return ANY when JSX.Element can't be resolved (e.g. no JSX types configured)
            TypeId::ANY
        }
    }
    /// Emit TS7026 for a JSX closing element if no `JSX.IntrinsicElements` exists.
    /// Covers the closing tag; opening tag is handled by `get_type_of_jsx_opening_element`.
    pub(crate) fn check_jsx_closing_element_for_implicit_any(&mut self, idx: NodeIndex) {
        // TS7026 is emitted unconditionally (not gated on noImplicitAny) when JSX.IntrinsicElements is absent.
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };
        let Some(jsx_closing) = self.ctx.arena.get_jsx_closing(node) else {
            return;
        };
        let tag_name_idx = jsx_closing.tag_name;
        let Some(tag_name_node) = self.ctx.arena.get(tag_name_idx) else {
            return;
        };
        let is_intrinsic = if tag_name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            self.ctx
                .arena
                .get_identifier(tag_name_node)
                .is_some_and(|id| {
                    id.escaped_text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_lowercase())
                })
        } else if tag_name_node.kind == syntax_kind_ext::JSX_NAMESPACED_NAME {
            // Namespaced tags (e.g., `</svg:path>`) are always intrinsic
            true
        } else {
            false
        };
        // Same suppression rules as the opening-element TS7026 check.
        use tsz_common::checker_options::JsxMode;
        let jsx_mode = self.ctx.compiler_options.jsx_mode;
        let uses_import_source = jsx_mode == JsxMode::ReactJsx
            || jsx_mode == JsxMode::ReactJsxDev
            || !self.ctx.compiler_options.jsx_import_source.is_empty();
        let file_has_any_parse_diag =
            self.ctx.has_parse_errors || !self.ctx.all_parse_error_positions.is_empty();
        let recovered_adjacent_sibling = self.file_has_same_line_adjacent_jsx_recovery_pattern();
        if is_intrinsic
            && self.get_intrinsic_elements_type().is_none()
            && !uses_import_source
            && !file_has_any_parse_diag
            && !recovered_adjacent_sibling
        {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                idx,
                diagnostic_codes::JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_NO_INTERFACE_JSX_EXISTS,
                &["IntrinsicElements"],
            );
            return;
        }

        if is_intrinsic
            && let Some(tag_name) = self.get_jsx_intrinsic_tag_name(tag_name_idx)
            && let Some(props) = self.get_jsx_intrinsic_props_for_tag(idx, &tag_name, true)
            && props == TypeId::ERROR
        {}
    }
    /// Get the global JSX namespace type (resolves factory-scoped then global `JSX`).
    pub(crate) fn get_jsx_namespace_type(&mut self) -> Option<SymbolId> {
        if let Some(jsx_sym) = self.resolve_jsx_namespace_from_factory() {
            return Some(jsx_sym);
        }
        if let Some(sym_id) = self.ctx.binder.file_locals.get("JSX") {
            return Some(sym_id);
        }
        let lib_binders = self.get_lib_binders();
        if let Some(sym_id) = self
            .ctx
            .binder
            .get_global_type_with_libs("JSX", &lib_binders)
        {
            return Some(sym_id);
        }

        None
    }

    // JSX Intrinsic Elements Type

    fn get_intrinsic_elements_symbol_id(&mut self) -> Option<SymbolId> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        exports.get("IntrinsicElements")
    }

    /// Get the JSX.IntrinsicElements interface type (maps tag names to prop types).
    pub(crate) fn get_intrinsic_elements_type(&mut self) -> Option<TypeId> {
        let intrinsic_elements_sym_id = self.get_intrinsic_elements_symbol_id()?;
        Some(self.type_reference_symbol_type(intrinsic_elements_sym_id))
    }

    /// Get the JSX.IntrinsicAttributes type (e.g. `{ key?: string }` in React).
    pub(super) fn get_intrinsic_attributes_type(&mut self) -> Option<TypeId> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let ia_sym_id = exports.get("IntrinsicAttributes")?;
        let ty = self.type_reference_symbol_type(ia_sym_id);
        let evaluated = self.evaluate_type_with_env(ty);
        if evaluated == TypeId::ANY || evaluated == TypeId::ERROR || evaluated == TypeId::UNKNOWN {
            return None;
        }
        Some(evaluated)
    }
    // JSX Element Type

    /// Get the JSX.Element type for fragments.
    ///
    /// Rule #36: Fragments resolve to JSX.Element type.
    pub(crate) fn get_jsx_element_type(&mut self, node_idx: NodeIndex) -> TypeId {
        self.check_jsx_factory_in_scope(node_idx);
        self.check_jsx_fragment_factory(node_idx);

        // Try to resolve JSX.Element from the JSX namespace
        if let Some(jsx_sym_id) = self.get_jsx_namespace_type() {
            let lib_binders = self.get_lib_binders();
            if let Some(symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(jsx_sym_id, &lib_binders)
                && let Some(exports) = symbol.exports.as_ref()
                && let Some(element_sym_id) = exports.get("Element")
            {
                return self.type_reference_symbol_type(element_sym_id);
            }
        }
        // Note: tsc 6.0 never emits TS7026 about "JSX.Element" (0 occurrences).
        // TS7026 is only emitted about "JSX.IntrinsicElements" for intrinsic elements.
        // For fragments, tsc emits TS17016 (missing jsxFragmentFactory) instead.
        TypeId::ANY
    }
    /// Get JSX.Element type for return type checking (no factory diagnostics).
    pub(crate) fn get_jsx_element_type_for_check(&mut self) -> Option<TypeId> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let element_sym_id = exports.get("Element")?;
        Some(self.type_reference_symbol_type(element_sym_id))
    }

    /// Get JSX.ElementClass type for class component return type checking.
    pub(super) fn get_jsx_element_class_type(&mut self) -> Option<TypeId> {
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let exports = symbol.exports.as_ref()?;
        let element_class_sym_id = exports.get("ElementClass")?;
        Some(self.type_reference_symbol_type(element_class_sym_id))
    }
    pub(super) fn get_jsx_children_prop_name(&mut self) -> String {
        use tsz_common::checker_options::JsxMode;

        if matches!(
            self.ctx.compiler_options.jsx_mode,
            JsxMode::ReactJsx | JsxMode::ReactJsxDev
        ) {
            return "children".to_string();
        }

        let Some(jsx_sym_id) = self.get_jsx_namespace_type() else {
            return "children".to_string();
        };
        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)
        else {
            return "children".to_string();
        };
        let Some(exports) = symbol.exports.as_ref() else {
            return "children".to_string();
        };
        let Some(eca_sym_id) = exports.get("ElementChildrenAttribute") else {
            return "children".to_string();
        };

        let eca_type = self.type_reference_symbol_type(eca_sym_id);
        let evaluated = self.evaluate_type_with_env(eca_type);
        if evaluated == TypeId::UNKNOWN || evaluated == TypeId::ERROR {
            return "children".to_string();
        }

        let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, evaluated)
        else {
            return "children".to_string();
        };

        shape
            .properties
            .first()
            .map(|prop| self.ctx.types.resolve_atom(prop.name))
            .unwrap_or_else(|| "children".to_string())
    }
    pub(crate) fn get_jsx_children_contextual_type(
        &mut self,
        opening_element_idx: NodeIndex,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(opening_element_idx)?;
        let jsx_opening = self.ctx.arena.get_jsx_opening(node)?;
        let tag_name_idx = jsx_opening.tag_name;
        let tag_name_node = self.ctx.arena.get(tag_name_idx)?;

        // Determine if intrinsic (lowercase) or component (uppercase/property access)
        let is_intrinsic = if tag_name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            self.ctx
                .arena
                .get_identifier(tag_name_node)
                .map(|id| id.escaped_text.as_str())
                .is_some_and(|n| n.chars().next().is_some_and(|c| c.is_ascii_lowercase()))
        } else {
            tag_name_node.kind == syntax_kind_ext::JSX_NAMESPACED_NAME
        };

        let props_type = if is_intrinsic {
            let tag_name = if tag_name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                self.ctx
                    .arena
                    .get_identifier(tag_name_node)
                    .map(|id| id.escaped_text.as_str().to_string())
            } else {
                // Namespaced tag
                self.ctx
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
                    })
            }?;
            let props =
                self.get_jsx_intrinsic_props_for_tag(opening_element_idx, &tag_name, false)?;
            if props == TypeId::ERROR {
                return None;
            }
            props
        } else {
            // Component: resolve tag name to get component type, extract props
            let component_type = self.compute_type_of_node(tag_name_idx);
            let resolved_component_type =
                self.normalize_jsx_component_type_for_resolution(component_type);
            if let Some(tag) = self.get_jsx_specific_string_literal_component_tag_name(
                tag_name_idx,
                resolved_component_type,
            ) && let Some(props) =
                self.get_jsx_intrinsic_props_for_tag(opening_element_idx, &tag, false)
                && props != TypeId::ERROR
            {
                props
            } else if let Some((props, _raw_has_type_params)) =
                self.recover_jsx_component_props_type(jsx_opening.attributes, component_type, None)
            {
                self.narrow_jsx_props_union_from_attributes(jsx_opening.attributes, props)
            } else if self.is_generic_jsx_component(resolved_component_type) {
                // Generic component: return ANY to avoid false implicit-any
                // diagnostics for callback and destructuring children.
                return Some(TypeId::ANY);
            } else {
                return None;
            }
        };

        let child_count = self
            .get_jsx_body_child_nodes(jsx_opening.attributes)
            .map_or(0, |children| children.len());

        self.get_jsx_children_prop_type(props_type)
            .map(|children_type| {
                self.jsx_children_contextual_type_for_body_shape(children_type, child_count)
            })
    }
    // JSX Attribute Name Extraction

    /// Extract the attribute name from a JSX attribute name node.
    ///
    /// Handles both simple identifiers (`name`) and namespaced names (`ns:name`).
    /// Returns `None` if the node is neither.
    pub(crate) fn get_jsx_attribute_name(
        &self,
        name_node: &tsz_parser::parser::node::Node,
    ) -> Option<String> {
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            Some(ident.escaped_text.as_str().to_string())
        } else if let Some(ns) = self.ctx.arena.get_jsx_namespaced_name(name_node) {
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
        } else {
            None
        }
    }

    /// Check if a specific attribute name exists as an EXPLICIT JSX attribute
    /// (not from a spread). Used for TS2710 double-specification detection.
    pub(super) fn has_explicit_jsx_attribute(&self, attributes_idx: NodeIndex, name: &str) -> bool {
        self.find_explicit_jsx_attribute(attributes_idx, name)
            .is_some()
    }

    /// Find an explicit JSX attribute by name, returning the attribute's name node index.
    fn find_explicit_jsx_attribute(
        &self,
        attributes_idx: NodeIndex,
        name: &str,
    ) -> Option<NodeIndex> {
        let attrs_node = self.ctx.arena.get(attributes_idx)?;
        let attrs = self.ctx.arena.get_jsx_attributes(attrs_node)?;
        for &attr_idx in &attrs.properties.nodes {
            let attr_node = self.ctx.arena.get(attr_idx)?;
            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                let attr_data = self.ctx.arena.get_jsx_attribute(attr_node)?;
                let name_node = self.ctx.arena.get(attr_data.name)?;
                if let Some(attr_name) = self.get_jsx_attribute_name(name_node)
                    && attr_name == name
                {
                    return Some(attr_data.name);
                }
            }
        }
        None
    }
}
