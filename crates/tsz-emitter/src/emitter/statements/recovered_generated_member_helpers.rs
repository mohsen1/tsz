use super::super::Printer;
use tsz_parser::parser::NodeList;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_recovered_generated_type_member_tail_after_variable_statement(
        &mut self,
        declarations: &NodeList,
    ) -> bool {
        let Some((tail_start, tail_end, emit_empty_statement)) =
            self.recovered_generated_type_member_tail_range(declarations)
        else {
            return false;
        };
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(tail) = text.get(tail_start..tail_end).map(str::trim) else {
            return false;
        };
        if tail.is_empty() || tail.contains('\n') || tail.contains('\r') {
            return false;
        }

        self.write_line();
        self.write(tail);
        self.write_semicolon();
        if emit_empty_statement {
            self.write_line();
            self.write_semicolon();
        }
        true
    }

    fn recovered_generated_type_member_tail_range(
        &self,
        declarations: &NodeList,
    ) -> Option<(usize, usize, bool)> {
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            if let Some(recovered) = decl_list.declarations.nodes.iter().find_map(|&decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;
                let decl = self.arena.get_variable_declaration(decl_node)?;
                let type_node = self.arena.get(decl.type_annotation)?;
                self.recovered_generated_type_member_tail_in_type_literal(type_node)
            }) {
                return Some(recovered);
            }
        }
        None
    }

    fn recovered_generated_type_member_tail_in_type_literal(
        &self,
        type_node: &Node,
    ) -> Option<(usize, usize, bool)> {
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return None;
        }
        let type_lit = self.arena.get_type_literal(type_node)?;
        for &member_idx in &type_lit.members.nodes {
            let member_node = self.arena.get(member_idx)?;
            if member_node.kind != syntax_kind_ext::CALL_SIGNATURE {
                continue;
            }
            if !self.call_signature_has_missing_generated_type_parameter(member_node) {
                continue;
            }

            let text = self.source_text?;
            let bytes = text.as_bytes();
            let member_start = (member_node.pos as usize).min(bytes.len());
            if bytes.get(member_start) != Some(&b'<') {
                continue;
            }

            let tail_start = self
                .skip_trivia_forward(member_node.pos.saturating_add(1), member_node.end)
                as usize;
            let tail_end = (member_node.end as usize).min(bytes.len());
            if tail_start >= tail_end {
                continue;
            }

            let emit_empty_statement =
                self.type_literal_recovery_has_trailing_source_semicolon(type_node, tail_end);
            return Some((tail_start, tail_end, emit_empty_statement));
        }
        None
    }

    fn call_signature_has_missing_generated_type_parameter(&self, node: &Node) -> bool {
        let Some(signature) = self.arena.get_signature(node) else {
            return false;
        };
        let Some(type_parameters) = signature.type_parameters.as_ref() else {
            return false;
        };

        type_parameters.nodes.iter().any(|&param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_type_parameter(param_node) else {
                return false;
            };
            let Some(name_node) = self.arena.get(param.name) else {
                return false;
            };
            name_node.pos == name_node.end
                && self
                    .arena
                    .get_identifier(name_node)
                    .is_some_and(|ident| ident.escaped_text.is_empty())
        })
    }

    fn type_literal_recovery_has_trailing_source_semicolon(
        &self,
        type_node: &Node,
        start: usize,
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let bytes = text.as_bytes();
        let limit = (type_node.end as usize).min(bytes.len());
        let mut pos = start.min(limit);

        while pos < limit && bytes[pos] != b'}' {
            pos += 1;
        }
        if pos >= limit {
            return false;
        }
        pos += 1;
        while pos < limit && matches!(bytes[pos], b' ' | b'\t' | b'\r' | b'\n') {
            pos += 1;
        }
        pos < limit && bytes[pos] == b';'
    }
}
