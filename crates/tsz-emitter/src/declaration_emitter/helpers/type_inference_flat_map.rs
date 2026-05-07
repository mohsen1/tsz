use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn flat_map_array_subclass_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let call = self.arena.get_call_expr(expr_node)?;
        let callee = self.arena.get(call.expression)?;
        let access = self.arena.get_access_expr(callee)?;
        if self.get_identifier_text(access.name_or_argument).as_deref() != Some("flatMap") {
            return None;
        }

        let callback_idx = call.arguments.as_ref()?.nodes.first().copied()?;
        let callback_idx = self.skip_parenthesized_expression(callback_idx)?;
        let callback_node = self.arena.get(callback_idx)?;
        let callback = self.arena.get_function(callback_node)?;
        let returned_expr =
            self.expression_function_body_single_return_expression(callback.body)?;
        let returned_node = self.arena.get(returned_expr)?;
        let assertion = self.arena.get_type_assertion(returned_node)?;
        let asserted_name = self
            .get_identifier_text(assertion.type_node)
            .or_else(|| self.simple_type_reference_name_text(assertion.type_node))?;
        let element_type = self.array_element_type_text_for_interface(&asserted_name)?;
        Some(Self::array_type_text_for_element(&element_type))
    }

    fn expression_function_body_single_return_expression(
        &self,
        body_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let body_idx = self.skip_parenthesized_expression(body_idx)?;
        let body_node = self.arena.get(body_idx)?;
        if body_node.kind != syntax_kind_ext::BLOCK {
            return Some(body_idx);
        }

        let block = self.arena.get_block(body_node)?;
        let stmt_idx = block.statements.nodes.first().copied()?;
        let stmt_node = self.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
            return None;
        }
        let ret = self.arena.get_return_statement(stmt_node)?;
        ret.expression
            .is_some()
            .then(|| self.skip_parenthesized_expression(ret.expression))?
    }

    fn array_element_type_text_for_interface(&self, interface_name: &str) -> Option<String> {
        let source_file_idx = self.current_source_file_idx.or_else(|| {
            self.arena
                .nodes
                .iter()
                .position(|node| self.arena.get_source_file(node).is_some())
                .and_then(|idx| u32::try_from(idx).ok())
                .map(NodeIndex)
        })?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = self.arena.get_interface(stmt_node) else {
                continue;
            };
            if self.get_identifier_text(interface.name).as_deref() != Some(interface_name) {
                continue;
            }
            let Some(heritage_clauses) = interface.heritage_clauses.as_ref() else {
                continue;
            };
            for &heritage_idx in &heritage_clauses.nodes {
                let Some(heritage) = self.arena.get_heritage_clause_at(heritage_idx) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &base_idx in &heritage.types.nodes {
                    let base_text =
                        self.emit_type_node_text_normalized(base_idx).or_else(|| {
                            self.arena
                                .get(base_idx)
                                .and_then(|node| self.get_source_slice(node.pos, node.end))
                        })?;
                    if let Some(inner) = Self::array_element_text_from_array_reference(&base_text) {
                        return Some(inner);
                    }
                }
            }
        }

        None
    }

    fn array_element_text_from_array_reference(type_text: &str) -> Option<String> {
        let trimmed = type_text.trim();
        let inner = trimmed
            .strip_prefix("Array<")
            .and_then(|rest| rest.strip_suffix('>'))?;
        Some(inner.trim().to_string())
    }

    fn array_type_text_for_element(element_type: &str) -> String {
        let trimmed = element_type.trim();
        let needs_parens = trimmed.contains(" | ")
            || trimmed.contains(" & ")
            || trimmed.starts_with("keyof ")
            || trimmed.contains(" extends ");
        if needs_parens {
            format!("({trimmed})[]")
        } else {
            format!("{trimmed}[]")
        }
    }
}
