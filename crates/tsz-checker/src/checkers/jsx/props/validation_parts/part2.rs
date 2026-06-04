impl<'a> CheckerState<'a> {
    pub(in crate::checkers_domain::jsx) fn normalize_jsx_function_context_type(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let type_id = self.resolve_type_for_property_access(type_id);
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            && shape.is_method
        {
            return self
                .ctx
                .types
                .factory()
                .function(tsz_solver::FunctionShape {
                    type_params: shape.type_params.clone(),
                    params: shape.params.clone(),
                    this_type: None,
                    return_type: shape.return_type,
                    type_predicate: shape.type_predicate,
                    is_constructor: shape.is_constructor,
                    is_method: false,
                });
        }

        type_id
    }

    /// Fallback: check `IntrinsicAttributes` when component props couldn't be extracted.
    pub(in crate::checkers_domain::jsx) fn check_jsx_intrinsic_attributes_only(
        &mut self,
        component_type: TypeId,
        attributes_idx: NodeIndex,
        tag_name_idx: NodeIndex,
    ) {
        let intrinsic_attrs_type = self.get_intrinsic_attributes_type();
        let intrinsic_class_attrs_type =
            self.get_intrinsic_class_attributes_type_for_component(component_type);
        if intrinsic_attrs_type.is_none() && intrinsic_class_attrs_type.is_none() {
            return;
        }

        // Collect provided attribute names with types
        let mut provided_attrs: Vec<(String, TypeId)> = Vec::new();
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
            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                if let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node)
                    && let Some(name_node) = self.ctx.arena.get(attr_data.name)
                    && let Some(attr_name) = self.get_jsx_attribute_name(name_node)
                {
                    provided_attrs.push((attr_name, TypeId::ANY));
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                // Spread of `any` covers all properties
                if let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) {
                    let spread_type = self.compute_type_of_node(spread_data.expression);
                    if spread_type == TypeId::ANY {
                        return; // any covers everything
                    }
                }
            }
        }

        if let Some(intrinsic_attrs_type) = intrinsic_attrs_type {
            self.check_missing_required_jsx_props(
                intrinsic_attrs_type,
                &provided_attrs,
                tag_name_idx,
                None,
                None,
            );
        }
        if let Some(intrinsic_class_attrs_type) = intrinsic_class_attrs_type {
            self.check_missing_required_jsx_props(
                intrinsic_class_attrs_type,
                &provided_attrs,
                tag_name_idx,
                None,
                None,
            );
        }
    }

    /// TS2322: Check spread attributes against `IntrinsicAttributes`.
    ///
    /// Covers both SFCs with declared type parameters (e.g. `<T>(props: T) => ...`) and
    /// SFCs that use free type variables from an outer generic (e.g. `function(props: P)`
    /// inside `function test<P>`). tsc emits TS2322 whenever an unconstrained type
    /// parameter spread doesn't satisfy `IntrinsicAttributes`, regardless of whether the
    /// type parameter is declared on the SFC itself or comes from an enclosing scope.
    pub(in crate::checkers_domain::jsx) fn check_generic_sfc_spread_intrinsic_attrs(
        &mut self,
        _component_type: TypeId,
        attributes_idx: NodeIndex,
        tag_name_idx: NodeIndex,
    ) {
        let Some(ia_type) = self.get_intrinsic_attributes_type() else {
            return;
        };

        // Get spread attributes
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
            if attr_node.kind != syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                continue;
            }
            let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                continue;
            };
            let spread_type = self.compute_type_of_node(spread_data.expression);

            if spread_type == TypeId::ANY || spread_type == TypeId::ERROR {
                continue;
            }

            // Build target: IntrinsicAttributes & spread_type
            let target = self.ctx.types.factory().intersection2(ia_type, spread_type);

            if !self.jsx_props_relation_outcome(spread_type, target).related {
                let spread_name = self.format_type(spread_type);
                let target_name = format!("IntrinsicAttributes & {spread_name}");
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&spread_name, &target_name],
                );
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    tag_name_idx,
                    &message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
        }
    }

    /// Grammar check: TS17000 for empty expressions in JSX attributes.
    /// Matches tsc's `checkGrammarJsxElement`: reports only the first empty
    /// expression per JSX opening element, then returns.
    pub(in crate::checkers_domain::jsx) fn check_grammar_jsx_element(
        &mut self,
        attributes_idx: NodeIndex,
    ) {
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
            if attr_data.initializer.is_none() {
                continue;
            }
            let Some(init_node) = self.ctx.arena.get(attr_data.initializer) else {
                continue;
            };
            if init_node.kind != syntax_kind_ext::JSX_EXPRESSION {
                continue;
            }
            let Some(expr_data) = self.ctx.arena.get_jsx_expression(init_node) else {
                continue;
            };
            // Empty expression {} without spread
            if expr_data.expression.is_none() && !expr_data.dot_dot_dot_token {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    attr_data.initializer,
                    "JSX attributes must only be assigned a non-empty 'expression'.",
                    diagnostic_codes::JSX_ATTRIBUTES_MUST_ONLY_BE_ASSIGNED_A_NON_EMPTY_EXPRESSION,
                );
                // tsc returns after the first grammar error per element
                return;
            }
        }
    }

    /// True when `attr_name` resolves to an optional property declared on an
    /// anonymous object type. tsc preserves `| undefined` for optional props
    /// from anonymous inline `IntrinsicElements` types but strips it for named
    /// interfaces/aliases.
    pub(crate) fn jsx_attr_prop_is_optional_in_anonymous_source(
        &self,
        direct_prop_access: &crate::query_boundaries::common::PropertyAccessResult,
        as_intrinsic_props: Option<TypeId>,
        props_type: TypeId,
        attr_name: &str,
    ) -> bool {
        let props_lookup = match (direct_prop_access, as_intrinsic_props) {
            (
                crate::query_boundaries::common::PropertyAccessResult::PropertyNotFound { .. },
                Some(intrinsic),
            ) => intrinsic,
            _ => props_type,
        };
        if jsx_queries::type_has_displayable_name(self.ctx.types, props_lookup) {
            return false;
        }
        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, props_lookup)
            .and_then(|shape| {
                shape.properties.iter().find_map(|p| {
                    if self.ctx.types.resolve_atom(p.name) == attr_name {
                        Some(p.optional)
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(false)
    }

    /// Pick the displayed target type for a JSX attribute TS2322. For
    /// optional props on anonymous source types, return `T | undefined`
    /// (matching tsc's display); otherwise return the write-position type.
    pub(crate) fn jsx_attr_display_target_type(
        &mut self,
        write_check_type: TypeId,
        declared_type_id: TypeId,
        prop_is_optional_in_anonymous_source: bool,
    ) -> TypeId {
        let display_type = if !prop_is_optional_in_anonymous_source {
            write_check_type
        } else if write_check_type == declared_type_id {
            self.ctx
                .types
                .factory()
                .union2(write_check_type, TypeId::UNDEFINED)
        } else {
            declared_type_id
        };

        if crate::query_boundaries::checkers::jsx::contains_index_access_type(
            self.ctx.types,
            display_type,
        ) {
            let alias_hint =
                crate::query_boundaries::checkers::jsx::index_access_type_arg_alias_hint(
                    self.ctx.types,
                    &self.ctx.definition_store,
                    display_type,
                );
            let evaluated = self.evaluate_type_with_env(display_type);
            if evaluated != display_type && evaluated != TypeId::ERROR {
                if let Some(alias_hint) = alias_hint {
                    let alias_evaluated = self.evaluate_type_with_env(alias_hint);
                    if self
                        .jsx_props_relation_outcome(alias_evaluated, evaluated)
                        .related
                        && self
                            .jsx_props_relation_outcome(evaluated, alias_evaluated)
                            .related
                    {
                        self.ctx.types.store_display_alias(evaluated, alias_hint);
                    }
                }
                return evaluated;
            }
        }

        display_type
    }

    /// Emit JSX bare-string writes to optional anonymous props with
    /// `T | undefined` as the displayed TS2322 target.
    pub(crate) fn try_emit_jsx_bare_string_attr_undefined_target(
        &mut self,
        actual_type: TypeId,
        expected_type: TypeId,
        original_property_type: TypeId,
        anchor_idx: NodeIndex,
        initializer_is_bare_string_literal: bool,
    ) -> Option<bool> {
        if !initializer_is_bare_string_literal
            || original_property_type == expected_type
            || self
                .jsx_props_relation_outcome(actual_type, expected_type)
                .related
        {
            return None;
        }
        self.error_type_not_assignable_at_with_display_types_widened(
            actual_type,
            original_property_type,
            anchor_idx,
        );
        Some(false)
    }
}
