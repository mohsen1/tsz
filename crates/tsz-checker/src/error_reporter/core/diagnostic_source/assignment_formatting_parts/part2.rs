impl<'a> CheckerState<'a> {
    fn new_expression_nominal_source_display(
        &mut self,
        expr_idx: NodeIndex,
        display_type: TypeId,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        // When the result type is a union (e.g., `number | Date` from
        // `new unionOfDifferentReturnType(10)` where unionOfDifferentReturnType
        // is `{ new (a: number): number } | { new (a: number): Date }`),
        // TSC shows the actual result type, not the constructor variable name.
        // Return None to let the fallback formatting handle it.
        if crate::query_boundaries::common::union_members(self.ctx.types, display_type).is_some() {
            return None;
        }

        if let Some(new_expr) = self.ctx.arena.get_call_expr(node)
            && let Some(mut ctor_display) = self.expression_text(new_expr.expression)
        {
            if let Some(type_args) = &new_expr.type_arguments
                && !type_args.nodes.is_empty()
            {
                let rendered_args: Vec<String> = type_args
                    .nodes
                    .iter()
                    .map(|&arg| self.get_source_text_for_node(arg))
                    .collect();
                ctor_display.push('<');
                ctor_display.push_str(&rendered_args.join(", "));
                ctor_display.push('>');
                return Some(ctor_display);
            }
            // With display alias: show the named type (e.g. `D<unknown>` not `D`).
            // Without: variable name is not a type name; let caller format the actual type.
            if self.ctx.types.get_display_alias(display_type).is_some() {
                return Some(self.format_type_diagnostic_structural(display_type));
            }
            return None;
        }

        Some(self.format_property_receiver_type_for_diagnostic(display_type))
    }

    fn js_constructor_instance_assignment_source_display(
        &mut self,
        source: TypeId,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, source)?;
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))?;
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let expr_node = self.ctx.arena.get(expr_idx)?;
        if expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let source_sym = self.resolve_identifier_symbol(expr_idx)?;
        let source_symbol = self
            .get_cross_file_symbol(source_sym)
            .or_else(|| self.ctx.binder.get_symbol(source_sym))?;
        if (source_symbol.flags & tsz_binder::symbol_flags::VARIABLE) == 0 {
            return None;
        }

        let current_file_idx = self.ctx.current_file_idx as u32;
        let this_pos = expr_node.pos;
        source_symbol
            .stable_declarations
            .iter()
            .copied()
            .chain(std::iter::once(source_symbol.stable_value_declaration))
            .filter_map(|stable_loc| {
                if !stable_loc.is_known() {
                    return None;
                }
                let file_idx = if stable_loc.has_file_idx() {
                    stable_loc.file_idx
                } else {
                    current_file_idx
                };
                let (decl_idx, arena) = self.ctx.node_at_stable_location(stable_loc)?;
                let decl_node = arena.get(decl_idx)?;
                let declaration = arena.get_variable_declaration(decl_node)?;
                if file_idx == current_file_idx && decl_node.pos > this_pos {
                    return None;
                }

                let init_idx = arena.skip_parenthesized_and_assertions(declaration.initializer);
                let init_node = arena.get(init_idx)?;
                if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
                    return None;
                }
                let new_expr = arena.get_call_expr(init_node)?;
                let ctor_idx = arena.skip_parenthesized_and_assertions(new_expr.expression);
                let ctor_node = arena.get(ctor_idx)?;
                let ident = arena.get_identifier(ctor_node)?;
                Some((
                    file_idx == current_file_idx,
                    decl_node.pos,
                    ident.escaped_text.clone(),
                ))
            })
            .max_by_key(|(same_file, decl_pos, _)| (*same_file, *decl_pos))
            .map(|(_, _, display)| display)
    }

    fn call_unknown_array_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        let call = self.ctx.arena.get_call_expr(node)?;

        let first_arg = *call.arguments.as_ref()?.nodes.first()?;
        let first_arg_type = self.get_type_of_node(first_arg);
        if matches!(first_arg_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        let element_type =
            crate::query_boundaries::common::array_element_type(self.ctx.types, first_arg_type)
                .or_else(|| {
                    tsz_solver::operations::get_iterator_info(self.ctx.types, first_arg_type, false)
                        .map(|info| info.yield_type)
                })?;
        if matches!(element_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        let recovered = self
            .ctx
            .types
            .array(self.widen_type_for_display(element_type));
        Some(self.format_assignability_type_for_message(recovered, target))
    }

    fn preferred_evaluated_source_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let preserve_literal_surface = self.target_preserves_literal_surface(target);
        if crate::query_boundaries::common::is_template_literal_type(self.ctx.types, source) {
            return Some(self.format_type_diagnostic_structural(source));
        }

        let evaluated = self.evaluate_type_for_assignability(source);
        if evaluated == source || evaluated == TypeId::ERROR {
            return None;
        }

        if crate::query_boundaries::common::literal_value(self.ctx.types, evaluated).is_some()
            || crate::query_boundaries::common::is_template_literal_type(self.ctx.types, evaluated)
            || crate::query_boundaries::common::string_intrinsic_components(
                self.ctx.types,
                evaluated,
            )
            .is_some()
        {
            return Some(if preserve_literal_surface {
                self.format_type_diagnostic(evaluated)
            } else {
                self.format_type_diagnostic_structural(evaluated)
            });
        }

        None
    }

    pub(in crate::error_reporter) fn broad_mapped_index_signature_source_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let mapped = crate::query_boundaries::common::mapped_type_info(self.ctx.types, source)?;
        if mapped.name_type.is_some() || mapped.optional_modifier.is_some() {
            return None;
        }
        let key_kind = match mapped.constraint {
            TypeId::STRING => "string",
            TypeId::NUMBER => "number",
            _ => return None,
        };
        if crate::query_boundaries::common::contains_type_parameter_named(
            self.ctx.types,
            mapped.template,
            mapped.type_param.name,
        ) {
            return None;
        }
        if self.assign_relation_outcome(source, target).related {
            return None;
        }

        let readonly_prefix = match mapped.readonly_modifier {
            Some(tsz_solver::MappedModifier::Add) => "readonly ",
            Some(tsz_solver::MappedModifier::Remove) => "-readonly ",
            None => "",
        };
        let value_display = self.format_type_for_assignability_message(mapped.template);
        Some(format!(
            "{{ {readonly_prefix}[x: {key_kind}]: {value_display}; }}"
        ))
    }

    fn type_assertion_mapped_alias_source_display(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        if !matches!(
            node.kind,
            syntax_kind_ext::AS_EXPRESSION | syntax_kind_ext::TYPE_ASSERTION
        ) {
            return None;
        }
        let assertion = self.ctx.arena.get_type_assertion(node)?;
        let assertion_type = self.get_type_from_type_node(assertion.type_node);
        let is_generic_mapped_alias =
            crate::query_boundaries::common::is_generic_application(self.ctx.types, assertion_type)
                && crate::query_boundaries::common::get_application_lazy_def_id(
                    self.ctx.types,
                    assertion_type,
                )
                .and_then(|def_id| self.ctx.definition_store.get(def_id))
                .is_some_and(|def| {
                    def.kind == tsz_solver::def::DefKind::TypeAlias
                        && def.body.is_some_and(|body| {
                            crate::query_boundaries::common::is_mapped_type(self.ctx.types, body)
                        })
                });
        if !is_generic_mapped_alias {
            return None;
        }
        self.node_text(assertion.type_node)
            .and_then(|text| self.sanitize_type_annotation_text_for_diagnostic(text, false))
            .map(|text| self.format_annotation_like_type(&text))
    }

    /// Whether to suppress the AST-literal short-circuit for an
    /// object-literal-property elaboration when the property elaboration has
    /// already widened the source (e.g. `1` → `number`). Mirrors tsc's
    /// `getWidenedLiteralLikeTypeForContextualType`: keep the literal display
    /// when the source's primitive kind appears as a literal kind somewhere in
    /// the target, otherwise widen.
    pub(in crate::error_reporter) fn property_elaboration_widening_required_for_display(
        &self,
        expr_idx: NodeIndex,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !self.is_property_assignment_initializer(expr_idx) {
            return false;
        }
        // Only fire when the caller passed in a non-literal primitive source
        // (i.e. the property elaboration already widened the literal). For
        // direct `let x: 1 = "abc"` style mismatches the source is still the
        // literal type, so this guard short-circuits.
        if !crate::query_boundaries::common::is_primitive_type(self.ctx.types, source) {
            return false;
        }
        if crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some() {
            return false;
        }
        let primitive_kind = source;
        !target_accepts_literal_primitive_kind(self.ctx.types, target, primitive_kind)
    }

    pub(in crate::error_reporter) fn array_elaboration_widening_required_for_display(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        use crate::query_boundaries::common;

        let source_primitive = if let Some(value) = common::literal_value(self.ctx.types, source) {
            value.primitive_type_id()
        } else if matches!(
            source,
            TypeId::STRING | TypeId::NUMBER | TypeId::BIGINT | TypeId::BOOLEAN
        ) {
            source
        } else {
            return false;
        };
        let target = common::evaluate_type(self.ctx.types, target);
        if target == TypeId::UNDEFINED || target == TypeId::NULL {
            return source_primitive != TypeId::BOOLEAN;
        }

        !target_accepts_literal_primitive_kind(self.ctx.types, target, source_primitive)
    }

    pub(in crate::error_reporter) fn array_literal_element_source_widening_required_for_display(
        &self,
        anchor_idx: NodeIndex,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !self.array_elaboration_widening_required_for_display(source, target) {
            return false;
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        self.ctx
            .arena
            .parent_of(expr_idx)
            .and_then(|parent_idx| self.ctx.arena.get(parent_idx))
            .is_some_and(|parent| parent.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
    }

    pub(in crate::error_reporter) fn is_object_rest_assignment_target_anchor(
        &self,
        anchor_idx: NodeIndex,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        let mut current = expr_idx;
        let mut saw_spread_wrapper = false;
        let mut object_idx = None;

        while let Some(parent_idx) = self.ctx.arena.parent_of(current) {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                || parent_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                saw_spread_wrapper = true;
                current = parent_idx;
                continue;
            }
            if parent_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION && saw_spread_wrapper
            {
                object_idx = Some(parent_idx);
                break;
            }
            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                || parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                || parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
            {
                break;
            }
            current = parent_idx;
        }
        let Some(object_idx) = object_idx else {
            return false;
        };

        self.assignment_target_expression(anchor_idx)
            .is_some_and(|target_idx| {
                self.ctx.arena.skip_parenthesized_and_assertions(target_idx) == object_idx
            })
    }
}
