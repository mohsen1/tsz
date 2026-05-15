use super::super::Printer;
use crate::safe_slice;
use tsz_parser::parser::NodeList;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn consume_recovered_expression_statement(
        &mut self,
        node: &Node,
    ) -> bool {
        let Some((start, end, ref expected_name)) =
            self.consumed_recovered_expression_statement_span
        else {
            return false;
        };

        let expression_matches = self
            .arena
            .get_expression_statement(node)
            .and_then(|stmt| self.arena.get(stmt.expression))
            .and_then(|expr| self.arena.get_identifier(expr))
            .is_some_and(|ident| ident.escaped_text == *expected_name);

        if (node.pos >= start && node.end <= end) || expression_matches {
            self.consumed_recovered_expression_statement_span = None;
            self.skip_comments_for_erased_node(node);
            return true;
        }

        if node.pos > end {
            self.consumed_recovered_expression_statement_span = None;
        }
        false
    }

    pub(in crate::emitter) fn recovered_parenthesized_arrow_property_tail(
        &self,
        declarations: &NodeList,
    ) -> Option<(String, (u32, u32))> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let effective_end = self.variable_statement_effective_end(declarations);
        let limit = std::cmp::min(effective_end as usize, bytes.len());

        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                let Some(init_node) = self.arena.get(decl.initializer) else {
                    continue;
                };
                if init_node.kind != syntax_kind_ext::ARROW_FUNCTION {
                    continue;
                }

                let init_start = std::cmp::min(init_node.pos as usize, limit);
                let init_end = std::cmp::min(init_node.end as usize, limit);
                let Ok(init_source) = safe_slice::slice(text, init_start, init_end) else {
                    continue;
                };
                if init_source.contains("=>") || !init_source.contains(':') {
                    continue;
                }

                let Some(dot) =
                    Self::find_recovered_parenthesized_property_dot(bytes, init_start, init_end)
                else {
                    continue;
                };

                let name_start = dot + 1;
                let mut name_end = name_start;
                while name_end < limit && Self::is_ascii_identifier_part(bytes[name_end]) {
                    name_end += 1;
                }
                if name_end == name_start {
                    continue;
                }

                let mut pos = name_end;
                while pos < limit && matches!(bytes[pos], b' ' | b'\t' | b'\r' | b'\n') {
                    pos += 1;
                }
                if pos < limit && bytes.get(pos) != Some(&b';') {
                    continue;
                }

                let tail_name = text[name_start..name_end].to_string();
                return Some((tail_name, (dot as u32, init_end as u32)));
            }
        }

        None
    }

    fn find_recovered_parenthesized_property_dot(
        bytes: &[u8],
        start: usize,
        end: usize,
    ) -> Option<usize> {
        let mut dot = start;
        while dot < end {
            if bytes.get(dot) != Some(&b'.') {
                dot += 1;
                continue;
            }

            let mut before_dot = dot;
            while before_dot > start && matches!(bytes[before_dot - 1], b' ' | b'\t') {
                before_dot -= 1;
            }
            if before_dot > start && bytes.get(before_dot - 1) == Some(&b')') {
                return Some(dot);
            }

            dot += 1;
        }
        None
    }

    const fn is_ascii_identifier_part(byte: u8) -> bool {
        byte == b'_' || byte == b'$' || byte.is_ascii_alphanumeric()
    }
}
