use crate::transforms::ir::{IRGeneratorCase, IRNode};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

use super::{AsyncES5Transformer, opcodes};

impl<'a> AsyncES5Transformer<'a> {
    pub(super) fn process_labeled_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        let Some(labeled) = self.arena.get_labeled_statement(node) else {
            return;
        };

        if !self.contains_await_recursive(labeled.statement) {
            current_statements.push(self.statement_to_ir(idx));
            return;
        }

        let label =
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, labeled.label);

        let Some(statement_node) = self.arena.get(labeled.statement) else {
            return;
        };
        if statement_node.kind == syntax_kind_ext::WHILE_STATEMENT {
            self.process_while_statement_in_async_with_label(
                labeled.statement,
                cases,
                current_statements,
                current_label,
                Some(&label),
            );
            return;
        }
        if statement_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
            && self.process_for_await_statement_in_async(
                labeled.statement,
                cases,
                current_statements,
                current_label,
                Some(&label),
            )
        {
            return;
        }
        if statement_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(statement_node)
        {
            for &stmt_idx in &block.statements.nodes {
                if self.is_break_to_label(stmt_idx, &label) {
                    let end_label = self.state.next_label();
                    current_statements.push(IRNode::ReturnStatement(Some(Box::new(
                        IRNode::GeneratorOp {
                            opcode: opcodes::BREAK,
                            value: Some(Box::new(IRNode::NumericLiteral(
                                end_label.to_string().into(),
                            ))),
                            comment: Some("break".to_string().into()),
                        },
                    ))));
                    cases.push(IRGeneratorCase {
                        label: *current_label,
                        statements: std::mem::take(current_statements),
                    });
                    *current_label = end_label;
                    return;
                }

                self.process_async_statement(stmt_idx, cases, current_statements, current_label);
            }
        } else {
            self.process_async_statement(
                labeled.statement,
                cases,
                current_statements,
                current_label,
            );
        }
    }

    fn is_break_to_label(&self, stmt_idx: NodeIndex, label: &str) -> bool {
        let Some(node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::BREAK_STATEMENT {
            return false;
        }
        let Some(jump) = self.arena.get_jump_data(node) else {
            return false;
        };
        crate::transforms::emit_utils::identifier_text_or_empty(self.arena, jump.label) == label
    }

    /// Get the catch variable name from a variable declaration index
    pub(super) fn get_catch_variable_name(&self, var_decl_idx: NodeIndex) -> String {
        if let Some(var_node) = self.arena.get(var_decl_idx)
            && let Some(var_decl) = self.arena.get_variable_declaration(var_node)
        {
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, var_decl.name)
        } else {
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, var_decl_idx)
        }
    }

    /// Process either a block or single statement in async context.
    /// Used by if/else and try/catch to handle both `{ ... }` and single-statement branches.
    pub(super) fn process_block_or_statement_in_async(
        &mut self,
        idx: NodeIndex,
        cases: &mut Vec<IRGeneratorCase>,
        current_statements: &mut Vec<IRNode>,
        current_label: &mut u32,
    ) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.arena.get_block(node) {
                self.process_async_statement_list(
                    &block.statements.nodes,
                    cases,
                    current_statements,
                    current_label,
                    &[],
                );
            }
        } else {
            self.process_async_statement(idx, cases, current_statements, current_label);
        }
    }

    pub(super) fn extract_preceding_line_comment(&self, pos: u32) -> Option<String> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let mut pos = pos as usize;
        if pos > bytes.len() {
            pos = bytes.len();
        }
        if pos == 0 {
            return None;
        }

        let line_start = text[..pos].rfind('\n').map_or(0, |i| i + 1);
        if line_start == 0 {
            return None;
        }
        let prev_line_end = line_start.saturating_sub(1);
        let prev_line_start = text[..prev_line_end].rfind('\n').map_or(0, |i| i + 1);
        let prev_line = &text[prev_line_start..prev_line_end];
        let trimmed = prev_line.trim_start();
        if trimmed.starts_with("//") && !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
        None
    }

    pub(super) fn generator_yield_operand_to_ir(&self, idx: NodeIndex) -> IRNode {
        let operand = self.expression_to_ir(idx);
        let Some(comment) = self.yield_operand_line_comment(idx) else {
            return operand;
        };
        let operand_text = crate::transforms::ir_printer::IRPrinter::emit_to_string(&operand);
        IRNode::Raw(format!("\n                {comment}\n                {operand_text}").into())
    }

    fn yield_operand_line_comment(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                let leaf_start = self.expression_leaf_start(paren.expression)?;
                let text = self.source_text?;
                let start = node.pos as usize;
                let end = (leaf_start as usize).min(text.len());
                if start < end {
                    let slice = &text[start..end];
                    if let Some(comment) = slice.lines().rev().find_map(|line| {
                        let trimmed = line.trim_start();
                        trimmed.starts_with("//").then(|| trimmed.to_string())
                    }) {
                        return Some(comment);
                    }
                }
                self.yield_operand_line_comment(paren.expression)
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                let assertion = self.arena.get_type_assertion(node)?;
                self.yield_operand_line_comment(assertion.expression)
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.yield_operand_line_comment(unary.expression)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.arena.get_binary_expr(node)?;
                self.yield_operand_line_comment(binary.left)
                    .or_else(|| self.yield_operand_line_comment(binary.right))
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                let conditional = self.arena.get_conditional_expr(node)?;
                self.yield_operand_line_comment(conditional.condition)
                    .or_else(|| self.yield_operand_line_comment(conditional.when_true))
                    .or_else(|| self.yield_operand_line_comment(conditional.when_false))
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                let access = self.arena.get_access_expr(node)?;
                self.yield_operand_line_comment(access.expression)
                    .or_else(|| self.yield_operand_line_comment(access.name_or_argument))
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(node)?;
                self.yield_operand_line_comment(call.expression)
                    .or_else(|| {
                        call.arguments.as_ref().and_then(|args| {
                            args.nodes
                                .iter()
                                .find_map(|&arg| self.yield_operand_line_comment(arg))
                        })
                    })
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                let tagged = self.arena.get_tagged_template(node)?;
                self.yield_operand_line_comment(tagged.tag)
            }
            _ => None,
        }
    }

    fn expression_leaf_start(&self, idx: NodeIndex) -> Option<u32> {
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.arena.get_parenthesized(node)?;
                self.expression_leaf_start(paren.expression)
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                let assertion = self.arena.get_type_assertion(node)?;
                self.expression_leaf_start(assertion.expression)
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.expression_leaf_start(unary.expression)
            }
            _ => Some(node.pos),
        }
    }
}
