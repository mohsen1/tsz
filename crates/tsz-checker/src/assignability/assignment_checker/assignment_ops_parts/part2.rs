impl<'a> CheckerState<'a> {
    // =========================================================================
    // Assignment Operator Utilities
    // =========================================================================

    /// TS2462: A rest element in array destructuring must be the last element.
    ///
    /// Enforce syntax for array destructuring assignment targets.
    fn check_array_destructuring_rest_position(&mut self, left_idx: NodeIndex) {
        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };
        if left_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return;
        }
        let Some(array_lit) = self.ctx.arena.get_literal_expr(left_node) else {
            return;
        };

        let elements_len = array_lit.elements.nodes.len();
        if elements_len == 0 {
            return;
        }
        for (i, &element_idx) in array_lit.elements.nodes.iter().enumerate() {
            if i + 1 >= elements_len {
                break;
            }
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                self.error_at_node_msg(
                    element_idx,
                    diagnostic_codes::A_REST_ELEMENT_MUST_BE_LAST_IN_A_DESTRUCTURING_PATTERN,
                    &[],
                );
            }
        }
    }

    /// TS1186: A rest element cannot have an initializer.
    ///
    /// In assignment destructuring, `[...x = a] = b` is parsed as a spread of
    /// the assignment expression `x = a`. TypeScript detects this and emits
    /// TS1186 when the spread expression is a binary `=` assignment.
    fn check_rest_element_initializer(&mut self, left_idx: NodeIndex) {
        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };

        let elements = if left_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            self.ctx
                .arena
                .get_literal_expr(left_node)
                .map(|lit| &lit.elements.nodes as &[NodeIndex])
        } else if left_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            self.ctx
                .arena
                .get_literal_expr(left_node)
                .map(|lit| &lit.elements.nodes as &[NodeIndex])
        } else {
            None
        };

        let Some(elements) = elements else { return };
        for &element_idx in elements {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            // Check spread elements and spread assignments
            if element_node.kind != syntax_kind_ext::SPREAD_ELEMENT
                && element_node.kind != syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                continue;
            }
            let spread_expr = self
                .ctx
                .arena
                .get_spread(element_node)
                .map(|s| s.expression)
                .or_else(|| {
                    self.ctx
                        .arena
                        .get_unary_expr_ex(element_node)
                        .map(|u| u.expression)
                });
            let Some(spread_expr) = spread_expr else {
                continue;
            };
            // If the spread expression is a binary assignment (x = a), emit TS1186.
            // tsc anchors this at the `=` operator token, not at the spread element's
            // `...` prefix or the left-hand name. Scan from the left operand's end to
            // find the `=` position.
            if let Some(spread_node) = self.ctx.arena.get(spread_expr)
                && spread_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.ctx.arena.get_binary_expr(spread_node)
                && bin.operator_token == SyntaxKind::EqualsToken as u16
            {
                // Find the `=` token position between left and right operands
                let eq_pos = self.ctx.arena.get(bin.left).map(|left_node| {
                    let search_start = left_node.end as usize;
                    self.ctx
                        .arena
                        .source_files
                        .first()
                        .and_then(|sf| {
                            sf.text[search_start..]
                                .find('=')
                                .map(|offset| (search_start + offset) as u32)
                        })
                        .unwrap_or(left_node.end)
                });
                if let Some(pos) = eq_pos {
                    let message = tsz_common::diagnostics::get_message_template(
                        diagnostic_codes::A_REST_ELEMENT_CANNOT_HAVE_AN_INITIALIZER,
                    )
                    .unwrap_or("");
                    self.error_at_position(
                        pos,
                        1,
                        message,
                        diagnostic_codes::A_REST_ELEMENT_CANNOT_HAVE_AN_INITIALIZER,
                    );
                }
            }
        }
    }

    pub(crate) fn check_assignment_compatibility(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        source_type: TypeId,
        target_type: TypeId,
        check_assignability: bool,
        suppress_error_for_error_types: bool,
    ) {
        if let Some((source_level, target_level)) =
            self.constructor_accessibility_mismatch_for_assignment(left_idx, right_idx)
        {
            self.error_constructor_accessibility_not_assignable(
                source_type,
                target_type,
                source_level,
                target_level,
                left_idx,
            );
            return;
        }

        if !check_assignability {
            return;
        }

        if suppress_error_for_error_types
            && (source_type == TypeId::ERROR || target_type == TypeId::ERROR)
        {
            return;
        }

        if let Some(generic_target) =
            self.deferred_generic_element_write_target(left_idx, source_type)
        {
            if (source_type != generic_target
                && !self
                    .generic_element_write_relation_outcome(source_type, generic_target)
                    .related)
                || (source_type != generic_target
                    && !crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        source_type,
                    )
                    && crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        generic_target,
                    ))
            {
                self.error_type_not_assignable_at_with_display_types(
                    source_type,
                    generic_target,
                    left_idx,
                );
            }
            return;
        }

        // tsc anchors some void-assignment diagnostics to the function identifier on
        // the RHS when the assignment target is `void` and the RHS is a function
        // symbol reference (e.g. `function f<T>(a: T) { ... }; x = f;`).
        // Use `has_call_signatures` instead of `is_callable_type` to exclude class
        // constructor types (which only have construct/new signatures). TSC anchors
        // class assignments (`x = C;`) at the LHS, not the RHS.
        if target_type == TypeId::VOID
            && crate::query_boundaries::common::has_call_signatures(self.ctx.types, source_type)
            && self.is_identifier_rhs(right_idx)
        {
            let _ = self.check_assignable_or_report_at_exact_anchor(
                source_type,
                target_type,
                right_idx,
                right_idx,
            );
            return;
        }

        // TS2322 anchoring should point at the assignment target (LHS), not the RHS expression.
        // This aligns diagnostic fingerprints with tsc for assignment-compatibility suites.
        let _ = self.check_assignable_or_report_at(source_type, target_type, right_idx, left_idx);
    }

    fn is_function_reference(&self, node_idx: NodeIndex) -> bool {
        self.is_identifier_rhs(node_idx)
    }

    fn is_identifier_rhs(&self, node_idx: NodeIndex) -> bool {
        let node_idx = self.ctx.arena.skip_parenthesized_and_assertions(node_idx);
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        true
    }

    fn deferred_generic_element_write_target(
        &mut self,
        left_idx: NodeIndex,
        source_type: TypeId,
    ) -> Option<TypeId> {
        if source_type == TypeId::ANY || source_type == TypeId::NEVER {
            return None;
        }

        let node = self.ctx.arena.get(left_idx)?;
        if node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let object_type = self
            .resolve_identifier_symbol(access.expression)
            .and_then(|sym_id| self.assignment_target_declared_type(sym_id))
            .filter(|declared| {
                crate::query_boundaries::common::is_type_parameter(self.ctx.types, *declared)
                    || crate::query_boundaries::common::is_this_type(self.ctx.types, *declared)
            })
            .unwrap_or_else(|| self.get_type_of_node(access.expression));
        if !crate::query_boundaries::common::is_type_parameter(self.ctx.types, object_type) {
            return None;
        }

        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let index_type = self.get_type_of_node(access.name_or_argument);
        self.ctx.preserve_literal_types = prev_preserve;

        if let Some(write_target) =
            self.constraint_keyof_write_target_for_type_param(index_type, object_type)
        {
            return Some(write_target);
        }

        if !self.is_valid_index_for_type_param(index_type, object_type) {
            return None;
        }

        Some(
            self.ctx
                .types
                .factory()
                .index_access(object_type, index_type),
        )
    }

    fn assignment_target_declared_type(&mut self, sym_id: tsz_binder::SymbolId) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let value_decl = symbol.value_declaration;
        if !value_decl.is_some() {
            return None;
        }

        let node = self.ctx.arena.get(value_decl)?;
        if let Some(param) = self.ctx.arena.get_parameter(node)
            && param.type_annotation.is_some()
        {
            return Some(self.get_type_from_type_node(param.type_annotation));
        }

        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
            && var_decl.type_annotation.is_some()
        {
            return Some(self.get_type_from_type_node(var_decl.type_annotation));
        }

        None
    }

    fn assignment_identifier_declared_type(&mut self, idx: NodeIndex) -> Option<TypeId> {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let node = self.ctx.arena.get(idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.ctx.binder.resolve_identifier(self.ctx.arena, idx)?;
        self.assignment_target_declared_type(sym_id)
    }

    fn recursive_tuple_declared_assignment_types(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
    ) -> Option<(TypeId, TypeId)> {
        let target_declared = self.assignment_identifier_declared_type(left_idx)?;
        let source_declared = self.assignment_identifier_declared_type(right_idx)?;

        let (target_base, target_args) = self.application_info_or_display_alias(target_declared)?;
        let (source_base, source_args) = self.application_info_or_display_alias(source_declared)?;
        if target_base != source_base || target_args.len() != source_args.len() {
            return None;
        }

        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, target_base)?;
        let def = self.ctx.definition_store.get(def_id)?;
        let name = self.ctx.types.resolve_atom_ref(def.name);
        if def.kind != tsz_solver::def::DefKind::TypeAlias || name.as_ref() != "TupleOf" {
            return None;
        }

        let has_type_parameter_arg = target_args.iter().chain(source_args.iter()).any(|arg| {
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, *arg)
        });
        if !has_type_parameter_arg || target_args == source_args {
            return None;
        }

        Some((source_declared, target_declared))
    }

    fn declared_same_alias_application_assignment_types(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
    ) -> Option<(TypeId, TypeId)> {
        let target_declared = self.assignment_identifier_declared_type(left_idx)?;
        let source_declared = self.assignment_identifier_declared_type(right_idx)?;

        let (target_base, target_args) = self.application_info_or_display_alias(target_declared)?;
        let (source_base, source_args) = self.application_info_or_display_alias(source_declared)?;
        if target_base != source_base || target_args.len() != source_args.len() {
            return None;
        }

        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, target_base)?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }

        Some((source_declared, target_declared))
    }

    fn declared_application_any_target_accepts(
        &self,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        let Some((source_base, source_args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, source_type)
        else {
            return false;
        };
        let Some((target_base, target_args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, target_type)
        else {
            return false;
        };
        source_base == target_base
            && source_args.len() == target_args.len()
            && target_args.iter().any(|arg| arg.is_any())
            && source_args
                .iter()
                .zip(target_args.iter())
                .all(|(source_arg, target_arg)| target_arg.is_any() || source_arg == target_arg)
    }
}
