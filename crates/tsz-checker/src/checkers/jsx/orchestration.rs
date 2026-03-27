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
    pub(super) fn refine_jsx_callable_contextual_type(&mut self, type_id: TypeId) -> TypeId {
        let resolved = self.resolve_type_for_property_access(type_id);
        let resolved = self.evaluate_type_with_env(resolved);
        let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, resolved)
        else {
            return resolved;
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
                callable_members.push(member);
            }
        }

        match callable_members.len() {
            0 => resolved,
            1 => callable_members[0],
            _ => self.ctx.types.factory().union(callable_members),
        }
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

        let members = tsz_solver::type_queries::get_union_members(self.ctx.types, type_id)?;
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
        tsz_solver::FunctionShape {
            params: func
                .params
                .iter()
                .map(|param| tsz_solver::ParamInfo {
                    name: param.name,
                    type_id: crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        param.type_id,
                        substitution,
                    ),
                    optional: param.optional,
                    rest: param.rest,
                })
                .collect(),
            return_type: crate::query_boundaries::common::instantiate_type(
                self.ctx.types,
                func.return_type,
                substitution,
            ),
            this_type: func.this_type.map(|this_type| {
                crate::query_boundaries::common::instantiate_type(
                    self.ctx.types,
                    this_type,
                    substitution,
                )
            }),
            type_params: vec![],
            type_predicate: func.type_predicate.as_ref().map(|predicate| {
                tsz_solver::TypePredicate {
                    asserts: predicate.asserts,
                    target: predicate.target.clone(),
                    type_id: predicate.type_id.map(|tid| {
                        crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            tid,
                            substitution,
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

        let function_shape = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            component_type,
        )?;
        if function_shape.type_params.is_empty() || function_shape.params.is_empty() {
            return None;
        }

        let children_prop_name = self.get_jsx_children_prop_name();
        let provided_attrs = self.collect_jsx_union_resolution_attrs(attributes_idx)?;
        let has_concrete_attr = provided_attrs.iter().any(|(name, ty)| {
            name != &children_prop_name && ty.is_some()
        });
        let provided_attrs: Vec<(String, Option<TypeId>)> = provided_attrs
            .into_iter()
            .filter_map(|(name, ty)| {
                if name == children_prop_name {
                    return None;
                }

                // Function-valued JSX attrs need contextual typing from the
                // recovered props type, but they should not override a more
                // specific concrete attribute such as `as="button"` when we
                // infer/default generic props.
                match ty {
                    Some(ty) => Some((name, Some(ty))),
                    None if has_concrete_attr => None,
                    None => Some((name, Some(TypeId::ANY))),
                }
            })
            .collect();
        let typed_attrs: Vec<(String, TypeId)> = provided_attrs
            .into_iter()
            .filter_map(|(name, ty)| ty.map(|ty| (name, ty)))
            .collect();

        let mut substitution = if typed_attrs.is_empty() {
            crate::query_boundaries::common::TypeSubstitution::new()
        } else {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&typed_attrs);
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

        self.infer_jsx_generic_component_props_type(attributes_idx, normalized_component_type)
            .or_else(|| {
                self.get_default_instantiated_generic_sfc_props_type(normalized_component_type)
            })
            .map(|props_type| (props_type, false))
    }

    pub(super) fn infer_jsx_generic_class_component_signature(
        &mut self,
        element_idx: NodeIndex,
        component_type: TypeId,
    ) -> Option<tsz_solver::FunctionShape> {
        let node = self.ctx.arena.get(element_idx)?;
        let opening = self.ctx.arena.get_jsx_opening(node)?;
        let call_sig =
            tsz_solver::type_queries::get_construct_signatures(self.ctx.types, component_type)?
                .first()?
                .clone();
        let function_shape = tsz_solver::FunctionShape {
            type_params: call_sig.type_params,
            params: call_sig.params,
            this_type: call_sig.this_type,
            return_type: call_sig.return_type,
            type_predicate: call_sig.type_predicate,
            is_constructor: true,
            is_method: call_sig.is_method,
        };
        if function_shape.type_params.is_empty() || function_shape.params.is_empty() {
            return None;
        }

        let children_prop_name = self.get_jsx_children_prop_name();
        let provided_attrs = self.collect_jsx_union_resolution_attrs(opening.attributes)?;
        let provided_attrs: Vec<(String, TypeId)> = provided_attrs
            .into_iter()
            .map(|(name, ty)| {
                if name == children_prop_name {
                    return (name, TypeId::ERROR);
                }
                (name, ty.unwrap_or(TypeId::ANY))
            })
            .filter(|(name, ty)| name != &children_prop_name && *ty != TypeId::ERROR)
            .collect();
        if provided_attrs.is_empty() {
            return None;
        }

        let attrs_type = self.build_jsx_provided_attrs_object_type(&provided_attrs);
        let substitution = {
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
        Some(self.instantiate_jsx_function_shape_with_substitution(&function_shape, &substitution))
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
            let uses_import_source = jsx_mode == JsxMode::ReactJsx
                || jsx_mode == JsxMode::ReactJsxDev
                || self.extract_jsx_import_source_pragma().is_some()
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

            // TS2786: component return type must be valid JSX element
            self.check_jsx_component_return_type(resolved_component_type, tag_name_idx);

            let reported_factory_arity =
                self.check_jsx_sfc_factory_arity(resolved_component_type, tag_name_idx);

            // Extract props type from the component and check attributes.
            // TS2607/TS2608 are emitted within props extraction when applicable.
            // Build display target with IntrinsicAttributes intersection for TS2322 messages.
            if !reported_factory_arity
                && let Some((props_type, raw_has_type_params)) = self
                    .recover_jsx_component_props_type(
                        jsx_opening.attributes,
                        component_metadata_type,
                        Some(idx),
                    )
            {
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
                    raw_has_type_params,
                    display_target,
                    preferred_props_display.as_deref(),
                    request,
                    children_ctx,
                );
            } else if self.is_overloaded_sfc(resolved_component_type) {
                // JSX overload resolution: try each non-generic call signature against
                // the provided attributes. If no overload matches, emit TS2769.
                self.check_jsx_overloaded_sfc(
                    resolved_component_type,
                    jsx_opening.attributes,
                    jsx_opening.tag_name,
                    children_ctx,
                );
            } else {
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
        // Same suppression rules as the opening-element TS7026 check:
        // - ReactJsx/ReactJsxDev use jsxImportSource (no global IntrinsicElements needed)
        // - @jsxImportSource pragma or config overrides to use import source
        // - File has parse errors → suppress to avoid double-reporting
        use tsz_common::checker_options::JsxMode;
        let jsx_mode = self.ctx.compiler_options.jsx_mode;
        let uses_import_source = jsx_mode == JsxMode::ReactJsx
            || jsx_mode == JsxMode::ReactJsxDev
            || self.extract_jsx_import_source_pragma().is_some()
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

        self.get_jsx_children_prop_type(props_type)
            .map(|children_type| self.refine_jsx_callable_contextual_type(children_type))
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
