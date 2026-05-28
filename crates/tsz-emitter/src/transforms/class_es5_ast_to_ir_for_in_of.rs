//! `for-in` and `for-of` statement conversion, including the ES5
//! array-indexing fast path used when downlevel iteration is disabled.
//!
//! Extracted from `class_es5_ast_to_ir.rs` so the central AST→IR conversion
//! file stays under the §19 2000-line cap. Behavior is unchanged.

use super::{AstToIr, IRNode, get_identifier_text};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::ForInOfData;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> AstToIr<'a> {
    pub(super) fn convert_for_in_of_statement(&self, idx: NodeIndex) -> IRNode {
        let Some(node) = self.arena.get(idx) else {
            return IRNode::ASTRef(idx);
        };
        if node.kind == syntax_kind_ext::FOR_OF_STATEMENT
            && self.can_delegate_es5_for_of_to_ast_printer(idx)
        {
            return IRNode::ASTRef(idx);
        }

        // Issue #3539: previously we returned `ASTRef(idx)` which delegates
        // to the AST printer that has no `_this` substitution context. The
        // body of `for-in`/`for-of` inside a derived ES5 constructor must
        // recurse through `convert_statement` so any `this` reference
        // becomes `_this`.
        let Some(loop_data) = self.arena.get_for_in_of(node) else {
            return IRNode::ASTRef(idx);
        };
        if node.kind == syntax_kind_ext::FOR_OF_STATEMENT
            && !self.downlevel_iteration
            && !loop_data.await_modifier
            && let Some(ir) = self.convert_for_of_array_indexing(loop_data)
        {
            return ir;
        }
        let kind = if node.kind == syntax_kind_ext::FOR_OF_STATEMENT {
            if loop_data.await_modifier {
                std::borrow::Cow::Borrowed("await of")
            } else {
                std::borrow::Cow::Borrowed("of")
            }
        } else {
            std::borrow::Cow::Borrowed("in")
        };
        // The initializer is either a VariableDeclarationList or an
        // expression. The variable-declaration case never references
        // `this`, so we keep it as ASTRef for simplicity. The expression
        // case still benefits from substitution, so use convert_expression.
        // The initializer is either a VariableDeclarationList or an
        // expression. For the variable-declaration case we synthesize
        // `var <name>` from the parsed VariableData rather than slicing
        // the source — the parser includes the trailing `of`/`in` keyword
        // in the VARIABLE_DECLARATION_LIST node's range, so an `ASTRef`
        // would re-emit the keyword. The expression case still benefits
        // from substitution, so use convert_expression.
        let initializer_node = self.arena.get(loop_data.initializer);
        let initializer = if let Some(init_node) = initializer_node
            && init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && let Some(var_data) = self.arena.get_variable(init_node)
        {
            let mut text = String::from("var ");
            for (i, &decl_idx) in var_data.declarations.nodes.iter().enumerate() {
                if i > 0 {
                    text.push_str(", ");
                }
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    text.push_str(&crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena, decl.name,
                    ));
                }
            }
            IRNode::Raw(text.into())
        } else {
            self.convert_expression(loop_data.initializer)
        };
        IRNode::ForInOfStatement {
            kind,
            initializer: Box::new(initializer),
            expression: Box::new(self.convert_expression(loop_data.expression)),
            body: Box::new(self.convert_statement(loop_data.statement)),
            multiline_body: false,
        }
    }

    fn convert_for_of_array_indexing(&self, loop_data: &ForInOfData) -> Option<IRNode> {
        let init_node = self.arena.get(loop_data.initializer)?;
        if init_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return None;
        }
        let var_data = self.arena.get_variable(init_node)?;
        if var_data.declarations.nodes.len() != 1 {
            return None;
        }
        let decl_idx = var_data.declarations.nodes[0];
        let decl_node = self.arena.get(decl_idx)?;
        let decl = self.arena.get_variable_declaration(decl_node)?;
        let binding_name = get_identifier_text(self.arena, decl.name)?;

        let index_name = if self.temp_var_counter.get() == 0 && !self.source_has_identifier("_i") {
            "_i".to_string()
        } else {
            self.generate_temp_name()
        };
        let array_name = self.generate_temp_name();
        let iterable = self.convert_expression(loop_data.expression);

        let mut body = vec![IRNode::VarDecl {
            name: binding_name.into(),
            initializer: Some(Box::new(IRNode::elem(
                IRNode::id(array_name.clone()),
                IRNode::id(index_name.clone()),
            ))),
        }];
        body.extend(self.convert_loop_body_statements(loop_data.statement));

        Some(IRNode::ForStatement {
            initializer: Some(Box::new(IRNode::VarDeclList(vec![
                IRNode::VarDecl {
                    name: index_name.clone().into(),
                    initializer: Some(Box::new(IRNode::number("0"))),
                },
                IRNode::VarDecl {
                    name: array_name.clone().into(),
                    initializer: Some(Box::new(iterable)),
                },
            ]))),
            condition: Some(Box::new(IRNode::BinaryExpr {
                left: Box::new(IRNode::id(index_name.clone())),
                operator: "<".into(),
                right: Box::new(IRNode::prop(IRNode::id(array_name), "length")),
            })),
            incrementor: Some(Box::new(IRNode::PostfixUnaryExpr {
                operand: Box::new(IRNode::id(index_name)),
                operator: "++".into(),
            })),
            body: Box::new(IRNode::Block(body)),
        })
    }

    fn convert_loop_body_statements(&self, statement_idx: NodeIndex) -> Vec<IRNode> {
        if let Some(statement_node) = self.arena.get(statement_idx)
            && statement_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(statement_node)
        {
            return block
                .statements
                .nodes
                .iter()
                .map(|&stmt_idx| self.convert_statement(stmt_idx))
                .collect();
        }
        vec![self.convert_statement(statement_idx)]
    }
}
