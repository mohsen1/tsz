use super::Printer;
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

    pub(super) fn emit_exponentiation_expression(&mut self, binary: &BinaryExprData) {
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

    pub(super) fn emit_logical_assignment_expression(&mut self, binary: &BinaryExprData) {
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

    pub(super) fn emit_nullish_coalescing_expression(&mut self, binary: &BinaryExprData) {
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

        node.kind == SyntaxKind::Identifier as u16
            || node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NumericLiteral as u16
    }

    pub(super) fn is_simple_nullish_expression(&self, node_idx: NodeIndex) -> bool {
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
}
