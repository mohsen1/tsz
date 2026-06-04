impl<'a> CheckerState<'a> {
    /// Check if all properties of an object literal are assignable to the
    /// target type when using literal types from the initializers. This catches
    /// cases where the widened object type (e.g., `{ kind: string }`) fails
    /// assignability against a discriminated union, but the literal property
    /// values (e.g., `"bluray"`) actually match a union member.
    fn all_object_literal_properties_assignable_with_literals(
        &mut self,
        obj_idx: NodeIndex,
        target_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let obj_node = match self.ctx.arena.get(obj_idx) {
            Some(node) if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => node,
            _ => return false,
        };

        let obj = match self.ctx.arena.get_literal_expr(obj_node) {
            Some(obj) => obj.clone(),
            None => return false,
        };

        if obj.elements.nodes.is_empty() {
            return false;
        }

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_property_assignment(elem_node) {
                        Some(prop) => (prop.name, prop.initializer),
                        None => continue,
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_shorthand_property(elem_node) {
                        Some(prop) => (prop.name, prop.name),
                        None => continue,
                    }
                }
                _ => continue,
            };

            let Some(prop_name) = self.object_literal_property_name_text(prop_name_idx) else {
                continue;
            };

            let Some((target_prop_type, _)) =
                self.object_literal_target_property_type(target_type, prop_name_idx, &prop_name)
            else {
                // Target doesn't have this property — can't confirm assignability
                return false;
            };

            if target_prop_type == TypeId::ERROR || target_prop_type == TypeId::ANY {
                continue;
            }

            // Try literal type first, then cached type
            let source_prop_type =
                if let Some(literal_type) = self.literal_type_from_initializer(prop_value_idx) {
                    literal_type
                } else {
                    self.get_type_of_node(prop_value_idx)
                };

            if source_prop_type == TypeId::ERROR || source_prop_type == TypeId::ANY {
                continue;
            }

            if !self
                .call_arg_relation_outcome(source_prop_type, target_prop_type)
                .related
            {
                return false;
            }
        }

        true
    }

    /// Returns true if `idx` resolves to an `OBJECT_LITERAL_EXPRESSION` after
    /// peeling parenthesized and comma-expression wrappers. Used to gate the
    /// var-decl elaboration entry so unrelated initializers (`null as any`,
    /// identifiers, ...) skip the elaboration path entirely. Calling
    /// `is_assignable_to` on those has cache side-effects that perturb
    /// downstream JSX/contextual-typing decisions.
    pub fn initializer_reaches_object_literal_through_wrappers(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = idx;
        for _ in 0..16 {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return true;
            }
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.ctx.arena.get_binary_expr(node)
                && bin.operator_token == SyntaxKind::CommaToken as u16
            {
                current = bin.right;
                continue;
            }
            return false;
        }
        false
    }

    /// Elaborate object literal property mismatches for variable declarations.
    ///
    /// Walks through parentheses and comma expressions to find the inner
    /// object literal: `var x: T = (1, 2, { ... })` and `var x: T = ({...})`
    /// both still drill into the trailing object literal. tsc anchors the
    /// per-property TS2322 to the deepest offending leaf inside the
    /// initializer's object literal regardless of these wrappers.
    pub fn try_elaborate_object_literal_properties_for_var_init(
        &mut self,
        init_idx: NodeIndex,
        declared_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = init_idx;
        for _ in 0..16 {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                // Inner per-property diagnostics suppress the outer whole-object error (tsc parity).
                if self.object_literal_has_inner_property_diagnostics(current) {
                    return true;
                }
                return self.try_elaborate_object_literal_properties(current, declared_type);
            }
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.ctx.arena.get_binary_expr(node)
                && bin.operator_token == SyntaxKind::CommaToken as u16
            {
                current = bin.right;
                continue;
            }
            return false;
        }
        false
    }

    /// True if a TS2322/TS2353/TS1360 diagnostic is anchored inside any of this object literal's property spans.
    fn object_literal_has_inner_property_diagnostics(&self, obj_idx: NodeIndex) -> bool {
        let Some(obj_node) = self.ctx.arena.get(obj_idx) else {
            return false;
        };
        let Some(obj) = self.ctx.arena.get_literal_expr(obj_node) else {
            return false;
        };
        for &elem_idx in &obj.elements.nodes {
            let Some((start, end)) = self.ctx.get_node_span(elem_idx) else {
                continue;
            };
            if self.ctx.diagnostics.iter().any(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                        | diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                        | diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID
                        | diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE
                ) && diag.start >= start
                    && diag.start < end
            }) {
                return true;
            }
        }
        false
    }

    /// Elaborate array literal element mismatches for variable declarations.
    pub fn try_elaborate_initializer_elements(
        &mut self,
        init_type: TypeId,
        declared_type: TypeId,
        init_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let init_node = match self.ctx.arena.get(init_idx) {
            Some(node) if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => node,
            _ => return false,
        };

        // Only elaborate when the overall assignment fails.
        if self
            .variable_initializer_relation_outcome(init_type, declared_type)
            .related
        {
            return false;
        }

        // Arity mismatch — report at whole-assignment level, not per-element.
        if let Some(arr) = self.ctx.arena.get_literal_expr(init_node) {
            let source_count = arr.elements.nodes.len();
            if let Some(target_count) = crate::query_boundaries::common::get_fixed_tuple_length(
                self.ctx.types,
                declared_type,
            ) && source_count > target_count
            {
                return false;
            }
        }

        // Delegate to array literal element elaboration
        self.try_elaborate_array_literal_elements(init_idx, declared_type)
    }
}
