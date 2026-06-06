use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn object_literal_last_shorthand_continuation_tail(
        &self,
        prop: NodeIndex,
        object_node: &Node,
    ) -> Option<String> {
        let source = self.source_text?;
        let prop_node = self.arena.get(prop)?;
        if prop_node.kind != syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
            return None;
        }
        let shorthand = self.arena.get_shorthand_property(prop_node)?;
        if shorthand.equals_token || shorthand.object_assignment_initializer != NodeIndex::NONE {
            return None;
        }
        let name_node = self.arena.get(shorthand.name)?;
        if name_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let ident = self.arena.get_identifier(name_node)?;

        let search_start = std::cmp::min(prop_node.pos as usize, source.len());
        let search_end = std::cmp::min(object_node.end as usize, source.len());
        if search_start >= search_end {
            return None;
        }
        let search = crate::safe_slice::slice(source, search_start, search_end).ok()?;
        let dot_pos = search.find('.')?;
        let before_tail = &search[..dot_pos];
        if before_tail.contains('\n') || before_tail.trim() != ident.escaped_text {
            return None;
        }

        let mut depth = 0_i32;
        let bytes = source.as_bytes();
        let mut tail_end = search_start + dot_pos;
        while tail_end < search_end {
            match bytes[tail_end] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        tail_end += 1;
                        break;
                    }
                }
                b';' | b'\n' | b'\r' if depth == 0 => break,
                _ => {}
            }
            tail_end += 1;
        }
        let tail = crate::safe_slice::slice(source, search_start + dot_pos, tail_end).ok()?;
        Some(format!(": {tail}"))
    }
}
