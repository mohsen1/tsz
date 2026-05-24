use super::super::Printer;
use tsz_parser::parser::node::{LoopData, Node};
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn try_emit_invalid_let_of_array_for_recovery(
        &mut self,
        node: &Node,
        loop_stmt: &LoopData,
    ) -> bool {
        let Some(header) = self.invalid_let_of_array_for_header(node, loop_stmt) else {
            return false;
        };

        self.write("for (");
        self.write(&header);
        self.write(")");
        self.write_line();
        self.increase_indent();
        self.write(";");
        self.decrease_indent();
        true
    }

    pub(in crate::emitter) fn for_in_invalid_let_header_needs_recovery_space(
        &self,
        node: &Node,
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let start = node.pos as usize;
        if start >= text.len() {
            return false;
        }
        let Some(header) = text[start..].split(')').next() else {
            return false;
        };
        let Some(open_paren) = header.find('(') else {
            return false;
        };
        let inner = header[open_paren + 1..].trim_start();
        is_keyword_followed_by(inner, "let", "in")
    }

    fn invalid_let_of_array_for_header(&self, node: &Node, loop_stmt: &LoopData) -> Option<String> {
        if loop_stmt.initializer.is_some()
            || loop_stmt.condition.is_some()
            || loop_stmt.incrementor.is_some()
            || self
                .arena
                .get(loop_stmt.statement)
                .is_none_or(|stmt| stmt.kind != syntax_kind_ext::EMPTY_STATEMENT)
        {
            return None;
        }

        let text = self.source_text?;
        let start = node.pos as usize;
        let header_end = text.get(start..)?.find(')').map(|offset| start + offset)?;
        let header = text.get(start..header_end)?;
        let open_paren = header.find('(')?;
        let inner = header[open_paren + 1..].trim_start();
        let after_let = keyword_tail(inner, "let")?.trim_start();
        let after_of = keyword_tail(after_let, "of")?.trim_start();
        let array = after_of.strip_prefix('[')?.strip_suffix(']')?;
        let elements = array
            .split(',')
            .map(str::trim)
            .filter(|element| !element.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("let of, []; {elements}; "))
    }
}

fn keyword_tail<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let tail = text.strip_prefix(keyword)?;
    if tail
        .chars()
        .next()
        .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$')
    {
        Some(tail)
    } else {
        None
    }
}

fn is_keyword_followed_by(text: &str, first: &str, second: &str) -> bool {
    let Some(tail) = keyword_tail(text, first) else {
        return false;
    };
    keyword_tail(tail.trim_start(), second).is_some()
}
