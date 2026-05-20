use super::super::super::Printer;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn estimate_assignment_destructuring_temps_in_constructor(
        &self,
        node: &Node,
    ) -> usize {
        match node.kind {
            kind if kind == syntax_kind_ext::BLOCK => {
                let Some(block) = self.arena.get_block(node) else {
                    return 0;
                };
                let mut count = 0;
                for &stmt_idx in &block.statements.nodes {
                    count += self.estimate_constructor_assignment_temps_in_statement(stmt_idx);
                }
                count
            }
            _ => 0,
        }
    }

    fn estimate_constructor_assignment_temps_in_statement(&self, stmt_idx: NodeIndex) -> usize {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return 0;
        };

        match stmt_node.kind {
            kind if kind == syntax_kind_ext::EXPRESSION_STATEMENT => {
                let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                    return 0;
                };
                self.estimate_destructuring_assignment_temps(expr_stmt.expression)
            }
            kind if kind == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.estimate_variable_decl_destructuring_temps(stmt_node)
            }
            kind if kind == syntax_kind_ext::BLOCK => {
                self.estimate_assignment_destructuring_temps_in_constructor(stmt_node)
            }
            _ => 0,
        }
    }

    fn estimate_variable_decl_destructuring_temps(&self, node: &Node) -> usize {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return 0;
        };
        let mut count = 0;
        for &decl_idx in &var_stmt.declarations.nodes {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if decl.initializer.is_none() {
                continue;
            }
            let Some(left_node) = self.arena.get(decl.name) else {
                continue;
            };
            if left_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
                && left_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
            {
                continue;
            }
            let is_simple = self
                .arena
                .get(decl.initializer)
                .is_some_and(|n| n.kind == SyntaxKind::Identifier as u16);
            if !is_simple {
                count += 1;
            }
        }
        count
    }

    fn estimate_destructuring_assignment_temps(&self, node_idx: NodeIndex) -> usize {
        let Some(node) = self.arena.get(node_idx) else {
            return 0;
        };
        match node.kind {
            kind if kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let Some(paren) = self.arena.get_parenthesized(node) else {
                    return 0;
                };
                self.estimate_destructuring_assignment_temps(paren.expression)
            }
            kind if kind == syntax_kind_ext::BINARY_EXPRESSION => {
                let Some(binary) = self.arena.get_binary_expr(node) else {
                    return 0;
                };
                let right_is_simple = self
                    .arena
                    .get(binary.right)
                    .is_some_and(|n| n.kind == SyntaxKind::Identifier as u16);
                let left = self.arena.get(binary.left);
                if binary.operator_token == SyntaxKind::CommaToken as u16 {
                    self.estimate_destructuring_assignment_temps(binary.left)
                        + self.estimate_destructuring_assignment_temps(binary.right)
                } else if binary.operator_token == SyntaxKind::EqualsToken as u16
                    && let Some(left_node) = left
                {
                    if matches!(
                        left_node.kind,
                        syntax_kind_ext::ARRAY_BINDING_PATTERN
                            | syntax_kind_ext::OBJECT_BINDING_PATTERN
                            | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    ) {
                        self.estimate_destructuring_pattern_temps(left_node, right_is_simple)
                    } else {
                        0
                    }
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    fn estimate_destructuring_pattern_temps(
        &self,
        pattern_node: &Node,
        rhs_is_simple: bool,
    ) -> usize {
        match pattern_node.kind {
            kind if kind == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
                    return 0;
                };
                let needs_temp = !rhs_is_simple;
                let mut count = if needs_temp { 1 } else { 0 };
                for &elem_idx in &pattern.elements.nodes {
                    if elem_idx.is_none() {
                        continue;
                    }
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };
                    if let Some(elem) = self.arena.get_binding_element(elem_node) {
                        let target = self.arena.get(elem.name);
                        if let Some(target_node) = target
                            && (target_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                || target_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN)
                        {
                            count += self.estimate_destructuring_pattern_temps(target_node, false);
                        }
                        if let Some(bin) = self.arena.get_binary_expr(elem_node)
                            && bin.operator_token == SyntaxKind::EqualsToken as u16
                        {
                            let rhs_node = self.arena.get(bin.right);
                            if rhs_node.is_some_and(|n| n.kind != SyntaxKind::Identifier as u16) {
                                count += 1;
                            }
                        }
                    }
                }
                count
            }
            kind if kind == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
                    return 0;
                };
                let needs_temp = !rhs_is_simple && !pattern.elements.nodes.is_empty();
                let mut count = if needs_temp { 1 } else { 0 };
                for &elem_idx in &pattern.elements.nodes {
                    if elem_idx.is_none() {
                        continue;
                    }
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };
                    if let Some(prop) = self.arena.get_property_assignment(elem_node)
                        && let Some(value_node) = self.arena.get(prop.initializer)
                    {
                        if matches!(
                            value_node.kind,
                            syntax_kind_ext::ARRAY_BINDING_PATTERN
                                | syntax_kind_ext::OBJECT_BINDING_PATTERN
                        ) {
                            count += self.estimate_destructuring_pattern_temps(value_node, false);
                        } else if value_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                            && let Some(bin) = self.arena.get_binary_expr(value_node)
                            && bin.operator_token == SyntaxKind::EqualsToken as u16
                        {
                            let left = self.arena.get(bin.left);
                            if let Some(left_node) = left {
                                if matches!(
                                    left_node.kind,
                                    syntax_kind_ext::ARRAY_BINDING_PATTERN
                                        | syntax_kind_ext::OBJECT_BINDING_PATTERN
                                ) {
                                    count +=
                                        self.estimate_destructuring_pattern_temps(left_node, false);
                                } else {
                                    count += 1;
                                }
                            } else {
                                count += 1;
                            }
                        }
                    }
                    if let Some(bin) = self.arena.get_binary_expr(elem_node)
                        && bin.operator_token == SyntaxKind::EqualsToken as u16
                        && let Some(bin_right) = self.arena.get(bin.right)
                        && bin_right.kind != SyntaxKind::Identifier as u16
                    {
                        count += 1;
                    }
                }
                count
            }
            _ => 0,
        }
    }
}
