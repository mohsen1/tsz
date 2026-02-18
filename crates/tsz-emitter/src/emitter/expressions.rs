use super::{Printer, get_operator_text};
use tsz_parser::parser::{
    NodeIndex,
    node::{AccessExprData, BinaryExprData, Node},
    syntax_kind_ext,
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
        let is_nullish = binary.operator_token == SyntaxKind::QuestionQuestionToken as u16;
        let supports_logical_assignment =
            (self.ctx.options.target as u8) >= (super::ScriptTarget::ES2021 as u8);

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
            self.write_space();
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
        self.write(get_operator_text(unary.operator));
    }

    pub(super) fn emit_call_expression(&mut self, node: &Node) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

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

        self.emit(call.expression);
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
        self.write(")");
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
            self.write("(");
            self.emit_comma_separated(&args.nodes);
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
                        self.emit(access.name_or_argument);
                        self.decrease_indent();
                    } else {
                        // Newline before dot: `expr\n    .name`
                        self.write_line();
                        self.increase_indent();
                        self.write(".");
                        self.emit(access.name_or_argument);
                        self.decrease_indent();
                    }
                } else {
                    self.write(".");
                    self.emit(access.name_or_argument);
                }
                return;
            }
        }

        self.write(".");
        self.emit(access.name_or_argument);
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

        self.emit(access.expression);
        self.write("[");
        self.emit(access.name_or_argument);
        self.write("]");
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
                    self.emit(access.name_or_argument);
                    self.decrease_indent();
                    return;
                } else {
                    self.emit(access.expression);
                    self.write(token);
                    self.emit(access.name_or_argument);
                    return;
                }
            }
        }

        self.emit(access.expression);
        self.write(token);
        self.emit(access.name_or_argument);
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
            self.emit(access.name_or_argument);
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
        self.emit(access.name_or_argument);
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

    pub(super) fn emit_parenthesized(&mut self, node: &Node) {
        let Some(paren) = self.arena.get_parenthesized(node) else {
            return;
        };

        // If the inner expression is a type assertion/as/satisfies expression,
        // the parens were only needed for the TS syntax (e.g., `(<Type>x).foo`).
        // In JS emit, the type assertion is stripped, making the parens unnecessary.
        // EXCEPTION: If the underlying expression (after unwrapping type assertions)
        // is an object literal, parens must be preserved to avoid block ambiguity
        // (e.g., `() => (<Error>{ name: "foo" })` → `() => ({ name: "foo" })`)
        if let Some(inner) = self.arena.get(paren.expression)
            && (inner.kind == syntax_kind_ext::TYPE_ASSERTION
                || inner.kind == syntax_kind_ext::AS_EXPRESSION
                || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
        {
            // Check if unwrapping the type assertion yields an object literal
            if !self.type_assertion_wraps_object_literal(paren.expression) {
                // Safe to strip parens — no block ambiguity
                self.emit(paren.expression);
                return;
            }
            // Fall through to emit with parens preserved
        }

        self.write("(");
        self.emit(paren.expression);
        self.write(")");
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

    pub(super) fn emit_array_literal(&mut self, node: &Node) {
        let Some(array) = self.arena.get_literal_expr(node) else {
            return;
        };

        if array.elements.nodes.is_empty() {
            // Emit any comments inside the brackets (e.g., `[ /* comment */]`).
            let bracket_pos = self.skip_trivia_forward(node.pos, node.end);
            self.write("[");
            if let Some(text) = self.source_text {
                while self.comment_emit_idx < self.all_comments.len() {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_end = self.all_comments[self.comment_emit_idx].end;
                    if c_pos > bracket_pos && c_end < node.end {
                        self.write_space();
                        let comment_text =
                            crate::printer::safe_slice::slice(text, c_pos as usize, c_end as usize);
                        self.write_comment(comment_text);
                        self.comment_emit_idx += 1;
                    } else {
                        break;
                    }
                }
            }
            self.write("]");
            return;
        }

        // Preserve multi-line formatting from source.
        // Check for newlines BETWEEN consecutive elements, not within the overall expression.
        // This avoids treating `[, [\n...\n]]` as multi-line when only the nested array
        // is multi-line, not the outer array's element separation.
        let is_multiline = self.source_text.is_some_and(|text| {
            // Check between consecutive elements for newlines
            for i in 0..array.elements.nodes.len().saturating_sub(1) {
                let curr = array.elements.nodes[i];
                let next = array.elements.nodes[i + 1];
                if let (Some(curr_node), Some(next_node)) =
                    (self.arena.get(curr), self.arena.get(next))
                {
                    let curr_end = std::cmp::min(curr_node.end as usize, text.len());
                    let next_start = std::cmp::min(next_node.pos as usize, text.len());
                    if curr_end <= next_start && text[curr_end..next_start].contains('\n') {
                        return true;
                    }
                }
            }
            // Also check between '[' and first element
            let bracket_pos = self.skip_trivia_forward(node.pos, node.end) as usize;
            if let Some(first_node) = array
                .elements
                .nodes
                .first()
                .and_then(|&n| self.arena.get(n))
            {
                let first_pos = std::cmp::min(first_node.pos as usize, text.len());
                let start = std::cmp::min(bracket_pos, first_pos);
                if start < first_pos && text[start..first_pos].contains('\n') {
                    return true;
                }
            } else if !array.elements.nodes.is_empty() {
                // All elements are NONE (elisions); check for newlines in the array body.
                let end = std::cmp::min(node.end as usize, text.len());
                if bracket_pos + 1 < end && text[bracket_pos + 1..end].contains('\n') {
                    return true;
                }
            }
            false
        });
        let has_trailing_comma = self.has_trailing_comma_in_source(node, &array.elements.nodes);

        if !is_multiline {
            // Emit any inline leading comment before the first element.
            // e.g., `[/* comment */ 1]` or `[/* c */ a, b]`
            // Skip for NONE-first (elision) arrays; those comments are trailing, handled below.
            let bracket_pos = self.skip_trivia_forward(node.pos, node.end);
            let first_elem_is_none = array
                .elements
                .nodes
                .first()
                .is_some_and(|&idx| idx.is_none());
            let first_elem_pos = if first_elem_is_none {
                bracket_pos + 1 // empty range → emit nothing as leading
            } else {
                array
                    .elements
                    .nodes
                    .first()
                    .and_then(|&idx| self.arena.get(idx))
                    .map(|n| n.pos)
                    .unwrap_or(node.end)
            };
            self.write("[");
            self.increase_indent();
            self.emit_unemitted_comments_between(bracket_pos + 1, first_elem_pos);
            self.emit_comma_separated(&array.elements.nodes);
            // Preserve trailing comma for elisions: [,,] must keep both commas
            // Elided elements are represented as NodeIndex::NONE, not OMITTED_EXPRESSION nodes
            if has_trailing_comma || array.elements.nodes.last().is_some_and(|idx| idx.is_none()) {
                self.write(",");
            }
            // Emit any trailing inline comments between last element and ']'.
            // e.g., `[1 /* comment */]` or `[1, /* comment */]`
            if let Some(text) = self.source_text {
                while self.comment_emit_idx < self.all_comments.len() {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_end = self.all_comments[self.comment_emit_idx].end;
                    if c_end < node.end {
                        self.write_space();
                        let comment_text =
                            crate::printer::safe_slice::slice(text, c_pos as usize, c_end as usize);
                        self.write_comment(comment_text);
                        self.comment_emit_idx += 1;
                    } else {
                        break;
                    }
                }
            }
            self.decrease_indent();
            self.write("]");
        } else {
            // Check if the first element is on a new line after '[' in the source.
            // TypeScript preserves the source formatting:
            // - `[elem1,\n  elem2]` -> first element on same line
            // - `[\n  elem1,\n  elem2\n]` -> first element on new line
            let first_elem_on_new_line = self.source_text.is_some_and(|text| {
                if let Some(first_elem) = array.elements.nodes.first() {
                    if let Some(first_node) = self.arena.get(*first_elem) {
                        let bracket_pos = self.skip_trivia_forward(node.pos, node.end) as usize;
                        let first_pos = first_node.pos as usize;
                        let end = std::cmp::min(first_pos, text.len());
                        let start = std::cmp::min(bracket_pos, end);
                        text[start..end].contains('\n')
                    } else {
                        // NONE (elision) first element: check for newline in array body after '['
                        let bracket_pos = self.skip_trivia_forward(node.pos, node.end) as usize;
                        let end = std::cmp::min(node.end as usize, text.len());
                        bracket_pos + 1 < end && text[bracket_pos + 1..end].contains('\n')
                    }
                } else {
                    false
                }
            });

            if first_elem_on_new_line {
                // Format: [\n  elem1,\n  elem2\n]
                //
                // Key invariant: the comma separator for element i is written AFTER
                // element i's content (and any "pre-separator" comments that precede the
                // comma in the source).  This mirrors TypeScript's emitter which treats
                // the separator comma as a pseudo-token with its own leading trivia.
                self.write("[");
                self.increase_indent();
                let elems: Vec<_> = array.elements.nodes.to_vec();
                let last_idx = elems.len().saturating_sub(1);
                for (i, &elem) in elems.iter().enumerate() {
                    let is_elision = elem.is_none();
                    self.write_line();

                    // --- Step A: emit leading comments before this element ---
                    // Only real elements have source positions; elisions don't.
                    if !is_elision {
                        let actual_start = self
                            .arena
                            .get(elem)
                            .map(|n| self.skip_whitespace_forward(n.pos, n.end))
                            .unwrap_or(0);
                        if let Some(text) = self.source_text {
                            while self.comment_emit_idx < self.all_comments.len() {
                                let c_end = self.all_comments[self.comment_emit_idx].end;
                                if c_end <= actual_start {
                                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                                    let comment_text = crate::printer::safe_slice::slice(
                                        text,
                                        c_pos as usize,
                                        c_end as usize,
                                    );
                                    self.write_comment(comment_text);
                                    // Determine separation from what follows (next comment or element):
                                    // if there's a newline between this comment's end and
                                    // actual_start, put on a new line; otherwise add a space.
                                    let c_end_u = c_end as usize;
                                    let gap_has_newline = c_end_u < actual_start as usize
                                        && text[c_end_u..actual_start as usize].contains('\n');
                                    if gap_has_newline {
                                        self.write_line();
                                    } else {
                                        self.write_space();
                                    }
                                    self.comment_emit_idx += 1;
                                } else {
                                    break;
                                }
                            }
                        }
                    }

                    // --- Step B: emit the element ---
                    self.emit(elem);

                    // --- Step C: emit pre-separator comments then write comma ---
                    // Only needed for non-last elements.
                    if i < last_idx {
                        if is_elision {
                            // Elisions have no content; write comma directly.
                            self.write(",");
                        } else {
                            // Find the separator comma in the source that follows this element.
                            let elem_end = self.arena.get(elem).map(|n| n.end).unwrap_or(0);
                            let comma_pos = self.find_comma_pos_after(elem_end, node.end);
                            // Emit any comments between the element's end and the comma.
                            // A comment on its own line → write_line() before it, then ` ,`.
                            // A same-line comment (e.g. `1 /* c */,`) → write_space(), then `,`.
                            let mut wrote_pre_sep = false;
                            let mut last_was_newline_comment = false;
                            if let (Some(sep), Some(text)) = (comma_pos, self.source_text) {
                                while self.comment_emit_idx < self.all_comments.len() {
                                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                                    let c_end = self.all_comments[self.comment_emit_idx].end;
                                    if c_pos >= elem_end && c_end <= sep {
                                        let preceded_by_newline =
                                            self.comment_preceded_by_newline(c_pos);
                                        if preceded_by_newline {
                                            self.write_line();
                                        } else {
                                            self.write_space();
                                        }
                                        let comment_text = crate::printer::safe_slice::slice(
                                            text,
                                            c_pos as usize,
                                            c_end as usize,
                                        );
                                        self.write_comment(comment_text);
                                        wrote_pre_sep = true;
                                        last_was_newline_comment = preceded_by_newline;
                                        self.comment_emit_idx += 1;
                                    } else {
                                        break;
                                    }
                                }
                            }
                            if wrote_pre_sep && last_was_newline_comment {
                                self.write(" ,");
                            } else {
                                self.write(",");
                            }
                        }
                    }
                }

                // Trailing comma for elisions (last element is None) or explicit trailing comma.
                if has_trailing_comma
                    || array.elements.nodes.last().is_some_and(|idx| idx.is_none())
                {
                    self.write(",");
                }

                // Emit any comments that appear between the last element and ']'.
                // Same-line comments (e.g. `, /* comment */`) are written inline with a space;
                // comments on their own line are written with write_line().
                if let Some(text) = self.source_text {
                    while self.comment_emit_idx < self.all_comments.len() {
                        let c_pos = self.all_comments[self.comment_emit_idx].pos;
                        let c_end = self.all_comments[self.comment_emit_idx].end;
                        if c_end <= node.end {
                            if self.comment_preceded_by_newline(c_pos) {
                                self.write_line();
                            } else {
                                self.write_space();
                            }
                            let comment_text = crate::printer::safe_slice::slice(
                                text,
                                c_pos as usize,
                                c_end as usize,
                            );
                            self.write_comment(comment_text);
                            self.comment_emit_idx += 1;
                        } else {
                            break;
                        }
                    }
                }

                self.write_line();
                self.decrease_indent();
                self.write("]");
            } else {
                // Format: [elem1,\n  elem2,\n  elem3]
                self.write("[");
                self.emit(array.elements.nodes[0]);
                self.increase_indent();
                for &elem in &array.elements.nodes[1..] {
                    self.write(",");
                    self.write_line();
                    self.emit(elem);
                }
                // Trailing comma for elisions
                if has_trailing_comma
                    || array.elements.nodes.last().is_some_and(|idx| idx.is_none())
                {
                    self.write(",");
                }
                self.decrease_indent();
                self.write("]");
            }
        }
    }

    pub(super) fn emit_object_literal(&mut self, node: &Node) {
        let Some(obj) = self.arena.get_literal_expr(node) else {
            return;
        };

        if obj.elements.nodes.is_empty() {
            self.write("{}");
            return;
        }

        // ES5 computed/spread lowering is handled via TransformDirective::ES5ObjectLiteral.
        // For ES2015-ES2017 targets, object spread must be lowered to Object.assign().
        // (ES2018+ supports native object spread syntax.)
        {
            use super::ScriptTarget;
            let has_spread = obj.elements.nodes.iter().any(|&idx| {
                self.arena
                    .get(idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
            });
            let target_num = self.ctx.options.target as u32;
            let es2018_num = ScriptTarget::ES2018 as u32;
            if has_spread && target_num < es2018_num {
                // Target is ES2015/ES2016/ES2017: lower to Object.assign()
                let elems: Vec<NodeIndex> = obj.elements.nodes.to_vec();
                self.emit_object_literal_with_object_assign(&elems);
                return;
            }
        }

        // Check if source had a trailing comma after the last element
        let has_trailing_comma = self.has_trailing_comma_in_source(node, &obj.elements.nodes);

        // Preserve single-line formatting from source by looking only at separators
        // between properties (not inside member bodies).
        let source_single_line = self.source_text.is_some_and(|text| {
            let start = std::cmp::min(node.pos as usize, text.len());
            let end = std::cmp::min(node.end as usize, text.len());
            if start >= end || obj.elements.nodes.is_empty() {
                return false;
            }

            let Some(first_node) = self.arena.get(obj.elements.nodes[0]) else {
                return false;
            };
            let first_pos = std::cmp::min(first_node.pos as usize, text.len());
            if start < first_pos && text[start..first_pos].contains('\n') {
                return false;
            }

            for pair in obj.elements.nodes.windows(2) {
                let Some(curr) = self.arena.get(pair[0]) else {
                    continue;
                };
                let Some(next) = self.arena.get(pair[1]) else {
                    continue;
                };
                let curr_end = std::cmp::min(curr.end as usize, text.len());
                let next_pos = std::cmp::min(next.pos as usize, text.len());
                if curr_end < next_pos && text[curr_end..next_pos].contains('\n') {
                    return false;
                }
            }

            let Some(last_node) = obj
                .elements
                .nodes
                .last()
                .and_then(|&idx| self.arena.get(idx))
            else {
                return false;
            };
            let last_end = std::cmp::min(last_node.end as usize, text.len());
            if last_end < end && text[last_end..end].contains('\n') {
                return false;
            }

            true
        });
        let has_multiline_object_member = if obj.elements.nodes.len() == 1 {
            false
        } else {
            obj.elements.nodes.iter().any(|&prop| {
                let Some(prop_node) = self.arena.get(prop) else {
                    return false;
                };

                match prop_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        let Some(method) = self.arena.get_method_decl(prop_node) else {
                            return false;
                        };
                        if method.body.is_none() {
                            return false;
                        }
                        self.node_text_contains_node(method.body)
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR => {
                        let Some(accessor) = self.arena.get_accessor(prop_node) else {
                            return false;
                        };
                        if accessor.body.is_none() {
                            return false;
                        }
                        self.node_text_contains_node(accessor.body)
                    }
                    k if k == syntax_kind_ext::SET_ACCESSOR => {
                        let Some(accessor) = self.arena.get_accessor(prop_node) else {
                            return false;
                        };
                        if accessor.body.is_none() {
                            return false;
                        }
                        self.node_text_contains_node(accessor.body)
                    }
                    _ => false,
                }
            })
        };

        if obj.elements.nodes.len() == 1 {
            let prop = obj.elements.nodes[0];
            let Some(prop_node) = self.arena.get(prop) else {
                return;
            };
            let is_callable_member = prop_node.kind == syntax_kind_ext::METHOD_DECLARATION
                || prop_node.kind == syntax_kind_ext::GET_ACCESSOR
                || prop_node.kind == syntax_kind_ext::SET_ACCESSOR;
            if !is_callable_member {
                // Fall through to the regular object-literal formatter so comments/trailing
                // commas on property assignments are preserved.
            } else {
                let newline_before_prop = self.source_text.is_some_and(|text| {
                    let start = std::cmp::min(node.pos as usize, text.len());
                    let prop_start = std::cmp::min(prop_node.pos as usize, text.len());
                    start < prop_start && text[start..prop_start].contains('\n')
                });
                let mut newline_before_close = self.source_text.is_some_and(|text| {
                    let bytes = text.as_bytes();
                    let mut close = std::cmp::min(node.end as usize, text.len());
                    while close > 0 {
                        close -= 1;
                        if bytes[close] == b'}' {
                            break;
                        }
                    }
                    let prop_end = std::cmp::min(prop_node.end as usize, close);
                    prop_end < close && text[prop_end..close].contains('\n')
                });
                if !newline_before_close {
                    newline_before_close = self.source_text.is_some_and(|text| {
                        let start = std::cmp::min(node.pos as usize, text.len());
                        let mut close = std::cmp::min(node.end as usize, text.len());
                        let bytes = text.as_bytes();
                        while close > 0 {
                            close -= 1;
                            if bytes[close] == b'}' {
                                break;
                            }
                        }
                        if close <= start {
                            return false;
                        }
                        text[start..close].contains('\n')
                    });
                }

                self.write("{");
                if newline_before_prop {
                    self.write_line();
                    self.increase_indent();
                } else {
                    self.write(" ");
                }

                self.emit(prop);
                if has_trailing_comma {
                    self.write(",");
                }

                if newline_before_prop {
                    self.write_line();
                    self.decrease_indent();
                    self.write("}");
                } else if newline_before_close {
                    self.write_line();
                    self.write("}");
                } else {
                    self.write(" }");
                }
                return;
            }
        }

        let should_emit_single_line = source_single_line && !has_multiline_object_member;
        if should_emit_single_line {
            self.write("{ ");
            for (i, &prop) in obj.elements.nodes.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.emit(prop);
            }
            self.write(" }");
        } else {
            // Multi-line format: preserve original line layout from source
            // TSC keeps properties that are on the same line together
            self.write("{");
            self.write_line();
            self.increase_indent();
            for (i, &prop) in obj.elements.nodes.iter().enumerate() {
                let Some(prop_node) = self.arena.get(prop) else {
                    continue;
                };
                self.emit(prop);

                let is_last = i == obj.elements.nodes.len() - 1;
                let has_trailing_line_comment = if is_last {
                    self.source_text.is_some_and(|text| {
                        let start = std::cmp::min(prop_node.end as usize, text.len());
                        let end = std::cmp::min(node.end as usize, text.len());
                        start < end && text[start..end].contains("//")
                    })
                } else {
                    false
                };
                if !is_last || has_trailing_comma || has_trailing_line_comment {
                    if is_last && has_trailing_line_comment {
                        self.write(", ");
                    } else {
                        self.write(",");
                    }
                }

                // Check if next property is on the same line in source
                if !is_last {
                    let next_prop = obj.elements.nodes[i + 1];
                    let wrote_newline = self.emit_unemitted_comments_between(
                        prop_node.end,
                        self.arena.get(next_prop).map_or(prop_node.end, |n| n.pos),
                    );
                    if wrote_newline {
                        // Line comment wrote the newline already — don't add another
                    } else if self.are_on_same_line_in_source(prop, next_prop) {
                        // Keep on same line
                        self.write(" ");
                    } else {
                        // Different lines in source
                        self.write_line();
                    }
                } else {
                    let wrote_newline =
                        self.emit_unemitted_comments_between(prop_node.end, node.end);
                    if !wrote_newline {
                        self.write_line();
                    }
                }
            }
            self.decrease_indent();
            self.write("}");
        }
    }

    pub(super) fn emit_property_assignment(&mut self, node: &Node) {
        let Some(prop) = self.arena.get_property_assignment(node) else {
            return;
        };

        // Shorthand property: parser creates PROPERTY_ASSIGNMENT with name == initializer
        // (same NodeIndex) for { name } instead of SHORTHAND_PROPERTY_ASSIGNMENT
        let is_shorthand = prop.name == prop.initializer;

        // For ES5 target, expand shorthand properties to full form: { x } → { x: x }
        // ES5 doesn't support shorthand property syntax (ES6 feature)
        if is_shorthand && self.ctx.target_es5 {
            self.emit(prop.name);
            self.write(": ");
            self.emit_expression(prop.initializer);
            return;
        }

        // For ES6+ target, preserve shorthand as-is
        if is_shorthand {
            self.emit(prop.name);
            return;
        }

        // Regular property: name: value
        self.emit(prop.name);
        self.write(": ");
        self.emit_expression(prop.initializer);
    }

    pub(super) fn emit_shorthand_property(&mut self, node: &Node) {
        let Some(shorthand) = self.arena.get_shorthand_property(node) else {
            // Fallback: try to get identifier data directly
            if let Some(ident) = self.arena.get_identifier(node) {
                self.write(&ident.escaped_text);
            }
            return;
        };

        // For ES5 target, expand shorthand properties to full form: { x } → { x: x }
        // ES5 doesn't support shorthand property syntax (ES6 feature)
        if self.ctx.target_es5 {
            self.emit(shorthand.name);
            self.write(": ");
            self.emit(shorthand.name);
            if shorthand.equals_token {
                // Object assignment pattern default value would go here
                // For now, this is handled by the destructuring transform
            }
            return;
        }

        // For ES6+ target, emit shorthand as-is
        self.emit(shorthand.name);
        if shorthand.equals_token {
            self.write(" = ");
            // Object assignment pattern default value would go here
        }
    }

    fn node_text_contains_node(&self, node_idx: tsz_parser::parser::NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        self.node_text_contains_newline(node.pos as usize, node.end as usize)
    }

    fn node_text_contains_newline(&self, start: usize, end: usize) -> bool {
        self.source_text
            .is_some_and(|text| start < end && end <= text.len() && text[start..end].contains('\n'))
    }

    /// Emit object literal with spread elements as `Object.assign()` for pre-ES2018 targets.
    ///
    /// TypeScript's object spread lowering for ES2015-ES2017:
    /// - `{ ...a }` → `Object.assign({}, a)`
    /// - `{ x: 1, ...a }` → `Object.assign({ x: 1 }, a)`
    /// - `{ ...a, x: 1 }` → `Object.assign(Object.assign({}, a), { x: 1 })`
    /// - `{ ...a, x: 1, ...b }` → `Object.assign(Object.assign(Object.assign({}, a), { x: 1 }), b)`
    ///
    /// The pattern left-folds: each spread/segment adds one more `Object.assign` wrapping.
    fn emit_object_literal_with_object_assign(&mut self, elements: &[NodeIndex]) {
        // Segment elements into alternating spans of regular props and spread elements.
        // Each segment is either a slice of regular properties or a single spread node.
        #[derive(Clone)]
        enum Seg<'a> {
            Props(&'a [NodeIndex]),
            Spread(NodeIndex),
        }

        let mut segs: Vec<Seg<'_>> = Vec::new();
        let mut seg_start = 0usize;
        for (i, &idx) in elements.iter().enumerate() {
            let is_spread = self
                .arena
                .get(idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::SPREAD_ASSIGNMENT);
            if is_spread {
                if seg_start < i {
                    segs.push(Seg::Props(&elements[seg_start..i]));
                }
                segs.push(Seg::Spread(idx));
                seg_start = i + 1;
            }
        }
        if seg_start < elements.len() {
            segs.push(Seg::Props(&elements[seg_start..]));
        }

        // Count how many Object.assign calls we need:
        // one for each spread + one if the first segment is a spread (needs empty {} seed).
        let num_assign = segs.len();
        // Opening parens for left-folding: (num_assign - 1) calls wrapping the first.
        // Write the opening Object.assign( calls.
        for _ in 0..num_assign.saturating_sub(1) {
            self.write("Object.assign(");
        }

        // Emit the first segment (the "seed" accumulator).
        let first_seg = segs.first().cloned();
        match &first_seg {
            Some(Seg::Props(props)) => {
                self.emit_inline_object_props(props);
            }
            Some(Seg::Spread(spread_idx)) => {
                // Starts with spread: seed is {}
                self.write("Object.assign({}, ");
                self.emit_spread_expression_node(*spread_idx);
                self.write(")");
            }
            None => {
                self.write("{}");
                return;
            }
        }

        // Emit remaining segments, each adding `, seg)` to close one Object.assign.
        for seg in segs.iter().skip(1) {
            self.write(", ");
            match seg {
                Seg::Props(props) => {
                    self.emit_inline_object_props(props);
                }
                Seg::Spread(spread_idx) => {
                    self.emit_spread_expression_node(*spread_idx);
                }
            }
            self.write(")");
        }
    }

    /// Emit `{ prop, prop, ... }` as an inline object literal (no lowering).
    fn emit_inline_object_props(&mut self, props: &[NodeIndex]) {
        self.write("{ ");
        for (i, &prop) in props.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit(prop);
        }
        self.write(" }");
    }

    /// Emit the expression part of a `SPREAD_ASSIGNMENT` node (the `x` in `...x`).
    fn emit_spread_expression_node(&mut self, spread_idx: NodeIndex) {
        if let Some(spread_node) = self.arena.get(spread_idx)
            && let Some(spread) = self.arena.get_spread(spread_node)
        {
            self.emit_expression(spread.expression);
        }
    }
}
