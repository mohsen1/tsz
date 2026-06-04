impl<'a> CheckerState<'a> {
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
