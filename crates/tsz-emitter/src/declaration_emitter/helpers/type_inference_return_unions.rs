use super::super::DeclarationEmitter;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn serialized_property_name_length(
        &self,
        name: &str,
    ) -> usize {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return 2;
        };
        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return name.len() + 2;
        }
        if chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()) {
            name.len()
        } else {
            name.len() + 2
        }
    }

    pub(in crate::declaration_emitter) fn numeric_literal_union_widens_to_number(
        source_type_text: &str,
        inferred_text: &str,
    ) -> bool {
        if inferred_text != "number" {
            return false;
        }

        let mut count = 0usize;
        for part in source_type_text.split(" | ") {
            let part = part.trim();
            if part.is_empty() || part.parse::<f64>().is_err() {
                return false;
            }
            count += 1;
        }
        count > 1
    }

    pub(in crate::declaration_emitter) fn string_literal_union_widens_to_string(
        source_type_text: &str,
        inferred_text: &str,
    ) -> bool {
        if inferred_text != "string" {
            return false;
        }

        let mut count = 0usize;
        for part in source_type_text.split(" | ") {
            let part = part.trim();
            let Some(first) = part.chars().next() else {
                return false;
            };
            let Some(last) = part.chars().next_back() else {
                return false;
            };
            if (first != '"' && first != '\'') || first != last || part.len() < 2 {
                return false;
            }
            count += 1;
        }
        count > 1
    }

    pub(in crate::declaration_emitter) fn simplify_uniform_object_keyof_index_access_text(
        &self,
        type_text: &str,
    ) -> Option<String> {
        let trimmed = type_text.trim();
        let (object_text, key_text) = trimmed.rsplit_once("}[keyof ")?;
        let key_alias = key_text.strip_suffix(']')?.trim();
        if !Self::is_simple_identifier_text(key_alias) {
            return None;
        }

        let mut value_type = None;
        let mut saw_member = false;
        for line in object_text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed == "{" {
                continue;
            }
            if trimmed.contains("?:") {
                return None;
            }
            let next_value_type = Self::object_literal_property_value_type(trimmed)?;
            if next_value_type.is_empty() {
                return None;
            }
            if let Some(existing) = value_type {
                if existing != next_value_type {
                    return None;
                }
            } else {
                value_type = Some(next_value_type);
            }
            saw_member = true;
        }

        saw_member.then(|| value_type.unwrap_or("never").to_string())
    }

    pub(in crate::declaration_emitter) fn function_body_numeric_literal_return_union_type_text(
        &self,
        statements: &NodeList,
    ) -> Option<String> {
        let mut literals = Vec::new();
        if !self.collect_numeric_literal_return_type_text_from_block(statements, &mut literals) {
            return None;
        }
        (literals.len() > 1).then(|| literals.join(" | "))
    }

    fn collect_numeric_literal_return_type_text_from_block(
        &self,
        statements: &NodeList,
        literals: &mut Vec<String>,
    ) -> bool {
        statements.nodes.iter().copied().all(|stmt_idx| {
            self.collect_numeric_literal_return_type_text_from_statement(stmt_idx, literals)
        })
    }

    fn collect_numeric_literal_return_type_text_from_statement(
        &self,
        stmt_idx: NodeIndex,
        literals: &mut Vec<String>,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return true;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                    return false;
                };
                self.collect_numeric_literal_return_type_text_from_expression(
                    ret.expression,
                    literals,
                )
            }
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    self.collect_numeric_literal_return_type_text_from_block(
                        &block.statements,
                        literals,
                    )
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    if if_data.else_statement.is_none() {
                        let mut ignored = literals.clone();
                        return self.collect_numeric_literal_return_type_text_from_statement(
                            if_data.then_statement,
                            &mut ignored,
                        );
                    }
                    self.collect_numeric_literal_return_type_text_from_statement(
                        if_data.then_statement,
                        literals,
                    ) && self.collect_numeric_literal_return_type_text_from_statement(
                        if_data.else_statement,
                        literals,
                    )
                }),
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.arena.get_try(stmt_node).is_some_and(|try_data| {
                    self.collect_numeric_literal_return_type_text_from_statement(
                        try_data.try_block,
                        literals,
                    ) && try_data.catch_clause.is_some()
                        && self.collect_numeric_literal_return_type_text_from_statement(
                            try_data.catch_clause,
                            literals,
                        )
                        && try_data.finally_block.is_some()
                        && self.collect_numeric_literal_return_type_text_from_statement(
                            try_data.finally_block,
                            literals,
                        )
                })
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => self
                .arena
                .get_catch_clause(stmt_node)
                .is_some_and(|catch_data| {
                    self.collect_numeric_literal_return_type_text_from_statement(
                        catch_data.block,
                        literals,
                    )
                }),
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                self.arena.get_case_clause(stmt_node).is_some_and(|clause| {
                    self.collect_numeric_literal_return_type_text_from_block(
                        &clause.statements,
                        literals,
                    )
                })
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.arena.get_switch(stmt_node).is_some_and(|switch_data| {
                    self.arena
                        .get(switch_data.case_block)
                        .and_then(|case_block_node| self.arena.get_block(case_block_node))
                        .is_some_and(|block| {
                            self.collect_numeric_literal_return_type_text_from_block(
                                &block.statements,
                                literals,
                            )
                        })
                })
            }
            _ => true,
        }
    }

    fn collect_numeric_literal_return_type_text_from_expression(
        &self,
        expr_idx: NodeIndex,
        literals: &mut Vec<String>,
    ) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        if self
            .get_node_type_or_names(&[expr_idx])
            .is_some_and(|type_id| type_id == tsz_solver::types::TypeId::NEVER)
        {
            return true;
        }

        let type_text = match expr_node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => self.js_literal_type_text(expr_idx),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                self.numeric_prefix_literal_type_text(expr_node)
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                let Some(conditional) = self.arena.get_conditional_expr(expr_node) else {
                    return false;
                };
                return self.collect_numeric_literal_return_type_text_from_expression(
                    conditional.when_true,
                    literals,
                ) && self.collect_numeric_literal_return_type_text_from_expression(
                    conditional.when_false,
                    literals,
                );
            }
            _ => None,
        };

        let Some(type_text) = type_text else {
            return false;
        };
        if !literals.contains(&type_text) {
            literals.push(type_text);
        }
        true
    }

    fn numeric_prefix_literal_type_text(
        &self,
        expr_node: &tsz_parser::parser::node::Node,
    ) -> Option<String> {
        let unary = self.arena.get_unary_expr(expr_node)?;
        let operand_node = self.arena.get(unary.operand)?;
        if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
            return None;
        }
        let literal = self.arena.get_literal(operand_node)?;
        let normalized = Self::normalize_numeric_literal(literal.text.as_ref());
        match unary.operator {
            k if k == SyntaxKind::MinusToken as u16 => Some(format!("-{normalized}")),
            k if k == SyntaxKind::PlusToken as u16 => Some(normalized),
            _ => None,
        }
    }
    pub(in crate::declaration_emitter) fn function_body_string_literal_return_union_type_text(
        &self,
        statements: &NodeList,
    ) -> Option<String> {
        let mut literals = Vec::new();
        if !self.collect_string_literal_return_type_text_from_block(statements, &mut literals) {
            return None;
        }
        (literals.len() > 1).then(|| literals.join(" | "))
    }

    fn collect_string_literal_return_type_text_from_block(
        &self,
        statements: &NodeList,
        literals: &mut Vec<String>,
    ) -> bool {
        statements.nodes.iter().copied().all(|stmt_idx| {
            self.collect_string_literal_return_type_text_from_statement(stmt_idx, literals)
        })
    }

    fn collect_string_literal_return_type_text_from_statement(
        &self,
        stmt_idx: NodeIndex,
        literals: &mut Vec<String>,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return true;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                    return false;
                };
                let Some(expr_idx) = self.skip_parenthesized_expression(ret.expression) else {
                    return false;
                };
                self.collect_string_literal_return_type_text_from_expression(expr_idx, literals)
            }
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    self.collect_string_literal_return_type_text_from_block(
                        &block.statements,
                        literals,
                    )
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    self.collect_string_literal_return_type_text_from_statement(
                        if_data.then_statement,
                        literals,
                    ) && (!if_data.else_statement.is_some()
                        || self.collect_string_literal_return_type_text_from_statement(
                            if_data.else_statement,
                            literals,
                        ))
                }),
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.arena.get_try(stmt_node).is_some_and(|try_data| {
                    self.collect_string_literal_return_type_text_from_statement(
                        try_data.try_block,
                        literals,
                    ) && (!try_data.catch_clause.is_some()
                        || self.collect_string_literal_return_type_text_from_statement(
                            try_data.catch_clause,
                            literals,
                        ))
                        && (!try_data.finally_block.is_some()
                            || self.collect_string_literal_return_type_text_from_statement(
                                try_data.finally_block,
                                literals,
                            ))
                })
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => self
                .arena
                .get_catch_clause(stmt_node)
                .is_some_and(|catch_data| {
                    self.collect_string_literal_return_type_text_from_statement(
                        catch_data.block,
                        literals,
                    )
                }),
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                self.arena.get_case_clause(stmt_node).is_some_and(|clause| {
                    self.collect_string_literal_return_type_text_from_block(
                        &clause.statements,
                        literals,
                    )
                })
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.arena.get_switch(stmt_node).is_some_and(|switch_data| {
                    self.arena
                        .get(switch_data.case_block)
                        .and_then(|case_block_node| self.arena.get_block(case_block_node))
                        .is_some_and(|block| {
                            self.collect_string_literal_return_type_text_from_block(
                                &block.statements,
                                literals,
                            )
                        })
                })
            }
            _ => true,
        }
    }

    fn collect_string_literal_return_type_text_from_expression(
        &self,
        expr_idx: NodeIndex,
        literals: &mut Vec<String>,
    ) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        let type_text = match expr_node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => self.js_literal_type_text(expr_idx),
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                let Some(conditional) = self.arena.get_conditional_expr(expr_node) else {
                    return false;
                };
                return self.collect_string_literal_return_type_text_from_expression(
                    conditional.when_true,
                    literals,
                ) && self.collect_string_literal_return_type_text_from_expression(
                    conditional.when_false,
                    literals,
                );
            }
            _ => None,
        };

        let Some(type_text) = type_text else {
            return false;
        };
        if !literals.contains(&type_text) {
            literals.push(type_text);
        }
        true
    }
}
