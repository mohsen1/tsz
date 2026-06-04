impl<'a> Printer<'a> {
    // =========================================================================
    // Expressions
    // =========================================================================

    pub(in crate::emitter) fn emit_prefix_unary(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr(node) else {
            return;
        };

        // Private field prefix mutation: `++this.#x` or `++(this.#x)`
        // → `__classPrivateFieldSet(this, _C_x, (_a = __classPrivateFieldGet(this, _C_x, "f"), ++_a), "f")`
        if (unary.operator == SyntaxKind::PlusPlusToken as u16
            || unary.operator == SyntaxKind::MinusMinusToken as u16)
            && let Some(pfa) = self.try_extract_private_field_access(unary.operand)
        {
            // For prefix, result is always the new value (same form for statement/value)
            self.emit_private_field_unary_mutation(pfa, unary.operator, true, false);
            return;
        }

        if self.emit_scoped_static_super_update(unary.operand, unary.operator, true) {
            return;
        }

        if (unary.operator == SyntaxKind::PlusPlusToken as u16
            || unary.operator == SyntaxKind::MinusMinusToken as u16)
            && let Some(operand_node) = self.arena.get(unary.operand)
            && operand_node.kind == SyntaxKind::Identifier as u16
        {
            let local_name = self.get_identifier_text_idx(unary.operand);
            if self.emit_system_live_export_prefix_unary(&local_name, unary.operator)
                || self.emit_cjs_live_export_prefix_unary(&local_name, unary.operator)
            {
                return;
            }
        }

        if unary.operator == SyntaxKind::DeleteKeyword as u16
            && !self.ctx.options.target.supports_es2020()
            && self.emit_delete_optional_chain(unary.operand)
        {
            return;
        }

        if (unary.operator == SyntaxKind::PlusPlusToken as u16
            || unary.operator == SyntaxKind::MinusMinusToken as u16)
            && !self.ctx.options.target.supports_es2020()
        {
            let mut tail = Vec::new();
            if let Some((access_kind, base, name_or_argument)) =
                self.collect_update_optional_access(unary.operand, &mut tail)
            {
                self.write(get_operator_text(unary.operator));
                self.emit_update_optional_access(access_kind, base, name_or_argument, &tail);
                return;
            }
        }

        self.write(get_operator_text(unary.operator));
        if unary.operator == SyntaxKind::AsteriskToken as u16 {
            self.write_space();
        }
        // Prevent `+ +x` from collapsing to `++x` (pre-increment) and
        // `- -x` from collapsing to `--x` (pre-decrement). When the operand
        // is also a prefix unary with the same sign (or is `++`/`--`),
        // insert a space to keep the tokens separate.
        if (unary.operator == SyntaxKind::PlusToken as u16
            || unary.operator == SyntaxKind::MinusToken as u16)
            && let Some(operand_node) = self.arena.get(unary.operand)
            && operand_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(inner) = self.arena.get_unary_expr(operand_node)
        {
            let same_sign = inner.operator == unary.operator;
            let is_update = (unary.operator == SyntaxKind::PlusToken as u16
                && inner.operator == SyntaxKind::PlusPlusToken as u16)
                || (unary.operator == SyntaxKind::MinusToken as u16
                    && inner.operator == SyntaxKind::MinusMinusToken as u16);
            if same_sign || is_update {
                self.write_space();
            }
        }
        // Set flag so yield-from-await knows to wrap in parens
        // e.g., `!await x` → `!(yield x)` not `!yield x`
        let prev = self.ctx.flags.in_binary_operand;
        self.ctx.flags.in_binary_operand = true;
        // When lowering optional chains or nullish coalescing (e.g., `++o?.a`, `!(a ?? b)`),
        // the ternary must be wrapped in parens to preserve precedence.
        let prev_optional = self.ctx.flags.optional_chain_needs_parens;
        let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
        self.ctx.flags.optional_chain_needs_parens = true;
        self.ctx.flags.nullish_coalescing_needs_parens = true;
        self.emit(unary.operand);
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        self.ctx.flags.in_binary_operand = prev;
    }

    fn emit_delete_optional_chain(&mut self, operand: NodeIndex) -> bool {
        let mut tail = Vec::new();
        self.emit_delete_optional_chain_inner(operand, &mut tail)
    }

    fn emit_delete_optional_chain_inner(
        &mut self,
        idx: NodeIndex,
        tail: &mut Vec<OptionalChainSegment>,
    ) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            let before_len = self.writer.len();
            let before_tail_len = tail.len();
            self.write("(");
            let emitted = self.emit_delete_optional_chain_inner(paren.expression, tail);
            if emitted {
                self.write(")");
            } else {
                self.writer.truncate(before_len);
                tail.truncate(before_tail_len);
            }
            return emitted;
        }

        if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(node)
        {
            if access.question_dot_token {
                self.emit_delete_optional_access(
                    node.kind,
                    access.expression,
                    access.name_or_argument,
                    tail,
                );
                return true;
            }

            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                tail.push(OptionalChainSegment::Property(access.name_or_argument));
            } else {
                tail.push(OptionalChainSegment::Element(access.name_or_argument));
            }
            return self.emit_delete_optional_chain_inner(access.expression, tail);
        }

        false
    }

    fn emit_delete_optional_access(
        &mut self,
        access_kind: u16,
        base: NodeIndex,
        name_or_argument: NodeIndex,
        tail: &[OptionalChainSegment],
    ) {
        if self.is_simple_nullish_expression(base) {
            self.emit(base);
            self.write(" === null || ");
            self.emit(base);
            self.write(" === void 0 ? true : delete ");
            self.emit(base);
            self.emit_optional_access_segment(access_kind, name_or_argument);
            self.emit_optional_chain_tail(tail);
            return;
        }

        let before = self.writer.len();
        self.emit(base);
        let after = self.writer.len();
        let full = self.writer.get_output().to_string();
        let base_expr = full[before..after].trim_start().to_string();
        self.writer.truncate(before);

        let base_temp = self.make_unique_name_hoisted();
        self.write("(");
        self.write(&base_temp);
        self.write(" = ");
        self.write(&base_expr);
        self.write(") === null || ");
        self.write(&base_temp);
        self.write(" === void 0 ? true : delete ");
        self.write(&base_temp);
        self.emit_optional_access_segment(access_kind, name_or_argument);
        self.emit_optional_chain_tail(tail);
    }

    fn collect_update_optional_access(
        &self,
        idx: NodeIndex,
        tail: &mut Vec<OptionalChainSegment>,
    ) -> Option<(u16, NodeIndex, NodeIndex)> {
        let node = self.arena.get(idx)?;

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            return self.collect_update_optional_access(paren.expression, tail);
        }

        if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(node)
        {
            if access.question_dot_token {
                return Some((node.kind, access.expression, access.name_or_argument));
            }

            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                tail.push(OptionalChainSegment::Property(access.name_or_argument));
            } else {
                tail.push(OptionalChainSegment::Element(access.name_or_argument));
            }
            return self.collect_update_optional_access(access.expression, tail);
        }

        None
    }

    fn emit_update_optional_access(
        &mut self,
        access_kind: u16,
        base: NodeIndex,
        name_or_argument: NodeIndex,
        tail: &[OptionalChainSegment],
    ) {
        self.parenthesized(|this| {
            if this.is_simple_nullish_expression(base) {
                this.emit(base);
                this.write(" === null || ");
                this.emit(base);
                this.write(" === void 0 ? void 0 : ");
                this.emit(base);
                this.emit_optional_access_segment(access_kind, name_or_argument);
                this.emit_optional_chain_tail(tail);
            } else {
                let base_temp = this.make_unique_name_hoisted();
                this.parenthesized(|this| {
                    this.write(&base_temp);
                    this.write(" = ");
                    this.emit(base);
                });
                this.write(" === null || ");
                this.write(&base_temp);
                this.write(" === void 0 ? void 0 : ");
                this.write(&base_temp);
                this.emit_optional_access_segment(access_kind, name_or_argument);
                this.emit_optional_chain_tail(tail);
            }
        });
    }

    fn emit_optional_access_segment(&mut self, access_kind: u16, name_or_argument: NodeIndex) {
        if access_kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            self.write(".");
            self.emit_property_name_without_import_substitution(name_or_argument);
        } else {
            self.open_bracket();
            self.emit(name_or_argument);
            self.close_bracket();
        }
    }

    fn emit_optional_chain_tail(&mut self, tail: &[OptionalChainSegment]) {
        for segment in tail.iter().rev() {
            match segment {
                OptionalChainSegment::Property(name) => {
                    self.write(".");
                    self.emit_property_name_without_import_substitution(*name);
                }
                OptionalChainSegment::Element(argument) => {
                    self.open_bracket();
                    self.emit(*argument);
                    self.close_bracket();
                }
            }
        }
    }

    pub(in crate::emitter) fn emit_postfix_unary(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr(node) else {
            return;
        };

        // Private field postfix mutation: `this.#x++` or `(this.#x)++`
        // Statement form: `__classPrivateFieldSet(this, _C_x, (_a = __classPrivateFieldGet(this, _C_x, "f"), _a++, _a), "f")`
        // Value form: `(__classPrivateFieldSet(this, _C_x, (_b = __classPrivateFieldGet(this, _C_x, "f"), _a = _b++, _b), "f"), _a)`
        if (unary.operator == SyntaxKind::PlusPlusToken as u16
            || unary.operator == SyntaxKind::MinusMinusToken as u16)
            && let Some(pfa) = self.try_extract_private_field_access(unary.operand)
        {
            let is_statement = self.ctx.flags.in_statement_expression;
            self.emit_private_field_unary_mutation(pfa, unary.operator, false, is_statement);
            return;
        }

        if self.emit_scoped_static_super_update(unary.operand, unary.operator, false) {
            return;
        }

        if (unary.operator == SyntaxKind::PlusPlusToken as u16
            || unary.operator == SyntaxKind::MinusMinusToken as u16)
            && !self.ctx.options.target.supports_es2020()
        {
            let mut tail = Vec::new();
            if let Some((access_kind, base, name_or_argument)) =
                self.collect_update_optional_access(unary.operand, &mut tail)
            {
                self.emit_update_optional_access(access_kind, base, name_or_argument, &tail);
                if let Some(operand_node) = self.arena.get(unary.operand) {
                    self.map_token_after_skipping_whitespace(operand_node.end, node.end);
                }
                self.write(get_operator_text(unary.operator));
                return;
            }
        }

        if (unary.operator == SyntaxKind::PlusPlusToken as u16
            || unary.operator == SyntaxKind::MinusMinusToken as u16)
            && let Some(operand_node) = self.arena.get(unary.operand)
            && operand_node.kind == SyntaxKind::Identifier as u16
        {
            let local_name = self.get_identifier_text_idx(unary.operand);
            let is_statement = self.ctx.flags.in_statement_expression;
            if self.emit_system_live_export_postfix_unary(&local_name, unary.operator, is_statement)
                || self.emit_cjs_live_export_postfix_unary(
                    &local_name,
                    unary.operator,
                    is_statement,
                )
            {
                return;
            }
        }

        // When lowering optional chains or nullish coalescing (e.g., `o?.a++`, `(a ?? b)++`),
        // the ternary must be wrapped in parens to preserve precedence.
        let prev_optional = self.ctx.flags.optional_chain_needs_parens;
        let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
        self.ctx.flags.optional_chain_needs_parens = true;
        self.ctx.flags.nullish_coalescing_needs_parens = true;
        self.emit(unary.operand);
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        // Map the postfix operator (e.g., ++ or --) to its source position
        if let Some(operand_node) = self.arena.get(unary.operand) {
            self.map_token_after_skipping_whitespace(operand_node.end, node.end);
        }
        self.write(get_operator_text(unary.operator));
    }

    pub(in crate::emitter) fn emit_new_expression(&mut self, node: &Node) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        if !self.ctx.options.target.supports_es2020()
            && self.emit_invalid_new_optional_chain(call.expression, call.arguments.as_ref())
        {
            return;
        }

        // Private field new: `new this.#C()` → `new (__classPrivateFieldGet(this, _C_C, "f"))()`
        let needs_private_parens = !self.private_field_weakmaps.is_empty()
            && self.arena.get(call.expression).is_some_and(|expr_node| {
                expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && self
                        .arena
                        .get_access_expr(expr_node)
                        .and_then(|access| self.arena.get(access.name_or_argument))
                        .is_some_and(|name_node| {
                            name_node.kind == SyntaxKind::PrivateIdentifier as u16
                        })
            });

        self.write("new ");
        if needs_private_parens {
            self.write("(");
        }
        // Signal new-callee position so `emit_parenthesized` preserves parens
        // around call expressions: `new (x() as T)` → `new (x())` not `new x()`.
        let prev_new = self.paren_in_new_callee;
        self.paren_in_new_callee = true;
        if !self.emit_invalid_new_type_assertion_callee(call.expression) {
            self.emit(call.expression);
        }
        self.paren_in_new_callee = prev_new;
        if needs_private_parens {
            self.write(")");
        }
        if let Some(ref args) = call.arguments {
            // Map opening `(` — scan forward from callee end
            if let Some(expr_node) = self.arena.get(call.expression) {
                self.map_token_after(expr_node.end, node.end, b'(');
            }
            self.write("(");
            // The new expression's own parens provide grouping, so clear
            // the "needs parens" flags to avoid double-parenthesization
            // when an argument contains a downlevel optional chain or
            // nullish coalescing expression.
            let prev_optional = self.ctx.flags.optional_chain_needs_parens;
            let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
            self.ctx.flags.optional_chain_needs_parens = false;
            self.ctx.flags.nullish_coalescing_needs_parens = false;
            let valid_args: Vec<_> = args.nodes.iter().copied().filter(|n| n.is_some()).collect();
            self.emit_comma_separated(&valid_args);
            self.ctx.flags.optional_chain_needs_parens = prev_optional;
            self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
            // Map closing `)` — scan backward from node end
            self.map_closing_paren(node);
            self.write(")");
            return;
        }

        if self.new_expression_has_explicit_parens(node, call.expression) {
            self.write("()");
        }
    }

    fn emit_invalid_new_optional_chain(
        &mut self,
        callee: NodeIndex,
        args: Option<&NodeList>,
    ) -> bool {
        let mut tail = Vec::new();
        let Some((access_kind, base, name_or_argument)) =
            self.collect_invalid_new_optional_access(callee, &mut tail)
        else {
            return false;
        };

        let needs_parens = self.ctx.flags.optional_chain_needs_parens;
        if needs_parens {
            self.open_paren();
            self.ctx.flags.optional_chain_needs_parens = false;
        }
        let temp = self.make_unique_name_hoisted();
        self.open_paren();
        self.write(&temp);
        self.write(" = new ");
        let prev_new = self.paren_in_new_callee;
        self.paren_in_new_callee = true;
        self.emit(base);
        self.paren_in_new_callee = prev_new;
        self.close_paren();
        self.write(" === null || ");
        self.write(&temp);
        self.write(" === void 0 ? void 0 : ");
        self.write(&temp);
        self.emit_optional_access_segment(access_kind, name_or_argument);
        self.emit_optional_chain_tail(&tail);
        if let Some(args) = args {
            self.open_paren();
            let valid_args: Vec<_> = args.nodes.iter().copied().filter(|n| n.is_some()).collect();
            self.emit_comma_separated(&valid_args);
            self.close_paren();
        }
        if needs_parens {
            self.close_paren();
        }
        true
    }

    fn collect_invalid_new_optional_access(
        &self,
        idx: NodeIndex,
        tail: &mut Vec<OptionalChainSegment>,
    ) -> Option<(u16, NodeIndex, NodeIndex)> {
        let node = self.arena.get(idx)?;

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            return self.collect_invalid_new_optional_access(paren.expression, tail);
        }

        if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(node)
        {
            if access.question_dot_token {
                return Some((node.kind, access.expression, access.name_or_argument));
            }

            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                tail.push(OptionalChainSegment::Property(access.name_or_argument));
            } else {
                tail.push(OptionalChainSegment::Element(access.name_or_argument));
            }
            return self.collect_invalid_new_optional_access(access.expression, tail);
        }

        None
    }

    fn emit_invalid_new_type_assertion_callee(&mut self, expression: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expression) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::TYPE_ASSERTION {
            return false;
        }
        let Some(assertion) = self.arena.get_type_assertion(expr_node) else {
            return false;
        };

        self.write(" < ");
        self.emit(assertion.type_node);
        self.write(" > ");
        self.emit(assertion.expression);
        true
    }
}
