impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter::call_errors) fn literal_call_argument_display(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        self.literal_expression_display(arg_idx)
    }

    fn object_literal_call_argument_display_with_target_literals(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        if !self.object_literal_is_missing_required_target_property(arg_idx, param_type) {
            return None;
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(arg_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let literal = self.ctx.arena.get_literal_expr(node)?;
        let elements = literal.elements.nodes.to_vec();
        let mut literal_overrides = FxHashMap::default();

        for elem_idx in elements {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                        continue;
                    };
                    (prop.name, prop.initializer)
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_shorthand_property(elem_node) else {
                        continue;
                    };
                    (prop.name, prop.name)
                }
                _ => continue,
            };
            let Some(prop_name) = self
                .object_literal_property_name_text(prop_name_idx)
                .or_else(|| self.get_property_name_resolved(prop_name_idx))
            else {
                continue;
            };
            let Some((target_prop_type, _)) =
                self.object_literal_target_property_type(param_type, prop_name_idx, &prop_name)
            else {
                continue;
            };
            if !self.is_literal_sensitive_assignment_target(target_prop_type) {
                continue;
            }
            let Some(literal_type) = self.literal_type_from_initializer(prop_value_idx) else {
                continue;
            };
            literal_overrides.insert(self.ctx.types.intern_string(&prop_name), literal_type);
        }

        if literal_overrides.is_empty() {
            return None;
        }

        let display_type = crate::query_boundaries::common::widen_argument_type_for_display(
            self.ctx.types,
            arg_type,
        );
        let shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, display_type)?;
        let mut props = shape.properties.clone();
        props.sort_by_key(|prop| prop.declaration_order);

        let mut rendered = Vec::new();
        for prop in props {
            let name = self.ctx.types.resolve_atom(prop.name);
            let ty_display = if let Some(&literal_type) = literal_overrides.get(&prop.name) {
                self.format_type_for_assignability_message(literal_type)
            } else {
                let widened =
                    crate::query_boundaries::common::widen_type(self.ctx.types, prop.type_id);
                let mut formatter = self
                    .ctx
                    .create_diagnostic_type_formatter()
                    .with_preserve_optional_parameter_surface_syntax(true);
                formatter.format(widened).into_owned()
            };
            let optional = if prop.optional { "?" } else { "" };
            rendered.push(format!("{name}{optional}: {ty_display};"));
        }

        Some(format!("{{ {} }}", rendered.join(" ")))
    }

    pub(in crate::error_reporter::call_errors) fn object_literal_is_missing_required_target_property(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(arg_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(literal) = self.ctx.arena.get_literal_expr(node) else {
            return false;
        };
        let elements = literal.elements.nodes.to_vec();
        let mut source_names = rustc_hash::FxHashSet::default();
        for elem_idx in elements {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let name_idx = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .map(|p| p.name),
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .map(|p| p.name),
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    self.ctx.arena.get_method_decl(elem_node).map(|m| m.name)
                }
                _ => None,
            };
            let Some(name_idx) = name_idx else {
                continue;
            };
            if let Some(name) = self
                .object_literal_property_name_text(name_idx)
                .or_else(|| self.get_property_name_resolved(name_idx))
            {
                source_names.insert(name);
            }
        }

        let resolved = self.resolve_type_for_property_access(param_type);
        let evaluated = self.evaluate_type_with_env(resolved);
        let evaluated = self.resolve_lazy_type(evaluated);
        let evaluated = self.evaluate_application_type(evaluated);
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated)
        else {
            return false;
        };

        static OBJECT_PROTO_METHODS: &[&str] = &[
            "constructor",
            "toString",
            "toLocaleString",
            "valueOf",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
        ];

        shape.properties.iter().any(|prop| {
            !prop.optional && {
                let name = self.ctx.types.resolve_atom(prop.name);
                !source_names.contains(name.as_str())
                    && !OBJECT_PROTO_METHODS.contains(&name.as_str())
            }
        })
    }

    fn jsdoc_constructor_identifier_source_display(
        &mut self,
        expr_idx: NodeIndex,
        arg_type: TypeId,
    ) -> Option<String> {
        let arg_type = self.evaluate_type_with_env(arg_type);
        let arg_type = self.resolve_type_for_property_access(arg_type);
        if !crate::query_boundaries::common::is_constructor_like_type(self.ctx.types, arg_type) {
            return None;
        }
        let expr_node = self.ctx.arena.get(expr_idx)?;
        let ident = self.ctx.arena.get_identifier(expr_node)?;
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        self.symbol_has_js_constructor_evidence(sym_id)
            .then(|| format!("typeof {}", ident.escaped_text))
    }

    fn zero_argument_call_list_display(&self, arg_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION
            && node.kind != syntax_kind_ext::NEW_EXPRESSION
        {
            return None;
        }
        let call = self.ctx.arena.get_call_expr(node)?;
        if call
            .arguments
            .as_ref()
            .is_none_or(|args| args.nodes.is_empty())
        {
            Some("[]".to_string())
        } else {
            None
        }
    }

    pub(in crate::error_reporter) fn format_call_argument_type_for_diagnostic(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> String {
        // A plain `expr as T` / `<T>expr` assertion argument yields the asserted
        // type `T` as written. `tsc` reports it with its literal element /
        // property types intact (a regular, non-fresh type) rather than widening
        // them as for a fresh array/object literal argument. Detect the assertion
        // before the `skip_parenthesized_and_assertions` below peels it away to
        // the inner literal (which would otherwise route the operand through the
        // fresh-literal widening). `format_type_diagnostic` renders the asserted
        // type with literals preserved. `as const` and `satisfies` are excluded.
        if self.expression_is_plain_type_assertion(arg_idx) {
            return self.format_type_diagnostic(arg_type);
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(arg_idx);
        let is_array_literal_arg = self
            .ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION);
        if let Some(expr_node) = self.ctx.arena.get(expr_idx)
            && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
            && ident.escaped_text == "arguments"
            && self.has_enclosing_regular_function(expr_idx)
        {
            return "IArguments".to_string();
        }
        if query_common::tuple_elements(self.ctx.types, arg_type).is_some()
            && self
                .ctx
                .arena
                .get(expr_idx)
                .is_none_or(|node| node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
        {
            return self.format_type_diagnostic(arg_type);
        }

        // When the only literal-sensitive member of the parameter type is
        // `undefined` contributed by an optional parameter (`b?: T`), tsc
        // strips the synthetic `| undefined` and widens the argument display
        // for the underlying target. Skip the literal-preserving branch in
        // that case so the argument widens to its widened display type
        // (e.g. `string` instead of `'"hello"'`).
        //
        // Additionally, only preserve the source literal when the target's
        // primitive structure makes the literal display informative — for a
        // mixed-primitive target like `string | "hello"` whose unique base
        // appears in plain primitive form, the source widens to its base to
        // match tsc's output. See `literal_widening_policy` for the full
        // rule.
        if self.is_literal_sensitive_assignment_target(param_type)
            && !self.literal_sensitivity_is_only_synthetic_optional_undefined(param_type, arg_idx)
            && self.source_literal_primitive_matches_target_literal(arg_type, arg_idx, param_type)
            && let Some(display) = self.literal_call_argument_display(arg_idx)
        {
            return display;
        }

        if self
            .ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION)
            && let Some(display) = self.conditional_callable_union_argument_display(arg_type)
        {
            return display;
        }

        if let Some(display) =
            self.contextual_function_argument_display(arg_type, param_type, arg_idx)
        {
            return display;
        }

        if let Some(display) = self.object_literal_call_argument_display_with_target_literals(
            arg_type, param_type, arg_idx,
        ) {
            return display;
        }

        if let Some(display) =
            self.identifier_array_object_literal_source_display(expr_idx, param_type)
        {
            return display;
        }
        if let Some(display) = self.jsdoc_constructor_identifier_source_display(expr_idx, arg_type)
        {
            return display;
        }
        if !is_array_literal_arg
            && let Some(display) = self.rebuilt_array_source_display(arg_type, param_type)
        {
            return display;
        }
        if let Some(display) =
            self.declared_identifier_source_display(expr_idx, param_type, arg_type)
        {
            return display;
        }

        if self.call_target_preserves_literal_argument_surface(param_type, arg_idx)
            && self.source_literal_primitive_matches_target_literal(arg_type, arg_idx, param_type)
            && let Some(display) = self.literal_call_argument_display(arg_idx)
        {
            if (display == "true" || display == "false")
                && self.call_target_should_widen_boolean_literal_display(param_type)
            {
                return "boolean".to_string();
            }
            return display;
        }

        let mut display_type = if param_type == TypeId::NEVER {
            if let Some(display) = self.zero_argument_call_list_display(arg_idx) {
                return display;
            }
            let direct_arg_type = self.elaboration_source_expression_type(arg_idx);
            if direct_arg_type == TypeId::ERROR || direct_arg_type == arg_type {
                arg_type
            } else {
                direct_arg_type
            }
        } else {
            crate::query_boundaries::common::widen_argument_type_for_display(
                self.ctx.types,
                arg_type,
            )
        };

        if crate::query_boundaries::common::is_mapped_type(self.ctx.types, display_type) {
            let evaluated_display = self.evaluate_type_for_assignability(display_type);
            if crate::query_boundaries::common::object_shape_for_type(
                self.ctx.types,
                evaluated_display,
            )
            .is_some()
            {
                display_type = evaluated_display;
            }
        }

        let should_widen_display = self
            .materialize_finite_mapped_call_parameter_display_type(param_type)
            .is_some()
            && crate::query_boundaries::common::object_shape_for_type(self.ctx.types, display_type)
                .is_some()
            || (is_array_literal_arg && !self.is_literal_sensitive_assignment_target(param_type));

        let display = if should_widen_display {
            self.format_type_diagnostic_widened(display_type)
        } else {
            self.format_type_for_assignability_message(display_type)
        };
        self.rewrite_source_display_for_non_literal_target_assignability(
            arg_type, param_type, display,
        )
    }
}
