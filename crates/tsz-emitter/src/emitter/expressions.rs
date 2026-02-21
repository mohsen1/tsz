use super::{Printer, get_operator_text};
use tsz_parser::parser::{
    NodeIndex,
    node::{AccessExprData, BinaryExprData, Node},
    node_flags, syntax_kind_ext,
};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Expressions
    // =========================================================================

    pub(super) fn emit_binary_expression(&mut self, node: &Node) {
        let Some(binary) = self.arena.get_binary_expr(node) else {
            return;
        };

        // ES5: lower assignment destructuring patterns
        if self.ctx.target_es5
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && let Some(left_node) = self.arena.get(binary.left)
            && matches!(
                left_node.kind,
                syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    | syntax_kind_ext::ARRAY_BINDING_PATTERN
                    | syntax_kind_ext::OBJECT_BINDING_PATTERN
            )
        {
            self.emit_assignment_destructuring_es5(left_node, binary.right);
            return;
        }

        // ES2015-ES2020: lower logical assignment and nullish-coalescing operators.
        let is_logical_assignment = binary.operator_token
            == SyntaxKind::AmpersandAmpersandEqualsToken as u16
            || binary.operator_token == SyntaxKind::BarBarEqualsToken as u16
            || binary.operator_token == SyntaxKind::QuestionQuestionEqualsToken as u16;
        let is_exponentiation = binary.operator_token == SyntaxKind::AsteriskAsteriskToken as u16;
        let is_exponentiation_assignment =
            binary.operator_token == SyntaxKind::AsteriskAsteriskEqualsToken as u16;
        let is_nullish = binary.operator_token == SyntaxKind::QuestionQuestionToken as u16;
        let supports_logical_assignment =
            (self.ctx.options.target as u8) >= (super::ScriptTarget::ES2021 as u8);

        if (is_exponentiation || is_exponentiation_assignment) && self.ctx.needs_es2016_lowering {
            self.emit_exponentiation_expression(binary);
            return;
        }

        if is_logical_assignment && !supports_logical_assignment {
            self.emit_logical_assignment_expression(binary);
            return;
        }

        if is_nullish && !self.ctx.options.target.supports_es2020() {
            self.emit_nullish_coalescing_expression(binary);
            return;
        }

        let prev_in_binary = self.ctx.flags.in_binary_operand;
        self.ctx.flags.in_binary_operand = true;
        self.emit(binary.left);

        // Check if there's a line break between the operator and the right operand
        // in the source. TypeScript preserves these line breaks.
        let has_newline_before_right = self.source_text.is_some_and(|text| {
            if let (Some(left_node), Some(right_node)) =
                (self.arena.get(binary.left), self.arena.get(binary.right))
            {
                let left_end = left_node.end as usize;
                let right_start = right_node.pos as usize;
                let end = std::cmp::min(right_start, text.len());
                let start = std::cmp::min(left_end, end);
                text[start..end].contains('\n')
            } else {
                false
            }
        });

        // Comma operator: no space before, space after (e.g., `(1, 2, 3)`)
        if binary.operator_token == SyntaxKind::CommaToken as u16 {
            if has_newline_before_right {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.emit(binary.right);
                self.decrease_indent();
                self.ctx.flags.in_binary_operand = prev_in_binary;
                return;
            }
            self.write(", ");
        } else {
            // Map the operator region to its source position (at left operand end,
            // matching tsc's pattern of mapping the transition point)
            if let Some(left_node) = self.arena.get(binary.left) {
                self.map_source_offset(left_node.end);
            }
            self.write(" ");
            self.write(get_operator_text(binary.operator_token));
            if has_newline_before_right {
                self.write_line();
                self.increase_indent();
                self.emit(binary.right);
                self.decrease_indent();
                self.ctx.flags.in_binary_operand = prev_in_binary;
                return;
            }
            self.write_space();
        }
        self.emit(binary.right);
        self.ctx.flags.in_binary_operand = prev_in_binary;
    }

    fn emit_exponentiation_expression(&mut self, binary: &BinaryExprData) {
        if binary.operator_token == SyntaxKind::AsteriskAsteriskEqualsToken as u16 {
            self.emit(binary.left);
            self.write(" = Math.pow(");
            self.emit(binary.left);
            self.write(", ");
            self.emit(binary.right);
            self.write(")");
        } else {
            self.write("Math.pow(");
            self.emit(binary.left);
            self.write(", ");
            self.emit(binary.right);
            self.write(")");
        }
    }

    fn emit_logical_assignment_expression(&mut self, binary: &BinaryExprData) {
        let is_nullish = binary.operator_token == SyntaxKind::QuestionQuestionEqualsToken as u16;
        let is_and = binary.operator_token == SyntaxKind::AmpersandAmpersandEqualsToken as u16;
        let left = self.unwrap_parenthesized_logical_assignment_left(binary.left);

        match self.arena.get(left) {
            Some(left_node) => {
                if left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                {
                    self.emit_logical_assignment_access(
                        left,
                        left_node,
                        is_and,
                        is_nullish,
                        binary.right,
                    );
                    return;
                }
            }
            None => return,
        }

        if is_and {
            self.emit_expression_for_logical_assignment(binary.left);
            self.write(" && (");
            self.emit_expression_for_logical_assignment(binary.left);
            self.write(" = ");
            self.emit_expression_for_logical_assignment(binary.right);
            self.write(")");
        } else if binary.operator_token == SyntaxKind::BarBarEqualsToken as u16 {
            self.emit_expression_for_logical_assignment(binary.left);
            self.write(" || (");
            self.emit_expression_for_logical_assignment(binary.left);
            self.write(" = ");
            self.emit_expression_for_logical_assignment(binary.right);
            self.write(")");
        } else if self.ctx.options.target.supports_es2020() {
            self.emit_expression_for_logical_assignment(binary.left);
            self.write(" ?? (");
            self.emit_expression_for_logical_assignment(binary.left);
            self.write(" = ");
            self.emit_expression_for_logical_assignment(binary.right);
            self.write(")");
        } else {
            self.emit_expression_for_logical_assignment(binary.left);
            self.write(" !== null && ");
            self.emit_expression_for_logical_assignment(binary.left);
            self.write(" !== void 0 ? ");
            self.emit_expression_for_logical_assignment(binary.left);
            self.write(" : (");
            self.emit_expression_for_logical_assignment(binary.left);
            self.write(" = ");
            self.emit_expression_for_logical_assignment(binary.right);
            self.write(")");
        }
    }

    fn unwrap_parenthesized_logical_assignment_left(
        &self,
        mut left: tsz_parser::parser::NodeIndex,
    ) -> tsz_parser::parser::NodeIndex {
        while let Some(left_node) = self.arena.get(left) {
            if left_node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                break;
            }
            let Some(paren) = self.arena.get_parenthesized(left_node) else {
                break;
            };
            if paren.expression.is_none() {
                break;
            }
            left = paren.expression;
        }
        left
    }

    fn emit_logical_assignment_access(
        &mut self,
        left_idx: tsz_parser::parser::NodeIndex,
        left_node: &Node,
        is_and: bool,
        is_nullish: bool,
        right: tsz_parser::parser::NodeIndex,
    ) {
        let Some(left_access) = self.arena.get_access_expr(left_node) else {
            return;
        };

        let access_is_simple = self.is_simple_logical_assignment_lhs(left_idx);
        if access_is_simple {
            if is_and {
                self.emit_expression_for_logical_assignment(left_idx);
                self.write(" && (");
                self.emit_expression_for_logical_assignment(left_idx);
                self.write(" = ");
                self.emit_expression_for_logical_assignment(right);
                self.write(")");
            } else if is_nullish && self.ctx.options.target.supports_es2020() {
                self.emit_expression_for_logical_assignment(left_idx);
                self.write(" ?? (");
                self.emit_expression_for_logical_assignment(left_idx);
                self.write(" = ");
                self.emit_expression_for_logical_assignment(right);
                self.write(")");
            } else if is_nullish {
                let value_temp = self.make_unique_name_hoisted_value();
                self.write("(");
                self.write(&value_temp);
                self.write(" = ");
                self.emit_expression_for_logical_assignment(left_idx);
                self.write(") !== null && ");
                self.write(&value_temp);
                self.write(" !== void 0 ? ");
                self.write(&value_temp);
                self.write(" : (");
                self.emit_expression_for_logical_assignment(left_idx);
                self.write(" = ");
                self.emit_expression_for_logical_assignment(right);
                self.write(")");
            } else {
                self.emit_expression_for_logical_assignment(left_idx);
                self.write(" || (");
                self.emit_expression_for_logical_assignment(left_idx);
                self.write(" = ");
                self.emit_expression_for_logical_assignment(right);
                self.write(")");
            }
            return;
        }

        let is_index_access = left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION;

        if is_nullish {
            if self.ctx.options.target.supports_es2020() {
                let mut base_temp = None;
                let mut index_temp = None;

                if !self.is_simple_logical_assignment_base(left_access.expression) {
                    base_temp = Some(self.make_unique_name_hoisted());
                }

                if left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    && !self.is_simple_logical_assignment_index(left_access.name_or_argument)
                {
                    index_temp = Some(self.make_unique_name_hoisted());
                }

                self.emit_access_reference(
                    left_access,
                    base_temp.as_deref(),
                    index_temp.as_deref(),
                    is_index_access,
                    true,
                );
                self.write(" ?? (");
                self.emit_access_target(
                    left_access,
                    base_temp.as_deref(),
                    index_temp.as_deref(),
                    is_index_access,
                );
                self.write(" = ");
                self.emit_expression_for_logical_assignment(right);
                self.write(")");
            } else {
                let value_temp = self.make_unique_name_hoisted_value();
                let mut base_temp = None;
                let mut index_temp = None;

                if !self.is_simple_logical_assignment_base(left_access.expression) {
                    base_temp = Some(self.make_unique_name_hoisted());
                }

                if left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    && !self.is_simple_logical_assignment_index(left_access.name_or_argument)
                {
                    index_temp = Some(self.make_unique_name_hoisted());
                }

                self.write("(");
                self.write(&value_temp);
                self.write(" = ");
                self.emit_access_reference(
                    left_access,
                    base_temp.as_deref(),
                    index_temp.as_deref(),
                    is_index_access,
                    true,
                );
                self.write(") !== null && ");
                self.write(&value_temp);
                self.write(" !== void 0 ? ");
                self.write(&value_temp);
                self.write(" : (");
                self.emit_access_target(
                    left_access,
                    base_temp.as_deref(),
                    index_temp.as_deref(),
                    is_index_access,
                );
                self.write(" = ");
                self.emit_expression_for_logical_assignment(right);
                self.write(")");
            }
            return;
        }

        let mut base_temp = None;
        let mut index_temp = None;

        if !self.is_simple_logical_assignment_base(left_access.expression) {
            base_temp = Some(self.make_unique_name_hoisted());
        }

        if left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            && !self.is_simple_logical_assignment_index(left_access.name_or_argument)
        {
            index_temp = Some(self.make_unique_name_hoisted());
        }

        self.emit_access_reference(
            left_access,
            base_temp.as_deref(),
            index_temp.as_deref(),
            is_index_access,
            true,
        );
        if is_and {
            self.write(" && (");
        } else {
            self.write(" || (");
        }
        self.emit_access_target(
            left_access,
            base_temp.as_deref(),
            index_temp.as_deref(),
            is_index_access,
        );
        self.write(" = ");
        self.emit_expression_for_logical_assignment(right);
        self.write(")");
    }

    fn emit_access_reference(
        &mut self,
        access: &AccessExprData,
        base_temp: Option<&str>,
        index_temp: Option<&str>,
        is_index_access: bool,
        assign_index: bool,
    ) {
        if let Some(base_name) = base_temp {
            self.write("(");
            self.write(base_name);
            self.write(" = ");
            self.emit(access.expression);
            self.write(")");
        } else {
            self.emit(access.expression);
        }

        if is_index_access || index_temp.is_some() {
            self.write("[");
            if let Some(index_name) = index_temp {
                if assign_index {
                    self.write(index_name);
                    self.write(" = ");
                    self.emit(access.name_or_argument);
                } else {
                    self.write(index_name);
                }
            } else {
                self.emit(access.name_or_argument);
            }
            self.write("]");
            return;
        }

        self.write(".");
        self.emit(access.name_or_argument);
    }

    fn emit_access_target(
        &mut self,
        access: &AccessExprData,
        base_temp: Option<&str>,
        index_temp: Option<&str>,
        is_index_access: bool,
    ) {
        if let Some(base_name) = base_temp {
            self.write(base_name);
        } else {
            self.emit(access.expression);
        }

        if let Some(name_node) = self.arena.get(access.name_or_argument) {
            if is_index_access
                || index_temp.is_some()
                || name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            {
                self.write("[");
                if let Some(index_name) = index_temp {
                    self.write(index_name);
                } else {
                    self.emit(access.name_or_argument);
                }
                self.write("]");
            } else if index_temp.is_some()
                && self.is_simple_logical_assignment_index(access.name_or_argument)
            {
                self.write("[");
                self.write(index_temp.unwrap_or(""));
                self.write("]");
            } else {
                self.write(".");
                self.emit(access.name_or_argument);
            }
        }
    }

    fn emit_expression_for_logical_assignment(&mut self, node_idx: tsz_parser::parser::NodeIndex) {
        let mut current = node_idx;
        let mut node = self.arena.get(current);
        while let Some(n) = node {
            if n.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                let Some(paren) = self.arena.get_parenthesized(n) else {
                    break;
                };

                let Some(inner) = self.arena.get(paren.expression) else {
                    break;
                };

                let should_unwrap = if matches!(
                    inner.kind,
                    syntax_kind_ext::TYPE_ASSERTION
                        | syntax_kind_ext::AS_EXPRESSION
                        | syntax_kind_ext::SATISFIES_EXPRESSION
                        | syntax_kind_ext::FUNCTION_EXPRESSION
                        | syntax_kind_ext::ARROW_FUNCTION
                ) {
                    true
                } else if let Some(binary) = self.arena.get_binary_expr(inner) {
                    binary.operator_token != SyntaxKind::CommaToken as u16
                } else {
                    true
                };

                if should_unwrap {
                    current = paren.expression;
                    node = self.arena.get(current);
                    continue;
                }
            }
            break;
        }

        if let Some(current_node) = self.arena.get(current)
            && let Some(binary) = self.arena.get_binary_expr(current_node)
            && binary.operator_token == SyntaxKind::QuestionQuestionToken as u16
            && !self.ctx.options.target.supports_es2020()
        {
            self.emit_nullish_coalescing_expression_for_logical_assignment(binary);
            return;
        }

        if self.arena.get(current).is_some() {
            self.emit(current);
        }
    }

    fn emit_nullish_coalescing_expression_for_logical_assignment(
        &mut self,
        binary: &BinaryExprData,
    ) {
        if self.is_simple_nullish_expression(binary.left) {
            self.emit(binary.left);
            self.write(" !== null && ");
            self.emit(binary.left);
            self.write(" !== void 0 ? ");
            self.emit(binary.left);
            self.write(" : ");
            self.emit(binary.right);
            return;
        }

        let value_temp = self.make_unique_name_hoisted_value();
        self.write("(");
        self.write(&value_temp);
        self.write(" = ");
        self.emit(binary.left);
        self.write(") !== null && ");
        self.write(&value_temp);
        self.write(" !== void 0 ? ");
        self.write(&value_temp);
        self.write(" : ");
        self.emit(binary.right);
    }

    fn emit_nullish_coalescing_expression(&mut self, binary: &BinaryExprData) {
        if self.is_simple_nullish_expression(binary.left) {
            self.emit(binary.left);
            self.write(" !== null && ");
            self.emit(binary.left);
            self.write(" !== void 0 ? ");
            self.emit(binary.left);
            self.write(" : ");
            self.emit(binary.right);
            return;
        }

        let value_temp = self.get_temp_var_name();
        self.write("(");
        self.write(&value_temp);
        self.write(" = ");
        self.emit(binary.left);
        self.write(") !== null && ");
        self.write(&value_temp);
        self.write(" !== void 0 ? ");
        self.write(&value_temp);
        self.write(" : ");
        self.emit(binary.right);
    }

    fn is_simple_logical_assignment_lhs(&self, node_idx: tsz_parser::parser::NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };

        match node.kind {
            kind if kind == SyntaxKind::Identifier as u16
                || kind == SyntaxKind::ThisKeyword as u16
                || kind == SyntaxKind::SuperKeyword as u16 =>
            {
                true
            }
            kind if kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self
                .arena
                .get_parenthesized(node)
                .is_some_and(|paren| self.is_simple_logical_assignment_lhs(paren.expression)),
            kind if kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let Some(access) = self.arena.get_access_expr(node) else {
                    return false;
                };

                !access.question_dot_token
                    && self.is_simple_logical_assignment_base(access.expression)
                    && self.is_simple_logical_assignment_name(access.name_or_argument)
            }
            kind if kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let Some(access) = self.arena.get_access_expr(node) else {
                    return false;
                };

                !access.question_dot_token
                    && self.is_simple_logical_assignment_base(access.expression)
                    && self.is_simple_logical_assignment_index(access.name_or_argument)
            }
            _ => false,
        }
    }

    fn is_simple_logical_assignment_base(&self, node_idx: tsz_parser::parser::NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };

        matches!(
            node.kind,
            kind if kind == SyntaxKind::Identifier as u16
                || kind == SyntaxKind::ThisKeyword as u16
                || kind == SyntaxKind::SuperKeyword as u16
        )
    }

    fn is_simple_logical_assignment_name(&self, node_idx: tsz_parser::parser::NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        self.arena
            .get_identifier(node)
            .is_some_and(|identifier| !identifier.escaped_text.is_empty())
    }

    fn is_simple_logical_assignment_index(&self, node_idx: tsz_parser::parser::NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };

        node.kind == SyntaxKind::Identifier as u16
            || node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NumericLiteral as u16
    }

    fn is_simple_nullish_expression(&self, node_idx: tsz_parser::parser::NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            return self.is_simple_nullish_expression(paren.expression);
        }

        node.kind == SyntaxKind::Identifier as u16
            || node.kind == SyntaxKind::ThisKeyword as u16
            || node.kind == SyntaxKind::SuperKeyword as u16
    }

    pub(super) fn emit_prefix_unary(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr(node) else {
            return;
        };

        self.write(get_operator_text(unary.operator));
        if unary.operator == SyntaxKind::AsteriskToken as u16 {
            self.write_space();
        }
        // Set flag so yield-from-await knows to wrap in parens
        // e.g., `!await x` → `!(yield x)` not `!yield x`
        let prev = self.ctx.flags.in_binary_operand;
        self.ctx.flags.in_binary_operand = true;
        self.emit(unary.operand);
        self.ctx.flags.in_binary_operand = prev;
    }

    pub(super) fn emit_postfix_unary(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr(node) else {
            return;
        };

        self.emit(unary.operand);
        // Map the postfix operator (e.g., ++ or --) to its source position
        if let Some(operand_node) = self.arena.get(unary.operand) {
            self.map_token_after_skipping_whitespace(operand_node.end, node.end);
        }
        self.write(get_operator_text(unary.operator));
    }

    pub(super) fn emit_call_expression(&mut self, node: &Node) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        if self.is_optional_chain(node) {
            if self.ctx.options.target.supports_es2020() {
                self.emit(call.expression);
                if self.has_optional_call_token(node, call.expression, call.arguments.as_ref()) {
                    self.write("?.");
                }
                self.emit_call_arguments(node, call.arguments.as_ref());
                return;
            }

            let has_optional_call_token =
                self.has_optional_call_token(node, call.expression, call.arguments.as_ref());
            if let Some(call_expr) = self.arena.get(call.expression)
                && (call_expr.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || call_expr.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            {
                self.emit_optional_method_call_expression(
                    call_expr,
                    node,
                    &call.arguments,
                    has_optional_call_token,
                );
                return;
            }

            self.emit_optional_call_expression(node, call.expression, &call.arguments);
            return;
        }

        if self.ctx.target_es5
            && let Some(expr_node) = self.arena.get(call.expression)
        {
            if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(expr_node)
                && let Some(base) = self.arena.get(access.expression)
                && base.kind == SyntaxKind::SuperKeyword as u16
            {
                self.write("_super.prototype.");
                self.emit(access.name_or_argument);
                self.write(".call(");
                if self.ctx.arrow_state.this_capture_depth > 0 {
                    self.write("_this");
                } else {
                    self.write("this");
                }
                if let Some(ref args) = call.arguments {
                    for &arg_idx in &args.nodes {
                        self.write(", ");
                        self.emit(arg_idx);
                    }
                }
                self.write(")");
                return;
            }
            if expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(expr_node)
                && let Some(base) = self.arena.get(access.expression)
                && base.kind == SyntaxKind::SuperKeyword as u16
            {
                self.write("_super.prototype[");
                self.emit(access.name_or_argument);
                self.write("].call(");
                if self.ctx.arrow_state.this_capture_depth > 0 {
                    self.write("_this");
                } else {
                    self.write("this");
                }
                if let Some(ref args) = call.arguments {
                    for &arg_idx in &args.nodes {
                        self.write(", ");
                        self.emit(arg_idx);
                    }
                }
                self.write(")");
                return;
            }
        }

        if self.ctx.is_commonjs()
            && !self.suppress_commonjs_named_import_substitution
            && let Some(expr_node) = self.arena.get(call.expression)
            && let Some(ident) = self.arena.get_identifier(expr_node)
            && let Some(subst) = self
                .commonjs_named_import_substitutions
                .get(&ident.escaped_text)
        {
            let subst = subst.clone();
            self.write("(0, ");
            self.write(&subst);
            self.write(")");
            self.emit_call_arguments(node, call.arguments.as_ref());
            return;
        }

        self.emit(call.expression);
        // Map the opening `(` to its source position
        if let Some(expr_node) = self.arena.get(call.expression) {
            self.map_token_after(expr_node.end, node.end, b'(');
        }
        self.write("(");
        if let Some(ref args) = call.arguments {
            // For the first argument, emit any comments between '(' and the argument
            // This handles: func(/*comment*/ arg)
            if let Some(first_arg) = args.nodes.first()
                && let Some(arg_node) = self.arena.get(*first_arg)
            {
                // Use node.end of the call expression to approximate '(' position
                // Actually, we need to find the '(' position more carefully
                let paren_pos = self.find_open_paren_position(node.pos, arg_node.pos);
                self.emit_unemitted_comments_between(paren_pos, arg_node.pos);
            }
            self.emit_comma_separated(&args.nodes);
        }
        // Map the closing `)` to its source position
        self.map_closing_paren(node);
        self.write(")");
    }

    fn emit_call_arguments(&mut self, node: &Node, args: Option<&tsz_parser::parser::NodeList>) {
        self.write("(");
        if let Some(args) = args {
            if let Some(first_arg) = args.nodes.first()
                && let Some(arg_node) = self.arena.get(*first_arg)
            {
                let paren_pos = self.find_open_paren_position(node.pos, arg_node.pos);
                self.emit_unemitted_comments_between(paren_pos, arg_node.pos);
            }
            self.emit_comma_separated(&args.nodes);
        }
        self.write(")");
    }

    fn emit_optional_call_expression(
        &mut self,
        node: &Node,
        callee: NodeIndex,
        args: &Option<tsz_parser::parser::NodeList>,
    ) {
        if self.is_simple_nullish_expression(callee) {
            self.emit(callee);
            self.write(" === null || ");
            self.emit(callee);
            self.write(" === void 0 ? void 0 : ");
            self.emit(callee);
            self.emit_call_arguments(node, args.as_ref());
        } else {
            let temp = self.get_temp_var_name();
            self.write("(");
            self.write(&temp);
            self.write(" = ");
            self.emit(callee);
            self.write(")");
            self.write(" === null || ");
            self.write(&temp);
            self.write(" === void 0 ? void 0 : ");
            self.write(&temp);
            self.emit_call_arguments(node, args.as_ref());
        }
    }

    fn emit_optional_method_call_expression(
        &mut self,
        access_node: &Node,
        call_node: &Node,
        args: &Option<tsz_parser::parser::NodeList>,
        has_optional_call_token: bool,
    ) {
        let Some(access) = self.arena.get_access_expr(access_node) else {
            return;
        };

        if !has_optional_call_token {
            let this_temp = self.get_temp_var_name();
            self.write("(");
            self.write(&this_temp);
            self.write(" = ");
            self.emit(access.expression);
            self.write(")");
            if access.question_dot_token {
                self.write(" === null || ");
                self.write(&this_temp);
                self.write(" === void 0 ? void 0 : ");
            }

            if access.question_dot_token {
                self.write(&this_temp);
            }
            if access_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.write(".");
                self.emit(access.name_or_argument);
            } else {
                self.write("[");
                self.emit(access.name_or_argument);
                self.write("]");
            }
            self.emit_call_arguments(call_node, args.as_ref());
            return;
        }

        let this_temp = self.get_temp_var_name();
        let func_temp = self.get_temp_var_name();

        self.write("(");
        self.write(&func_temp);
        self.write(" = ");
        self.write("(");
        self.write(&this_temp);
        self.write(" = ");
        self.emit(access.expression);
        self.write(")");
        if access.question_dot_token {
            self.write(" === null || ");
            self.write(&this_temp);
            self.write(" === void 0 ? void 0 : ");
        }
        if access_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if access.question_dot_token {
                self.write(&this_temp);
            }
            self.write(".");
            self.emit(access.name_or_argument);
        } else {
            if access.question_dot_token {
                self.write(&this_temp);
            }
            self.write("[");
            self.emit(access.name_or_argument);
            self.write("]");
        }
        self.write(") === null || ");
        self.write(&func_temp);
        self.write(" === void 0 ? void 0 : ");
        self.write(&func_temp);
        self.write(".call(");
        self.write(&this_temp);
        self.emit_optional_call_tail_arguments(args.as_ref());
        self.write(")");
    }

    fn emit_optional_call_tail_arguments(&mut self, args: Option<&tsz_parser::parser::NodeList>) {
        if let Some(args) = args
            && !args.nodes.is_empty()
        {
            self.write(", ");
            self.emit_comma_separated(&args.nodes);
        }
        self.write(")");
    }

    const fn is_optional_chain(&self, node: &Node) -> bool {
        (node.flags as u32) & node_flags::OPTIONAL_CHAIN != 0
    }

    fn has_optional_call_token(
        &self,
        call_node: &Node,
        callee: NodeIndex,
        args: Option<&tsz_parser::parser::NodeList>,
    ) -> bool {
        let Some(source) = self.source_text_for_map() else {
            let Some(callee_node) = self.arena.get(callee) else {
                return false;
            };
            if self.arena.get_access_expr(callee_node).is_none() {
                return true;
            }
            return false;
        };

        let Some(callee_node) = self.arena.get(callee) else {
            return false;
        };
        let Some(open_paren) = self.find_call_open_paren_position(call_node, args) else {
            return false;
        };

        let bytes = source.as_bytes();
        let mut i = std::cmp::min(open_paren as usize, source.len());
        let start = std::cmp::min(callee_node.pos as usize, source.len());

        while i > start {
            if i == 0 {
                break;
            }
            match bytes[i - 1] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    i -= 1;
                }
                b'/' if i >= 2 && bytes[i - 2] == b'/' => {
                    while i > start && bytes[i - 1] != b'\n' {
                        i -= 1;
                    }
                    if i > start {
                        i -= 1;
                    }
                }
                b'/' if i >= 2 && bytes[i - 2] == b'*' => {
                    if i >= 2 {
                        i -= 2;
                    }
                    while i >= 2 && !(bytes[i - 2] == b'*' && bytes[i - 1] == b'/') {
                        i -= 1;
                    }
                    if i >= 2 {
                        i -= 2;
                    }
                }
                b'?' if i >= 2 && bytes[i - 2] == b'.' => {
                    return true;
                }
                b'.' if i >= 2 && bytes[i - 2] == b'?' && bytes[i - 1] == b'.' => {
                    return true;
                }
                _ => return false,
            }
        }

        false
    }

    fn find_call_open_paren_position(
        &self,
        call_node: &Node,
        args: Option<&tsz_parser::parser::NodeList>,
    ) -> Option<u32> {
        let text = self.source_text_for_map()?;
        let bytes = text.as_bytes();
        let start = std::cmp::min(call_node.pos as usize, bytes.len());
        let mut end = std::cmp::min(call_node.end as usize, bytes.len());
        if let Some(args) = args
            && let Some(first) = args.nodes.first()
            && let Some(first_node) = self.arena.get(*first)
        {
            end = std::cmp::min(first_node.pos as usize, end);
        }
        (start..end)
            .position(|i| bytes[i] == b'(')
            .map(|offset| (start + offset) as u32)
    }

    /// Find the position of the opening parenthesis in a call expression.
    /// Scans forward from `start_pos` looking for '(' before `arg_pos`.
    fn find_open_paren_position(&self, start_pos: u32, arg_pos: u32) -> u32 {
        let Some(text) = self.source_text else {
            return start_pos;
        };
        let bytes = text.as_bytes();
        let start = start_pos as usize;
        let end = std::cmp::min(arg_pos as usize, bytes.len());

        if let Some(offset) = (start..end).position(|i| bytes[i] == b'(') {
            return (start + offset) as u32;
        }
        start_pos
    }

    pub(super) fn emit_new_expression(&mut self, node: &Node) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        self.write("new ");
        self.emit(call.expression);
        if let Some(ref args) = call.arguments {
            // Map opening `(` — scan forward from callee end
            if let Some(expr_node) = self.arena.get(call.expression) {
                self.map_token_after(expr_node.end, node.end, b'(');
            }
            self.write("(");
            self.emit_comma_separated(&args.nodes);
            // Map closing `)` — scan backward from node end
            self.map_closing_paren(node);
            self.write(")");
            return;
        }

        if self.new_expression_has_explicit_parens(node, call.expression) {
            self.write("()");
        }
    }

    fn new_expression_has_explicit_parens(
        &self,
        node: &Node,
        callee: tsz_parser::parser::NodeIndex,
    ) -> bool {
        let Some(source) = self.source_text else {
            return false;
        };

        let Some(callee_node) = self.arena.get(callee) else {
            return false;
        };

        let bytes = source.as_bytes();
        let mut i = callee_node.end as usize;
        let end = std::cmp::min(node.end as usize, source.len());

        while i < end {
            match bytes[i] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    i += 1;
                }
                b'/' if i + 1 < end && bytes[i + 1] == b'/' => {
                    // Line comment: skip to end of line
                    while i < end && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < end && bytes[i + 1] == b'*' => {
                    // Block comment: skip to closing */
                    i += 2;
                    while i + 1 < end && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        i += 1;
                    }
                    if i + 1 < end {
                        i += 2;
                    }
                }
                b'(' => return true,
                _ => return false,
            }
        }
        false
    }

    pub(super) fn emit_property_access(&mut self, node: &Node) {
        let Some(access) = self.arena.get_access_expr(node) else {
            return;
        };

        if access.question_dot_token {
            if self.ctx.options.target.supports_es2020() {
                self.emit_optional_property_access(access, "?.");
            } else {
                self.emit_optional_property_access_downlevel(access);
            }
            return;
        }

        if self.emit_parenthesized_object_literal_access(access.expression, |this| {
            this.write(".");
            this.emit_property_name_without_import_substitution(access.name_or_argument);
        }) {
            return;
        }

        self.emit(access.expression);

        // Preserve multi-line property access chains from the original source.
        // TypeScript preserves the original line break pattern. If there's a
        // newline between expression end and the property name, we need to
        // reproduce the original layout:
        // - If dot is before newline: `expr.\n    name` -> emit ".\n    name"
        // - If dot is after newline: `expr\n    .name` -> emit "\n    .name"
        if let Some(text) = self.source_text
            && let Some(expr_node) = self.arena.get(access.expression)
            && let Some(name_node) = self.arena.get(access.name_or_argument)
        {
            let expr_end = expr_node.end as usize;
            let name_start = name_node.pos as usize;
            let between_end = std::cmp::min(name_start, text.len());
            let between_start = std::cmp::min(expr_end, between_end);
            let between = &text[between_start..between_end];
            if between.contains('\n') {
                // Find where the dot is relative to the newline
                if let Some(dot_pos) = between.find('.') {
                    let after_dot = &between[dot_pos + 1..];
                    if after_dot.contains('\n') {
                        // Dot before newline: `expr.\n    name`
                        self.write(".");
                        self.write_line();
                        self.increase_indent();
                        self.emit_property_name_without_import_substitution(
                            access.name_or_argument,
                        );
                        self.decrease_indent();
                    } else {
                        // Newline before dot: `expr\n    .name`
                        self.write_line();
                        self.increase_indent();
                        self.write(".");
                        self.emit_property_name_without_import_substitution(
                            access.name_or_argument,
                        );
                        self.decrease_indent();
                    }
                } else {
                    self.write(".");
                    self.emit_property_name_without_import_substitution(access.name_or_argument);
                }
                return;
            }
        }

        // Map the `.` token to its source position
        if let Some(expr_node) = self.arena.get(access.expression) {
            self.map_token_after(expr_node.end, node.end, b'.');
        }
        self.write(".");
        self.emit_property_name_without_import_substitution(access.name_or_argument);
    }

    pub(super) fn emit_element_access(&mut self, node: &Node) {
        let Some(access) = self.arena.get_access_expr(node) else {
            return;
        };

        if access.question_dot_token {
            if self.ctx.options.target.supports_es2020() {
                self.emit(access.expression);
                self.write("?.[");
                self.emit(access.name_or_argument);
                self.write("]");
            } else {
                self.emit_optional_element_access_downlevel(access);
            }
            return;
        }

        if self.emit_parenthesized_object_literal_access(access.expression, |this| {
            this.write("[");
            this.emit(access.name_or_argument);
            this.write("]");
        }) {
            return;
        }

        self.emit(access.expression);
        self.write("[");
        self.emit(access.name_or_argument);
        self.write("]");
    }

    fn emit_parenthesized_object_literal_access<F>(
        &mut self,
        expr: NodeIndex,
        emit_suffix: F,
    ) -> bool
    where
        F: FnOnce(&mut Self),
    {
        let Some(expr_node) = self.arena.get(expr) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return false;
        }
        let Some(paren) = self.arena.get_parenthesized(expr_node) else {
            return false;
        };
        if !self.type_assertion_wraps_object_literal(paren.expression) {
            return false;
        }

        self.write("(");
        self.emit(paren.expression);
        emit_suffix(self);
        self.write(")");
        true
    }

    fn emit_optional_property_access(&mut self, access: &AccessExprData, token: &str) {
        if let Some(text) = self.source_text
            && let Some(expr_node) = self.arena.get(access.expression)
            && let Some(name_node) = self.arena.get(access.name_or_argument)
        {
            let expr_end = expr_node.end as usize;
            let name_start = name_node.pos as usize;
            let between_end = std::cmp::min(name_start, text.len());
            let between_start = std::cmp::min(expr_end, between_end);
            let between = &text[between_start..between_end];
            if between.contains('\n')
                && let Some(dot_pos) = between.find('.')
            {
                let after_dot = &between[dot_pos + 1..];
                if after_dot.contains('\n') {
                    self.emit(access.expression);
                    self.write(token);
                    self.write_line();
                    self.increase_indent();
                    self.emit_property_name_without_import_substitution(access.name_or_argument);
                    self.decrease_indent();
                    return;
                } else {
                    self.emit(access.expression);
                    self.write(token);
                    self.emit_property_name_without_import_substitution(access.name_or_argument);
                    return;
                }
            }
        }

        self.emit(access.expression);
        self.write(token);
        self.emit_property_name_without_import_substitution(access.name_or_argument);
    }

    fn emit_optional_property_access_downlevel(&mut self, access: &AccessExprData) {
        let base_simple = self.is_simple_nullish_expression(access.expression);
        if base_simple {
            self.emit(access.expression);
            self.write(" === null || ");
            self.emit(access.expression);
            self.write(" === void 0 ? void 0 : ");
            self.emit(access.expression);
            self.write(".");
            self.emit_property_name_without_import_substitution(access.name_or_argument);
            return;
        }

        let base_temp = self.get_temp_var_name();
        self.write("(");
        self.write(&base_temp);
        self.write(" = ");
        self.emit(access.expression);
        self.write(")");
        self.write(" === null || ");
        self.write(&base_temp);
        self.write(" === void 0 ? void 0 : ");
        self.write(&base_temp);
        self.write(".");
        self.emit_property_name_without_import_substitution(access.name_or_argument);
    }

    fn emit_optional_element_access_downlevel(&mut self, access: &AccessExprData) {
        let base_simple = self.is_simple_nullish_expression(access.expression);
        if base_simple {
            self.emit(access.expression);
            self.write(" === null || ");
            self.emit(access.expression);
            self.write(" === void 0 ? void 0 : ");
            self.emit(access.expression);
            self.write("[");
            self.emit(access.name_or_argument);
            self.write("]");
            return;
        }

        let base_temp = self.get_temp_var_name();
        self.write("(");
        self.write(&base_temp);
        self.write(" = ");
        self.emit(access.expression);
        self.write(")");
        self.write(" === null || ");
        self.write(&base_temp);
        self.write(" === void 0 ? void 0 : ");
        self.write(&base_temp);
        self.write("[");
        self.emit(access.name_or_argument);
        self.write("]");
    }

    fn emit_property_name_without_import_substitution(&mut self, node: NodeIndex) {
        let prev = self.suppress_commonjs_named_import_substitution;
        self.suppress_commonjs_named_import_substitution = true;
        self.emit(node);
        self.suppress_commonjs_named_import_substitution = prev;
    }

    pub(super) fn emit_parenthesized(&mut self, node: &Node) {
        let Some(paren) = self.arena.get_parenthesized(node) else {
            return;
        };

        // If the inner expression is a type assertion/as/satisfies expression,
        // the parens were only needed for the TS syntax (e.g., `(<Type>x).foo`).
        // In JS emit, the type assertion is stripped, making the parens unnecessary
        // UNLESS the underlying expression (after unwrapping type assertions) is:
        //   - An object literal (block ambiguity)
        //   - A binary/complex expression (operator precedence would change)
        if let Some(inner) = self.arena.get(paren.expression)
            && (inner.kind == syntax_kind_ext::TYPE_ASSERTION
                || inner.kind == syntax_kind_ext::AS_EXPRESSION
                || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
        {
            let unwrapped_kind = self.unwrap_type_assertion_kind(paren.expression);
            // Only strip parens if the unwrapped expression is a simple primary that
            // cannot change meaning without parens: identifiers, property access,
            // element access, `this`, template literals, literals, class/function expr.
            let can_strip = matches!(
                unwrapped_kind,
                Some(k) if k == SyntaxKind::Identifier as u16
                    || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    || k == SyntaxKind::ThisKeyword as u16
                    || k == SyntaxKind::SuperKeyword as u16
                    || k == SyntaxKind::NullKeyword as u16
                    || k == SyntaxKind::TrueKeyword as u16
                    || k == SyntaxKind::FalseKeyword as u16
                    || k == SyntaxKind::NumericLiteral as u16
                    || k == SyntaxKind::BigIntLiteral as u16
                    || k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::RegularExpressionLiteral as u16
                    || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::NON_NULL_EXPRESSION
                    || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            );

            if can_strip {
                // Safe to strip parens
                self.emit(paren.expression);
                return;
            }

            // Check if the unwrapped expression is already parenthesized
            if self.type_assertion_result_is_parenthesized(paren.expression) {
                self.emit(paren.expression);
                return;
            }
            // Fall through to emit with parens preserved
        }

        // If the inner expression is another ParenExpr, avoid double-parenthesization
        // when the inner already handles object literal protection.
        if let Some(inner) = self.arena.get(paren.expression)
            && inner.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(inner_paren) = self.arena.get_parenthesized(inner)
            && let Some(inner_inner) = self.arena.get(inner_paren.expression)
            && (inner_inner.kind == syntax_kind_ext::TYPE_ASSERTION
                || inner_inner.kind == syntax_kind_ext::AS_EXPRESSION
                || inner_inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && self.type_assertion_wraps_object_literal(inner_paren.expression)
        {
            // The inner ParenExpr already preserves parens for the object literal.
            // Our outer parens are redundant.
            self.emit(paren.expression);
            return;
        }

        self.write("(");
        self.emit(paren.expression);
        self.write(")");
    }

    /// Unwrap type assertion chain and return the kind of the underlying expression.
    fn unwrap_type_assertion_kind(&self, mut idx: NodeIndex) -> Option<u16> {
        loop {
            let node = self.arena.get(idx)?;
            match node.kind {
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    if let Some(ta) = self.arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                _ => return Some(node.kind),
            }
        }
    }

    /// Check if unwrapping a type assertion chain yields a parenthesized expression.
    /// Used to detect redundant outer parens: `((<Error>({...})))` → the type
    /// assertion wraps `({...})` which is already parenthesized, so outer parens
    /// are redundant.
    fn type_assertion_result_is_parenthesized(&self, mut idx: NodeIndex) -> bool {
        // Unwrap type assertions to find the underlying expression
        loop {
            let Some(node) = self.arena.get(idx) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    if let Some(ta) = self.arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => return true,
                _ => return false,
            }
        }
    }

    /// Check if a type assertion/as/satisfies chain ultimately wraps an object literal.
    fn type_assertion_wraps_object_literal(&self, mut idx: NodeIndex) -> bool {
        loop {
            let Some(node) = self.arena.get(idx) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    if let Some(ta) = self.arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(p) = self.arena.get_parenthesized(node) {
                        idx = p.expression;
                    } else {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => return true,
                _ => return false,
            }
        }
    }

    pub(super) fn emit_type_assertion_expression(&mut self, node: &Node) {
        let Some(assertion) = self.arena.get_type_assertion(node) else {
            self.write("void 0");
            return;
        };

        self.emit_expression(assertion.expression);
    }

    pub(super) fn emit_non_null_expression(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr_ex(node) else {
            self.write("void 0");
            return;
        };

        self.emit_expression(unary.expression);
    }

    pub(super) fn emit_conditional(&mut self, node: &Node) {
        let Some(cond) = self.arena.get_conditional_expr(node) else {
            return;
        };

        let prev = self.ctx.flags.in_binary_operand;
        self.ctx.flags.in_binary_operand = true;
        self.emit(cond.condition);
        self.write(" ? ");
        self.emit(cond.when_true);
        self.write(" : ");
        self.emit(cond.when_false);
        self.ctx.flags.in_binary_operand = prev;
    }
}
