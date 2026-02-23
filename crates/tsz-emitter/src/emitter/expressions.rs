use super::{Printer, get_operator_text};
use tsz_parser::parser::{
    NodeIndex,
    node::{AccessExprData, Node},
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

        // Assignment and comma operators accept AssignmentExpression operands,
        // which includes YieldExpression. So yield-from-await doesn't need
        // parens in those positions. Only non-assignment, non-comma binary
        // operators need the in_binary_operand flag to trigger yield wrapping.
        let op = binary.operator_token;
        let is_assignment_or_comma = op == SyntaxKind::CommaToken as u16
            || op == SyntaxKind::EqualsToken as u16
            || op == SyntaxKind::PlusEqualsToken as u16
            || op == SyntaxKind::MinusEqualsToken as u16
            || op == SyntaxKind::AsteriskEqualsToken as u16
            || op == SyntaxKind::SlashEqualsToken as u16
            || op == SyntaxKind::PercentEqualsToken as u16
            || op == SyntaxKind::AsteriskAsteriskEqualsToken as u16
            || op == SyntaxKind::LessThanLessThanEqualsToken as u16
            || op == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
            || op == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
            || op == SyntaxKind::AmpersandEqualsToken as u16
            || op == SyntaxKind::CaretEqualsToken as u16
            || op == SyntaxKind::BarEqualsToken as u16
            || op == SyntaxKind::BarBarEqualsToken as u16
            || op == SyntaxKind::AmpersandAmpersandEqualsToken as u16
            || op == SyntaxKind::QuestionQuestionEqualsToken as u16;

        let prev_in_binary = self.ctx.flags.in_binary_operand;
        if !is_assignment_or_comma {
            self.ctx.flags.in_binary_operand = true;
        }
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

    pub(super) fn emit_prefix_unary(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr(node) else {
            return;
        };

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

        // Signal access position so `(new a)()` keeps parens (vs `new a()`).
        let prev = self.paren_in_access_position;
        self.paren_in_access_position = true;
        self.emit(call.expression);
        self.paren_in_access_position = prev;
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
            let temp = self.make_unique_name_hoisted();
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
            let this_temp = self.make_unique_name_hoisted();
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

        let this_temp = self.make_unique_name_hoisted();
        let func_temp = self.make_unique_name_hoisted();

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
        // Signal new-callee position so `emit_parenthesized` preserves parens
        // around call expressions: `new (x() as T)` → `new (x())` not `new x()`.
        let prev_new = self.paren_in_new_callee;
        self.paren_in_new_callee = true;
        self.emit(call.expression);
        self.paren_in_new_callee = prev_new;
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

        // Signal that the expression is in access position so `emit_parenthesized`
        // preserves parens around `new` expressions: `(new a).b` vs `new a.b`.
        let prev = self.paren_in_access_position;
        self.paren_in_access_position = true;
        self.emit(access.expression);
        self.paren_in_access_position = prev;

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

        let prev = self.paren_in_access_position;
        self.paren_in_access_position = true;
        self.emit(access.expression);
        self.paren_in_access_position = prev;
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

        let base_temp = self.make_unique_name_hoisted();
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

        let base_temp = self.make_unique_name_hoisted();
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
                    // CALL_EXPRESSION can strip parens unless in new-callee position:
                    // `(x() as T).foo` → `x().foo` is fine, but `new (x() as T)` must keep
                    // parens because `new x()` has different semantics (constructs `x`).
                    || (k == syntax_kind_ext::CALL_EXPRESSION && !self.paren_in_new_callee)
                    // NEW_EXPRESSION can strip parens only when NOT in access position:
                    // `(new a)` → `new a` is fine, but `(new a).b` must keep parens
                    // because `new a.b` has different semantics (constructs `a.b`).
                    || (k == syntax_kind_ext::NEW_EXPRESSION && !self.paren_in_access_position)
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
    pub(super) fn unwrap_type_assertion_kind(&self, mut idx: NodeIndex) -> Option<u16> {
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

        // Detect newlines between the three operands in source to preserve
        // multiline ternary formatting (tsc preserves these line breaks).
        let (newline_before_question, newline_before_colon) =
            self.detect_conditional_newlines(cond.condition, cond.when_true, cond.when_false);

        self.emit(cond.condition);

        if newline_before_question {
            // Newline between condition and `?` — e.g.:
            //   var v = a
            //       ? b
            //       : c;
            self.write_line();
            self.increase_indent();
            self.write("? ");
            self.emit(cond.when_true);
            if newline_before_colon {
                self.write_line();
                self.write(": ");
            } else {
                self.write(" : ");
            }
            self.emit(cond.when_false);
            self.decrease_indent();
        } else if newline_before_colon {
            self.write(" ? ");
            self.emit(cond.when_true);
            let colon_on_new_line = self.colon_starts_new_line(cond.when_true, cond.when_false);
            if colon_on_new_line {
                // Newline before `:` — e.g.:
                //   var v = a ? b
                //       : c;
                self.write_line();
                self.increase_indent();
                self.write(": ");
                self.emit(cond.when_false);
                self.decrease_indent();
            } else {
                // `:` trailing on same line, alternate on next — e.g.:
                //   var v = a ? b :
                //       c;
                self.write(" :");
                self.write_line();
                self.increase_indent();
                self.emit(cond.when_false);
                self.decrease_indent();
            }
        } else {
            self.write(" ? ");
            self.emit(cond.when_true);
            self.write(" : ");
            self.emit(cond.when_false);
        }

        self.ctx.flags.in_binary_operand = prev;
    }

    /// Detect whether the source text has newlines between the parts of a
    /// conditional expression. Returns (`newline_before_question`, `newline_before_colon`).
    fn detect_conditional_newlines(
        &self,
        condition: NodeIndex,
        when_true: NodeIndex,
        when_false: NodeIndex,
    ) -> (bool, bool) {
        let Some(text) = self.source_text else {
            return (false, false);
        };
        let cond_node = self.arena.get(condition);
        let true_node = self.arena.get(when_true);
        let false_node = self.arena.get(when_false);

        let newline_before_question = match (cond_node, true_node) {
            (Some(c), Some(t)) => {
                let start = std::cmp::min(c.end as usize, text.len());
                let end = std::cmp::min(t.pos as usize, text.len());
                start < end && text[start..end].contains('\n')
            }
            _ => false,
        };

        let newline_before_colon = match (true_node, false_node) {
            (Some(t), Some(f)) => {
                let start = std::cmp::min(t.end as usize, text.len());
                let end = std::cmp::min(f.pos as usize, text.len());
                start < end && text[start..end].contains('\n')
            }
            _ => false,
        };

        (newline_before_question, newline_before_colon)
    }

    /// Check whether the `:` token in a conditional expression starts on a new
    /// line (relative to `when_true`'s end). This determines formatting:
    ///   `a ? b\n    : c`  (colon on new line)  vs  `a ? b :\n    c`  (colon trailing)
    fn colon_starts_new_line(&self, when_true: NodeIndex, when_false: NodeIndex) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let (Some(t), Some(f)) = (self.arena.get(when_true), self.arena.get(when_false)) else {
            return false;
        };
        let start = t.end as usize;
        let end = std::cmp::min(f.pos as usize, text.len());
        if start >= end {
            return false;
        }
        let region = &text[start..end];
        // Find the newline and colon positions
        let newline_pos = region.find('\n');
        let colon_pos = region.find(':');
        match (newline_pos, colon_pos) {
            (Some(nl), Some(c)) => nl < c, // newline comes before colon
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    /// Dynamic `import('path')` expressions must emit the `import` keyword.
    /// Previously the emitter's `emit_node_by_kind` dispatch had no handler for
    /// `SyntaxKind::ImportKeyword`, so the keyword was silently dropped and the
    /// output became just `('path')`.
    #[test]
    fn dynamic_import_emits_import_keyword() {
        let source = r#"const m = import("./module");"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(r#"import("./module")"#),
            "Dynamic import must emit the 'import' keyword.\nOutput:\n{output}"
        );
    }

    /// `import.meta` property access must emit the `import` keyword.
    #[test]
    fn import_meta_emits_import_keyword() {
        let source = r#"const url = import.meta.url;"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("import.meta.url"),
            "import.meta must emit the 'import' keyword.\nOutput:\n{output}"
        );
    }

    /// Dynamic import inside an async function body.
    #[test]
    fn dynamic_import_in_async_function() {
        let source = r#"async function load() { return await import("./lib"); }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(r#"import("./lib")"#),
            "Dynamic import inside async function must emit 'import' keyword.\nOutput:\n{output}"
        );
    }

    /// When async functions are lowered to generator functions (ES2015 target),
    /// `await expr` becomes `yield expr`. Yield has lower precedence than most
    /// operators, so it needs parens inside binary operators like `||`:
    /// `await p || a` → `(yield p) || a`. But assignment RHS and comma
    /// expression operands accept `AssignmentExpression` (which includes yield),
    /// so no extra parens are needed there.
    #[test]
    fn yield_from_await_no_extra_parens_in_assignment_rhs() {
        let source = r#"async function func() { o.a = await p; }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("o.a = yield p;"),
            "yield-from-await in assignment RHS must NOT have extra parens.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("(yield p)"),
            "yield-from-await in assignment RHS should not be wrapped in parens.\nOutput:\n{output}"
        );
    }

    /// Yield-from-await in comma expression LHS should not have extra parens.
    /// `(await p, a)` → `(yield p, a)`, NOT `((yield p), a)`.
    #[test]
    fn yield_from_await_no_extra_parens_in_comma_expr() {
        let source = r#"async function func() { var b = (await p, a); }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(yield p, a)"),
            "yield-from-await in comma expression must NOT have extra parens.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("((yield p)"),
            "yield-from-await should not be double-wrapped.\nOutput:\n{output}"
        );
    }

    /// Yield-from-await inside a binary operator like `||` still NEEDS parens.
    /// `await p || a` → `(yield p) || a` (otherwise it would parse as `yield (p || a)`).
    #[test]
    fn yield_from_await_keeps_parens_in_binary_operator() {
        let source = r#"async function func() { var b = await p || a; }"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(yield p) || a"),
            "yield-from-await in || operand MUST have parens for correct precedence.\nOutput:\n{output}"
        );
    }

    /// Multiline ternary: colon trailing on previous line, alternate on next.
    /// `a ? b :\n    c` must preserve the line break after `:`.
    #[test]
    fn conditional_preserves_newline_after_colon() {
        let source = "var v = a ? b :\n  c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a ? b :\n"),
            "Ternary with colon trailing must preserve newline after `:`.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    c"),
            "Alternate must be indented on the new line.\nOutput:\n{output}"
        );
    }

    /// Multiline ternary: colon leading on new line.
    /// `a ? b\n    : c` must preserve the line break before `:`.
    #[test]
    fn conditional_preserves_newline_before_colon() {
        let source = "var v = a ? b\n  : c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a ? b\n"),
            "Ternary with colon leading must preserve newline before `:`.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    : c"),
            "Colon must lead on the new indented line.\nOutput:\n{output}"
        );
    }

    /// Multiline ternary: both `?` and `:` on new lines.
    /// `a\n    ? b\n    : c` must preserve both line breaks.
    #[test]
    fn conditional_preserves_newline_before_question_and_colon() {
        let source = "var v = a\n  ? b\n  : c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a\n"),
            "Must preserve newline after condition.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    ? b\n"),
            "Question mark must lead on the new indented line.\nOutput:\n{output}"
        );
        assert!(
            output.contains("    : c"),
            "Colon must lead on the new indented line.\nOutput:\n{output}"
        );
    }

    /// Type assertion around a call expression should strip parens:
    /// `(<any>a.b()).c` → `a.b().c` (not `(a.b()).c`).
    #[test]
    fn type_assertion_call_expression_strips_parens() {
        let source = "var b = (<any>a.b()).c;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("a.b().c"),
            "Parens around type-asserted call expression should be stripped.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("(a.b()).c"),
            "Should not have redundant parens around call expression.\nOutput:\n{output}"
        );
    }

    /// Type assertion around `new` expression strips parens when not in access position:
    /// `(<any>new a)` → `new a`.
    #[test]
    fn type_assertion_new_expression_strips_parens() {
        let source = "var b = (<any>new a);\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var b = new a;"),
            "Parens around type-asserted new expression should be stripped.\nOutput:\n{output}"
        );
    }

    /// Type assertion around `new a.b` strips parens when not in access position:
    /// `(<any>new a.b)` → `new a.b`.
    #[test]
    fn type_assertion_new_expression_with_member_strips_parens() {
        let source = "var b = (<any>new a.b);\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var b = new a.b;"),
            "Parens around type-asserted new a.b should be stripped.\nOutput:\n{output}"
        );
    }

    /// Type assertion around `new a` keeps parens when in property access position:
    /// `(<any>new a).b` → `(new a).b` (removing parens would change semantics).
    #[test]
    fn type_assertion_new_expression_keeps_parens_in_access() {
        let source = "var b = (<any>new a).b;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("(new a).b"),
            "Parens around new expression in access position must be preserved.\nOutput:\n{output}"
        );
    }

    /// Type assertion around call expression in `new` callee position keeps parens:
    /// `new (x() as any)` → `new (x())` (not `new x()` which has different semantics).
    #[test]
    fn type_assertion_call_in_new_callee_keeps_parens() {
        let source = "new (x() as any);\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("new (x())"),
            "Parens around call expression in new callee must be preserved.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("new x()"),
            "Should NOT strip parens to `new x()` (different semantics).\nOutput:\n{output}"
        );
    }

    /// `as` type assertion around call expression in `new` callee position keeps parens:
    /// `new (x() as any)` → `new (x())`.
    #[test]
    fn as_assertion_call_in_new_callee_keeps_parens() {
        // Use angle-bracket style too: `new (<any>x())`
        let source = "new (<any>x());\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("new (x())"),
            "Parens around angle-bracket-asserted call in new callee must be preserved.\nOutput:\n{output}"
        );
    }

    /// Call expressions with type assertions outside `new` context still strip parens:
    /// `(<any>x()).foo` → `x().foo`.
    #[test]
    fn type_assertion_call_outside_new_still_strips_parens() {
        let source = "var b = (<any>x()).foo;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("x().foo"),
            "Parens around type-asserted call in access position should still strip.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("(x()).foo"),
            "Should not have redundant parens.\nOutput:\n{output}"
        );
    }

    /// When lowering nullish coalescing (`??`) to ES2019 and below for complex
    /// (non-identifier) LHS expressions, the emitter uses a temp variable:
    /// `(temp = f()) !== null && temp !== void 0 ? temp : 'fallback'`
    /// This temp must be declared as `var _a;` at the top of the enclosing scope.
    #[test]
    fn nullish_coalescing_emits_hoisted_temp_var_decl() {
        // Top-level: hoisted temp goes at file scope
        let source = "let gg = f() ?? 'foo';\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var _a;"),
            "Nullish coalescing lowering must emit `var _a;` for the hoisted temp.\nOutput:\n{output}"
        );
        assert!(
            output.contains("(_a = f())"),
            "Nullish coalescing lowering must use temp in assignment.\nOutput:\n{output}"
        );
    }

    /// When lowering optional property access (`?.`) to ES2019 and below for
    /// complex base expressions, the emitter uses a temp variable:
    /// `(temp = expr) === null || temp === void 0 ? void 0 : temp.prop`
    /// This temp must be declared as `var _a;` at the top of the enclosing scope.
    #[test]
    fn optional_chain_emits_hoisted_temp_var_decl() {
        // Multi-line function body to exercise the function-scoped hoisting path
        let source = "function h() {\n    let x = getObj()?.value;\n    return x;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::es6());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var _a;"),
            "Optional chain lowering must emit `var _a;` for the hoisted temp.\nOutput:\n{output}"
        );
        assert!(
            output.contains("(_a = getObj())"),
            "Optional chain lowering must use temp in assignment.\nOutput:\n{output}"
        );
    }

    /// Nested unary `+` operators must be separated by a space to prevent
    /// `+ +y` from collapsing to `++y` (pre-increment).
    #[test]
    fn prefix_plus_plus_gets_space() {
        let source = "var z = + +y;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("+ +y"),
            "Nested unary `+` must have space between to avoid `++y`.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("++y"),
            "Must NOT collapse `+ +y` into `++y` (pre-increment).\nOutput:\n{output}"
        );
    }

    /// Nested unary `-` operators must be separated by a space to prevent
    /// `- -y` from collapsing to `--y` (pre-decrement).
    #[test]
    fn prefix_minus_minus_gets_space() {
        let source = "var c = - -y;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("- -y"),
            "Nested unary `-` must have space between to avoid `--y`.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("--y"),
            "Must NOT collapse `- -y` into `--y` (pre-decrement).\nOutput:\n{output}"
        );
    }

    /// Unary `+` before `++` must insert a space: `+ ++x` not `+++x`.
    #[test]
    fn prefix_plus_before_increment_gets_space() {
        let source = "var z = + ++x;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("+ ++x"),
            "Unary `+` before `++x` must have space.\nOutput:\n{output}"
        );
    }
}
