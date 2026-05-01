use super::super::Printer;
use tsz_parser::parser::{
    NodeIndex,
    node::{AccessExprData, BinaryExprData, Node},
    syntax_kind_ext,
};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Binary expression downlevel emission (ES5/ES2015-ES2020)
    //
    // Exponentiation (**), logical assignment (&&=, ||=, ??=),
    // and nullish coalescing (??) lowering for older targets.
    // =========================================================================

    pub(in crate::emitter) fn emit_exponentiation_expression(&mut self, binary: &BinaryExprData) {
        // Unwrap type assertion parens from operands: `(<number>--temp) ** 3` has
        // parens around a type assertion that gets stripped in JS, leaving `--temp`
        // which doesn't need parens inside Math.pow() args.
        // Non-type-assertion parens like `(void --temp)` must be preserved.
        let left = self.unwrap_type_assertion_paren(binary.left);
        let right = self.unwrap_type_assertion_paren(binary.right);
        if binary.operator_token == SyntaxKind::AsteriskAsteriskEqualsToken as u16 {
            self.emit_exponentiation_assignment(binary.left, left, right);
        } else {
            self.write("Math.pow(");
            self.emit(left);
            self.write(", ");
            self.emit(right);
            self.write(")");
        }
    }

    /// Emit `lhs **= rhs` as `lhs = Math.pow(lhs, rhs)` with temp variables
    /// for complex LHS expressions to avoid double-evaluation of side effects.
    ///
    /// tsc patterns:
    /// - `x **= y` → `x = Math.pow(x, y)`
    /// - `a.b **= y` → `(_a = a).b = Math.pow(_a.b, y)` (temp for base if complex)
    /// - `a[i] **= y` → `(_a = a)[_b = i] = Math.pow(_a[_b], y)` (temps for base + index, always)
    ///
    /// tsc allocates temps bottom-up (inner `**=` first, then outer) because it
    /// uses a recursive transformer. We match this by emitting the RHS into a
    /// buffer first, which allocates inner temps, then allocating our own temps.
    fn emit_exponentiation_assignment(
        &mut self,
        original_left: NodeIndex,
        unwrapped_left: NodeIndex,
        right: NodeIndex,
    ) {
        let Some(left_node) = self.arena.get(original_left) else {
            self.emit(original_left);
            self.write(" = Math.pow(");
            self.emit(unwrapped_left);
            self.write(", ");
            self.emit(right);
            self.write(")");
            return;
        };

        let is_element_access = left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION;
        let is_property_access = left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION;

        if left_node.kind == SyntaxKind::SuperKeyword as u16 {
            let base_temp = self.make_unique_name_hoisted();
            self.write("(");
            self.write(&base_temp);
            self.write(" = ");
            self.emit(original_left);
            self.write("). = Math.pow(");
            self.write(&base_temp);
            self.write("., ");
            self.emit(right);
            self.write(")");
            return;
        }

        if !is_element_access && !is_property_access {
            // Simple identifier: `x **= y` → `x = Math.pow(x, y)`
            self.emit(original_left);
            self.write(" = Math.pow(");
            self.emit(unwrapped_left);
            self.write(", ");
            self.emit(right);
            self.write(")");
            return;
        }

        let Some(access) = self.arena.get_access_expr(left_node) else {
            self.emit(original_left);
            self.write(" = Math.pow(");
            self.emit(unwrapped_left);
            self.write(", ");
            self.emit(right);
            self.write(")");
            return;
        };

        let base_is_simple = self.is_simple_logical_assignment_base(access.expression);
        let needs_base_temp = if is_element_access {
            // Element access: tsc always temps both base and index
            true
        } else {
            // Property access: temp only for complex base
            !base_is_simple
        };

        if !needs_base_temp {
            // Simple property access: `obj.prop **= y` → `obj.prop = Math.pow(obj.prop, y)`
            self.emit(original_left);
            self.write(" = Math.pow(");
            self.emit(unwrapped_left);
            self.write(", ");
            self.emit(right);
            self.write(")");
            return;
        }

        // Emit the RHS into a buffer first, so inner `**=` expressions allocate
        // their temps before we allocate ours (matching tsc's bottom-up order).
        let rhs_text = self.capture_emit(right);

        if is_property_access {
            // `expr.prop **= y` → `(_a = expr).prop = Math.pow(_a.prop, <rhs>)`
            let base_temp = self.make_unique_name_hoisted();
            self.write("(");
            self.write(&base_temp);
            self.write(" = ");
            self.emit(access.expression);
            self.write(").");
            self.emit(access.name_or_argument);
            self.write(" = Math.pow(");
            self.write(&base_temp);
            self.write(".");
            self.emit(access.name_or_argument);
            self.write(", ");
            self.write(&rhs_text);
            self.write(")");
        } else {
            // Element access: `a[i] **= y` → `(_a = a)[_b = i] = Math.pow(_a[_b], <rhs>)`
            let base_temp = self.make_unique_name_hoisted();
            let index_temp = self.make_unique_name_hoisted();

            // LHS: (_a = expr)[_b = idx]
            self.write("(");
            self.write(&base_temp);
            self.write(" = ");
            self.emit(access.expression);
            self.write(")[");
            self.write(&index_temp);
            self.write(" = ");
            self.emit(access.name_or_argument);
            self.write("]");

            // = Math.pow(_a[_b], <rhs>)
            self.write(" = Math.pow(");
            self.write(&base_temp);
            self.write("[");
            self.write(&index_temp);
            self.write("], ");
            self.write(&rhs_text);
            self.write(")");
        }
    }

    /// Emit a node into a temporary buffer string, then rewind the writer.
    /// This allows pre-emitting the RHS to allocate inner temp names first.
    fn capture_emit(&mut self, node: NodeIndex) -> String {
        let start = self.writer.len();
        self.emit(node);
        let output = self.writer.get_output()[start..].to_string();
        self.writer.truncate(start);
        output
    }

    /// Unwrap `ParenthesizedExpression` wrapping type assertions for `Math.pow()` args.
    ///
    /// When `(<number>--temp) ** 3` is lowered to `Math.pow(...)`, the type assertion
    /// `<number>` is stripped, and the remaining `(--temp)` parens are unnecessary
    /// inside a function argument. But `(void --temp) ** 3` must keep its parens
    /// because `Math.pow(void --temp, 3)` would be parsed differently.
    fn unwrap_type_assertion_paren(&self, idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.arena.get(idx) else {
            return idx;
        };
        if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return idx;
        }
        let Some(paren) = self.arena.get_parenthesized(node) else {
            return idx;
        };
        if paren.expression.is_none() {
            return idx;
        }
        // Only unwrap if the inner expression is a type assertion chain
        let inner_expr = self.unwrap_type_assertions(paren.expression);
        if inner_expr != paren.expression {
            // The paren wrapped a type assertion — return the underlying expression
            inner_expr
        } else {
            // Not a type assertion — keep the paren as-is
            idx
        }
    }

    /// Unwrap chains of `TypeAssertion` / `AsExpression` / `SatisfiesExpression`
    /// to find the underlying runtime expression.
    fn unwrap_type_assertions(&self, mut idx: NodeIndex) -> NodeIndex {
        while let Some(node) = self.arena.get(idx) {
            if matches!(
                node.kind,
                syntax_kind_ext::TYPE_ASSERTION
                    | syntax_kind_ext::AS_EXPRESSION
                    | syntax_kind_ext::SATISFIES_EXPRESSION
            ) && let Some(ta) = self.arena.get_type_assertion(node)
            {
                idx = ta.expression;
                continue;
            }
            break;
        }
        idx
    }

    pub(in crate::emitter) fn emit_logical_assignment_expression(
        &mut self,
        binary: &BinaryExprData,
    ) {
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

    fn unwrap_parenthesized_logical_assignment_left(&self, mut left: NodeIndex) -> NodeIndex {
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
        left_idx: NodeIndex,
        left_node: &Node,
        is_and: bool,
        is_nullish: bool,
        right: NodeIndex,
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

    fn emit_expression_for_logical_assignment(&mut self, node_idx: NodeIndex) {
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

        // Capture-emit the left side first so that inner ?? chains allocate
        // their temp variables before we allocate ours (matching tsc's order).
        let left_text = self.capture_emit(binary.left);
        let value_temp = self.make_unique_name_hoisted_value();
        self.write("(");
        self.write(&value_temp);
        self.write(" = ");
        self.write(&left_text);
        self.write(") !== null && ");
        self.write(&value_temp);
        self.write(" !== void 0 ? ");
        self.write(&value_temp);
        self.write(" : ");
        self.emit(binary.right);
    }

    pub(in crate::emitter) fn emit_nullish_coalescing_expression(
        &mut self,
        binary: &BinaryExprData,
    ) {
        // When the lowered ternary appears inside a binary operand, conditional
        // condition, or unary expression, wrap in parens to preserve precedence.
        // e.g., `(a ?? b) || c` → `(a !== null && a !== void 0 ? a : b) || c`
        let needs_parens = self.ctx.flags.nullish_coalescing_needs_parens;
        if needs_parens {
            self.write("(");
            self.ctx.flags.nullish_coalescing_needs_parens = false;
        }

        if self.is_simple_nullish_expression(binary.left) {
            self.emit(binary.left);
            self.write(" !== null && ");
            self.emit(binary.left);
            self.write(" !== void 0 ? ");
            self.emit(binary.left);
            self.write(" : ");
            self.emit(binary.right);
        } else {
            // Capture-emit the left side first so that inner ?? chains allocate
            // their temp variables before we allocate ours. This matches tsc's
            // bottom-up (innermost-first) temp variable ordering.
            let left_text = self.capture_emit(binary.left);
            let value_temp = self.make_unique_name_hoisted();
            self.write("(");
            self.write(&value_temp);
            self.write(" = ");
            self.write(&left_text);
            self.write(") !== null && ");
            self.write(&value_temp);
            self.write(" !== void 0 ? ");
            self.write(&value_temp);
            self.write(" : ");
            self.emit(binary.right);
        }

        if needs_parens {
            self.write(")");
        }
    }

    fn is_simple_logical_assignment_lhs(&self, node_idx: NodeIndex) -> bool {
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

    fn is_simple_logical_assignment_base(&self, node_idx: NodeIndex) -> bool {
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

    fn is_simple_logical_assignment_name(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        self.arena
            .get_identifier(node)
            .is_some_and(|identifier| !identifier.escaped_text.is_empty())
    }

    fn is_simple_logical_assignment_index(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };

        node.is_identifier() || node.is_string_literal() || node.is_numeric_literal()
    }

    pub(in crate::emitter) fn is_simple_nullish_expression(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };

        // Match tsc's isSimpleCopiableExpression: identifiers, keywords, and literals
        // are all safe to repeat without side effects.
        // Note: tsc does NOT unwrap parenthesized expressions here.
        node.is_identifier()
            || (node.kind >= SyntaxKind::BreakKeyword as u16
                && node.kind <= SyntaxKind::DeferKeyword as u16)
            || node.is_numeric_literal()
            || node.is_string_literal()
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
    }

    /// Check if a token is a compound assignment operator (+=, -=, etc.)
    pub(in crate::emitter) const fn is_compound_assignment(&self, token: u16) -> bool {
        token == SyntaxKind::PlusEqualsToken as u16
            || token == SyntaxKind::MinusEqualsToken as u16
            || token == SyntaxKind::AsteriskEqualsToken as u16
            || token == SyntaxKind::SlashEqualsToken as u16
            || token == SyntaxKind::PercentEqualsToken as u16
            || token == SyntaxKind::AsteriskAsteriskEqualsToken as u16
            || token == SyntaxKind::LessThanLessThanEqualsToken as u16
            || token == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
            || token == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
            || token == SyntaxKind::AmpersandEqualsToken as u16
            || token == SyntaxKind::CaretEqualsToken as u16
            || token == SyntaxKind::BarEqualsToken as u16
    }

    /// Get the base operator for a compound assignment (e.g., `+=` → `+`)
    pub(in crate::emitter) fn get_compound_base_operator(&self, token: u16) -> String {
        match token {
            t if t == SyntaxKind::PlusEqualsToken as u16 => "+".to_string(),
            t if t == SyntaxKind::MinusEqualsToken as u16 => "-".to_string(),
            t if t == SyntaxKind::AsteriskEqualsToken as u16 => "*".to_string(),
            t if t == SyntaxKind::SlashEqualsToken as u16 => "/".to_string(),
            t if t == SyntaxKind::PercentEqualsToken as u16 => "%".to_string(),
            t if t == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => "**".to_string(),
            t if t == SyntaxKind::LessThanLessThanEqualsToken as u16 => "<<".to_string(),
            t if t == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16 => ">>".to_string(),
            t if t == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16 => {
                ">>>".to_string()
            }
            t if t == SyntaxKind::AmpersandEqualsToken as u16 => "&".to_string(),
            t if t == SyntaxKind::CaretEqualsToken as u16 => "^".to_string(),
            t if t == SyntaxKind::BarEqualsToken as u16 => "|".to_string(),
            _ => "=".to_string(),
        }
    }
}
