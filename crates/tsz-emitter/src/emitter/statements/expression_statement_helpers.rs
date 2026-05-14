use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn expression_statement_consumed_invalid_backslash_semicolon(
        &self,
        node: &Node,
        expression: NodeIndex,
    ) -> bool {
        let (Some(source_text), Some(expr_node)) = (self.source_text, self.arena.get(expression))
        else {
            return false;
        };
        let start = expr_node.end as usize;
        let end = node.end as usize;
        start < end
            && end <= source_text.len()
            && source_text[start..end].contains('\\')
            && source_text[start..end].contains(';')
    }
}
